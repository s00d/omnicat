//! Streaming statement reader for MySQL dumps.

use std::io::{BufRead, BufReader, Read};

use anyhow::{Context, Result};

use crate::io::source::open_reader;

/// Incrementally read SQL statements terminated by `;`.
pub struct DumpReader<R: BufRead> {
    inner: R,
    carry: String,
    bytes_read: u64,
}

pub type DumpFileReader = DumpReader<BufReader<Box<dyn Read + Send>>>;

impl DumpReader<BufReader<Box<dyn Read>>> {
    pub fn open(path: &std::path::Path) -> Result<DumpFileReader> {
        let reader = open_reader(path, true).context("open dump")?;
        Ok(DumpReader {
            inner: BufReader::with_capacity(1024 * 1024, reader),
            carry: String::new(),
            bytes_read: 0,
        })
    }
}
impl<R: BufRead> DumpReader<R> {
    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    pub fn next_statement(&mut self) -> Result<Option<String>> {
        loop {
            if let Some(end) = find_statement_end(&self.carry) {
                let stmt = self.carry[..end].trim().to_string();
                self.carry.drain(..=end);
                if !stmt.is_empty() {
                    return Ok(Some(stmt));
                }
                continue;
            }
            let mut buf = [0u8; 256 * 1024];
            let n = self.inner.read(&mut buf).context("read dump")?;
            if n == 0 {
                if self.carry.trim().is_empty() {
                    return Ok(None);
                }
                let rest = std::mem::take(&mut self.carry);
                return Ok(Some(rest.trim().to_string()));
            }
            self.bytes_read += n as u64;
            self.carry.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    }
}

fn find_statement_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    while i < bytes.len() {
        let b = bytes[i];
        if escape {
            escape = false;
            i += 1;
            continue;
        }
        if in_single {
            if b == b'\\' {
                escape = true;
            } else if b == b'\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            if b == b'`' {
                in_backtick = false;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' => in_single = true,
            b'"' => in_double = true,
            b'`' => in_backtick = true,
            b';' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn splits_on_semicolon_outside_quotes() {
        let data = b"INSERT INTO t VALUES ('a;b',1);\nCREATE TABLE u (id INT);";
        let mut r = DumpReader {
            inner: std::io::BufReader::new(Cursor::new(data.as_slice())),
            carry: String::new(),
            bytes_read: 0,
        };
        let s1 = r.next_statement().unwrap().unwrap();
        assert!(s1.starts_with("INSERT"));
        let s2 = r.next_statement().unwrap().unwrap();
        assert!(s2.starts_with("CREATE"));
    }
}
