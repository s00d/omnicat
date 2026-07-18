mod widgets;

use std::path::{Path, PathBuf};

use anyhow::Result;
use eframe::egui;

use crate::config::OmnicatConfig;
use crate::content::PreviewContent;
use crate::sinks::gui::widgets::{code_theme_for_config, render_content, SourceEditorState};

pub fn run(path: &Path, config: &OmnicatConfig, content: &PreviewContent) -> Result<()> {
    let title = config.gui.window.title_template.replace(
        "{file}",
        &path.file_name().unwrap_or_default().to_string_lossy(),
    );

    let width = config.gui.window.width as f32;
    let height = config.gui.window.height as f32;
    let font_size = config.gui.theme.font_size;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_resizable(config.gui.window.resizable)
            .with_title(title),
        ..Default::default()
    };

    let config_for_theme = config.clone();
    let app = PreviewApp {
        path: path.to_path_buf(),
        content: content.clone(),
        editor: SourceEditorState::new(content),
        font_size,
        show_toolbar: config.gui.preview.show_toolbar,
        show_status: config.gui.preview.show_status_bar,
        config: config.clone(),
        status: format!(
            "{} — {}",
            path.display(),
            if path.is_dir() {
                "directory".to_string()
            } else {
                format!(
                    "{} bytes",
                    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
                )
            }
        ),
    };

    eframe::run_native(
        "omnicat preview",
        native_options,
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx, &config_for_theme);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

fn apply_theme(ctx: &egui::Context, config: &OmnicatConfig) {
    let mut visuals = if config.gui.theme.mode == "light" {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    if let Some(accent) = parse_accent(&config.gui.theme.accent) {
        visuals.hyperlink_color = accent;
        visuals.widgets.active.bg_fill = accent;
    }
    ctx.set_visuals(visuals);
}

fn parse_accent(hex: &str) -> Option<egui::Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

struct PreviewApp {
    path: PathBuf,
    content: PreviewContent,
    editor: SourceEditorState,
    font_size: f32,
    show_toolbar: bool,
    show_status: bool,
    config: OmnicatConfig,
    status: String,
}

impl eframe::App for PreviewApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if std::env::var("OMNICAT_PREVIEW_AUTO_CLOSE").ok().as_deref() == Some("1") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if self.show_toolbar {
            egui::Panel::top("toolbar").show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("omnicat preview");
                    ui.separator();
                    if ui
                        .add(egui::Slider::new(&mut self.font_size, 10.0..=28.0).text("font"))
                        .changed()
                    {
                        ctx.request_repaint();
                    }
                    if ui.button("Close").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        }

        if self.show_status {
            egui::Panel::bottom("status").show_inside(ui, |ui| {
                ui.label(&self.status);
            });
        }

        self.editor.sync_buffer(&self.content);
        let code_theme = code_theme_for_config(&ctx, ui.style(), self.font_size, &self.config);

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let size = ui.available_size();
            ui.allocate_ui(size, |ui| {
                if widgets::uses_builtin_scroll(&self.content) {
                    render_content(
                        ui,
                        &self.path,
                        &self.content,
                        self.font_size,
                        &self.config,
                        &mut self.editor,
                        &code_theme,
                    );
                } else {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            render_content(
                                ui,
                                &self.path,
                                &self.content,
                                self.font_size,
                                &self.config,
                                &mut self.editor,
                                &code_theme,
                            );
                        });
                }
            });
        });
    }
}
