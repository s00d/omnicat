//! Sequential line iterator with memchr-accelerated scanning.

use std::io::{self, Read};

use memchr::memchr;

const CHUNK: usize = 1024 * 1024;

/// Iterate lines from a sequential reader, returning owned line bytes.
pub struct LineIter<R: Read> {
    reader: R,
    buf: Vec<u8>,
    carry: Vec<u8>,
    line_no: u64,
    bytes_read: u64,
    max_bytes: Option<u64>,
    eof: bool,
}

impl<R: Read> LineIter<R> {
    pub fn new(reader: R, max_bytes: Option<u64>) -> Self {
        Self {
            reader,
            buf: Vec::with_capacity(CHUNK),
            carry: Vec::new(),
            line_no: 0,
            bytes_read: 0,
            max_bytes,
            eof: false,
        }
    }

    fn over_limit(&self) -> bool {
        self.max_bytes.is_some_and(|max| self.bytes_read >= max)
    }
}

impl<R: Read> Iterator for LineIter<R> {
    type Item = io::Result<(u64, Vec<u8>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(line) = take_line(&mut self.carry) {
                self.line_no += 1;
                return Some(Ok((self.line_no, line)));
            }
            if self.eof || self.over_limit() {
                if self.carry.is_empty() {
                    return None;
                }
                self.line_no += 1;
                return Some(Ok((self.line_no, std::mem::take(&mut self.carry))));
            }
            self.buf.clear();
            self.buf.resize(CHUNK, 0);
            let n = match self.reader.read(&mut self.buf) {
                Ok(0) => {
                    self.eof = true;
                    continue;
                }
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            self.bytes_read += n as u64;
            self.carry.extend_from_slice(&self.buf[..n]);
        }
    }
}

fn take_line(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    if let Some(pos) = memchr(b'\n', buf) {
        let mut line = buf.drain(..=pos).collect::<Vec<_>>();
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        return Some(line);
    }
    None
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
}
