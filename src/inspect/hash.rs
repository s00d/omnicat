//! Multi-algorithm checksums for files and deterministic directory digests.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use md5::{Digest as _, Md5};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

use crate::config::OmnicatConfig;
use crate::inspect::report::{human_size, HashReport};

const CHUNK: usize = 64 * 1024;

pub fn build_hash(path: &Path, display: &str, config: &OmnicatConfig) -> Result<HashReport> {
    let meta = fs::symlink_metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.is_dir() {
        hash_directory(path, display, config)
    } else if meta.is_file() {
        hash_file(path, display, config)
    } else {
        bail!("hash not supported for special file {}", path.display());
    }
}

fn hash_file(path: &Path, display: &str, config: &OmnicatConfig) -> Result<HashReport> {
    let size = fs::metadata(path)?.len();
    let max = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes as u64)
    };
    let digests = hash_bytes_streaming(path, max)?;
    let truncated = max.map(|m| size > m).unwrap_or(false);
    Ok(HashReport {
        path: display.to_string(),
        kind: "file".into(),
        size,
        size_human: human_size(size),
        md5: digests.md5,
        sha1: digests.sha1,
        sha256: digests.sha256,
        sha512: digests.sha512,
        blake3: digests.blake3,
        truncated,
        entries: 0,
        note: if truncated {
            Some(format!(
                "Hashed first {} bytes. Use --all to disable limit.",
                max.unwrap_or(0)
            ))
        } else {
            None
        },
    })
}

fn hash_directory(path: &Path, display: &str, config: &OmnicatConfig) -> Result<HashReport> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_files(path, path, &mut files, 0)?;
    files.sort();

    let max = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes as u64)
    };

    let mut hasher = blake3::Hasher::new();
    let mut total_size = 0u64;
    for rel in &files {
        let abs = path.join(rel);
        let size = fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
        total_size = total_size.saturating_add(size);
        let digests = hash_bytes_streaming(&abs, max)?;
        // Deterministic: relative path (posix) + blake3 of content
        hasher.update(rel.to_string_lossy().replace('\\', "/").as_bytes());
        hasher.update(&[0]);
        hasher.update(digests.blake3.as_bytes());
        hasher.update(&[0]);
    }

    let combined = hasher.finalize().to_hex().to_string();
    Ok(HashReport {
        path: display.to_string(),
        kind: "directory".into(),
        size: total_size,
        size_human: human_size(total_size),
        md5: String::new(),
        sha1: String::new(),
        sha256: String::new(),
        sha512: String::new(),
        blake3: combined,
        truncated: false,
        entries: files.len() as u64,
        note: Some(
            "Directory digest is BLAKE3 over sorted relative paths + per-file BLAKE3".into(),
        ),
    })
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
    if depth > 64 {
        return Ok(());
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let ft = entry.file_type()?;
        let p = entry.path();
        if ft.is_dir() {
            collect_files(root, &p, out, depth + 1)?;
        } else if ft.is_file() {
            let rel = p.strip_prefix(root).unwrap_or(&p).to_path_buf();
            out.push(rel);
        }
    }
    Ok(())
}

struct Digests {
    md5: String,
    sha1: String,
    sha256: String,
    sha512: String,
    blake3: String,
}

fn hash_bytes_streaming(path: &Path, max_bytes: Option<u64>) -> Result<Digests> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut sha512 = Sha512::new();
    let mut blake = blake3::Hasher::new();
    let mut buf = [0u8; CHUNK];
    let mut remaining = max_bytes;
    loop {
        let to_read = match remaining {
            Some(0) => break,
            Some(n) => (n as usize).min(CHUNK),
            None => CHUNK,
        };
        let n = file.read(&mut buf[..to_read])?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        md5.update(chunk);
        sha1.update(chunk);
        sha256.update(chunk);
        sha512.update(chunk);
        blake.update(chunk);
        if let Some(r) = remaining.as_mut() {
            *r = r.saturating_sub(n as u64);
        }
    }
    Ok(Digests {
        md5: hex::encode(md5.finalize()),
        sha1: hex::encode(sha1.finalize()),
        sha256: hex::encode(sha256.finalize()),
        sha512: hex::encode(sha512.finalize()),
        blake3: blake.finalize().to_hex().to_string(),
    })
}

/// Content hash used by `--duplicates` (BLAKE3 of full file, respecting max_bytes).
pub fn file_blake3(path: &Path, max_bytes: Option<u64>) -> Result<String> {
    Ok(hash_bytes_streaming(path, max_bytes)?.blake3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;

    #[test]
    fn hashes_known_payload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.bin");
        std::fs::write(&path, b"abc").unwrap();
        let report = build_hash(&path, "x.bin", &OmnicatConfig::default()).unwrap();
        assert_eq!(report.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert!(!report.blake3.is_empty());
        assert!(!report.sha256.is_empty());
    }

    #[test]
    fn directory_digest_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"b").unwrap();
        let cfg = OmnicatConfig::default();
        let h1 = build_hash(dir.path(), "d", &cfg).unwrap().blake3;
        let h2 = build_hash(dir.path(), "d", &cfg).unwrap().blake3;
        assert_eq!(h1, h2);
        assert!(h1.len() >= 32);
    }
}
