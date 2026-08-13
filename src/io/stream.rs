//! Sequential line scanning with memchr — reusable buffer, early-stop friendly.

use std::io::{self, Read};
use std::ops::ControlFlow;

use memchr::memchr;

const CHUNK: usize = 1024 * 1024;

/// Iterate lines from a sequential reader.
/// Prefer [`for_each_line`] for early-stop scans without O(n²) drain shifts.
pub struct LineIter<R: Read> {
    reader: R,
    read_buf: Vec<u8>,
    carry: Vec<u8>,
    start: usize,
    line_no: u64,
    bytes_read: u64,
    max_bytes: Option<u64>,
    eof: bool,
}

impl<R: Read> LineIter<R> {
    pub fn new(reader: R, max_bytes: Option<u64>) -> Self {
        Self {
            reader,
            read_buf: vec![0u8; CHUNK],
            carry: Vec::with_capacity(CHUNK),
            start: 0,
            line_no: 0,
            bytes_read: 0,
            max_bytes,
            eof: false,
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    fn over_limit(&self) -> bool {
        self.max_bytes.is_some_and(|max| self.bytes_read >= max)
    }

    fn compact(&mut self) {
        if self.start > 0 {
            self.carry.drain(..self.start);
            self.start = 0;
        }
    }

    fn fill_more(&mut self) -> io::Result<()> {
        self.compact();
        let want = match self.max_bytes {
            Some(max) => {
                let left = max.saturating_sub(self.bytes_read);
                if left == 0 {
                    self.eof = true;
                    return Ok(());
                }
                (left as usize).min(CHUNK)
            }
            None => CHUNK,
        };
        let n = self.reader.read(&mut self.read_buf[..want])?;
        if n == 0 {
            self.eof = true;
        } else {
            self.bytes_read += n as u64;
            self.carry.extend_from_slice(&self.read_buf[..n]);
        }
        Ok(())
    }

    /// Scan lines with a callback. `Break` stops reading further I/O.
    pub fn visit_lines<F>(&mut self, mut f: F) -> io::Result<()>
    where
        F: FnMut(u64, &[u8]) -> ControlFlow<()>,
    {
        loop {
            // Find next newline in unread region
            let rel = memchr(b'\n', &self.carry[self.start..]);
            if let Some(rel) = rel {
                let abs_nl = self.start + rel;
                let mut end = abs_nl;
                if end > self.start && self.carry[end - 1] == b'\r' {
                    end -= 1;
                }
                self.line_no += 1;
                let line_no = self.line_no;
                let flow = f(line_no, &self.carry[self.start..end]);
                self.start = abs_nl + 1;
                if matches!(flow, ControlFlow::Break(())) {
                    return Ok(());
                }
                continue;
            }

            if self.eof || self.over_limit() {
                if self.start < self.carry.len() {
                    self.line_no += 1;
                    let line_no = self.line_no;
                    let flow = f(line_no, &self.carry[self.start..]);
                    self.start = self.carry.len();
                    let _ = flow; // last line; always done after
                }
                return Ok(());
            }

            self.fill_more()?;
        }
    }
}

impl<R: Read> Iterator for LineIter<R> {
    type Item = io::Result<(u64, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut out: Option<(u64, Vec<u8>)> = None;
        let result = self.visit_lines(|line_no, bytes| {
            out = Some((line_no, bytes.to_vec()));
            ControlFlow::Break(())
        });
        match result {
            Err(e) => Some(Err(e)),
            Ok(()) => out.map(Ok),
        }
    }
}

/// Zero-copy-ish line scan: callback sees `&[u8]` (no trailing newline).
pub fn for_each_line<R: Read, F>(reader: R, max_bytes: Option<u64>, f: F) -> io::Result<()>
where
    F: FnMut(u64, &[u8]) -> ControlFlow<()>,
{
    LineIter::new(reader, max_bytes).visit_lines(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn splits_crlf_and_lf() {
        let data = b"a\r\nb\nc\n";
        let cur = Cursor::new(data.as_slice());
        let lines: Vec<_> = LineIter::new(cur, None).map(|l| l.unwrap().1).collect();
        assert_eq!(lines, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn for_each_early_stop() {
        let data = b"hit\nskip1\nskip2\n";
        let mut seen = 0u64;
        for_each_line(Cursor::new(data.as_slice()), None, |_n, line| {
            seen += 1;
            if line == b"hit" {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .unwrap();
        assert_eq!(seen, 1);
    }

    #[test]
    fn for_each_reads_all() {
        let data = b"a\nb\nc\n";
        let mut n = 0u64;
        for_each_line(Cursor::new(data.as_slice()), None, |_, _| {
            n += 1;
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(n, 3);
    }
}
