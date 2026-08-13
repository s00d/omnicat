use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{build_tree_from_entries, PreviewContent, PreviewContext, TreeEntry};
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
        let (max_depth, max_entries) = directory_limits(config);
        let show_hidden = config.terminal.directory.show_hidden;

        let mut entries: Vec<TreeEntry> = Vec::new();
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

        let title = Some(path.display().to_string());
        let tree = build_tree_from_entries(title, entries, max_entries);
        Ok(PreviewContent::Tree(tree))
    }
}

fn directory_limits(config: &OmnicatConfig) -> (usize, usize) {
    if config.inspect.no_limit || config.inspect.max_rows == 0 {
        return (32, usize::MAX);
    }
    // Terminal depth only.
    (
        config.terminal.directory.max_depth.max(1),
        config.terminal.directory.max_entries.max(1),
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_dir(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    show_hidden: bool,
    entries: &mut Vec<TreeEntry>,
    count: &mut usize,
) -> Result<()> {
    if *count >= max_entries || depth > max_depth {
        return Ok(());
    }

    let mut items: Vec<_> = fs::read_dir(current)?.filter_map(|e| e.ok()).collect();
    items.sort_by(|a, b| {
        let a_dir = a.metadata().map(|m| m.is_dir()).unwrap_or(false);
        let b_dir = b.metadata().map(|m| m.is_dir()).unwrap_or(false);
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

        // symlink_metadata: never follow links into other trees.
        let meta = fs::symlink_metadata(entry.path())?;
        let is_link = meta.file_type().is_symlink();
        let is_dir = meta.is_dir() && !is_link;

        let rel = entry
            .path()
            .strip_prefix(root)
            .unwrap_or(entry.path().as_path())
            .to_string_lossy()
            .replace('\\', "/");

        let leaf_name = if is_link {
            let target = fs::read_link(entry.path()).unwrap_or_default();
            format!("{name} -> {}", target.display())
        } else {
            name
        };

        let rel_path = {
            let parent = Path::new(&rel)
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .filter(|p| !p.is_empty());
            match parent {
                Some(p) => format!("{p}/{leaf_name}"),
                None => leaf_name,
            }
        };

        let size = if is_dir || is_link {
            None
        } else {
            Some(meta.len())
        };

        let hint = if is_dir && depth + 1 >= max_depth {
            Some(count_immediate_children(&entry.path(), show_hidden)?)
        } else {
            None
        };

        entries.push((rel_path, is_dir, size, None, hint));
        *count += 1;

        // max_depth = path segments under root to show (1 = flat listing).
        if is_dir && depth + 1 < max_depth {
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

fn count_immediate_children(path: &Path, show_hidden: bool) -> Result<usize> {
    let mut n = 0usize;
    for entry in fs::read_dir(path)?.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !show_hidden && name.starts_with('.') {
            continue;
        }
        n += 1;
    }
    Ok(n)
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

    #[test]
    fn default_depth_is_shallow() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.txt"), "x").unwrap();
        fs::write(dir.path().join("a/mid.txt"), "m").unwrap();
        fs::write(dir.path().join("root.txt"), "y").unwrap();

        let cfg = OmnicatConfig::default();
        assert_eq!(cfg.terminal.directory.max_depth, 1);
        let ctx = preview_context(dir.path());
        let content = DirectoryDriver.build(dir.path(), &cfg, &ctx).unwrap();
        let PreviewContent::Tree(tree) = content else {
            panic!("expected tree");
        };
        let mut out = String::new();
        crate::content::render_tree_unicode(&tree.root, "", true, false, &mut out);
        assert!(out.contains("root.txt"), "{out}");
        assert!(out.contains("a/"), "{out}");
        assert!(
            out.contains("a/") && out.contains("(2)"),
            "collapsed top-level dir should hint child count: {out}"
        );
        assert!(
            !out.contains("mid.txt"),
            "must not expand past depth 1: {out}"
        );
        assert!(
            !out.contains("deep.txt"),
            "must not expand past depth 1: {out}"
        );
        assert!(!out.contains("drwx"), "no fake modes: {out}");
        assert!(!out.starts_with(' '), "no leading indent: {out:?}");
    }

    #[test]
    fn all_lifts_depth_cap() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.txt"), "x").unwrap();

        let mut cfg = OmnicatConfig::default();
        cfg.inspect.no_limit = true;
        let ctx = preview_context(dir.path());
        let content = DirectoryDriver.build(dir.path(), &cfg, &ctx).unwrap();
        let PreviewContent::Tree(tree) = content else {
            panic!("expected tree");
        };
        let mut out = String::new();
        crate::content::render_tree_unicode(&tree.root, "", true, false, &mut out);
        assert!(out.contains("deep.txt"), "--all must expand: {out}");
    }
}
