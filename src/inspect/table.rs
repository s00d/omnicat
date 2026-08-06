use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::report::InspectReport;

pub fn table_view(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
    columns_only: bool,
    head: Option<usize>,
    tail: Option<usize>,
) -> Result<InspectReport> {
    let (headers, rows) = load_table(path, kind, config)?;
    let total_rows = rows.len();
    let total_cols = headers.len();

    if columns_only {
        return Ok(InspectReport::Table {
            path: display.to_string(),
            headers: headers.clone(),
            rows: headers.iter().map(|h| vec![h.clone()]).collect(),
            total_rows,
            total_cols,
            note: Some("columns".into()),
        });
    }

    let sliced: Vec<Vec<String>> = match (head, tail) {
        (Some(n), _) => rows.into_iter().take(n).collect(),
        (_, Some(n)) => {
            let skip = total_rows.saturating_sub(n);
            rows.into_iter().skip(skip).collect()
        }
        _ => {
            let max = crate::inspect::effective_max_rows(config);
            rows.into_iter().take(max).collect()
        }
    };

    let note = if sliced.len() < total_rows {
        Some(format!(
            "Showing {} of {} rows. Use --head/--tail/--all.",
            sliced.len(),
            total_rows
        ))
    } else {
        None
    };

    Ok(InspectReport::Table {
        path: display.to_string(),
        headers,
        rows: sliced,
        total_rows,
        total_cols,
        note,
    })
}

fn load_table(
    path: &Path,
    kind: HandlerKind,
    _config: &OmnicatConfig,
) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    match kind {
        HandlerKind::Data => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "csv" => read_csv(path, b','),
                "tsv" => read_csv(path, b'\t'),
                "jsonl" | "ndjson" => read_jsonl(path),
                _ => bail!("table view requires csv/tsv/jsonl"),
            }
        }
        HandlerKind::Spreadsheet => {
            use calamine::{open_workbook_auto, Data, Reader};
            let mut wb = open_workbook_auto(path)?;
            let sheet = wb
                .sheet_names()
                .first()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no sheets"))?;
            let range = wb.worksheet_range(&sheet)?;
            let mut rows_iter = range.rows();
            let headers = rows_iter
                .next()
                .map(|r| {
                    r.iter()
                        .enumerate()
                        .map(|(i, c)| match c {
                            Data::String(s) if !s.is_empty() => s.clone(),
                            _ => format!("col_{}", i + 1),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let rows = rows_iter
                .map(|r| r.iter().map(|c| format!("{c}")).collect())
                .collect();
            Ok((headers, rows))
        }
        HandlerKind::Database => {
            use rusqlite::Connection;
            let conn = Connection::open(path)?;
            let table: String = conn.query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
                [],
                |r| r.get(0),
            )?;
            let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\""))?;
            let col_count = stmt.column_count();
            let headers: Vec<String> = (0..col_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();
            let mapped = stmt.query_map([], |row| {
                let mut cells = Vec::new();
                for i in 0..col_count {
                    let val: rusqlite::types::Value = row.get(i)?;
                    cells.push(match val {
                        rusqlite::types::Value::Null => "NULL".into(),
                        rusqlite::types::Value::Integer(n) => n.to_string(),
                        rusqlite::types::Value::Real(f) => f.to_string(),
                        rusqlite::types::Value::Text(s) => s,
                        rusqlite::types::Value::Blob(b) => format!("<blob {}>", b.len()),
                    });
                }
                Ok(cells)
            })?;
            let rows = mapped.filter_map(|r| r.ok()).collect();
            Ok((headers, rows))
        }
        _ => bail!("table view not supported for {}", kind.name()),
    }
}

fn read_csv(path: &Path, delim: u8) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let raw = fs::read_to_string(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delim)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        rows.push(rec?.iter().map(|s| s.to_string()).collect());
    }
    Ok((headers, rows))
}

fn read_jsonl(path: &Path) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    use crate::content::{cell_value, plan_columns};
    use crate::io::{FileHandle, OpenOptions};

    let mut keys: Vec<String> = Vec::new();
    let mut key_set = std::collections::BTreeSet::new();
    let mut objects = Vec::new();
    FileHandle::open(path, OpenOptions::stream())?.for_each_line(|line| {
        let text = line.as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Ok(());
        }
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text)
        {
            for k in map.keys() {
                if key_set.insert(k.clone()) {
                    keys.push(k.clone());
                }
            }
            objects.push(map);
        }
        Ok(())
    })?;
    let cols = plan_columns(&keys);
    let rows = objects
        .into_iter()
        .map(|map| {
            cols.source_keys
                .iter()
                .map(|k| cell_value(&map, k))
                .collect()
        })
        .collect();
    Ok((cols.headers, rows))
}
