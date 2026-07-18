use std::path::Path;
use std::sync::Arc;

use eframe::egui;
use egui_extras::syntax_highlighting::{highlight_with, CodeTheme, SyntectSettings};

use crate::config::OmnicatConfig;
use crate::content::PreviewContent;

pub struct SourceEditorState {
    pub buffer: String,
    pub syntect: Arc<SyntectSettings>,
}

impl SourceEditorState {
    pub fn new(content: &PreviewContent) -> Self {
        Self {
            buffer: content.plain_text(),
            syntect: Arc::new(SyntectSettings::default()),
        }
    }

    pub fn sync_buffer(&mut self, content: &PreviewContent) {
        let text = content.plain_text();
        if self.buffer != text {
            self.buffer = text;
        }
    }
}

pub fn language_for_content(content: &PreviewContent, path: &Path) -> String {
    match content {
        PreviewContent::Markdown(_) => "Markdown".to_string(),
        PreviewContent::HighlightedCode(code) => code.lang.clone(),
        PreviewContent::Text(_) => path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Plain Text".to_string()),
        _ => "Plain Text".to_string(),
    }
}

pub fn code_theme_for_config(
    ctx: &egui::Context,
    style: &egui::Style,
    font_size: f32,
    _config: &OmnicatConfig,
) -> CodeTheme {
    CodeTheme::from_memory(ctx, style).with_font_size(font_size)
}

pub fn render_source_view(
    ui: &mut egui::Ui,
    source: &str,
    buffer: &mut String,
    language: &str,
    font_size: f32,
    code_theme: &CodeTheme,
    syntect: &SyntectSettings,
) {
    if buffer.as_str() != source {
        *buffer = source.to_string();
    }

    let bg = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(30, 30, 30)
    } else {
        egui::Color32::from_rgb(250, 250, 250)
    };

    egui::Frame::new()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.style_mut().override_font_id = Some(egui::FontId::monospace(font_size));

            let theme = code_theme.clone();
            let lang = language.to_string();

            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    let mut job =
                        highlight_with(ui.ctx(), ui.style(), &theme, source, &lang, syntect);
                    job.wrap.max_width = f32::INFINITY;
                    ui.add(egui::Label::new(job).selectable(true));
                });
        });
}
