use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;
use zip::ZipArchive;

/// Resolved reference to a file or a virtual path inside an archive.
#[derive(Debug)]
pub struct InputRef {
    /// On-disk root (archive or plain file/dir).
    pub root: PathBuf,
    /// Path inside the container, if any (`archive.zip/inner.md` → `inner.md`).
    pub virtual_path: Option<String>,
    /// Materialized path used for detection/preview (root, or temp extract).
    pub resolved: PathBuf,
    /// Temp directory holding extracted content (cleaned on drop).
    _temp: Option<tempfile::TempDir>,
}

impl InputRef {
    /// Parse a user path that may contain a virtual archive entry.
    pub fn parse(raw: &str) -> Result<Self> {
        let path = PathBuf::from(raw);
        if path.exists() {
            return Ok(Self {
                root: path.clone(),
                virtual_path: None,
                resolved: path,
                _temp: None,
            });
        }

        let mut candidates = Vec::new();
        let mut cur = path.as_path();
        while let Some(parent) = cur.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            candidates.push(parent.to_path_buf());
            cur = parent;
        }

        for root in candidates {
            if root.is_file() {
                let virt = path
                    .strip_prefix(&root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .filter(|s| !s.is_empty());
                if let Some(ref entry) = virt {
                    let (resolved, temp) = extract_entry(&root, entry)?;
                    return Ok(Self {
                        root,
                        virtual_path: virt,
                        resolved,
                        _temp: temp,
                    });
                }
            }
        }

        bail!("path not found: {raw}")
    }

    pub fn display_name(&self) -> String {
        match &self.virtual_path {
            Some(v) => format!("{}/{}", self.root.display(), v),
            None => self.root.display().to_string(),
        }
    }

    pub fn path_for_ops(&self) -> &Path {
        &self.resolved
    }
}

fn extract_entry(archive: &Path, entry: &str) -> Result<(PathBuf, Option<tempfile::TempDir>)> {
    let ext = archive
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = archive
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let entry = entry.trim_start_matches('/');

    if matches!(
        ext.as_str(),
        "zip" | "jar" | "war" | "ear" | "apk" | "ipa" | "xpi" | "whl" | "nupkg" | "epub" | "cbz"
    ) {
        return extract_zip(archive, entry);
    }

    if name.ends_with(".tar.gz") || ext == "tgz" {
        return extract_tar(archive, entry, TarCompression::Gzip);
    }
    if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
        return extract_tar(archive, entry, TarCompression::Zstd);
    }
    if ext == "tar" {
        return extract_tar(archive, entry, TarCompression::None);
    }

    bail!("virtual paths not supported for {}", archive.display())
}

#[derive(Clone, Copy)]
enum TarCompression {
    None,
    Gzip,
    Zstd,
}

fn tar_reader(file: fs::File, compression: TarCompression) -> Result<Box<dyn std::io::Read>> {
    Ok(match compression {
        TarCompression::None => Box::new(file),
        TarCompression::Gzip => Box::new(GzDecoder::new(file)),
        TarCompression::Zstd => Box::new(zstd::Decoder::new(file).context("zstd decoder")?),
    })
}

fn extract_tar(
    archive: &Path,
    entry: &str,
    compression: TarCompression,
) -> Result<(PathBuf, Option<tempfile::TempDir>)> {
    let temp = tempfile::tempdir()?;
    let prefix = entry.trim_end_matches('/');
    let as_dir = entry.ends_with('/');

    let is_prefix_dir = {
        let file = fs::File::open(archive)?;
        tar_has_prefix(file, compression, prefix)?
    };

    let file = fs::File::open(archive)?;
    let resolved = if as_dir || is_prefix_dir || prefix.is_empty() {
        let out_dir = temp.path().join("tree");
        fs::create_dir_all(&out_dir)?;
        unpack_tar_prefix(file, compression, prefix, &out_dir)?;
        out_dir
    } else {
        let file_name = Path::new(entry)
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("extracted"));
        let dest = temp.path().join(file_name);
        unpack_tar_file(file, compression, entry, prefix, &dest)?;
        dest
    };

    Ok((resolved, Some(temp)))
}

fn tar_has_prefix(file: fs::File, compression: TarCompression, prefix: &str) -> Result<bool> {
    if prefix.is_empty() {
        return Ok(true);
    }
    let needle = format!("{prefix}/");
    let reader = tar_reader(file, compression)?;
    let mut archive = Archive::new(reader);
    for ent in archive.entries()? {
        let ent = ent?;
        let p = ent.path()?.to_string_lossy().replace('\\', "/");
        if p.starts_with(&needle) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn unpack_tar_prefix(
    file: fs::File,
    compression: TarCompression,
    prefix: &str,
    out_dir: &Path,
) -> Result<()> {
    let mut found = false;
    let reader = tar_reader(file, compression)?;
    let mut archive = Archive::new(reader);
    for ent in archive.entries()? {
        let _ = unpack_one_tar_entry(ent?, prefix, out_dir, &mut found)?;
    }
    if !found && !prefix.is_empty() {
        bail!("entry not found in archive: {prefix}");
    }
    Ok(())
}

fn unpack_one_tar_entry<R: std::io::Read>(
    mut ent: tar::Entry<'_, R>,
    prefix: &str,
    out_dir: &Path,
    found: &mut bool,
) -> Result<bool> {
    let p = ent.path()?.to_string_lossy().replace('\\', "/");
    let matches = if prefix.is_empty() {
        !p.is_empty()
    } else {
        p == prefix || p.starts_with(&format!("{prefix}/"))
    };
    if !matches {
        return Ok(false);
    }
    *found = true;
    let rel = if prefix.is_empty() {
        p.clone()
    } else {
        p.trim_start_matches(prefix)
            .trim_start_matches('/')
            .to_string()
    };
    if rel.is_empty() {
        return Ok(true);
    }
    let dest = out_dir.join(&rel);
    if ent.header().entry_type().is_dir() {
        fs::create_dir_all(&dest)?;
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        ent.unpack(&dest)?;
    }
    Ok(true)
}

fn unpack_tar_file(
    file: fs::File,
    compression: TarCompression,
    entry: &str,
    prefix: &str,
    dest: &Path,
) -> Result<()> {
    let mut found = false;
    let reader = tar_reader(file, compression)?;
    let mut archive = Archive::new(reader);
    for ent in archive.entries()? {
        if try_unpack_tar_file(ent?, entry, prefix, dest, &mut found)? {
            break;
        }
    }
    if !found {
        bail!("entry not found in archive: {entry}");
    }
    Ok(())
}

fn try_unpack_tar_file<R: std::io::Read>(
    mut ent: tar::Entry<'_, R>,
    entry: &str,
    prefix: &str,
    dest: &Path,
    found: &mut bool,
) -> Result<bool> {
    let p = ent.path()?.to_string_lossy().replace('\\', "/");
    if p == entry || p == prefix {
        if ent.header().entry_type().is_dir() {
            bail!("entry is a directory; append / to browse: {entry}/");
        }
        ent.unpack(dest)?;
        *found = true;
        return Ok(true);
    }
    Ok(false)
}

fn extract_zip(archive: &Path, entry: &str) -> Result<(PathBuf, Option<tempfile::TempDir>)> {
    let file = fs::File::open(archive).context("open archive")?;
    let mut zip = ZipArchive::new(file).context("invalid zip")?;
    let temp = tempfile::tempdir().context("temp dir")?;
    let prefix = entry.trim_end_matches('/');
    let as_dir = entry.ends_with('/') || zip_has_prefix(&mut zip, prefix);

    if as_dir {
        let out_dir = temp.path().join("tree");
        fs::create_dir_all(&out_dir)?;
        let mut found = false;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let name = zf.name().replace('\\', "/");
            let matches = if prefix.is_empty() {
                !name.is_empty()
            } else {
                name == prefix || name.starts_with(&format!("{prefix}/"))
            };
            if !matches {
                continue;
            }
            found = true;
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                name.trim_start_matches(prefix)
                    .trim_start_matches('/')
                    .to_string()
            };
            if rel.is_empty() {
                continue;
            }
            let dest = out_dir.join(&rel);
            if zf.is_dir() || name.ends_with('/') {
                fs::create_dir_all(&dest)?;
            } else {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out = fs::File::create(&dest)?;
                std::io::copy(&mut zf, &mut out)?;
            }
        }
        if !found && !prefix.is_empty() {
            bail!("entry not found in archive: {entry}");
        }
        return Ok((out_dir, Some(temp)));
    }

    let index = {
        let mut found = None;
        for i in 0..zip.len() {
            if let Ok(zf) = zip.by_index(i) {
                let name = zf.name();
                if name == entry || name == prefix {
                    found = Some(i);
                    break;
                }
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("entry not found: {entry}"))?
    };
    let mut zf = zip.by_index(index)?;
    let file_name = Path::new(entry)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("extracted"));
    let dest = temp.path().join(file_name);
    let mut out = fs::File::create(&dest)?;
    std::io::copy(&mut zf, &mut out)?;
    out.flush()?;
    Ok((dest, Some(temp)))
}

fn zip_has_prefix(zip: &mut ZipArchive<fs::File>, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let needle = format!("{prefix}/");
    for i in 0..zip.len() {
        if let Ok(zf) = zip.by_index(i) {
            let name = zf.name();
            if name.starts_with(&needle) || name == format!("{prefix}/") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn parse_plain_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "hi").unwrap();
        let input = InputRef::parse(path.to_str().unwrap()).unwrap();
        assert!(input.virtual_path.is_none());
        assert_eq!(input.resolved, path);
    }

    #[test]
    fn parse_zip_virtual() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("t.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            zip.start_file("inner/hello.txt", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"hello").unwrap();
            zip.finish().unwrap();
        }
        let raw = format!("{}/inner/hello.txt", zip_path.display());
        let input = InputRef::parse(&raw).unwrap();
        assert_eq!(input.virtual_path.as_deref(), Some("inner/hello.txt"));
        assert_eq!(fs::read_to_string(&input.resolved).unwrap(), "hello");
    }

    #[test]
    fn parse_tar_zst_virtual() {
        let dir = tempfile::tempdir().unwrap();
        let tar_zst = dir.path().join("b.tar.zst");
        {
            let tar_path = dir.path().join("b.tar");
            {
                let file = fs::File::create(&tar_path).unwrap();
                let mut builder = tar::Builder::new(file);
                let mut header = tar::Header::new_gnu();
                let data = b"zstd-virt";
                header.set_size(data.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder
                    .append_data(&mut header, "inner/hi.txt", &data[..])
                    .unwrap();
                builder.finish().unwrap();
            }
            let tar_bytes = fs::read(&tar_path).unwrap();
            let mut enc = zstd::Encoder::new(fs::File::create(&tar_zst).unwrap(), 1).unwrap();
            std::io::copy(&mut tar_bytes.as_slice(), &mut enc).unwrap();
            enc.finish().unwrap();
        }
        let raw = format!("{}/inner/hi.txt", tar_zst.display());
        let input = InputRef::parse(&raw).unwrap();
        assert_eq!(input.virtual_path.as_deref(), Some("inner/hi.txt"));
        assert_eq!(fs::read_to_string(&input.resolved).unwrap(), "zstd-virt");
    }
}
