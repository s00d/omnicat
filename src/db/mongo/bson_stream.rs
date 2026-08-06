//! Streaming BSON document reader.

use std::io::{Read, Seek, SeekFrom};

use anyhow::{Context, Result};
use bson::Document;

pub struct BsonReader<R: Read> {
    inner: R,
    bytes_read: u64,
}

impl<R: Read> BsonReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            bytes_read: 0,
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.bytes_read
    }

    pub fn next_document(&mut self) -> Result<Option<Document>> {
        let mut len_buf = [0u8; 4];
        match self.inner.read_exact(&mut len_buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        self.bytes_read += 4;
        let len = i32::from_le_bytes(len_buf);
        if len < 5 {
            anyhow::bail!("invalid BSON length {len}");
        }
        let len = len as usize;
        let mut buf = vec![0u8; len];
        buf[..4].copy_from_slice(&len_buf);
        self.inner
            .read_exact(&mut buf[4..])
            .context("read BSON body")?;
        self.bytes_read += (len - 4) as u64;
        let doc = bson::from_slice(&buf).context("decode BSON")?;
        Ok(Some(doc))
    }
}

impl<R: Read + Seek> BsonReader<R> {
    pub fn reset(&mut self) -> Result<()> {
        self.inner.seek(SeekFrom::Start(0))?;
        self.bytes_read = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_two_docs() {
        let d1 = bson::doc! {"a": 1i32};
        let d2 = bson::doc! {"b": 2i32};
        let mut buf = Vec::new();
        for d in [d1, d2] {
            buf.extend(bson::to_vec(&d).unwrap());
        }
        let mut r = BsonReader::new(Cursor::new(buf));
        assert!(r.next_document().unwrap().is_some());
        assert!(r.next_document().unwrap().is_some());
        assert!(r.next_document().unwrap().is_none());
    }
}
