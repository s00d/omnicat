use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InfoReport {
    pub path: String,
    pub handler: String,
    pub type_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fs: Option<FsInfo>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, serde_json::Value>,
}

/// Filesystem metadata shared by every `--info` report.
#[derive(Debug, Clone, Serialize)]
pub struct FsInfo {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed: Option<String>,
    pub readonly: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nlink: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeReport {
    pub path: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
    pub handler: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaField {
    pub name: String,
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaReport {
    pub path: String,
    pub handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<usize>,
    pub fields: Vec<SchemaField>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tables: Vec<SchemaTable>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaTable {
    pub name: String,
    pub fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindMatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindReport {
    pub path: String,
    pub query: String,
    pub matches: Vec<FindMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatsReport {
    pub path: String,
    pub handler: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub groups: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryReport {
    pub path: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<Vec<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub left: String,
    pub right: String,
    pub kind: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EncodingReport {
    pub path: String,
    pub encoding: String,
    pub bom: bool,
    pub line_endings: String,
    pub characters: usize,
    pub lines: usize,
    pub longest_line: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilitiesReport {
    pub path: Option<String>,
    pub handler: Option<String>,
    pub capabilities: Vec<CapabilityLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityLine {
    pub name: String,
    pub supported: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HashReport {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub size_human: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub md5: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha1: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha256: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sha512: String,
    pub blake3: String,
    pub truncated: bool,
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub entries: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size: u64,
    pub size_human: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicatesReport {
    pub path: String,
    pub scanned_files: u64,
    pub groups: Vec<DuplicateGroup>,
    pub reclaimable_bytes: u64,
    pub reclaimable_human: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "report", rename_all = "snake_case")]
pub enum InspectReport {
    Info(Box<InfoReport>),
    Type(TypeReport),
    Schema(SchemaReport),
    Find(FindReport),
    Stats(StatsReport),
    Query(QueryReport),
    Diff(DiffReport),
    Encoding(EncodingReport),
    Capabilities(CapabilitiesReport),
    Hash(HashReport),
    Duplicates(DuplicatesReport),
    Text {
        path: String,
        text: String,
    },
    Table {
        path: String,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        total_rows: usize,
        total_cols: usize,
        note: Option<String>,
    },
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}
