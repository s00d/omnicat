//! Compression layer detection and reader wrapping.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use liblzma::read::XzDecoder;

/// Compression kind inferred from file name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

pub fn detect_compression(path: &Path) -> Compression {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Compression::Gzip;
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
        return Compression::Zstd;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "gz" if !name.ends_with(".tar.gz") => Compression::Gzip,
        "bz2" => Compression::Bzip2,
        "xz" => Compression::Xz,
        "zst" | "zstd" => Compression::Zstd,
        _ => Compression::None,
    }
}

/// Boxed reader, optionally decompressed.
pub fn open_reader(path: &Path, decompress: bool) -> Result<Box<dyn Read + Send>> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    if !decompress {
        return Ok(Box::new(BufReader::with_capacity(256 * 1024, file)));
    }
    match detect_compression(path) {
        Compression::None => Ok(Box::new(BufReader::with_capacity(256 * 1024, file))),
        Compression::Gzip => Ok(Box::new(BufReader::with_capacity(
            256 * 1024,
            GzDecoder::new(file),
        ))),
        Compression::Bzip2 => Ok(Box::new(BufReader::with_capacity(
            256 * 1024,
            BzDecoder::new(file),
        ))),
        Compression::Xz => Ok(Box::new(BufReader::with_capacity(
            256 * 1024,
            XzDecoder::new(file),
        ))),
        Compression::Zstd => Ok(Box::new(BufReader::with_capacity(
            256 * 1024,
            zstd::Decoder::new(file).context("zstd decoder")?,
        ))),
    }
}

/// Plain file for tail / mmap (no decompression wrapper on mmap path).
pub fn open_plain_file(path: &Path) -> Result<File> {
    File::open(path).with_context(|| format!("open {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_gz() {
        assert_eq!(
            detect_compression(Path::new("app.log.gz")),
            Compression::Gzip
        );
    }

    #[test]
    fn detects_zst() {
        assert_eq!(
            detect_compression(Path::new("app.log.zst")),
            Compression::Zstd
        );
    }
}
