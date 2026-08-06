//! Incremental follow reader (tail -f) with rotation/truncation detection.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::io::line::LineRef;

const POLL_MS: u64 = 200;

/// Blocking iterator of new lines appended to a file.
pub struct FollowIter {
    path: PathBuf,
    reader: BufReader<File>,
    line_no: u64,
    buf: String,
}

impl FollowIter {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        file.seek(SeekFrom::End(0))?;
        Ok(Self {
            path: path.to_path_buf(),
            reader: BufReader::new(file),
            line_no: 0,
            buf: String::new(),
        })
    }

    fn maybe_reopen(&mut self) -> io::Result<()> {
        let meta_len = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        let pos = self.reader.stream_position().unwrap_or(0);
        if pos > meta_len {
            let mut f = File::open(&self.path)?;
            f.seek(SeekFrom::Start(0))?;
            self.reader = BufReader::new(f);
        }
        Ok(())
    }
}

impl Iterator for FollowIter {
    type Item = io::Result<LineRef<'static>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.buf.clear();
            match self.reader.read_line(&mut self.buf) {
                Ok(0) => {
                    if self.maybe_reopen().is_err() {
                        return Some(Err(io::Error::other("follow reopen failed")));
                    }
                    thread::sleep(Duration::from_millis(POLL_MS));
                }
                Ok(_) => {
                    self.line_no += 1;
                    let mut bytes = self.buf.as_bytes().to_vec();
                    if bytes.last() == Some(&b'\n') {
                        bytes.pop();
                    }
                    if bytes.last() == Some(&b'\r') {
                        bytes.pop();
                    }
                    return Some(Ok(LineRef {
                        bytes: Box::leak(bytes.into_boxed_slice()),
                        line_no: self.line_no,
                    }));
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
        on_line(item?)?;
    }
    Ok(())
}
