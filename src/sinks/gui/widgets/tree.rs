use eframe::egui;

use crate::config::OmnicatConfig;
use crate::content::FileTree;

pub fn render_tree(ui: &mut egui::Ui, tree: &FileTree, config: &OmnicatConfig) {
    if let Some(title) = &tree.title {
        ui.heading(title);
    }
    render_node(ui, &tree.root, config, 0);
}

fn render_node(
    ui: &mut egui::Ui,
    node: &crate::content::TreeNode,
    config: &OmnicatConfig,
    depth: usize,
) {
    if depth > config.gui.directory.max_depth {
        return;
    }

    if node.name.is_empty() && node.path.is_empty() {
        for child in &node.children {
            render_node(ui, child, config, depth);
        }
        return;
    }

    let label = if node.is_dir {
        format!("📁 {}", node.name)
    } else {
        let size = node.size.map(|s| format!(" ({s} B)")).unwrap_or_default();
        format!("📄 {}{size}", node.name)
    };

    if node.is_dir && !node.children.is_empty() {
        egui::CollapsingHeader::new(label)
            .default_open(depth < 2)
            .show(ui, |ui| {
                for child in &node.children {
                    render_node(ui, child, config, depth + 1);
                }
            });
    } else {
        ui.label(label);
    }
}
