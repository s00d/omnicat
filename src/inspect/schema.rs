use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::report::{SchemaField, SchemaReport, SchemaTable};
use crate::io::{FileHandle, OpenOptions};

pub fn build_schema(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<SchemaReport> {
    match kind {
        HandlerKind::Database => schema_sqlite(path, display),
        HandlerKind::Data => schema_data(path, display, config),
        HandlerKind::Spreadsheet => schema_spreadsheet(path, display),
        _ => bail!("schema not supported for {}", kind.name()),
    }
}

fn schema_sqlite(path: &Path, display: &str) -> Result<SchemaReport> {
    let conn = Connection::open(path).context("open sqlite")?;
    let mut tables = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for name in &names {
        let pragma = format!("PRAGMA table_info(\"{name}\")");
        let mut info = conn.prepare(&pragma)?;
        let fields: Vec<SchemaField> = info
            .query_map([], |row| {
                let col_name: String = row.get(1)?;
                let type_name: String = row.get(2)?;
                let notnull: i64 = row.get(3)?;
                let pk: i64 = row.get(5)?;
                let mut notes = Vec::new();
                if pk != 0 {
                    notes.push("PRIMARY KEY");
                }
                if notnull != 0 {
                    notes.push("NOT NULL");
                }
                Ok(SchemaField {
                    name: col_name,
                    type_name: if type_name.is_empty() {
                        "BLOB".into()
                    } else {
                        type_name
                    },
                    nullable: Some(notnull == 0 && pk == 0),
                    notes: if notes.is_empty() {
                        None
                    } else {
                        Some(notes.join(" "))
                    },
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        tables.push(SchemaTable {
            name: name.clone(),
            fields,
        });
    }

    Ok(SchemaReport {
        path: display.to_string(),
        handler: "database".into(),
        title: Some("SQLite".into()),
        rows: None,
        columns: None,
        fields: Vec::new(),
        tables,
    })
}

fn schema_data(path: &Path, display: &str, config: &OmnicatConfig) -> Result<SchemaReport> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "csv" | "tsv" => schema_csv(path, display, if ext == "tsv" { b'\t' } else { b',' }),
        "json" => schema_json(path, display, config),
        "jsonl" | "ndjson" => schema_jsonl(path, display, config),
        "parquet" => schema_parquet(path, display),
        "feather" => schema_feather(path, display),
        _ => bail!("schema not supported for .{ext}"),
    }
}

fn schema_csv(path: &Path, display: &str, delim: u8) -> Result<SchemaReport> {
    let raw = fs::read_to_string(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delim)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut samples: Vec<Vec<String>> = Vec::new();
    for (i, rec) in rdr.records().enumerate() {
        if i >= 100 {
            break;
        }
        samples.push(rec?.iter().map(|s| s.to_string()).collect());
    }
    let row_count = {
        let mut rdr2 = csv::ReaderBuilder::new()
            .has_headers(true)
            .delimiter(delim)
            .from_reader(raw.as_bytes());
        rdr2.records().count() as u64
    };

    let fields: Vec<SchemaField> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let col_samples: Vec<&str> = samples
                .iter()
                .filter_map(|r| r.get(i).map(String::as_str))
                .collect();
            SchemaField {
                name: name.clone(),
                type_name: infer_col_type(&col_samples),
                nullable: Some(col_samples.iter().any(|s| s.is_empty())),
                notes: None,
            }
        })
        .collect();

    Ok(SchemaReport {
        path: display.to_string(),
        handler: "data".into(),
        title: Some(ext_title(path)),
        rows: Some(row_count),
        columns: Some(fields.len()),
        fields,
        tables: Vec::new(),
    })
}

fn schema_json(path: &Path, display: &str, config: &OmnicatConfig) -> Result<SchemaReport> {
    let max = if config.inspect.no_limit {
        0
    } else {
        config.inspect.max_bytes
    };
    let bytes = crate::io::read_bytes_capped(path, max, true)?;
    let text = String::from_utf8_lossy(&bytes);
    let value: serde_json::Value = serde_json::from_str(&text)?;
    let fields = fields_from_json(&value);
    Ok(SchemaReport {
        path: display.to_string(),
        handler: "data".into(),
        title: Some("JSON".into()),
        rows: match &value {
            serde_json::Value::Array(a) => Some(a.len() as u64),
            _ => None,
        },
        columns: Some(fields.len()),
        fields,
        tables: Vec::new(),
    })
}

fn schema_jsonl(path: &Path, display: &str, config: &OmnicatConfig) -> Result<SchemaReport> {
    let max_bytes = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes as u64)
    };
    let handle = FileHandle::open(
        path,
        OpenOptions {
            max_scan_bytes: max_bytes,
            ..OpenOptions::stream()
        },
    )?;
    let mut keys = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut sample_rows = 0usize;
    let mut total_rows = 0u64;
    handle.for_each_line(|line| {
        let text = line.text_lossy();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        total_rows += 1;
        if sample_rows >= 200 {
            return Ok(());
        }
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(trimmed)
        {
            sample_rows += 1;
            for (k, v) in map {
                keys.entry(k).or_default().push(json_type_name(&v));
            }
        }
        Ok(())
    })?;
    let fields: Vec<SchemaField> = keys
        .into_iter()
        .map(|(name, types)| {
            let type_name = majority_type(&types);
            SchemaField {
                name,
                type_name,
                nullable: Some(true),
                notes: None,
            }
        })
        .collect();
    Ok(SchemaReport {
        path: display.to_string(),
        handler: "data".into(),
        title: Some("JSONL".into()),
        rows: Some(total_rows),
        columns: Some(fields.len()),
        fields,
        tables: Vec::new(),
    })
}

fn schema_parquet(path: &Path, display: &str) -> Result<SchemaReport> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let file = fs::File::open(path)?;
    let reader = SerializedFileReader::new(file)?;
    let meta = reader.metadata().file_metadata();
    let schema = meta.schema_descr();
    let fields: Vec<SchemaField> = (0..schema.num_columns())
        .map(|i| {
            let col = schema.column(i);
            SchemaField {
                name: col.name().to_string(),
                type_name: format!("{:?}", col.physical_type()),
                nullable: None,
                notes: None,
            }
        })
        .collect();
    Ok(SchemaReport {
        path: display.to_string(),
        handler: "data".into(),
        title: Some("Parquet".into()),
        rows: Some(meta.num_rows() as u64),
        columns: Some(fields.len()),
        fields,
        tables: Vec::new(),
    })
}

fn schema_feather(path: &Path, display: &str) -> Result<SchemaReport> {
    use arrow::ipc::reader::FileReader as ArrowIpcReader;
    let file = fs::File::open(path)?;
    let reader = ArrowIpcReader::try_new(file, None)?;
    let schema = reader.schema();
    let fields: Vec<SchemaField> = schema
        .fields()
        .iter()
        .map(|f| SchemaField {
            name: f.name().clone(),
            type_name: format!("{:?}", f.data_type()),
            nullable: Some(f.is_nullable()),
            notes: None,
        })
        .collect();
    Ok(SchemaReport {
        path: display.to_string(),
        handler: "data".into(),
        title: Some("Feather".into()),
        rows: None,
        columns: Some(fields.len()),
        fields,
        tables: Vec::new(),
    })
}

fn schema_spreadsheet(path: &Path, display: &str) -> Result<SchemaReport> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut wb = open_workbook_auto(path).context("open spreadsheet")?;
    let sheets = wb.sheet_names().to_vec();
    let mut tables = Vec::new();
    for name in sheets {
        if let Ok(range) = wb.worksheet_range(&name) {
            let mut fields = Vec::new();
            if let Some(first) = range.rows().next() {
                for (i, cell) in first.iter().enumerate() {
                    let col_name = match cell {
                        Data::String(s) if !s.is_empty() => s.clone(),
                        _ => format!("col_{}", i + 1),
                    };
                    let samples: Vec<String> = range
                        .rows()
                        .skip(1)
                        .take(50)
                        .filter_map(|r| r.get(i).map(|c| format!("{c}")))
                        .collect();
                    let sample_refs: Vec<&str> = samples.iter().map(String::as_str).collect();
                    fields.push(SchemaField {
                        name: col_name,
                        type_name: infer_col_type(&sample_refs),
                        nullable: None,
                        notes: None,
                    });
                }
            }
            tables.push(SchemaTable { name, fields });
        }
    }
    Ok(SchemaReport {
        path: display.to_string(),
        handler: "spreadsheet".into(),
        title: Some("Spreadsheet".into()),
        rows: None,
        columns: None,
        fields: Vec::new(),
        tables,
    })
}

fn fields_from_json(value: &serde_json::Value) -> Vec<SchemaField> {
    match value {
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| SchemaField {
                name: k.clone(),
                type_name: json_type_name(v),
                nullable: Some(v.is_null()),
                notes: None,
            })
            .collect(),
        serde_json::Value::Array(arr) => {
            if let Some(serde_json::Value::Object(map)) = arr.first() {
                map.iter()
                    .map(|(k, v)| SchemaField {
                        name: k.clone(),
                        type_name: json_type_name(v),
                        nullable: Some(true),
                        notes: None,
                    })
                    .collect()
            } else {
                vec![SchemaField {
                    name: "item".into(),
                    type_name: arr
                        .first()
                        .map(json_type_name)
                        .unwrap_or_else(|| "unknown".into()),
                    nullable: None,
                    notes: None,
                }]
            }
        }
        other => vec![SchemaField {
            name: "value".into(),
            type_name: json_type_name(other),
            nullable: None,
            notes: None,
        }],
    }
}

fn json_type_name(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Bool(_) => "boolean".into(),
        serde_json::Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer".into()
            } else {
                "float".into()
            }
        }
        serde_json::Value::String(_) => "string".into(),
        serde_json::Value::Array(_) => "array".into(),
        serde_json::Value::Object(_) => "object".into(),
    }
}

fn infer_col_type(samples: &[&str]) -> String {
    if samples.is_empty() {
        return "string".into();
    }
    let non_empty: Vec<&str> = samples.iter().copied().filter(|s| !s.is_empty()).collect();
    if non_empty.is_empty() {
        return "string".into();
    }
    if non_empty.iter().all(|s| s.parse::<i64>().is_ok()) {
        return "integer".into();
    }
    if non_empty.iter().all(|s| s.parse::<f64>().is_ok()) {
        return "float".into();
    }
    if non_empty
        .iter()
        .all(|s| matches!(s.to_ascii_lowercase().as_str(), "true" | "false"))
    {
        return "boolean".into();
    }
    "string".into()
}

fn majority_type(types: &[String]) -> String {
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for t in types {
        *counts.entry(t.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(t, _)| t.to_string())
        .unwrap_or_else(|| "string".into())
}

fn ext_title(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("data")
        .to_ascii_uppercase()
}
