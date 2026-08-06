//! MongoDB dump overview and collection stats.

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::db::mongo::archive::{collection_names_from_archive, stream_archive};
use crate::db::mongo::bson_stream::BsonReader;
use crate::db::mongo::detect::{is_archive, list_collections, CollectionRef};
use crate::db::report::{MongoDumpOverview, TableInfo};
use crate::io::source::open_reader;

pub fn dump_overview(path: &Path) -> Result<MongoDumpOverview> {
    if is_archive(path) {
        let names = collection_names_from_archive(path)?;
        let tables: Vec<TableInfo> = names
            .into_iter()
            .map(|n| TableInfo {
                name: n,
                rows: 0,
                bytes: 0,
            })
            .collect();
        return Ok(MongoDumpOverview {
            path: path.display().to_string(),
            collections: tables.len() as u64,
            documents: 0,
            bytes_scanned: fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            largest: tables,
        });
    }
    let collections = list_collections(path)?;
    let mut largest = Vec::new();
    let mut total_docs = 0u64;
    let mut total_bytes = 0u64;
    for c in &collections {
        let (docs, bytes) = count_bson_file(&c.bson_path)?;
        total_docs += docs;
        total_bytes += bytes;
        largest.push(TableInfo {
            name: c.full_name(),
            rows: docs,
            bytes,
        });
    }
    largest.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    largest.truncate(20);
    Ok(MongoDumpOverview {
        path: path.display().to_string(),
        collections: collections.len() as u64,
        documents: total_docs,
        bytes_scanned: total_bytes,
        largest,
    })
}

pub fn dump_tables(path: &Path) -> Result<Vec<TableInfo>> {
    if is_archive(path) {
        return Ok(collection_names_from_archive(path)?
            .into_iter()
            .map(|n| TableInfo {
                name: n,
                rows: 0,
                bytes: 0,
            })
            .collect());
    }
    let mut out = Vec::new();
    for c in list_collections(path)? {
        let (docs, bytes) = count_bson_file(&c.bson_path)?;
        out.push(TableInfo {
            name: c.full_name(),
            rows: docs,
            bytes,
        });
    }
    out.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    Ok(out)
}

pub fn dump_stats(path: &Path) -> Result<MongoDumpOverview> {
    dump_overview(path)
}

fn count_bson_file(path: &Path) -> Result<(u64, u64)> {
    let reader = open_reader(path, true)?;
    let mut bson = BsonReader::new(std::io::BufReader::with_capacity(256 * 1024, reader));
    let mut docs = 0u64;
    while bson.next_document()?.is_some() {
        docs += 1;
    }
    Ok((docs, bson.bytes_read()))
}

pub fn dump_find(path: &Path, pattern: &str, limit: usize) -> Result<Vec<String>> {
    let mut matches = Vec::new();
    if is_archive(path) {
        for entry in stream_archive(path)? {
            let entry = entry?;
            if entry.document.contains_key("metadata") {
                continue;
            }
            let line = crate::db::mongo::archive::doc_to_json_lines(&entry.document);
            if line.contains(pattern) && matches.len() < limit {
                matches.push(line.chars().take(200).collect());
            }
        }
        return Ok(matches);
    }
    for c in list_collections(path)? {
        scan_bson_find(&c, pattern, limit, &mut matches)?;
        if matches.len() >= limit {
            break;
        }
    }
    Ok(matches)
}

fn scan_bson_find(
    coll: &CollectionRef,
    pattern: &str,
    limit: usize,
    matches: &mut Vec<String>,
) -> Result<()> {
    let reader = open_reader(&coll.bson_path, true)?;
    let mut bson = BsonReader::new(std::io::BufReader::with_capacity(256 * 1024, reader));
    while let Some(doc) = bson.next_document()? {
        let line = crate::db::mongo::archive::doc_to_json_lines(&doc);
        if line.contains(pattern) && matches.len() < limit {
            matches.push(line.chars().take(200).collect());
        }
    }
    Ok(())
}

pub fn sample_collection(path: &Path, coll_name: Option<&str>, n: usize) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if is_archive(path) {
        for entry in stream_archive(path)? {
            let entry = entry?;
            if entry.document.contains_key("metadata") {
                continue;
            }
            if let Some(name) = coll_name {
                let ns = entry.namespace.as_deref().unwrap_or("");
                if !ns.is_empty() && !ns.ends_with(name) && ns != name {
                    continue;
                }
            }
            out.push(crate::db::mongo::archive::doc_to_json_lines(&entry.document));
            if out.len() >= n {
                break;
            }
        }
        return Ok(out);
    }
    for c in list_collections(path)? {
        if let Some(name) = coll_name {
            if !c.name.eq_ignore_ascii_case(name) && !c.full_name().eq_ignore_ascii_case(name) {
                continue;
            }
        }
        let reader = open_reader(&c.bson_path, true)?;
        let mut bson = BsonReader::new(std::io::BufReader::with_capacity(256 * 1024, reader));
        while let Some(doc) = bson.next_document()? {
            out.push(crate::db::mongo::archive::doc_to_json_lines(&doc));
            if out.len() >= n {
                return Ok(out);
            }
        }
    }
    Ok(out)
}
