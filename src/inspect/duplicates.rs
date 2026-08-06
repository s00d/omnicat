//! Find duplicate files under a directory (size bucket → content hash).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config::OmnicatConfig;
use crate::inspect::hash::file_blake3;
use crate::inspect::report::{human_size, DuplicateGroup, DuplicatesReport};

pub fn build_duplicates(
    path: &Path,
    display: &str,
    config: &OmnicatConfig,
) -> Result<DuplicatesReport> {
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if !meta.is_dir() {
        bail!("--duplicates expects a directory, got {}", path.display());
    }

    let max_file = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes as u64)
    };

    let mut by_size: BTreeMap<u64, Vec<PathBuf>> = BTreeMap::new();
    let mut scanned = 0u64;
    let mut skipped_large = 0u64;
    collect_files(
        path,
        &mut by_size,
        &mut scanned,
        &mut skipped_large,
        max_file,
        0,
    )?;

    // Only sizes with 2+ files can be duplicates.
    let mut hash_groups: BTreeMap<String, Vec<(PathBuf, u64)>> = BTreeMap::new();
    for (size, paths) in by_size {
        if paths.len() < 2 || size == 0 {
            continue;
        }
        for p in paths {
            let digest = match file_blake3(&p, None) {
                Ok(h) => h,
                Err(_) => continue,
            };
            hash_groups.entry(digest).or_default().push((p, size));
        }
    }

    let mut groups = Vec::new();
    let mut reclaimable = 0u64;
    for (hash, mut files) in hash_groups {
        if files.len() < 2 {
            continue;
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let size = files[0].1;
        reclaimable = reclaimable.saturating_add(size.saturating_mul((files.len() - 1) as u64));
        groups.push(DuplicateGroup {
            hash,
            size,
            size_human: human_size(size),
            files: files
                .into_iter()
                .map(|(p, _)| p.to_string_lossy().into_owned())
                .collect(),
        });
    }
    groups.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.hash.cmp(&b.hash)));

    let mut note = None;
    if skipped_large > 0 {
        note = Some(format!(
            "Skipped {skipped_large} file(s) larger than max_bytes. Use --all to include them."
        ));
    }

    Ok(DuplicatesReport {
        path: display.to_string(),
        scanned_files: scanned,
        groups,
        reclaimable_bytes: reclaimable,
        reclaimable_human: human_size(reclaimable),
        note,
    })
}

fn collect_files(
    dir: &Path,
    by_size: &mut BTreeMap<u64, Vec<PathBuf>>,
    scanned: &mut u64,
    skipped_large: &mut u64,
    max_file: Option<u64>,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let p = entry.path();
        if ft.is_dir() {
            collect_files(&p, by_size, scanned, skipped_large, max_file, depth + 1)?;
        } else if ft.is_file() {
            *scanned += 1;
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if let Some(max) = max_file {
                if size > max {
                    *skipped_large += 1;
                    continue;
                }
            }
            by_size.entry(size).or_default().push(p);
        }
    }
    Ok(())
}
