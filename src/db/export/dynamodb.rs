//! AWS DynamoDB export (JSON / directory manifest).

use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::db::options::DbOptions;
use crate::db::report::{DbReport, DynamoDbExportOverview, TableInfo};

pub fn run(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() {
        bail!("DynamoDB export: --query not in V1; use Parquet path with future DataFusion support");
    }
    if opts.find.is_some() || opts.sample.is_some() {
        bail!("DynamoDB export supports overview, --tables, --stats only");
    }
    let overview = if path.is_dir() {
        overview_from_dir(path)?
    } else {
        overview_from_file(path)?
    };
    if opts.tables {
        return Ok(DbReport::Tables {
            tables: overview
                .tables
                .iter()
                .map(|t| TableInfo {
                    name: t.clone(),
                    rows: 0,
                    bytes: 0,
                })
                .collect(),
        });
    }
    Ok(DbReport::OverviewDynamoDb(overview))
}

fn overview_from_dir(path: &Path) -> Result<DynamoDbExportOverview> {
    let mut tables = Vec::new();
    let mut warnings = Vec::new();
    if let Ok(text) = fs::read_to_string(path.join("manifest-summary.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            if let Some(arr) = v.get("tableArns").and_then(|a| a.as_array()) {
                for t in arr {
                    if let Some(s) = t.as_str() {
                        tables.push(s.rsplit('/').next().unwrap_or(s).to_string());
                    }
                }
            }
        }
    }
    if tables.is_empty() {
        warnings.push("no tables in manifest-summary.json".into());
    }
    Ok(DynamoDbExportOverview {
        path: path.display().to_string(),
        format: "directory".into(),
        tables,
        warnings,
    })
}

fn overview_from_file(path: &Path) -> Result<DynamoDbExportOverview> {
    let mut warnings = Vec::new();
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export")
        .to_string();
    if path.extension().and_then(|e| e.to_str()) == Some("parquet") {
        return Ok(DynamoDbExportOverview {
            path: path.display().to_string(),
            format: "parquet".into(),
            tables: vec![name],
            warnings,
        });
    }
    warnings.push("JSON export: table list inferred from filename".into());
    Ok(DynamoDbExportOverview {
        path: path.display().to_string(),
        format: "json".into(),
        tables: vec![name],
        warnings,
    })
}
