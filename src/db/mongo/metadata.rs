//! Parse mongodump *.metadata.json files.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::db::mongo::detect::CollectionRef;
use crate::db::report::{ColumnInfo, IndexInfo, TableSchema};

pub fn schema_from_metadata(meta_path: &Path, full_name: &str) -> Result<TableSchema> {
    let text = fs::read_to_string(meta_path)
        .with_context(|| format!("read {}", meta_path.display()))?;
    let v: Value = serde_json::from_str(&text).context("parse metadata.json")?;
    let indexes = parse_indexes(&v);
    Ok(TableSchema {
        name: full_name.to_string(),
        columns: infer_columns(&v),
        indexes,
    })
}

pub fn schema_for_collection(coll: &CollectionRef) -> Result<TableSchema> {
    let full = coll.full_name();
    if let Some(meta) = &coll.metadata_path {
        return schema_from_metadata(meta, &full);
    }
    Ok(TableSchema {
        name: full,
        columns: Vec::new(),
        indexes: Vec::new(),
    })
}

fn parse_indexes(v: &Value) -> Vec<IndexInfo> {
    let Some(arr) = v.get("indexes").and_then(|i| i.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|idx| {
            let name = idx.get("name")?.as_str()?.to_string();
            let key = idx.get("key")?.as_object()?;
            let cols: Vec<String> = key.keys().cloned().collect();
            let unique = idx.get("unique").and_then(|u| u.as_bool()).unwrap_or(false);
            Some(IndexInfo {
                name,
                columns: cols,
                unique,
                kind: if unique { "UNIQUE".into() } else { "INDEX".into() },
            })
        })
        .collect()
}

fn infer_columns(v: &Value) -> Vec<ColumnInfo> {
    let mut out = Vec::new();
    if let Some(opts) = v.get("options").and_then(|o| o.as_object()) {
        if let Some(validator) = opts.get("validator") {
            out.push(ColumnInfo {
                name: "validator".into(),
                type_name: validator.to_string(),
                nullable: true,
            });
        }
    }
    if out.is_empty() {
        out.push(ColumnInfo {
            name: "_id".into(),
            type_name: "ObjectId".into(),
            nullable: false,
        });
    }
    out
}
