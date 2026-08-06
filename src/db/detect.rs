//! Detect database backup / persistence source kind from a path.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    MysqlDump,
    MysqlDatadir,
    RedisRdb,
    RedisAof,
    PostgresDump,
    MongoDump,
    MongoDatadir,
    MongoExportJson,
    Sqlite,
    DynamoDbExport,
    ElasticsearchSnapshot,
}

#[derive(Debug, Clone)]
pub struct DbSource {
    pub path: PathBuf,
    pub kind: SourceKind,
}

pub fn detect_source(path: &Path) -> Result<DbSource> {
    if !path.exists() {
        bail!("path not found: {}", path.display());
    }
    let kind = if path.is_dir() {
        detect_dir_kind(path)?
    } else {
        detect_file_kind(path)?
    };
    Ok(DbSource {
        path: path.to_path_buf(),
        kind,
    })
}

fn detect_dir_kind(path: &Path) -> Result<SourceKind> {
    if looks_like_pg_dump_dir(path) {
        return Ok(SourceKind::PostgresDump);
    }
    if looks_like_mongo_wiredtiger(path) {
        return Ok(SourceKind::MongoDatadir);
    }
    if looks_like_mongodump_dir(path) {
        return Ok(SourceKind::MongoDump);
    }
    if looks_like_es_snapshot(path) {
        return Ok(SourceKind::ElasticsearchSnapshot);
    }
    if looks_like_dynamodb_export_dir(path) {
        return Ok(SourceKind::DynamoDbExport);
    }
    if looks_like_mysql_datadir(path) {
        return Ok(SourceKind::MysqlDatadir);
    }
    if looks_like_redis_aof_dir(path) {
        return Ok(SourceKind::RedisAof);
    }
    bail!(
        "directory is not a recognized database source: {}",
        path.display()
    )
}

fn detect_file_kind(path: &Path) -> Result<SourceKind> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let ext_chain = extension_chain(path);

    if sniff_sqlite(path)? {
        return Ok(SourceKind::Sqlite);
    }
    if sniff_pg_custom(path)? {
        return Ok(SourceKind::PostgresDump);
    }
    if name.ends_with(".archive") || ext_chain.ends_with(".archive.gz") {
        return Ok(SourceKind::MongoDump);
    }
    if name.ends_with(".bson") {
        return Ok(SourceKind::MongoDump);
    }
    if name.ends_with(".dump") || name.ends_with(".backup") {
        return Ok(SourceKind::PostgresDump);
    }
    if name.ends_with(".parquet") {
        return Ok(SourceKind::DynamoDbExport);
    }
    if is_sql_extension(&name, &ext_chain) {
        return Ok(SourceKind::MysqlDump);
    }
    if name.ends_with(".rdb") || name == "dump.rdb" {
        return Ok(SourceKind::RedisRdb);
    }
    if name.ends_with(".aof") || name == "appendonly.aof" || name.ends_with(".aof.manifest") {
        return Ok(SourceKind::RedisAof);
    }
    if path.extension().is_none() && sniff_sql_dump(path)? {
        return Ok(SourceKind::MysqlDump);
    }
    if sniff_mongoexport_json(path)? {
        return Ok(SourceKind::MongoExportJson);
    }
    if sniff_dynamodb_json(path)? {
        return Ok(SourceKind::DynamoDbExport);
    }
    bail!("unrecognized database source: {}", path.display())
}

fn is_sql_extension(name: &str, ext_chain: &str) -> bool {
    name.ends_with(".sql")
        || ext_chain.ends_with(".sql.gz")
        || ext_chain.ends_with(".sql.zst")
        || ext_chain.ends_with(".sql.zstd")
        || ext_chain.ends_with(".sql.bz2")
        || ext_chain.ends_with(".sql.xz")
        || name.ends_with(".sql.gz")
        || name.ends_with(".sql.zst")
        || name.ends_with(".sql.bz2")
        || name.ends_with(".sql.xz")
}

pub fn extension_chain(path: &Path) -> String {
    let mut parts = Vec::new();
    let mut cur = path;
    while let Some(ext) = cur.extension().and_then(|e| e.to_str()) {
        parts.push(ext.to_ascii_lowercase());
        cur = Path::new(cur.file_stem().unwrap_or_default());
    }
    parts.reverse();
    parts.join(".")
}

fn looks_like_pg_dump_dir(path: &Path) -> bool {
    path.join("toc.dat").is_file() && path.join("dat").is_dir()
}

fn looks_like_mongo_wiredtiger(path: &Path) -> bool {
    path.join("WiredTiger").is_file() && path.join("journal").is_dir()
}

pub fn looks_like_mongodump_dir(path: &Path) -> bool {
    let mut bson = 0u32;
    let mut meta = 0u32;
    walk_dir_files(path, 4, |p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with(".bson") {
            bson += 1;
        }
        if name.ends_with(".metadata.json") {
            meta += 1;
        }
    });
    bson > 0 && meta > 0
}

fn looks_like_es_snapshot(path: &Path) -> bool {
    path.join("index.latest").is_file()
        || path.join("meta-data").is_dir()
        || path.join("indices").is_dir()
}

fn looks_like_dynamodb_export_dir(path: &Path) -> bool {
    path.join("manifest-summary.json").is_file()
        || path.join("manifest-files.json").is_file()
        || path.join("manifest-summary.md5").is_file()
}

fn looks_like_mysql_datadir(path: &Path) -> bool {
    let mut ibd = 0u32;
    let mut ibdata = false;
    if let Ok(entries) = fs::read_dir(path) {
        for ent in entries.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("ibdata") {
                ibdata = true;
            }
            if lower.ends_with(".ibd") {
                ibd += 1;
            }
        }
    }
    ibdata || ibd >= 2
}

fn looks_like_redis_aof_dir(path: &Path) -> bool {
    path.join("appendonly.aof").is_file() || path.join("appendonly.aof.manifest").is_file()
}

fn walk_dir_files(path: &Path, max_depth: u32, mut f: impl FnMut(&Path)) {
    fn walk(path: &Path, depth: u32, max_depth: u32, f: &mut impl FnMut(&Path)) {
        if depth > max_depth {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for ent in entries.flatten() {
            let p = ent.path();
            if p.is_file() {
                f(&p);
            } else if p.is_dir() {
                walk(&p, depth + 1, max_depth, f);
            }
        }
    }
    walk(path, 0, max_depth, &mut f);
}

fn sniff_sqlite(path: &Path) -> Result<bool> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "db" | "sqlite" | "sqlite3" | "db3") {
        return Ok(false);
    }
    let mut buf = [0u8; 16];
    let n = read_sniff_head(path, &mut buf)?;
    Ok(n >= 15 && &buf[..15] == b"SQLite format 3")
}

fn sniff_sql_dump(path: &Path) -> Result<bool> {
    let mut buf = [0u8; 4096];
    let n = read_sniff_head(path, &mut buf)?;
    let head = String::from_utf8_lossy(&buf[..n]);
    Ok(head.contains("CREATE TABLE") || head.contains("INSERT INTO") || head.contains("mysqldump"))
}

fn sniff_pg_custom(path: &Path) -> Result<bool> {
    let mut buf = [0u8; 8];
    let n = read_sniff_head(path, &mut buf)?;
    Ok(n >= 5 && &buf[..5] == b"PGDMP")
}

fn sniff_mongoexport_json(path: &Path) -> Result<bool> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" && ext != "jsonl" {
        return Ok(false);
    }
    let mut buf = [0u8; 512];
    let n = read_sniff_head(path, &mut buf)?;
    let head = String::from_utf8_lossy(&buf[..n]);
    Ok(head.contains("\"_id\"") || head.contains("ObjectId(") || head.contains("ISODate("))
}

fn sniff_dynamodb_json(path: &Path) -> Result<bool> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" && ext != "jsonl" {
        return Ok(false);
    }
    let mut buf = [0u8; 512];
    let n = read_sniff_head(path, &mut buf)?;
    let head = String::from_utf8_lossy(&buf[..n]);
    Ok(head.contains("\"Item\"") || head.contains("AWSDynamoDB"))
}

fn read_sniff_head(path: &Path, buf: &mut [u8]) -> Result<usize> {
    use std::io::Read;
    let chain = extension_chain(path);
    if chain.ends_with(".gz") {
        let f = fs::File::open(path)?;
        let mut d = flate2::read::GzDecoder::new(f);
        Ok(d.read(buf)?)
    } else if chain.ends_with(".bz2") {
        let f = fs::File::open(path)?;
        let mut d = bzip2::read::BzDecoder::new(f);
        Ok(d.read(buf)?)
    } else if chain.ends_with(".xz") {
        let f = fs::File::open(path)?;
        let mut d = liblzma::read::XzDecoder::new(f);
        Ok(d.read(buf)?)
    } else {
        let mut f = fs::File::open(path)?;
        Ok(f.read(buf)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sql_by_extension() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("backup.sql");
        std::fs::write(&p, "CREATE TABLE t (id INT);").unwrap();
        assert_eq!(detect_source(&p).unwrap().kind, SourceKind::MysqlDump);
    }

    #[test]
    fn detects_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("app.db");
        let mut data = b"SQLite format 3\0".to_vec();
        data.extend(std::iter::repeat_n(0u8, 512));
        std::fs::write(&p, data).unwrap();
        assert_eq!(detect_source(&p).unwrap().kind, SourceKind::Sqlite);
    }

    #[test]
    fn detects_datadir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ibdata1"), b"x").unwrap();
        std::fs::write(dir.path().join("users.ibd"), b"x").unwrap();
        assert_eq!(
            detect_source(dir.path()).unwrap().kind,
            SourceKind::MysqlDatadir
        );
    }
}
