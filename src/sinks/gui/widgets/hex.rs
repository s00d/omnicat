use eframe::egui;

use crate::config::OmnicatConfig;
use crate::content::HexContent;

pub fn render_hex(ui: &mut egui::Ui, hex: &HexContent, font_size: f32, config: &OmnicatConfig) {
    if !hex.metadata.is_empty() {
        ui.label(&hex.metadata);
    }
    let cols = config.gui.hex.bytes_per_row;
    let dump = crate::content::hex_preview(&hex.bytes, cols);
    ui.label(egui::RichText::new(dump).monospace().size(font_size));
}
