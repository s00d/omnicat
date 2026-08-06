//! MongoDB --query via JSON filter (streaming).

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use anyhow::{bail, Context, Result};
use bson::Document;
use serde_json::Value;

use crate::db::mongo::archive::stream_collection_from_archive;
use crate::db::mongo::bson_stream::BsonReader;
use crate::db::mongo::detect::{is_archive, is_single_bson, resolve_collection};
use crate::db::mongo::filter::doc_matches_filter;
use crate::db::options::DbOptions;
use crate::db::query::{print_query_from_results, write_extract, write_query_output, QueryDialect};
use crate::db::report::{DbReport, QueryResultReport};
use crate::io::source::open_reader;

#[derive(Debug, Clone)]
pub struct MongoQuerySpec {
    pub collection: String,
    pub filter: Value,
    pub limit: Option<usize>,
    pub projection: Option<BTreeMap<String, i32>>,
}

pub fn parse_mongo_query(raw: &str, table: Option<&str>) -> Result<MongoQuerySpec> {
    let v: Value = serde_json::from_str(raw).context("parse --query as JSON")?;
    if let Some(obj) = v.as_object() {
        if obj.contains_key("collection") || obj.contains_key("filter") {
            let collection = obj
                .get("collection")
                .and_then(|c| c.as_str())
                .map(str::to_string)
                .or_else(|| table.map(str::to_string))
                .ok_or_else(|| anyhow::anyhow!("collection required in query or --table"))?;
            let filter = obj
                .get("filter")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::new()));
            let limit = obj.get("limit").and_then(|l| l.as_u64()).map(|n| n as usize);
            let projection = obj.get("projection").and_then(parse_projection);
            return Ok(MongoQuerySpec {
                collection,
                filter,
                limit,
                projection,
            });
        }
    }
    let collection = table
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("--table COLLECTION required when --query is a filter object"))?;
    Ok(MongoQuerySpec {
        collection,
        filter: v,
        limit: None,
        projection: None,
    })
}

fn parse_projection(v: &Value) -> Option<BTreeMap<String, i32>> {
    let obj = v.as_object()?;
    let mut out = BTreeMap::new();
    for (k, val) in obj {
        if let Some(n) = val.as_i64() {
            out.insert(k.clone(), n as i32);
        }
    }
    Some(out)
}

pub fn format_query_display(spec: &MongoQuerySpec) -> String {
    let filter = spec.filter.to_string();
    let mut s = format!("db.{}.find({filter})", spec.collection);
    if let Some(n) = spec.limit {
        s.push_str(&format!(".limit({n})"));
    }
    if spec.projection.is_some() {
        s.push_str(".project(...)");
    }
    s
}

pub fn run_mongo_query(path: &Path, opts: &DbOptions, raw: &str) -> Result<DbReport> {
    let spec = parse_mongo_query(raw, opts.table.as_deref())?;
    let report = execute_query(path, &spec)?;
    if opts.print_query {
        print_query_from_results(QueryDialect::Mongo, &spec.collection, &report)?;
        return Ok(DbReport::Text { lines: Vec::new() });
    }
    let mut out = io::stdout().lock();
    write_query_output(&report, opts.output, &mut out)?;
    if let Some(extract) = &opts.extract {
        write_extract(Path::new(extract), &report, opts.output)?;
    }
    Ok(DbReport::Text { lines: Vec::new() })
}

fn execute_query(path: &Path, spec: &MongoQuerySpec) -> Result<QueryResultReport> {
    let limit = spec.limit.unwrap_or(usize::MAX);
    let mut rows = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    let mut scanned = 0u64;
    let mut matched_count = 0usize;

    let mut on_doc = |doc: Document| -> Result<bool> {
        scanned += 1;
        if !doc_matches_filter(&doc, &spec.filter) {
            return Ok(false);
        }
        if matched_count >= limit {
            return Ok(true);
        }
        let flat = flatten_doc(&doc, spec.projection.as_ref());
        for k in flat.keys() {
            if !columns.contains(k) {
                columns.push(k.clone());
            }
        }
        let row: Vec<String> = columns
            .iter()
            .map(|c| flat.get(c).cloned().unwrap_or_else(|| "NULL".into()))
            .collect();
        rows.push(row);
        matched_count += 1;
        Ok(matched_count >= limit)
    };

    if is_archive(path) {
        let ns = resolve_archive_ns(path, &spec.collection)?;
        for doc in stream_collection_from_archive(path, &ns)? {
            if on_doc(doc?)? {
                break;
            }
        }
    } else if is_single_bson(path) {
        let reader = open_reader(path, true)?;
        let mut bson = BsonReader::new(std::io::BufReader::with_capacity(1024 * 1024, reader));
        while let Some(doc) = bson.next_document()? {
            if on_doc(doc)? {
                break;
            }
        }
    } else if path.is_dir() {
        let coll = resolve_collection(path, &spec.collection)?;
        let reader = open_reader(&coll.bson_path, true)?;
        let mut bson = BsonReader::new(std::io::BufReader::with_capacity(1024 * 1024, reader));
        while let Some(doc) = bson.next_document()? {
            if on_doc(doc)? {
                break;
            }
        }
    } else {
        bail!("unsupported mongodump path: {}", path.display());
    }

    let matched = rows.len() as u64;
    Ok(QueryResultReport {
        columns,
        rows,
        rows_scanned: scanned,
        rows_matched: matched,
    })
}

fn resolve_archive_ns(path: &Path, collection: &str) -> Result<String> {
    if collection.contains('.') {
        return Ok(collection.to_string());
    }
    let names = crate::db::mongo::archive::collection_names_from_archive(path)?;
    names
        .into_iter()
        .find(|n| n.ends_with(&format!(".{collection}")) || n == collection)
        .ok_or_else(|| anyhow::anyhow!("collection '{collection}' not found in archive"))
}

fn flatten_doc(doc: &Document, projection: Option<&BTreeMap<String, i32>>) -> BTreeMap<String, String> {
    let json = crate::db::mongo::archive::doc_to_json_lines(doc);
    let v: Value = serde_json::from_str(&json).unwrap_or(Value::Null);
    let mut out = BTreeMap::new();
    if let Value::Object(map) = v {
        for (k, val) in map {
            if let Some(proj) = projection {
                if proj.get(&k).is_some_and(|&n| n == 0) {
                    continue;
                }
            }
            out.insert(k, json_scalar(&val));
        }
    }
    out
}

fn json_scalar(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_filter_only() {
        let s = parse_mongo_query(r#"{"status":"failed"}"#, Some("users")).unwrap();
        assert_eq!(s.collection, "users");
    }
}
