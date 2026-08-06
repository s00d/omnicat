//! Read last N lines by scanning backwards from EOF.

use std::io::{self, Read, Seek, SeekFrom};

use memchr::memchr_iter;

use crate::io::line::LineRef;
use crate::io::source::open_reader;

const CHUNK: usize = 64 * 1024;

/// Read the last `n` lines from a (possibly compressed) file.
pub fn tail_lines(
    path: &std::path::Path,
    n: usize,
    decompress: bool,
) -> io::Result<Vec<LineRef<'static>>> {
    if n == 0 {
        return Ok(Vec::new());
    }

    // For compressed streams, forward-read into a ring buffer of last n lines.
    if decompress
        && crate::io::source::detect_compression(path) != crate::io::source::Compression::None
    {
        return tail_compressed(path, n, decompress);
    }

    let mut file =
        crate::io::source::open_plain_file(path).map_err(|e| io::Error::other(e.to_string()))?;
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(Vec::new());
    }

    let mut pos = len;
    let mut carry = Vec::new();
    let mut lines_found: Vec<Vec<u8>> = Vec::new();

    while pos > 0 && lines_found.len() <= n {
        let step = CHUNK.min(pos as usize);
        pos -= step as u64;
        file.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; step];
        file.read_exact(&mut chunk)?;
        let mut block = chunk;
        block.extend_from_slice(&carry);
        carry = block;

        let mut start = 0usize;
        for nl in memchr_iter(b'\n', &carry) {
            if nl > start {
                let mut line = carry[start..nl].to_vec();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                lines_found.push(line);
            }
            start = nl + 1;
        }
        carry = carry[start..].to_vec();
    }

    if !carry.is_empty() && lines_found.len() < n {
        lines_found.push(carry);
    }

    let skip = lines_found.len().saturating_sub(n);
    let out: Vec<LineRef<'static>> = lines_found
        .into_iter()
        .skip(skip)
        .enumerate()
        .map(|(i, b)| LineRef {
            bytes: Box::leak(b.into_boxed_slice()),
            line_no: (i + 1) as u64,
        })
        .collect();
    Ok(out)
}

fn tail_compressed(
    path: &std::path::Path,
    n: usize,
    decompress: bool,
) -> io::Result<Vec<LineRef<'static>>> {
    let mut reader = open_reader(path, decompress).map_err(|e| io::Error::other(e.to_string()))?;
    let mut ring: std::collections::VecDeque<Vec<u8>> =
        std::collections::VecDeque::with_capacity(n + 1);
    let mut buf = [0u8; CHUNK];
    let mut carry = Vec::new();
    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..read]);
        while let Some(pos) = memchr::memchr(b'\n', &carry) {
            let mut line = carry.drain(..=pos).collect::<Vec<_>>();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if ring.len() >= n {
                ring.pop_front();
            }
            ring.push_back(line);
        }
    }
    if !carry.is_empty() {
        if ring.len() >= n {
            ring.pop_front();
        }
        ring.push_back(carry);
    }
    Ok(ring
        .into_iter()
        .enumerate()
        .map(|(i, b)| LineRef {
            bytes: Box::leak(b.into_boxed_slice()),
            line_no: (i + 1) as u64,
        })
        .collect())
}
