//! Overview, schema, and table listing for MySQL dumps.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::db::mysql::dump::reader::DumpReader;
use crate::db::mysql::dump::schema::{
    count_insert_rows, parse_create_table, parse_insert_table, schemas_from_create, TableDef,
};
use crate::db::report::{MysqlDumpOverview, MysqlDumpStats, TableInfo, TableSchema};

pub fn dump_overview(path: &Path) -> Result<MysqlDumpOverview> {
    let mut reader = DumpReader::open(path)?;
    let mut defs: BTreeMap<String, TableDef> = BTreeMap::new();
    let mut row_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut byte_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut inserts = 0u64;

    while let Some(stmt) = reader.next_statement()? {
        let stmt_bytes = stmt.len() as u64;
        if let Some(def) = parse_create_table(&stmt) {
            defs.insert(def.name.clone(), def);
            continue;
        }
        if let Some(table) = parse_insert_table(&stmt) {
            inserts += 1;
            *byte_counts.entry(table.clone()).or_default() += stmt_bytes;
            *row_counts.entry(table).or_default() += count_insert_rows(&stmt);
        }
    }

    let mut largest: Vec<TableInfo> = row_counts
        .iter()
        .map(|(name, rows)| TableInfo {
            name: name.clone(),
            rows: *rows,
            bytes: *byte_counts.get(name).unwrap_or(rows),
        })
        .collect();
    largest.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    largest.truncate(20);

    Ok(MysqlDumpOverview {
        path: path.display().to_string(),
        tables: defs.len() as u64,
        inserts,
        bytes_scanned: reader.bytes_read(),
        largest,
    })
}

pub fn dump_tables(path: &Path) -> Result<Vec<TableInfo>> {
    let mut reader = DumpReader::open(path)?;
    let mut row_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut byte_counts: BTreeMap<String, u64> = BTreeMap::new();

    while let Some(stmt) = reader.next_statement()? {
        let stmt_bytes = stmt.len() as u64;
        if let Some(table) = parse_insert_table(&stmt) {
            *byte_counts.entry(table.clone()).or_default() += stmt_bytes;
            *row_counts.entry(table).or_default() += count_insert_rows(&stmt);
        }
    }

    let mut all_tables: Vec<TableInfo> = row_counts
        .iter()
        .map(|(name, rows)| TableInfo {
            name: name.clone(),
            rows: *rows,
            bytes: *byte_counts.get(name).unwrap_or(rows),
        })
        .collect();
    all_tables.sort_by_key(|t| std::cmp::Reverse(t.bytes));
    Ok(all_tables)
}

pub fn dump_stats(path: &Path) -> Result<MysqlDumpStats> {
    let mut reader = DumpReader::open(path)?;
    let mut row_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut byte_counts: BTreeMap<String, u64> = BTreeMap::new();

    while let Some(stmt) = reader.next_statement()? {
        let stmt_bytes = stmt.len() as u64;
        if let Some(table) = parse_insert_table(&stmt) {
            *byte_counts.entry(table.clone()).or_default() += stmt_bytes;
            *row_counts.entry(table).or_default() += count_insert_rows(&stmt);
        }
    }

    let mut tables: Vec<TableInfo> = row_counts
        .iter()
        .map(|(name, rows)| TableInfo {
            name: name.clone(),
            rows: *rows,
            bytes: *byte_counts.get(name).unwrap_or(rows),
        })
        .collect();
    tables.sort_by_key(|t| std::cmp::Reverse(t.bytes));

    Ok(MysqlDumpStats {
        path: path.display().to_string(),
        bytes_scanned: reader.bytes_read(),
        tables,
    })
}

pub fn dump_find(path: &Path, pattern: &str, limit: usize) -> Result<Vec<String>> {
    let mut reader = DumpReader::open(path)?;
    let mut matches = Vec::new();
    while let Some(stmt) = reader.next_statement()? {
        if stmt.contains(pattern) && matches.len() < limit {
            let preview: String = stmt.chars().take(200).collect();
            matches.push(preview);
        }
    }
    Ok(matches)
}

pub fn dump_schema(path: &Path) -> Result<Vec<TableSchema>> {
    let mut reader = DumpReader::open(path)?;
    let mut defs: BTreeMap<String, TableDef> = BTreeMap::new();
    while let Some(stmt) = reader.next_statement()? {
        if let Some(def) = parse_create_table(&stmt) {
            defs.insert(def.name.clone(), def);
            continue;
        }
        if parse_insert_table(&stmt).is_some() {
            break;
        }
    }
    Ok(schemas_from_create(&defs))
}
