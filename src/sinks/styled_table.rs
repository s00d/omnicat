use std::io::Write;

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Attribute, Cell, Color, Table};

use crate::config::OmnicatConfig;

pub fn heading_color(config: &OmnicatConfig) -> Color {
    match config.terminal.markdown.heading_color.as_str() {
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "white" => Color::White,
        "cyan" => Color::Cyan,
        _ => Color::Cyan,
    }
}

pub fn style_header_cell(text: impl ToString, config: &OmnicatConfig) -> Cell {
    Cell::new(text)
        .fg(heading_color(config))
        .add_attribute(Attribute::Bold)
}

pub fn style_body_cell(text: impl ToString, col_index: usize, config: &OmnicatConfig) -> Cell {
    if col_index == 0 {
        Cell::new(text).fg(heading_color(config))
    } else {
        Cell::new(text).fg(Color::Reset)
    }
}

pub fn configure_table(table: &mut Table, bordered: bool) {
    table.enforce_styling();
    if bordered {
        table.load_preset(UTF8_FULL);
    }
}

pub fn set_styled_header(table: &mut Table, headers: Vec<String>, config: &OmnicatConfig) {
    table.set_header(
        headers
            .into_iter()
            .map(|h| style_header_cell(h, config))
            .collect::<Vec<_>>(),
    );
}

pub fn add_styled_row(table: &mut Table, row: Vec<String>, config: &OmnicatConfig) {
    table.add_row(
        row.into_iter()
            .enumerate()
            .map(|(col, c)| style_body_cell(c, col, config))
            .collect::<Vec<_>>(),
    );
}

pub fn write_styled_table(
    out: &mut dyn Write,
    title: Option<&str>,
    headers: &[String],
    rows: &[Vec<String>],
    config: &OmnicatConfig,
    bordered: bool,
) -> Result<()> {
    if let Some(title) = title {
        writeln!(out, "{title}")?;
    }
    let mut table = Table::new();
    configure_table(&mut table, bordered);
    if !headers.is_empty() {
        set_styled_header(&mut table, headers.to_vec(), config);
    }
    for row in rows {
        add_styled_row(&mut table, row.clone(), config);
    }
    writeln!(out, "{table}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styled_table_has_ansi_header() {
        let mut buf = Vec::new();
        let cfg = OmnicatConfig::default();
        write_styled_table(
            &mut buf,
            None,
            &["A".into(), "B".into()],
            &[vec!["1".into(), "2".into()]],
            &cfg,
            true,
        )
        .unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b["), "expected ANSI styling: {s}");
    }
}
