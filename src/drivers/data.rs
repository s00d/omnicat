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
use crate::sinks::progress::ProgressReporter;

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

    /// Terminal preview with optional `--progress` (stream read + render ticks).
    pub fn render_terminal(
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
        out: &mut dyn Write,
    ) -> Result<()> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "csv" => render_delimited_path_terminal(path, b',', config, ctx, out),
            "tsv" => render_delimited_path_terminal(path, b'\t', config, ctx, out),
            "jsonl" | "ndjson" => {
                let content = build_jsonl_table(path, config, ctx)?;
                crate::sinks::terminal::write_content(&content, config, out)
            }
            _ => DataDriver.render(path, config, out),
        }
    }
}

impl PreviewDriver for DataDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Data
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "csv",
            "tsv",
            "json",
            "jsonl",
            "ndjson",
            "yaml",
            "yml",
            "toml",
            "ini",
            "conf",
            "cfg",
            "properties",
            "env",
            "parquet",
            "feather",
            "msgpack",
        ]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
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
            "csv" => Ok(PreviewContent::Table(parse_delimited_path(
                path, b',', config, ctx,
            )?)),
            "tsv" => Ok(PreviewContent::Table(parse_delimited_path(
                path, b'\t', config, ctx,
            )?)),
            "jsonl" | "ndjson" => build_jsonl_table(path, config, ctx),
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
    render_delimited_terminal(raw, b',', config)
}

fn render_tsv(raw: &str, config: &OmnicatConfig) -> Result<String> {
    render_delimited_terminal(raw, b'\t', config)
}

fn preview_row_limit(config: &OmnicatConfig) -> usize {
    if config.inspect.no_limit || config.inspect.max_rows == 0 {
        return usize::MAX;
    }
    config.terminal.data.max_rows.max(1)
}

fn parse_delimited_path(
    path: &Path,
    delimiter: u8,
    config: &OmnicatConfig,
    ctx: &PreviewContext,
) -> Result<TableContent> {
    let max_rows = preview_row_limit(config);
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut progress = ProgressReporter::new("preview", ctx.progress);
    if ctx.progress {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        progress.status(&format!("{name}  {}", human_bytes(ctx.size)));
    }
    let (headers, rows) = read_delimited_limited(
        file,
        delimiter,
        max_rows,
        config.terminal.data.header_row,
        Some(&mut progress),
        ctx.size,
    )?;
    progress.finish();
    Ok(TableContent {
        title: None,
        headers,
        rows,
    })
}

fn render_delimited_path_terminal(
    path: &Path,
    delimiter: u8,
    config: &OmnicatConfig,
    ctx: &PreviewContext,
    out: &mut dyn Write,
) -> Result<()> {
    let mut progress = ProgressReporter::new("preview", ctx.progress);
    render_delimited_path_terminal_with(path, delimiter, config, ctx, &mut progress, out)
}

fn render_delimited_path_terminal_with(
    path: &Path,
    delimiter: u8,
    config: &OmnicatConfig,
    ctx: &PreviewContext,
    progress: &mut ProgressReporter,
    out: &mut dyn Write,
) -> Result<()> {
    let max_rows = preview_row_limit(config);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    progress.status(&format!("{name}  {}", human_bytes(ctx.size)));

    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let (headers, rows) = read_delimited_limited(
        file,
        delimiter,
        max_rows,
        config.terminal.data.header_row,
        Some(progress),
        ctx.size,
    )?;

    crate::sinks::styled_table::write_styled_table(
        out,
        None,
        &headers,
        &rows,
        config,
        config.terminal.data.table_border,
    )?;
    let cols = headers
        .len()
        .max(rows.first().map(|r| r.len()).unwrap_or(0));
    writeln!(out, "\n{} rows × {cols} columns", rows.len())?;
    out.flush()?;
    progress.finish();
    Ok(())
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

fn render_delimited_terminal(raw: &str, delimiter: u8, config: &OmnicatConfig) -> Result<String> {
    let max = preview_row_limit(config);
    let (headers, rows) = read_delimited_limited(raw.as_bytes(), delimiter, max, true, None, 0)?;
    let total = rows.len();
    let mut table = Table::new();
    crate::sinks::styled_table::configure_table(&mut table, config.terminal.data.table_border);
    if !headers.is_empty() {
        crate::sinks::styled_table::set_styled_header(&mut table, headers.clone(), config);
    }
    for row in &rows {
        crate::sinks::styled_table::add_styled_row(&mut table, row.clone(), config);
    }
    let cols = headers
        .len()
        .max(rows.first().map(|r| r.len()).unwrap_or(0));
    Ok(format!("{table}\n\n{total} rows × {cols} columns"))
}

fn read_delimited_limited<R: std::io::Read>(
    reader: R,
    delimiter: u8,
    max_rows: usize,
    header_row: bool,
    mut progress: Option<&mut ProgressReporter>,
    file_size: u64,
) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(reader);
    let mut records = Vec::new();
    let mut i = 0usize;
    loop {
        let limit = if header_row {
            max_rows.saturating_add(1)
        } else {
            max_rows
        };
        if i >= limit {
            break;
        }
        let mut record = csv::StringRecord::new();
        if !rdr.read_record(&mut record)? {
            break;
        }
        records.push(record.iter().map(str::to_string).collect());
        if let Some(p) = progress.as_mut() {
            let shown = if header_row { i } else { i + 1 };
            if shown > 0 {
                if max_rows != usize::MAX {
                    p.tick_count_total(shown as u64, max_rows.max(1) as u64, "rows");
                } else if file_size > 0 {
                    p.tick_bytes(rdr.position().byte(), Some(file_size));
                } else {
                    p.tick_count(shown as u64, "rows");
                }
            }
        }
        i += 1;
    }
    Ok(table_from_records(records, max_rows, header_row))
}

fn build_jsonl_table(
    path: &Path,
    config: &OmnicatConfig,
    ctx: &PreviewContext,
) -> Result<PreviewContent> {
    use crate::content::{cell_value, plan_columns};
    use crate::io::{FileHandle, OpenOptions};
    use std::ops::ControlFlow;

    let mut keys: Vec<String> = Vec::new();
    let mut key_set = std::collections::BTreeSet::new();
    let mut objects = Vec::new();
    let max = preview_row_limit(config);
    let mut progress = ProgressReporter::new("read", ctx.progress);
    FileHandle::open(path, OpenOptions::stream())?.for_each_line(|line| {
        let text = line.as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Ok(ControlFlow::Continue(()));
        }
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text)
        {
            for k in map.keys() {
                if key_set.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
            objects.push(map);
            if ctx.size > 0 {
                progress.tick_bytes(line.line_no.saturating_mul(64), Some(ctx.size));
            } else {
                progress.tick_count(objects.len() as u64, "rows");
            }
            if objects.len() >= max {
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    })?;
    progress.finish();
    let cols = plan_columns(&keys);
    let rows: Vec<Vec<String>> = objects
        .into_iter()
        .map(|map| {
            cols.source_keys
                .iter()
                .map(|k| cell_value(&map, k))
                .collect()
        })
        .collect();
    Ok(PreviewContent::Table(TableContent {
        title: Some("JSONL".into()),
        headers: cols.headers,
        rows,
    }))
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

fn highlight_data(
    text: &str,
    ext: &str,
    config: &OmnicatConfig,
    out: &mut dyn Write,
) -> Result<()> {
    if matches!(ext, "csv" | "tsv") || config.terminal.plain {
        writeln!(out, "{text}")?;
        return Ok(());
    }

    let syntax_name = match ext {
        "json" | "jsonl" | "ndjson" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "ini" | "conf" | "cfg" | "properties" | "env" => "INI",
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
    let max_rows = config.terminal.data.max_rows.min(20);
    let mut preview = Table::new();
    preview.load_preset(UTF8_FULL);
    if let Ok(batch_reader) =
        parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)
    {
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
            .and_then(|a| {
                if a.is_valid(row) {
                    Some(a.value(row).to_string())
                } else {
                    None
                }
            })
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
        table.add_row(vec![
            field.name().clone(),
            format!("{:?}", field.data_type()),
        ]);
    }
    let max_rows = config.terminal.data.max_rows.min(20);
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
            .build(
                &path,
                &OmnicatConfig::default(),
                &crate::content::preview_context(&path),
            )
            .unwrap();
        match content {
            PreviewContent::Table(table) => {
                assert_eq!(table.headers, vec!["name", "score"]);
                assert_eq!(table.rows.len(), 2);
                let flat = format!("{table:?}");
                assert!(
                    !flat.contains("\x1b["),
                    "table must not contain ANSI escapes"
                );
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn csv_terminal_progress_emits_sticky_status_before_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.csv");
        let mut csv = String::from("id,name\n");
        for i in 0..800 {
            csv.push_str(&format!("{i},row-{i}\n"));
        }
        std::fs::write(&path, csv).unwrap();

        let mut cfg = OmnicatConfig::default();
        cfg.terminal.data.max_rows = 500;
        cfg.terminal.data.header_row = true;

        let mut ctx = crate::content::preview_context(&path);
        ctx.progress = true;
        let mut progress = ProgressReporter::for_test("preview");
        let mut out = Vec::new();
        render_delimited_path_terminal_with(&path, b',', &cfg, &ctx, &mut progress, &mut out)
            .unwrap();

        let table = String::from_utf8(out).unwrap();

        assert!(
            progress.paint_count() >= 2,
            "expected status + read ticks, paint_count={}",
            progress.paint_count()
        );
        assert!(
            table.contains("id") && table.contains("row-0"),
            "stdout must contain the table, got {} bytes",
            table.len()
        );
        assert!(
            table.contains("500 rows ×"),
            "default cap should keep footer at 500, got footer slice {:?}",
            table.lines().last()
        );
    }

    #[test]
    fn csv_all_lifts_preview_row_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.csv");
        let mut csv = String::from("id,name\n");
        for i in 0..800 {
            csv.push_str(&format!("{i},row-{i}\n"));
        }
        std::fs::write(&path, csv).unwrap();

        let mut cfg = OmnicatConfig::default();
        cfg.terminal.data.max_rows = 500;
        cfg.inspect.no_limit = true;
        cfg.inspect.max_rows = 0;

        let ctx = crate::content::preview_context(&path);
        let mut progress = ProgressReporter::for_test("preview");
        let mut out = Vec::new();
        render_delimited_path_terminal_with(&path, b',', &cfg, &ctx, &mut progress, &mut out)
            .unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("800 rows ×"),
            "--all / no_limit must show all 800 rows, got {:?}",
            text.lines().last()
        );
        assert!(
            text.contains("row-799"),
            "last data row missing under --all"
        );
        assert!(
            text.contains('┌') || text.contains('|'),
            "--all must still render a table, not raw CSV"
        );
    }

    #[test]
    fn preview_context_sniffs_prefix_not_whole_file() {
        use std::time::Instant;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.csv");
        // ~8 MiB of CSV — full read would be noticeable; sniff is 8 KiB.
        let row = "x".repeat(80) + "\n";
        let mut data = String::from("col\n");
        while data.len() < 8 * 1024 * 1024 {
            data.push_str(&row);
        }
        std::fs::write(&path, &data).unwrap();
        let t0 = Instant::now();
        let ctx = crate::content::preview_context(&path);
        let elapsed = t0.elapsed();
        assert_eq!(ctx.size, data.len() as u64);
        assert!(
            elapsed.as_millis() < 500,
            "preview_context read too much: {:?}",
            elapsed
        );
    }
}
