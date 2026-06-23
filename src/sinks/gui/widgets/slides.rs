use eframe::egui;
use egui_extras::syntax_highlighting::{CodeTheme, SyntectSettings};

use crate::sinks::gui::widgets::render_source_view;

pub fn render_slides(
    ui: &mut egui::Ui,
    slides: &[String],
    font_size: f32,
    code_theme: &CodeTheme,
    syntect: &SyntectSettings,
) {
    for (i, slide) in slides.iter().enumerate() {
        ui.heading(format!("Cell {}", i + 1));
        let mut buffer = slide.clone();
        render_source_view(
            ui,
            slide,
            &mut buffer,
            "Markdown",
            font_size,
            code_theme,
            syntect,
        );
        ui.separator();
    }
}
