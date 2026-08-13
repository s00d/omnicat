//! Directory / archive tree model and unicode rendering.

#[derive(Debug, Clone, Default)]
pub struct FileTree {
    pub title: Option<String>,
    pub root: TreeNode,
}

#[derive(Debug, Clone, Default)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub mode: Option<String>,
    /// When a directory was not expanded, approximate immediate child count.
    pub entries_hint: Option<usize>,
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new_dir(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            is_dir: true,
            size: None,
            mode: None,
            entries_hint: None,
            children: Vec::new(),
        }
    }

    pub fn insert_entry(
        &mut self,
        entry_path: &str,
        is_dir: bool,
        size: Option<u64>,
        mode: Option<String>,
    ) {
        self.insert_entry_hint(entry_path, is_dir, size, mode, None);
    }

    pub fn insert_entry_hint(
        &mut self,
        entry_path: &str,
        is_dir: bool,
        size: Option<u64>,
        mode: Option<String>,
        entries_hint: Option<usize>,
    ) {
        let parts: Vec<&str> = entry_path
            .trim_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            return;
        }
        Self::insert_parts(self, &parts, is_dir, size, mode, entries_hint);
    }

    fn insert_parts(
        node: &mut TreeNode,
        parts: &[&str],
        is_dir: bool,
        size: Option<u64>,
        mode: Option<String>,
        entries_hint: Option<usize>,
    ) {
        let head = parts[0];
        let rest = &parts[1..];
        let child_path = if node.path.is_empty() {
            head.to_string()
        } else {
            format!("{}/{}", node.path, head)
        };

        let idx = node.children.iter().position(|c| c.name == head);
        if rest.is_empty() {
            if let Some(i) = idx {
                let c = &mut node.children[i];
                c.is_dir = is_dir;
                c.size = size.or(c.size);
                c.mode = mode.or(c.mode.take());
                c.entries_hint = entries_hint.or(c.entries_hint);
            } else {
                node.children.push(TreeNode {
                    name: head.to_string(),
                    path: child_path,
                    is_dir,
                    size,
                    mode,
                    entries_hint,
                    children: Vec::new(),
                });
            }
        } else {
            let child = if let Some(i) = idx {
                &mut node.children[i]
            } else {
                node.children.push(TreeNode::new_dir(head, &child_path));
                node.children.last_mut().unwrap()
            };
            child.is_dir = true;
            Self::insert_parts(child, rest, is_dir, size, mode, entries_hint);
        }
    }

    pub fn sort_recursive(&mut self) {
        self.children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        for c in &mut self.children {
            c.sort_recursive();
        }
    }

    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.count_nodes()).sum::<usize>()
    }
}

/// Entry: relative path, is_dir, size, mode, optional collapsed child-count hint.
pub type TreeEntry = (String, bool, Option<u64>, Option<String>, Option<usize>);

pub fn build_tree_from_entries(
    title: Option<String>,
    entries: impl IntoIterator<Item = TreeEntry>,
    max_entries: usize,
) -> FileTree {
    // Empty name/path = invisible root (no leading indent).
    let mut root = TreeNode::new_dir("", "");
    for (count, (path, is_dir, size, mode, hint)) in entries.into_iter().enumerate() {
        if count >= max_entries {
            break;
        }
        root.insert_entry_hint(&path, is_dir, size, mode, hint);
    }
    root.sort_recursive();
    FileTree { title, root }
}

pub fn render_tree_unicode(
    node: &TreeNode,
    prefix: &str,
    is_last: bool,
    icons: bool,
    out: &mut String,
) {
    let is_root = node.name.is_empty() && node.path.is_empty();
    if !is_root {
        let branch = if is_last { "└── " } else { "├── " };
        let icon = if icons {
            if node.is_dir {
                "📁 "
            } else {
                "📄 "
            }
        } else {
            ""
        };
        out.push_str(prefix);
        out.push_str(branch);
        out.push_str(icon);
        out.push_str(&format_node_label(node));
        out.push('\n');
    }

    let child_prefix = if is_root {
        prefix.to_string()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    let n = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        render_tree_unicode(child, &child_prefix, i + 1 == n, icons, out);
    }
}

fn format_node_label(node: &TreeNode) -> String {
    if node.is_dir {
        let mut s = format!("{}/", node.name);
        if node.children.is_empty() {
            if let Some(n) = node.entries_hint {
                s.push_str(&format!("  ({n})"));
            }
        }
        s
    } else {
        let size = node
            .size
            .map(|b| format!("  {}", human_bytes(b)))
            .unwrap_or_default();
        // Symlink targets are already baked into `name` as `name -> target`.
        let mode = node
            .mode
            .as_ref()
            .filter(|m| !m.starts_with('d') && !m.starts_with('-'))
            .map(|m| format!("  {m}"))
            .unwrap_or_default();
        format!("{}{size}{mode}", node.name)
    }
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_paths_build_tree() {
        let mut root = TreeNode::new_dir("", "");
        root.insert_entry("a/b/c.txt", false, Some(10), None);
        root.insert_entry("a/d.txt", false, Some(5), None);
        root.sort_recursive();
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "a");
        assert_eq!(root.children[0].children.len(), 2);
    }

    #[test]
    fn root_has_no_leading_indent() {
        let tree = build_tree_from_entries(
            None,
            vec![
                ("a.txt".into(), false, Some(10), None, None),
                ("sub".into(), true, None, None, Some(3)),
            ],
            100,
        );
        let mut out = String::new();
        render_tree_unicode(&tree.root, "", true, false, &mut out);
        assert!(
            out.starts_with("├── ") || out.starts_with("└── "),
            "unexpected leading indent: {out:?}"
        );
        assert!(out.contains("sub/  (3)"));
        assert!(out.contains("a.txt  10 B"));
        assert!(!out.contains("drwx"));
        assert!(!out.contains('📁'));
    }
}
