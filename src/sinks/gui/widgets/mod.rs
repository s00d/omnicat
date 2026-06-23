use std::path::Path;

use eframe::egui;
use egui_extras::syntax_highlighting::CodeTheme;

use crate::config::OmnicatConfig;
use crate::content::PreviewContent;

mod hex;
mod image;
mod slides;
mod source_editor;
mod table;
mod tree;

pub use hex::render_hex;
pub use image::render_image;
pub use slides::render_slides;
pub use source_editor::{
    code_theme_for_config, language_for_content, render_source_view, SourceEditorState,
};
pub use table::render_table;
pub use tree::render_tree;

pub fn render_content(
    ui: &mut egui::Ui,
    path: &Path,
    content: &PreviewContent,
    font_size: f32,
    config: &OmnicatConfig,
    editor: &mut SourceEditorState,
    code_theme: &CodeTheme,
) {
    match content {
        PreviewContent::Text(text) => {
            let lang = language_for_content(content, path);
            render_source_view(
                ui,
                text,
                &mut editor.buffer,
                &lang,
                font_size,
                code_theme,
                editor.syntect.as_ref(),
            );
        }
        PreviewContent::Markdown(source) => {
            render_source_view(
                ui,
                source,
                &mut editor.buffer,
                "Markdown",
                font_size,
                code_theme,
                editor.syntect.as_ref(),
            );
        }
        PreviewContent::HighlightedCode(code) => {
            let text = code.plain_text();
            render_source_view(
                ui,
                &text,
                &mut editor.buffer,
                &code.lang,
                font_size,
                code_theme,
                editor.syntect.as_ref(),
            );
        }
        PreviewContent::Table(table) => render_table(ui, table, config, font_size),
        PreviewContent::Tree(tree) => {
            ui.style_mut()
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::monospace(font_size));
            render_tree(ui, tree, config);
        }
        PreviewContent::Slides(slides) => {
            render_slides(ui, slides, font_size, code_theme, editor.syntect.as_ref());
        }
        PreviewContent::Image(img) => render_image(ui, img, config),
        PreviewContent::Hex(hex) => {
            ui.style_mut()
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::monospace(font_size));
            render_hex(ui, hex, font_size, config);
        }
        PreviewContent::MediaInfo(m) => {
            ui.heading(&m.title);
            ui.label(format!("format: {}", m.format));
            if let Some(d) = m.duration_secs {
                ui.label(format!("duration: {d:.1}s"));
            }
            if let Some(c) = &m.codec {
                ui.label(format!("codec: {c}"));
            }
            for (k, v) in &m.extra {
                ui.label(format!("{k}: {v}"));
            }
        }
        PreviewContent::FontInfo(f) => {
            ui.heading(&f.family);
            ui.label(format!("style: {}  weight: {}", f.style, f.weight));
            ui.label(format!("glyphs: {}", f.glyph_count));
            ui.separator();
            ui.label(egui::RichText::new(&f.sample).size(font_size * 1.5));
        }
        PreviewContent::Database(d) => {
            ui.label(egui::RichText::new(&d.schema).monospace().size(font_size));
            ui.separator();
            render_table(
                ui,
                &crate::content::TableContent {
                    title: Some(d.table_name.clone()),
                    headers: d.headers.clone(),
                    rows: d.rows.clone(),
                },
                config,
                font_size,
            );
        }
        PreviewContent::Unsupported { reason, suggestion } => {
            ui.colored_label(egui::Color32::YELLOW, reason);
            ui.label(suggestion);
        }
    }
}

pub fn uses_builtin_scroll(content: &PreviewContent) -> bool {
    matches!(
        content,
        PreviewContent::Text(_)
            | PreviewContent::Markdown(_)
            | PreviewContent::HighlightedCode(_)
            | PreviewContent::Table(_)
    )
}
