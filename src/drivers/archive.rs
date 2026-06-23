use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::content::{build_tree_from_entries, FileTree, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

type ArchiveEntry = (String, bool, Option<u64>, Option<String>);

pub struct ArchiveDriver;

impl PreviewDriver for ArchiveDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Archive
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["zip", "tar", "tgz", "gz", "bz2", "xz", "7z"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let tree = build_archive_tree(path, config)?;
        Ok(PreviewContent::Tree(tree))
    }
}

fn build_archive_tree(path: &Path, config: &OmnicatConfig) -> Result<FileTree> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let max = config.terminal.archive.max_entries;
    let title = Some(format!("Archive: {}", path.display()));

    let entries: Vec<ArchiveEntry> = if ext == "zip" {
        collect_zip(path)?
    } else if ext == "7z" {
        collect_7z(path)?
    } else if name.ends_with(".tar.gz") || ext == "tgz" {
        collect_tar(path, true)?
    } else if ext == "tar" {
        collect_tar(path, false)?
    } else if ext == "gz" && !name.ends_with(".tar.gz") {
        vec![(
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string(),
            false,
            Some(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)),
            Some("-rw-r--r--".into()),
        )]
    } else if ext == "bz2" || ext == "xz" {
        vec![(
            path.display().to_string(),
            false,
            Some(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)),
            None,
        )]
    } else {
        collect_tar(path, false)?
    };

    Ok(build_tree_from_entries(title, entries.into_iter().take(max), max))
}

fn collect_zip(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid zip")?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).context("zip entry")?;
        let mode = if entry.is_dir() {
            "drwxr-xr-x"
        } else {
            "-rw-r--r--"
        };
        out.push((
            entry.name().to_string(),
            entry.is_dir(),
            Some(entry.size()),
            Some(mode.into()),
        ));
    }
    Ok(filter_noise_entries(out))
}

fn is_archive_noise(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.starts_with("._")
        || base == ".DS_Store"
        || path.starts_with("__MACOSX/")
        || path.contains("/__MACOSX/")
        || base == "PaxHeader"
        || path.starts_with("PaxHeader/")
        || path.contains("/PaxHeader/")
}

fn filter_noise_entries(entries: Vec<ArchiveEntry>) -> Vec<ArchiveEntry> {
    entries
        .into_iter()
        .filter(|(path, _, _, _)| !is_archive_noise(path))
        .collect()
}

fn collect_tar(path: &Path, gz: bool) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(path)?;
    let mut out = Vec::new();
    if gz {
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);
        for entry in archive.entries().context("tar entries")? {
            let entry = entry.context("tar entry")?;
            let p = entry.path().context("tar path")?;
            out.push((
                p.display().to_string(),
                entry.header().entry_type().is_dir(),
                Some(entry.header().size().unwrap_or(0)),
                Some(format_mode(entry.header().mode().unwrap_or(0))),
            ));
        }
    } else {
        let mut archive = Archive::new(file);
        for entry in archive.entries().context("tar entries")? {
            let entry = entry.context("tar entry")?;
            let p = entry.path().context("tar path")?;
            out.push((
                p.display().to_string(),
                entry.header().entry_type().is_dir(),
                Some(entry.header().size().unwrap_or(0)),
                Some(format_mode(entry.header().mode().unwrap_or(0))),
            ));
        }
    }
    Ok(filter_noise_entries(out))
}

fn collect_7z(path: &Path) -> Result<Vec<ArchiveEntry>> {
    use sevenz_rust::Archive;

    let archive = Archive::open(path).context("7z read failed")?;
    let mut out = Vec::new();
    for entry in archive.files {
        out.push((
            entry.name().to_string(),
            entry.is_directory(),
            Some(entry.size()),
            None,
        ));
    }
    Ok(out)
}

fn format_mode(mode: u32) -> String {
    let file_type = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut s = String::new();
    s.push(file_type);
    for bit in [
        0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001,
    ] {
        s.push(if mode & bit != 0 {
            match bit {
                0o400 | 0o040 | 0o004 => 'r',
                0o200 | 0o020 | 0o002 => 'w',
                _ => 'x',
            }
        } else {
            '-'
        });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn plain_tar_is_not_gzip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.tar");
        {
            let file = File::create(&path).unwrap();
            let mut builder = tar::Builder::new(file);
            let mut data = b"hello".as_slice();
            let mut header = tar::Header::new_gnu();
            header.set_path("inner.txt").unwrap();
            header.set_size(5);
            header.set_cksum();
            builder.append(&header, &mut data).unwrap();
            builder.finish().unwrap();
        }

        let cfg = OmnicatConfig::default();
        let tree = build_archive_tree(&path, &cfg).unwrap();
        assert!(tree.root.count_nodes() >= 2);
    }

    #[test]
    fn filters_macos_tar_noise() {
        assert!(is_archive_noise("._archive-inner.txt"));
        assert!(is_archive_noise("PaxHeader/archive-inner.txt"));
        assert!(!is_archive_noise("archive-inner.txt"));
    }

    #[test]
    fn zip_builds_nested_tree() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.zip");
        let file = File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        zip.start_file("a/b/c.txt", opts).unwrap();
        write!(zip, "hello").unwrap();
        zip.finish().unwrap();

        let cfg = OmnicatConfig::default();
        let tree = build_archive_tree(&path, &cfg).unwrap();
        assert!(tree.root.count_nodes() >= 2);
    }
}
