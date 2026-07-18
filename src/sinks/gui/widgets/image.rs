use eframe::egui;

use crate::config::OmnicatConfig;
use crate::content::ImageContent;

pub fn render_image(ui: &mut egui::Ui, img: &ImageContent, config: &OmnicatConfig) {
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [img.width as usize, img.height as usize],
        &img.rgba,
    );
    let tex = ui
        .ctx()
        .load_texture("preview-image", color_image, egui::TextureOptions::LINEAR);

    let size = match config.gui.image.fit.as_str() {
        "original" => egui::Vec2::new(img.width as f32, img.height as f32),
        "cover" => ui.available_size(),
        _ => {
            let max = ui.available_size();
            let scale = (max.x / img.width as f32)
                .min(max.y / img.height as f32)
                .min(1.0);
            egui::Vec2::new(img.width as f32 * scale, img.height as f32 * scale)
        }
    };
    ui.image((tex.id(), size));
}
