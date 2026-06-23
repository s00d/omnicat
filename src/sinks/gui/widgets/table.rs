use eframe::egui;

use crate::config::OmnicatConfig;
use crate::content::TableContent;

pub fn render_table(
    ui: &mut egui::Ui,
    table: &TableContent,
    config: &OmnicatConfig,
    font_size: f32,
) {
    if let Some(title) = &table.title {
        ui.heading(title);
    }

    let col_count = column_count(table);
    if col_count == 0 {
        return;
    }

    let accent =
        parse_accent(&config.gui.theme.accent).unwrap_or_else(|| ui.visuals().hyperlink_color);
    let header_fg = accent;
    let first_col_fg = accent.gamma_multiply(0.88);
    let body_fg = ui.visuals().text_color();
    let has_header = config.gui.spreadsheet.header_row && !table.headers.is_empty();
    let grid_lines = config.gui.spreadsheet.grid_lines;
    let table_id = table
        .title
        .as_deref()
        .unwrap_or("omnicat-table");

    egui::ScrollArea::both()
        .id_salt(table_id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::Grid::new(table_id)
                .num_columns(col_count)
                .spacing([12.0, 6.0])
                .min_col_width(font_size * 4.0)
                .striped(false)
                .show(ui, |ui| {
                    if has_header {
                        for h in &table.headers {
                            cell_label(ui, h, header_fg, font_size, true);
                        }
                        ui.end_row();
                        if grid_lines {
                            grid_line(ui);
                        }
                    }

                    for row in &table.rows {
                        for col in 0..col_count {
                            let text = row.get(col).map(String::as_str).unwrap_or("");
                            let fg = if col == 0 { first_col_fg } else { body_fg };
                            cell_label(ui, text, fg, font_size, false);
                        }
                        ui.end_row();
                        if grid_lines {
                            grid_line(ui);
                        }
                    }
                });
        });
}

fn column_count(table: &TableContent) -> usize {
    table
        .headers
        .len()
        .max(table.rows.iter().map(|r| r.len()).max().unwrap_or(0))
}

fn cell_label(ui: &mut egui::Ui, text: &str, fg: egui::Color32, font_size: f32, bold: bool) {
    let rich = egui::RichText::new(text)
        .size(font_size)
        .monospace()
        .color(fg);
    if bold {
        ui.label(rich.strong());
    } else {
        ui.label(rich);
    }
}

fn grid_line(ui: &mut egui::Ui) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    let y = ui.cursor().top();
    let x_start = ui.max_rect().left();
    let x_end = ui.max_rect().right();
    ui.painter().hline(x_start..=x_end, y, stroke);
    ui.add_space(2.0);
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
