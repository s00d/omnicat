//! mongoexport JSON / JSONL files.

use std::io::BufRead;
use std::path::Path;

use anyhow::Result;

use crate::db::mongo::filter::doc_matches_filter;
use crate::db::mongo::query::parse_mongo_query;
use crate::db::options::DbOptions;
use crate::db::report::{DbReport, MongoDumpOverview, TableInfo};
use crate::io::source::open_reader;

pub fn run(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if let Some(q) = &opts.query {
        return run_jsonl_query(path, opts, q);
    }
    if opts.find.is_some() {
        return Ok(DbReport::Find {
            matches: find_in_jsonl(path, opts.find.as_deref().unwrap(), 100)?,
        });
    }
    if opts.schema || opts.tables || opts.stats {
        let info = TableInfo {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export")
                .to_string(),
            rows: count_lines(path)?,
            bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        };
        if opts.tables || opts.stats {
            return Ok(DbReport::Tables {
                tables: vec![info],
            });
        }
    }
    Ok(DbReport::OverviewMongoDump(MongoDumpOverview {
        path: path.display().to_string(),
        collections: 1,
        documents: count_lines(path)?,
        bytes_scanned: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        largest: vec![TableInfo {
            name: "export".into(),
            rows: count_lines(path)?,
            bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
        }],
    }))
}

fn run_jsonl_query(path: &Path, opts: &DbOptions, raw: &str) -> Result<DbReport> {
    let spec = parse_mongo_query(raw, opts.table.as_deref())?;
    let limit = spec.limit.unwrap_or(usize::MAX);
    let mut rows = Vec::new();
    let mut columns = Vec::new();
    let mut scanned = 0u64;
    let reader = open_reader(path, true)?;
    for line in std::io::BufReader::new(reader).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        scanned += 1;
        let v: serde_json::Value = serde_json::from_str(&line)?;
        let doc: bson::Document = bson::from_bson(bson::to_bson(&v)?)?;
        if !doc_matches_filter(&doc, &spec.filter) {
            continue;
        }
        if rows.len() >= limit {
            break;
        }
        if let serde_json::Value::Object(map) = v {
            for k in map.keys() {
                if !columns.contains(k) {
                    columns.push(k.clone());
                }
            }
            let row: Vec<String> = columns
                .iter()
                .map(|c| map.get(c).map(|x| x.to_string()).unwrap_or_else(|| "NULL".into()))
                .collect();
            rows.push(row);
        }
    }
    let report = crate::db::report::QueryResultReport {
        columns,
        rows: rows.clone(),
        rows_scanned: scanned,
        rows_matched: rows.len() as u64,
    };
    if opts.print_query {
        crate::db::query::print_query_from_results(
            crate::db::query::QueryDialect::Mongo,
            &spec.collection,
            &report,
        )?;
        return Ok(DbReport::Text { lines: Vec::new() });
    }
    let mut out = std::io::stdout().lock();
    crate::db::query::write_query_output(&report, opts.output, &mut out)?;
    Ok(DbReport::Text { lines: Vec::new() })
}

fn count_lines(path: &Path) -> Result<u64> {
    let reader = open_reader(path, true)?;
    Ok(std::io::BufReader::new(reader)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .count() as u64)
}

fn find_in_jsonl(path: &Path, pat: &str, limit: usize) -> Result<Vec<String>> {
    let reader = open_reader(path, true)?;
    let mut out = Vec::new();
    for line in std::io::BufReader::new(reader).lines() {
        let line = line?;
        if line.contains(pat) && out.len() < limit {
            out.push(line.chars().take(200).collect());
        }
    }
    Ok(out)
}
