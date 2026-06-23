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
        let parts: Vec<&str> = entry_path
            .trim_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        if parts.is_empty() {
            return;
        }
        Self::insert_parts(self, &parts, is_dir, size, mode);
    }

    fn insert_parts(
        node: &mut TreeNode,
        parts: &[&str],
        is_dir: bool,
        size: Option<u64>,
        mode: Option<String>,
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
            } else {
                node.children.push(TreeNode {
                    name: head.to_string(),
                    path: child_path,
                    is_dir,
                    size,
                    mode,
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
            Self::insert_parts(child, rest, is_dir, size, mode);
        }
    }

    pub fn sort_recursive(&mut self) {
        self.children.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });
        for c in &mut self.children {
            c.sort_recursive();
        }
    }

    pub fn count_nodes(&self) -> usize {
        1 + self.children.iter().map(|c| c.count_nodes()).sum::<usize>()
    }
}

pub fn build_tree_from_entries(
    title: Option<String>,
    entries: impl IntoIterator<Item = (String, bool, Option<u64>, Option<String>)>,
    max_entries: usize,
) -> FileTree {
    let mut root = TreeNode::new_dir(".", "");
    for (count, (path, is_dir, size, mode)) in entries.into_iter().enumerate() {
        if count >= max_entries {
            break;
        }
        root.insert_entry(&path, is_dir, size, mode);
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
    if !node.name.is_empty() || !node.path.is_empty() {
        let branch = if is_last { "└── " } else { "├── " };
        let icon = if icons {
            if node.is_dir { "📁 " } else { "📄 " }
        } else {
            ""
        };
        let size = node
            .size
            .map(|s| format!(" ({s} B)"))
            .unwrap_or_default();
        let mode = node
            .mode
            .as_ref()
            .map(|m| format!(" {m}"))
            .unwrap_or_default();
        if !node.path.is_empty() || node.name != "." {
            out.push_str(prefix);
            out.push_str(branch);
            out.push_str(icon);
            out.push_str(&node.name);
            out.push_str(&size);
            out.push_str(&mode);
            out.push('\n');
        }
    }

    let child_prefix = if node.name.is_empty() && node.path.is_empty() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_paths_build_tree() {
        let mut root = TreeNode::new_dir(".", "");
        root.insert_entry("a/b/c.txt", false, Some(10), None);
        root.insert_entry("a/d.txt", false, Some(5), None);
        root.sort_recursive();
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].name, "a");
        assert_eq!(root.children[0].children.len(), 2);
    }
}
