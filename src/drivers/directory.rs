use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{build_tree_from_entries, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct DirectoryDriver;

impl PreviewDriver for DirectoryDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Directory
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[]
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
        let max_depth = config
            .terminal
            .directory
            .max_depth
            .min(config.gui.directory.max_depth);
        let max_entries = config.terminal.directory.max_entries;
        let show_hidden = config.terminal.directory.show_hidden;

        let mut entries = Vec::new();
        let mut count = 0usize;
        collect_dir(
            path,
            path,
            0,
            max_depth,
            max_entries,
            show_hidden,
            &mut entries,
            &mut count,
        )?;

        let title = Some(format!("Directory: {}", path.display()));
        let tree = build_tree_from_entries(title, entries, max_entries);
        Ok(PreviewContent::Tree(tree))
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_dir(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    show_hidden: bool,
    entries: &mut Vec<(String, bool, Option<u64>, Option<String>)>,
    count: &mut usize,
) -> Result<()> {
    if *count >= max_entries || depth > max_depth {
        return Ok(());
    }

    let mut items: Vec<_> = fs::read_dir(current)?
        .filter_map(|e| e.ok())
        .collect();
    items.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    for entry in items {
        if *count >= max_entries {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path().as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let meta = entry.metadata()?;
        let is_dir = meta.is_dir();
        let is_link = meta.file_type().is_symlink();
        let display_name = if is_link {
            let target = fs::read_link(entry.path()).unwrap_or_default();
            format!("{name} -> {}", target.display())
        } else {
            name.clone()
        };
        let rel_path = if rel.is_empty() { display_name.clone() } else { rel };
        let size = if is_dir { None } else { Some(meta.len()) };
        let mode = if is_dir {
            Some("drwxr-xr-x".into())
        } else if is_link {
            Some("lrwxrwxrwx".into())
        } else {
            Some("-rw-r--r--".into())
        };
        entries.push((rel_path, is_dir, size, mode));
        *count += 1;

        if is_dir && !is_link && depth < max_depth {
            collect_dir(
                root,
                &entry.path(),
                depth + 1,
                max_depth,
                max_entries,
                show_hidden,
                entries,
                count,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::preview_context;

    #[test]
    fn directory_tree_has_children() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "x").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/b.txt"), "y").unwrap();
        let cfg = OmnicatConfig::default();
        let ctx = preview_context(dir.path());
        let content = DirectoryDriver.build(dir.path(), &cfg, &ctx).unwrap();
        assert!(matches!(content, PreviewContent::Tree(_)));
    }
}
