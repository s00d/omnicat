//! Incremental follow reader (tail -f) with rotation/truncation detection.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::io::line::LineRef;
use crate::io::source::{detect_compression, open_reader, Compression};

const POLL_MS: u64 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId {
    len: u64,
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

impl FileId {
    fn from_meta(meta: &std::fs::Metadata) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Self {
                len: meta.len(),
                dev: meta.dev(),
                ino: meta.ino(),
            }
        }
        #[cfg(not(unix))]
        {
            Self { len: meta.len() }
        }
    }

    fn identity_eq(&self, other: &Self) -> bool {
        #[cfg(unix)]
        {
            self.dev == other.dev && self.ino == other.ino
        }
        #[cfg(not(unix))]
        {
            let _ = other;
            true
        }
    }
}

/// Blocking iterator of new lines appended to a file.
pub struct FollowIter {
    path: PathBuf,
    /// Plain (uncompressed) follow state.
    plain: Option<PlainFollow>,
    /// Compressed follow: keep decoder open (no re-decompress from 0 each poll).
    compressed: Option<CompressedFollow>,
    line_no: u64,
    buf: String,
    id: FileId,
}

struct PlainFollow {
    reader: BufReader<File>,
}

struct CompressedFollow {
    reader: BufReader<Box<dyn Read + Send>>,
    /// Compressed file length when decoder was opened (rotation/truncation detect).
    opened_len: u64,
}

impl FollowIter {
    pub fn open(path: &Path) -> io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let id = FileId::from_meta(&meta);
        let compressed = detect_compression(path) != Compression::None;
        if compressed {
            let reader = open_reader(path, true).map_err(io::Error::other)?;
            let mut br = BufReader::new(reader);
            // Start at EOF of the *decompressed* stream.
            let mut line = String::new();
            let mut seen = 0u64;
            while br.read_line(&mut line).unwrap_or(0) > 0 {
                seen += 1;
                line.clear();
            }
            Ok(Self {
                path: path.to_path_buf(),
                plain: None,
                compressed: Some(CompressedFollow {
                    reader: br,
                    opened_len: id.len,
                }),
                line_no: seen,
                buf: String::new(),
                id,
            })
        } else {
            let mut file = File::open(path)?;
            file.seek(SeekFrom::End(0))?;
            Ok(Self {
                path: path.to_path_buf(),
                plain: Some(PlainFollow {
                    reader: BufReader::new(file),
                }),
                compressed: None,
                line_no: 0,
                buf: String::new(),
                id,
            })
        }
    }

    fn refresh_id(&mut self) -> io::Result<FileId> {
        let meta = std::fs::metadata(&self.path)?;
        Ok(FileId::from_meta(&meta))
    }

    fn maybe_reopen_plain(&mut self) -> io::Result<()> {
        let Ok(path_id) = self.refresh_id() else {
            return Ok(());
        };
        let Some(plain) = self.plain.as_mut() else {
            return Ok(());
        };
        let pos = plain.reader.stream_position().unwrap_or(0);
        let rotated = !self.id.identity_eq(&path_id);
        let truncated = pos > path_id.len;
        if rotated || truncated {
            let mut f = File::open(&self.path)?;
            f.seek(SeekFrom::Start(0))?;
            plain.reader = BufReader::new(f);
            self.id = path_id;
        }
        Ok(())
    }

    fn maybe_reopen_compressed(&mut self) -> io::Result<()> {
        let Ok(path_id) = self.refresh_id() else {
            return Ok(());
        };
        let Some(comp) = self.compressed.as_mut() else {
            return Ok(());
        };
        let rotated = !self.id.identity_eq(&path_id);
        let truncated = path_id.len < comp.opened_len;
        if rotated || truncated {
            let reader = open_reader(&self.path, true).map_err(io::Error::other)?;
            comp.reader = BufReader::new(reader);
            comp.opened_len = path_id.len;
            self.id = path_id;
            self.line_no = 0;
        } else if path_id.len > comp.opened_len {
            // File grew: for concatenated gzip members, reopen and skip already-seen lines.
            // Keeping the old decoder often stalls at member EOF even when more bytes exist.
            let skip = self.line_no;
            let reader = open_reader(&self.path, true).map_err(io::Error::other)?;
            let mut br = BufReader::new(reader);
            let mut line = String::new();
            let mut idx = 0u64;
            while idx < skip && br.read_line(&mut line)? > 0 {
                idx += 1;
                line.clear();
            }
            comp.reader = br;
            comp.opened_len = path_id.len;
            self.id = path_id;
        }
        Ok(())
    }

    fn next_compressed_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        self.maybe_reopen_compressed()?;
        let Some(comp) = self.compressed.as_mut() else {
            return Ok(None);
        };
        self.buf.clear();
        match comp.reader.read_line(&mut self.buf)? {
            0 => Ok(None),
            _ => {
                let mut bytes = std::mem::take(&mut self.buf).into_bytes();
                if bytes.last() == Some(&b'\n') {
                    bytes.pop();
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                Ok(Some(bytes))
            }
        }
    }
}

impl Iterator for FollowIter {
    type Item = io::Result<(u64, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.compressed.is_some() {
                match self.next_compressed_line() {
                    Ok(Some(bytes)) => {
                        self.line_no += 1;
                        return Some(Ok((self.line_no, bytes)));
                    }
                    Ok(None) => {
                        thread::sleep(Duration::from_millis(POLL_MS));
                    }
                    Err(e) => return Some(Err(e)),
                }
                continue;
            }

            self.buf.clear();
            let plain = self.plain.as_mut()?;
            match plain.reader.read_line(&mut self.buf) {
                Ok(0) => {
                    if self.maybe_reopen_plain().is_err() {
                        return Some(Err(io::Error::other("follow reopen failed")));
                    }
                    thread::sleep(Duration::from_millis(POLL_MS));
                }
                Ok(_) => {
                    self.line_no += 1;
                    let mut bytes = std::mem::take(&mut self.buf).into_bytes();
                    if bytes.last() == Some(&b'\n') {
                        bytes.pop();
                    }
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    return Some(Ok((self.line_no, bytes)));
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

/// Run a follow loop, calling `on_line` for each new line. Blocks forever.
pub fn follow_loop<F>(path: &Path, mut on_line: F) -> io::Result<()>
where
    F: FnMut(LineRef<'_>) -> io::Result<()>,
{
    for item in FollowIter::open(path)? {
        let (line_no, bytes) = item?;
        on_line(LineRef {
            line_no,
            bytes: &bytes,
        })?;
    }
    Ok(())
}
