use std::collections::VecDeque;
use std::path::Path;

use anyhow::{bail, Result};

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::report::InspectReport;

enum RowMode {
    First(usize),
    Last(usize),
}

pub fn table_view(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
    columns_only: bool,
    head: Option<usize>,
    tail: Option<usize>,
) -> Result<InspectReport> {
    let mode = match (head, tail) {
        (Some(n), _) => RowMode::First(n.max(1)),
        (_, Some(n)) => RowMode::Last(n.max(1)),
        _ => RowMode::First(crate::inspect::effective_max_rows(config).max(1)),
    };

    let (headers, rows, scanned) = load_table(path, kind, &mode)?;
    let total_cols = headers.len();

    if columns_only {
        return Ok(InspectReport::Table {
            path: display.to_string(),
            headers: headers.clone(),
            rows: headers.iter().map(|h| vec![h.clone()]).collect(),
            total_rows: scanned,
            total_cols,
            note: Some("columns".into()),
        });
    }

    let note = if matches!(mode, RowMode::First(_)) && rows.len() < scanned {
        Some(format!(
            "Showing {} of {}+ rows. Use --head/--tail/--all.",
            rows.len(),
            scanned
        ))
    } else if matches!(mode, RowMode::Last(_)) && scanned > rows.len() {
        Some(format!(
            "Showing last {} of {} rows. Use --head/--all.",
            rows.len(),
            scanned
        ))
    } else {
        None
    };

    Ok(InspectReport::Table {
        path: display.to_string(),
        headers,
        rows,
        total_rows: scanned,
        total_cols,
        note,
    })
}

fn load_table(
    path: &Path,
    kind: HandlerKind,
    mode: &RowMode,
) -> Result<(Vec<String>, Vec<Vec<String>>, usize)> {
    match kind {
        HandlerKind::Data => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "csv" => read_csv(path, b',', mode),
                "tsv" => read_csv(path, b'\t', mode),
                "jsonl" | "ndjson" => read_jsonl(path, mode),
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
            Ok(collect_rows(
                rows_iter.map(|r| r.iter().map(|c| format!("{c}")).collect()),
                mode,
            ))
            .map(|(rows, scanned)| (headers, rows, scanned))
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
            let iter = mapped.filter_map(|r| r.ok());
            let (rows, scanned) = collect_rows(iter, mode);
            Ok((headers, rows, scanned))
        }
        _ => bail!("table view not supported for {}", kind.name()),
    }
}

fn collect_rows<I>(iter: I, mode: &RowMode) -> (Vec<Vec<String>>, usize)
where
    I: Iterator<Item = Vec<String>>,
{
    let mut scanned = 0usize;
    match mode {
        RowMode::First(n) => {
            let mut out = Vec::with_capacity(*n);
            for row in iter {
                scanned += 1;
                if out.len() < *n {
                    out.push(row);
                }
            }
            (out, scanned)
        }
        RowMode::Last(n) => {
            let mut ring = VecDeque::with_capacity(*n);
            for row in iter {
                scanned += 1;
                if ring.len() == *n {
                    ring.pop_front();
                }
                ring.push_back(row);
            }
            (ring.into_iter().collect(), scanned)
        }
    }
}

fn read_csv(
    path: &Path,
    delim: u8,
    mode: &RowMode,
) -> Result<(Vec<String>, Vec<Vec<String>>, usize)> {
    let file = std::fs::File::open(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delim)
        .flexible(true)
        .from_reader(file);
    let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut scanned = 0usize;
    let rows = match mode {
        RowMode::First(n) => {
            let mut out = Vec::with_capacity(*n);
            for rec in rdr.records() {
                scanned += 1;
                if out.len() < *n {
                    out.push(rec?.iter().map(|s| s.to_string()).collect());
                } else {
                    // Still count remaining rows for the note without storing them.
                    for rest in rdr.records() {
                        let _ = rest?;
                        scanned += 1;
                    }
                    break;
                }
            }
            out
        }
        RowMode::Last(n) => {
            let mut ring = VecDeque::with_capacity(*n);
            for rec in rdr.records() {
                scanned += 1;
                if ring.len() == *n {
                    ring.pop_front();
                }
                ring.push_back(rec?.iter().map(|s| s.to_string()).collect());
            }
            ring.into_iter().collect()
        }
    };
    Ok((headers, rows, scanned))
}

fn read_jsonl(path: &Path, mode: &RowMode) -> Result<(Vec<String>, Vec<Vec<String>>, usize)> {
    use crate::content::{cell_value, plan_columns};
    use crate::io::{FileHandle, OpenOptions};

    let mut keys: Vec<String> = Vec::new();
    let mut key_set = std::collections::BTreeSet::new();
    let mut scanned = 0usize;
    let mut first: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    let mut ring: VecDeque<serde_json::Map<String, serde_json::Value>> = VecDeque::new();
    let cap = match mode {
        RowMode::First(n) | RowMode::Last(n) => *n,
    };

    FileHandle::open(path, OpenOptions::stream())?.for_each_line(|line| {
        let text = line.as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Ok(std::ops::ControlFlow::Continue(()));
        }
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text)
        else {
            return Ok(std::ops::ControlFlow::Continue(()));
        };
        scanned += 1;
        for k in map.keys() {
            if key_set.insert(k.clone()) {
                keys.push(k.clone());
            }
        }
        match mode {
            RowMode::First(_) => {
                if first.len() < cap {
                    first.push(map);
                }
            }
            RowMode::Last(_) => {
                if ring.len() == cap {
                    ring.pop_front();
                }
                ring.push_back(map);
            }
        }
        Ok(std::ops::ControlFlow::Continue(()))
    })?;

    let cols = plan_columns(&keys);
    let objects: Vec<_> = match mode {
        RowMode::First(_) => first,
        RowMode::Last(_) => ring.into_iter().collect(),
    };
    let rows = objects
        .into_iter()
        .map(|map| {
            cols.source_keys
                .iter()
                .map(|k| cell_value(&map, k))
                .collect()
        })
        .collect();
    Ok((cols.headers, rows, scanned))
}
