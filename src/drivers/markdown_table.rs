use std::io::Write;

use anyhow::Result;
use comfy_table::{CellAlignment, ContentArrangement, Table};

use pulldown_cmark::Alignment;

use crate::config::OmnicatConfig;
use crate::sinks::styled_table;

/// Render a markdown table sized to the terminal width with in-cell wrapping and row borders.
pub fn render_table(
    out: &mut dyn Write,
    header: Option<Vec<String>>,
    rows: Vec<Vec<String>>,
    alignments: &[Alignment],
    config: &OmnicatConfig,
) -> Result<()> {
    if header.is_none() && rows.is_empty() {
        return Ok(());
    }

    let col_count = column_count(header.as_ref(), &rows);
    if col_count == 0 {
        return Ok(());
    }

    let max_rows = config.terminal.data.max_rows.max(1);
    let term_width = terminal_width(config);
    let total_rows = rows.len();
    let show_rows = rows.len().min(max_rows);
    let truncated_rows = &rows[..show_rows];

    let mut table = Table::new();
    styled_table::configure_table(&mut table, true);
    table
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(term_width.min(u16::MAX as usize) as u16);

    if let Some(headers) = header {
        styled_table::set_styled_header(
            &mut table,
            headers.iter().map(|h| normalize_cell(h)).collect(),
            config,
        );
    }

    for row in truncated_rows {
        let cells: Vec<String> = (0..col_count)
            .map(|col| normalize_cell(row.get(col).map(String::as_str).unwrap_or("")))
            .collect();
        styled_table::add_styled_row(&mut table, cells, config);
    }

    for (i, alignment) in alignments.iter().enumerate() {
        if let Some(column) = table.column_mut(i) {
            column.set_cell_alignment(to_cell_alignment(*alignment));
        }
    }

    writeln!(out, "{}", table)?;

    if total_rows > show_rows {
        if config.terminal.plain {
            writeln!(
                out,
                "… {} more row(s) (terminal.data.max_rows = {max_rows})",
                total_rows - show_rows
            )?;
        } else {
            writeln!(
                out,
                "\x1b[90m… {} more row(s) (terminal.data.max_rows = {max_rows})\x1b[0m",
                total_rows - show_rows
            )?;
        }
    }

    writeln!(out)?;
    Ok(())
}

fn column_count(header: Option<&Vec<String>>, rows: &[Vec<String>]) -> usize {
    header
        .map(|h| h.len())
        .or_else(|| rows.first().map(|r| r.len()))
        .unwrap_or(0)
}

fn terminal_width(config: &OmnicatConfig) -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or_else(|_| {
            if config.terminal.markdown.wrap_width > 0 {
                config.terminal.markdown.wrap_width as usize
            } else {
                80
            }
        })
        .max(40)
}

fn normalize_cell(s: &str) -> String {
    s.replace('\n', " ")
}

fn to_cell_alignment(alignment: Alignment) -> CellAlignment {
    match alignment {
        Alignment::Left => CellAlignment::Left,
        Alignment::Center => CellAlignment::Center,
        Alignment::Right => CellAlignment::Right,
        Alignment::None => CellAlignment::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_table_stays_table_not_cards() {
        let mut buf = Vec::new();
        let header = vec![
            "Driver".into(),
            "Extensions".into(),
            "Rust crate".into(),
            "Recommended external".into(),
        ];
        let rows = vec![vec![
            "markdown".into(),
            "md, markdown".into(),
            "pulldown-cmark".into(),
            "glow, mdcat".into(),
        ]];
        let mut config = OmnicatConfig::default();
        config.terminal.markdown.wrap_width = 80;
        render_table(&mut buf, Some(header), rows, &[], &config).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(!s.contains("── row"));
        assert!(!s.contains("Driver:"));
        assert!(s.contains("Driver") || s.contains('│'));
    }

    #[test]
    fn table_has_horizontal_row_separators() {
        let mut buf = Vec::new();
        let header = vec!["A".into(), "B".into()];
        let rows = vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]];
        let mut config = OmnicatConfig::default();
        config.terminal.markdown.wrap_width = 80;
        render_table(&mut buf, Some(header), rows, &[], &config).unwrap();
        let s = String::from_utf8(buf).unwrap();
        // UTF8_FULL draws a dashed rule between body rows (├…┤)
        assert!(
            s.contains('├') || s.contains('╌') || s.contains('┼'),
            "expected row separators: {s}"
        );
    }

    #[test]
    fn truncates_row_count() {
        let mut buf = Vec::new();
        let header = vec!["A".into(), "B".into()];
        let rows: Vec<Vec<String>> = (0..50).map(|i| vec![format!("r{i}"), "x".into()]).collect();
        let mut config = OmnicatConfig::default();
        config.terminal.data.max_rows = 5;
        render_table(&mut buf, Some(header), rows, &[], &config).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("more row"));
    }
}
