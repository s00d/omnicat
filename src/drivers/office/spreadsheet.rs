use std::path::Path;

use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, Reader};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext, TableContent};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct SpreadsheetDriver;

impl PreviewDriver for SpreadsheetDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Spreadsheet
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xlsx", "xls", "xlsm", "xlsb", "ods"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let mut workbook = open_workbook_auto(path).context("failed to open spreadsheet")?;
        let sheet_names = workbook.sheet_names().to_vec();
        let sheet_name = sheet_names
            .first()
            .cloned()
            .unwrap_or_else(|| "Sheet1".into());
        let range = workbook
            .worksheet_range(&sheet_name)
            .context("failed to read sheet")?;

        let max_rows = config
            .gui
            .spreadsheet
            .max_rows
            .min(config.terminal.data.max_rows);
        let max_cols = config.gui.spreadsheet.max_cols;

        let mut headers = Vec::new();
        let mut rows = Vec::new();

        for (i, row) in range.rows().enumerate() {
            if i >= max_rows {
                break;
            }
            let cells: Vec<String> = row.iter().take(max_cols).map(cell_to_string).collect();
            if i == 0 && config.gui.spreadsheet.header_row {
                headers = cells;
            } else {
                rows.push(cells);
            }
        }

        Ok(PreviewContent::Table(TableContent {
            title: Some(format!("Sheet: {sheet_name}")),
            headers,
            rows,
        }))
    }
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => d.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("{e:?}"),
    }
}
