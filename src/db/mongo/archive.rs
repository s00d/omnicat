//! mongodump archive streaming (BSON length-prefixed stream).

use std::path::Path;

use anyhow::{Context, Result};
use bson::{Bson, Document};

use crate::db::mongo::bson_stream::BsonReader;
use crate::io::source::open_reader;

#[derive(Debug, Clone)]
pub struct ArchiveDoc {
    pub namespace: Option<String>,
    pub document: Document,
}

pub fn stream_archive(path: &Path) -> Result<impl Iterator<Item = Result<ArchiveDoc>>> {
    let reader = open_reader(path, true).context("open archive")?;
    let mut bson = BsonReader::new(std::io::BufReader::with_capacity(1024 * 1024, reader));
    Ok(std::iter::from_fn(move || match read_archive_entry(&mut bson) {
        Ok(Some(v)) => Some(Ok(v)),
        Ok(None) => None,
        Err(e) => Some(Err(e)),
    }))
}

fn read_archive_entry<R: std::io::Read>(
    reader: &mut BsonReader<R>,
) -> Result<Option<ArchiveDoc>> {
    let Some(doc) = reader.next_document()? else {
        return Ok(None);
    };
    let namespace = doc.get_str("ns").ok().map(str::to_string).or_else(|| {
        doc.get_document("metadata")
            .ok()
            .and_then(|m| m.get_str("ns").ok())
            .map(str::to_string)
    });
    if doc.contains_key("metadata") {
        return Ok(Some(ArchiveDoc {
            namespace,
            document: doc,
        }));
    }
    Ok(Some(ArchiveDoc {
        namespace,
        document: doc,
    }))
}

pub fn stream_collection_from_archive(
    path: &Path,
    target_ns: &str,
) -> Result<impl Iterator<Item = Result<Document>>> {
    let target = target_ns.to_string();
    let iter = stream_archive(path)?;
    Ok(iter.filter_map(move |item| {
        item.ok().and_then(|entry| {
            if entry.document.contains_key("metadata") {
                return None;
            }
            let ns = entry.namespace.as_deref().unwrap_or("");
            if !ns.is_empty() && !ns_match(ns, &target) {
                return None;
            }
            Some(Ok(entry.document))
        })
    }))
}

fn ns_match(ns: &str, target: &str) -> bool {
    ns.eq_ignore_ascii_case(target)
        || ns.ends_with(&format!(".{target}"))
        || target.ends_with(&format!(".{ns}"))
}

pub fn collection_names_from_archive(path: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in stream_archive(path)? {
        let entry = entry?;
        if let Some(ns) = entry.namespace {
            if !names.iter().any(|n: &String| n == &ns) {
                names.push(ns);
            }
        }
    }
    names.sort();
    Ok(names)
}

pub fn doc_to_json_lines(doc: &Document) -> String {
    bson_to_json_value(&Bson::Document(doc.clone())).to_string()
}

fn bson_to_json_value(b: &Bson) -> serde_json::Value {
    match b {
        Bson::Null => serde_json::Value::Null,
        Bson::Boolean(v) => serde_json::Value::Bool(*v),
        Bson::Int32(v) => serde_json::json!(v),
        Bson::Int64(v) => serde_json::json!(v),
        Bson::Double(v) => serde_json::json!(v),
        Bson::String(v) => serde_json::Value::String(v.clone()),
        Bson::Array(a) => {
            serde_json::Value::Array(a.iter().map(bson_to_json_value).collect())
        }
        Bson::Document(d) => {
            let mut map = serde_json::Map::new();
            for (k, v) in d {
                map.insert(k.clone(), bson_to_json_value(v));
            }
            serde_json::Value::Object(map)
        }
        Bson::ObjectId(oid) => serde_json::Value::String(oid.to_hex()),
        Bson::DateTime(dt) => serde_json::Value::String(dt.timestamp_millis().to_string()),
        other => serde_json::Value::String(format!("{other:?}")),
    }
}
