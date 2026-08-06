//! Memory-mapped read-only view for random access / search.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use memmap2::Mmap;

use crate::io::line::LineRef;

/// Read-only mmap of a plain (uncompressed) file.
pub struct MmapView {
    _file: File,
    pub data: Mmap,
}

impl MmapView {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("mmap open {}", path.display()))?;
        // SAFETY: read-only mapping; caller must not use on growing files during follow.
        let data = unsafe { Mmap::map(&file).context("mmap failed")? };
        Ok(Self { _file: file, data })
    }

    pub fn lines(&self) -> MmapLineIter<'_> {
        MmapLineIter {
            data: &self.data,
            offset: 0,
            line_no: 0,
        }
    }

    pub fn find_substring(&self, needle: &[u8]) -> Vec<usize> {
        let mut out = Vec::new();
        let mut start = 0;
        while start < self.data.len() {
            if let Some(pos) = memchr::memmem::find(&self.data[start..], needle) {
                out.push(start + pos);
                start += pos + needle.len().max(1);
            } else {
                break;
            }
        }
        out
    }
}

pub struct MmapLineIter<'a> {
    data: &'a [u8],
    offset: usize,
    line_no: u64,
}

impl<'a> Iterator for MmapLineIter<'a> {
    type Item = LineRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.data.len() {
            return None;
        }
        let rest = &self.data[self.offset..];
        let end = memchr::memchr(b'\n', rest)
            .map(|p| p + 1)
            .unwrap_or(rest.len());
        let slice = &rest[..end];
        self.offset += end;
        self.line_no += 1;
        let mut line = slice;
        if line.last() == Some(&b'\n') {
            line = &line[..line.len() - 1];
        }
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        Some(LineRef {
            bytes: line,
            line_no: self.line_no,
        })
    }
}
