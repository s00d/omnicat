use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use comfy_table::{presets::UTF8_FULL, Table};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext, TableContent};
use crate::detect::HandlerKind;
use crate::drivers::theme::resolve_theme;
use crate::drivers::{capture_render, PreviewDriver};

pub struct DataDriver;

impl DataDriver {
    pub fn render(&self, path: &Path, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let raw = fs::read_to_string(path)?;

        let formatted = match ext.as_str() {
            "json" if config.terminal.data.pretty => pretty_json(&raw)?,
            "yaml" | "yml" if config.terminal.data.pretty => pretty_yaml(&raw)?,
            "toml" if config.terminal.data.pretty => pretty_toml(&raw)?,
            "ini" if config.terminal.data.pretty => pretty_ini(&raw)?,
            "csv" => render_csv(&raw, config)?,
            "tsv" => render_tsv(&raw, config)?,
            _ => raw,
        };

        highlight_data(&formatted, &ext, config, out)
    }
}

impl PreviewDriver for DataDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Data
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "csv", "tsv", "json", "yaml", "yml", "toml", "ini", "parquet", "feather", "msgpack",
        ]
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
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "parquet" => build_parquet_table(path, config),
            "feather" => build_feather_table(path, config),
            "msgpack" => build_msgpack_text(path),
            "csv" => {
                let raw = fs::read_to_string(path)?;
                Ok(PreviewContent::Table(parse_delimited(&raw, b',', config)?))
            }
            "tsv" => {
                let raw = fs::read_to_string(path)?;
                Ok(PreviewContent::Table(parse_delimited(&raw, b'\t', config)?))
            }
            _ => {
                let text = capture_render(|buf| self.render(path, config, buf))?;
                Ok(PreviewContent::Text(text))
            }
        }
    }
}

fn pretty_json(raw: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(raw).context("invalid json")?;
    Ok(serde_json::to_string_pretty(&value)?)
}

fn pretty_yaml(raw: &str) -> Result<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(raw).context("invalid yaml")?;
    Ok(serde_yaml::to_string(&value)?)
}

fn pretty_toml(raw: &str) -> Result<String> {
    let value: toml::Value = toml::from_str(raw).context("invalid toml")?;
    Ok(toml::to_string_pretty(&value)?)
}

fn pretty_ini(raw: &str) -> Result<String> {
    let map = ini::macro_read(raw);
    let mut out = String::new();
    for (section, props) in map.iter() {
        if !section.is_empty() {
            out.push_str(&format!("[{section}]\n"));
        }
        for (k, v) in props.iter() {
            let value = v.as_deref().unwrap_or("");
            out.push_str(&format!("{k} = {value}\n"));
        }
        out.push('\n');
    }
    Ok(out)
}

fn render_csv(raw: &str, config: &OmnicatConfig) -> Result<String> {
    Ok(render_delimited_terminal(raw, b',', config)?)
}

fn render_tsv(raw: &str, config: &OmnicatConfig) -> Result<String> {
    Ok(render_delimited_terminal(raw, b'\t', config)?)
}

fn parse_delimited(raw: &str, delimiter: u8, config: &OmnicatConfig) -> Result<TableContent> {
    let records = read_delimited_records(raw, delimiter)?;
    let max_rows = config.gui.spreadsheet.max_rows.min(config.terminal.data.max_rows);
    let (headers, rows) =
        table_from_records(records, max_rows, config.gui.spreadsheet.header_row);
    Ok(TableContent {
        title: None,
        headers,
        rows,
    })
}

fn render_delimited_terminal(raw: &str, delimiter: u8, config: &OmnicatConfig) -> Result<String> {
    let records = read_delimited_records(raw, delimiter)?;
    let mut table = Table::new();
    crate::sinks::styled_table::configure_table(&mut table, config.terminal.data.table_border);
    if let Some(header) = records.first() {
        crate::sinks::styled_table::set_styled_header(&mut table, header.clone(), config);
        for row in records.iter().skip(1) {
            crate::sinks::styled_table::add_styled_row(&mut table, row.clone(), config);
        }
    } else {
        for row in &records {
            crate::sinks::styled_table::add_styled_row(&mut table, row.clone(), config);
        }
    }
    Ok(table.to_string())
}

fn read_delimited_records(raw: &str, delimiter: u8) -> Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let mut records = Vec::new();
    for result in rdr.records() {
        records.push(result.context("csv parse error")?.iter().map(str::to_string).collect());
    }
    Ok(records)
}

fn table_from_records(
    records: Vec<Vec<String>>,
    max_rows: usize,
    header_row: bool,
) -> (Vec<String>, Vec<Vec<String>>) {
    if records.is_empty() {
        return (Vec::new(), Vec::new());
    }
    if header_row {
        let headers = records[0].clone();
        let body: Vec<Vec<String>> = records.into_iter().skip(1).take(max_rows).collect();
        (headers, body)
    } else {
        let body: Vec<Vec<String>> = records.into_iter().take(max_rows).collect();
        (Vec::new(), body)
    }
}

fn highlight_data(text: &str, ext: &str, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
    if matches!(ext, "csv" | "tsv") {
        writeln!(out, "{text}")?;
        return Ok(());
    }

    let syntax_name = match ext {
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "ini" => "INI",
        _ => "Plain Text",
    };

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps
        .find_syntax_by_name(syntax_name)
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    let theme = resolve_theme(&ts, &config.terminal.code.theme);
    let mut h = HighlightLines::new(syntax, theme);

    for line in LinesWithEndings::from(text) {
        let ranges = h.highlight_line(line, &ps).context("highlight failed")?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        write!(out, "{escaped}")?;
    }

    Ok(())
}

fn build_parquet_table(path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
    use parquet::file::reader::FileReader;
    let file = File::open(path)?;
    let reader = parquet::file::reader::SerializedFileReader::new(file)?;
    let meta = reader.metadata().file_metadata();
    let schema = meta.schema_descr();
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Column", "Physical type"]);
    for i in 0..schema.num_columns() {
        let col = schema.column(i);
        table.add_row(vec![
            col.name().to_string(),
            format!("{:?}", col.physical_type()),
        ]);
    }
    let max_rows = config.gui.spreadsheet.max_rows.min(20);
    let mut preview = Table::new();
    preview.load_preset(UTF8_FULL);
        if let Ok(batch_reader) = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        File::open(path)?,
    ) {
        if let Ok(mut batches) = batch_reader.build() {
            if let Some(Ok(batch)) = batches.next() {
                let cols: Vec<String> = (0..batch.num_columns())
                    .map(|i| batch.schema().field(i).name().clone())
                    .collect();
                preview.set_header(cols);
                let rows = batch.num_rows().min(max_rows);
                for row in 0..rows {
                    let cells: Vec<String> = (0..batch.num_columns())
                        .map(|col| array_cell_string(batch.column(col).as_ref(), row))
                        .collect();
                    preview.add_row(cells);
                }
            }
        }
    }
    let out = format!(
        "Parquet — rows: {}, row groups: {}\n\nSchema:\n{}\n\nPreview:\n{}",
        meta.num_rows(),
        reader.metadata().num_row_groups(),
        table,
        preview
    );
    Ok(PreviewContent::Text(out))
}

fn array_cell_string(array: &dyn arrow::array::Array, row: usize) -> String {
    use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
    match array.data_type() {
        arrow::datatypes::DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .and_then(|a| if a.is_valid(row) { Some(a.value(row).to_string()) } else { None })
            .unwrap_or_default(),
        arrow::datatypes::DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        arrow::datatypes::DataType::Float64 => array
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        arrow::datatypes::DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        other => format!("<{other:?}>"),
    }
}

fn build_feather_table(path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
    use arrow::ipc::reader::FileReader as ArrowIpcReader;
    let file = File::open(path)?;
    let reader = ArrowIpcReader::try_new(file, None).context("open feather/ipc")?;
    let schema = reader.schema();
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Column", "Type"]);
    for field in schema.fields() {
        table.add_row(vec![field.name().clone(), format!("{:?}", field.data_type())]);
    }
    let max_rows = config.gui.spreadsheet.max_rows.min(20);
    let mut preview = Table::new();
    preview.load_preset(UTF8_FULL);
    if let Some(Ok(batch)) = reader.into_iter().next() {
        let cols: Vec<String> = (0..batch.num_columns())
            .map(|i| batch.schema().field(i).name().clone())
            .collect();
        preview.set_header(cols);
        let rows = batch.num_rows().min(max_rows);
        for row in 0..rows {
            let cells: Vec<String> = (0..batch.num_columns())
                .map(|col| array_cell_string(batch.column(col).as_ref(), row))
                .collect();
            preview.add_row(cells);
        }
    }
    let out = format!("Feather/IPC\n\nSchema:\n{table}\n\nPreview:\n{preview}");
    Ok(PreviewContent::Text(out))
}

fn build_msgpack_text(path: &Path) -> Result<PreviewContent> {
    let bytes = fs::read(path)?;
    let value: serde_json::Value = rmp_serde::from_slice(&bytes).context("msgpack decode")?;
    let pretty = serde_json::to_string_pretty(&value)?;
    Ok(PreviewContent::Text(pretty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::PreviewContent;

    #[test]
    fn build_csv_returns_plain_table_without_ansi() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.csv");
        std::fs::write(&path, "name,score\nalpha,1\nbeta,2\n").unwrap();
        let content = DataDriver
            .build(&path, &OmnicatConfig::default(), &crate::content::preview_context(&path))
            .unwrap();
        match content {
            PreviewContent::Table(table) => {
                assert_eq!(table.headers, vec!["name", "score"]);
                assert_eq!(table.rows.len(), 2);
                let flat = format!("{table:?}");
                assert!(!flat.contains("\x1b["), "table must not contain ANSI escapes");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }
}
