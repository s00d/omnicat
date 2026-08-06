//! Unified large-file I/O engine for omnicat.
//!
//! Streaming sequential scan, tail-from-EOF, follow, and mmap — no DB/index.

pub mod follow;
pub mod line;
pub mod mmap;
pub mod mode;
pub mod source;
pub mod stream;
pub mod tail;

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub use follow::{follow_loop, FollowIter};
pub use line::LineRef;
pub use mmap::MmapView;
pub use mode::{OpenOptions, ReadMode};
pub use source::{detect_compression, open_reader, Compression};
pub use stream::LineIter;
pub use tail::tail_lines;

/// Handle to a file opened through the I/O engine.
pub struct FileHandle {
    pub path: PathBuf,
    pub options: OpenOptions,
}

impl FileHandle {
    pub fn open(path: impl AsRef<Path>, options: OpenOptions) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            options,
        })
    }

    /// Iterate lines according to the configured read mode.
    pub fn for_each_line<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(LineRef<'_>) -> Result<()>,
    {
        match self.options.mode {
            ReadMode::Stream => {
                let reader =
                    open_reader(&self.path, self.options.decompress).context("open reader")?;
                self.stream_reader(reader, &mut f)
            }
            ReadMode::Tail { lines } => {
                let refs =
                    tail_lines(&self.path, lines, self.options.decompress).context("tail lines")?;
                for line in refs {
                    f(line)?;
                }
                Ok(())
            }
            ReadMode::Follow { .. } => {
                follow::follow_loop(&self.path, |line| {
                    f(line).map_err(|e| std::io::Error::other(format!("{e:#}")))
                })?;
                Ok(())
            }
            ReadMode::Mmap => {
                let view = MmapView::open(&self.path)?;
                for line in view.lines() {
                    f(line)?;
                }
                Ok(())
            }
        }
    }

    fn stream_reader<R: Read, F: FnMut(LineRef<'_>) -> Result<()>>(
        &self,
        reader: R,
        f: &mut F,
    ) -> Result<()> {
        for item in LineIter::new(reader, self.options.max_scan_bytes) {
            let (line_no, bytes) = item.context("read line")?;
            f(LineRef {
                bytes: &bytes,
                line_no,
            })?;
        }
        Ok(())
    }

    /// Collect tail lines (convenience).
    pub fn tail(&self, n: usize) -> Result<Vec<(u64, Vec<u8>)>> {
        Ok(tail_lines(&self.path, n, self.options.decompress)?
            .into_iter()
            .map(|l| (l.line_no, l.bytes.to_vec()))
            .collect())
    }
}

/// Default tail line count for huge files when no mode specified.
pub const DEFAULT_TAIL_LINES: usize = 100;

/// If file is larger than this, default view uses tail instead of full stream.
pub const HUGE_FILE_BYTES: u64 = 16 * 1024 * 1024;

pub fn file_len(path: &Path) -> Result<u64> {
    Ok(std::fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?
        .len())
}

/// Read up to `max_bytes` from a file (0 = unlimited). Honors compression when `decompress`.
pub fn read_bytes_capped(path: &Path, max_bytes: usize, decompress: bool) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut reader = source::open_reader(path, decompress).context("open reader")?;
    let mut buf = Vec::new();
    if max_bytes == 0 {
        reader.read_to_end(&mut buf)?;
    } else {
        reader
            .take(max_bytes as u64)
            .read_to_end(&mut buf)
            .context("read capped bytes")?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_reads_all_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.log");
        std::fs::write(&p, b"one\ntwo\nthree\n").unwrap();
        let h = FileHandle::open(&p, OpenOptions::stream()).unwrap();
        let mut lines = Vec::new();
        h.for_each_line(|l| {
            lines.push(l.text_lossy());
            Ok(())
        })
        .unwrap();
        assert_eq!(lines, vec!["one", "two", "three"]);
    }

    #[test]
    fn tail_last_two() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.log");
        std::fs::write(&p, b"a\nb\nc\nd\n").unwrap();
        let lines = tail_lines(&p, 2, false).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text_lossy(), "c");
        assert_eq!(lines[1].text_lossy(), "d");
    }
}
