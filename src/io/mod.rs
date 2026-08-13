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
use std::ops::ControlFlow;
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

    /// Iterate lines. Return [`ControlFlow::Break`] from the callback to stop I/O early.
    pub fn for_each_line<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(LineRef<'_>) -> Result<ControlFlow<()>>,
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
                    if matches!(f(line)?, ControlFlow::Break(())) {
                        break;
                    }
                }
                Ok(())
            }
            ReadMode::Follow { .. } => {
                follow::follow_loop(&self.path, |line| match f(line) {
                    Ok(ControlFlow::Continue(())) => Ok(()),
                    Ok(ControlFlow::Break(())) => Err(std::io::Error::other("follow break")),
                    Err(e) => Err(std::io::Error::other(format!("{e:#}"))),
                })?;
                Ok(())
            }
            ReadMode::Mmap => {
                let view = MmapView::open(&self.path)?;
                for line in view.lines() {
                    if matches!(f(line)?, ControlFlow::Break(())) {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    fn stream_reader<R: Read, F: FnMut(LineRef<'_>) -> Result<ControlFlow<()>>>(
        &self,
        reader: R,
        f: &mut F,
    ) -> Result<()> {
        let mut iter = LineIter::new(reader, self.options.max_scan_bytes);
        let mut err: Option<anyhow::Error> = None;
        iter.visit_lines(|line_no, bytes| {
            let line = LineRef { bytes, line_no };
            match f(line) {
                Ok(ControlFlow::Continue(())) => ControlFlow::Continue(()),
                Ok(ControlFlow::Break(())) => ControlFlow::Break(()),
                Err(e) => {
                    err = Some(e);
                    ControlFlow::Break(())
                }
            }
        })
        .context("read line")?;
        if let Some(e) = err {
            return Err(e);
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

/// Test helper: wrap a reader and count how many bytes were pulled.
#[derive(Debug)]
pub struct CountingReader<R> {
    inner: R,
    pub bytes: u64,
}

impl<R> CountingReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, bytes: 0 }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes += n as u64;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn stream_reads_all_lines() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.log");
        std::fs::write(&p, b"one\ntwo\nthree\n").unwrap();
        let h = FileHandle::open(&p, OpenOptions::stream()).unwrap();
        let mut lines = Vec::new();
        h.for_each_line(|l| {
            lines.push(l.text_lossy());
            Ok(ControlFlow::Continue(()))
        })
        .unwrap();
        assert_eq!(lines, vec!["one", "two", "three"]);
    }

    #[test]
    fn for_each_line_early_stop_limits_io() {
        let mut body = String::from("MATCH\n");
        while body.len() < 2 * 1024 * 1024 + 64 {
            body.push_str("noise-line-xxxx\n");
        }
        let total = body.len() as u64;
        let mut counter = CountingReader::new(Cursor::new(body.into_bytes()));
        let mut hits = 0usize;
        LineIter::new(&mut counter, None)
            .visit_lines(|_, line| {
                if line == b"MATCH" {
                    hits += 1;
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            })
            .unwrap();
        assert_eq!(hits, 1);
        // First fill may pull one CHUNK (1 MiB), but must not consume the whole 2+ MiB file.
        assert!(
            counter.bytes <= 1024 * 1024 + 4096,
            "read too much: {} of {}",
            counter.bytes,
            total
        );
        assert!(counter.bytes < total / 2);
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
