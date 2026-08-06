//! Resolve mongodump paths and collection files.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::db::detect::{extension_chain, looks_like_mongodump_dir};

#[derive(Debug, Clone)]
pub struct CollectionRef {
    pub db: String,
    pub name: String,
    pub bson_path: PathBuf,
    pub metadata_path: Option<PathBuf>,
}

pub fn is_archive(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".archive") || extension_chain(path).ends_with(".archive.gz")
}

pub fn is_single_bson(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("bson"))
}

pub fn list_collections(root: &Path) -> Result<Vec<CollectionRef>> {
    if is_archive(root) {
        return Ok(vec![]);
    }
    if is_single_bson(root) {
        let name = root
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("collection")
            .to_string();
        return Ok(vec![CollectionRef {
            db: "default".into(),
            name,
            bson_path: root.to_path_buf(),
            metadata_path: sibling_metadata(root),
        }]);
    }
    if !root.is_dir() {
        bail!("not a mongodump directory: {}", root.display());
    }
    let mut out = Vec::new();
    collect_from_dir(root, root, &mut out)?;
    out.sort_by_key(|a| a.full_name());
    Ok(out)
}

impl CollectionRef {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.db, self.name)
    }
}

fn collect_from_dir(root: &Path, dir: &Path, out: &mut Vec<CollectionRef>) -> Result<()> {
    for ent in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let ent = ent?;
        let p = ent.path();
        if p.is_dir() {
            collect_from_dir(root, &p, out)?;
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".bson") {
            continue;
        }
        let coll = name.trim_end_matches(".bson");
        let db = relative_db_name(root, p.parent().unwrap_or(dir));
        out.push(CollectionRef {
            db,
            name: coll.to_string(),
            bson_path: p.clone(),
            metadata_path: sibling_metadata(&p),
        });
    }
    Ok(())
}

fn relative_db_name(root: &Path, parent: &Path) -> String {
    parent
        .strip_prefix(root)
        .ok()
        .and_then(|rel| rel.components().next())
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("default")
        .to_string()
}

fn sibling_metadata(bson: &Path) -> Option<PathBuf> {
    let stem = bson.file_stem()?.to_str()?;
    let meta = bson.with_file_name(format!("{stem}.metadata.json"));
    meta.is_file().then_some(meta)
}

pub fn resolve_collection(root: &Path, table: &str) -> Result<CollectionRef> {
    let collections = list_collections(root)?;
    if collections.is_empty() && is_archive(root) {
        bail!("use archive query path for {}", root.display());
    }
    let table = table.trim();
    let found = collections
        .iter()
        .find(|c| {
            c.name.eq_ignore_ascii_case(table)
                || c.full_name().eq_ignore_ascii_case(table)
                || format!("{}.{}", c.db, c.name).eq_ignore_ascii_case(table)
        })
        .cloned();
    found.ok_or_else(|| anyhow::anyhow!("collection '{table}' not found in dump"))
}

pub fn looks_like_dump(path: &Path) -> bool {
    path.is_dir() && looks_like_mongodump_dir(path)
}
