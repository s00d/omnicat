//! Human and JSON output for `omnicat db` reports.

use std::io::Write;

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::db::report::{DbReport, QueryResultReport};

pub fn write_db_report(report: &DbReport, json: bool, out: &mut dyn Write) -> Result<()> {
    if matches!(report, DbReport::Text { lines } if lines.is_empty()) {
        return Ok(());
    }
    if json {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    match report {
        DbReport::OverviewMysqlDump(o) => {
            writeln!(out, "MySQL dump: {}", o.path)?;
            writeln!(out, "{}", "─".repeat(40))?;
            writeln!(out, "{:<16} {}", "Tables", o.tables)?;
            writeln!(out, "{:<16} {}", "INSERT stmts", o.inserts)?;
            writeln!(out, "{:<16} {}", "Bytes scanned", o.bytes_scanned)?;
            if !o.largest.is_empty() {
                writeln!(out)?;
                writeln!(out, "Largest tables (by rows):")?;
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(["Table", "Rows", "Bytes"]);
                for ti in &o.largest {
                    t.add_row([&ti.name, &ti.rows.to_string(), &ti.bytes.to_string()]);
                }
                write!(out, "{t}")?;
            }
        }
        DbReport::StatsMysqlDump(s) => {
            writeln!(out, "MySQL dump stats: {}", s.path)?;
            writeln!(out, "{}", "─".repeat(40))?;
            writeln!(out, "{:<16} {}", "Bytes scanned", s.bytes_scanned)?;
            if !s.tables.is_empty() {
                writeln!(out)?;
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(["Table", "Rows", "Bytes"]);
                for ti in &s.tables {
                    t.add_row([&ti.name, &ti.rows.to_string(), &ti.bytes.to_string()]);
                }
                write!(out, "{t}")?;
            }
        }
        DbReport::OverviewPostgresDump(o) => {
            writeln!(out, "PostgreSQL dump: {}", o.path)?;
            writeln!(out, "Format: {}", o.format)?;
            for w in &o.warnings {
                writeln!(out, "WARNING: {w}")?;
            }
            if !o.databases.is_empty() {
                writeln!(out)?;
                writeln!(out, "Databases:")?;
                for db in &o.databases {
                    writeln!(out, "  {db}")?;
                }
            }
            if !o.tables.is_empty() {
                writeln!(out)?;
                writeln!(out, "Tables:")?;
                for (db, tbl) in &o.tables {
                    writeln!(out, "  {db}.{tbl}")?;
                }
            }
        }
        DbReport::OverviewMongoDump(o) | DbReport::StatsMongoDump(o) => {
            let title = if matches!(report, DbReport::StatsMongoDump(_)) {
                "MongoDB dump stats"
            } else {
                "MongoDB dump"
            };
            writeln!(out, "{title}: {}", o.path)?;
            writeln!(out, "{}", "─".repeat(40))?;
            writeln!(out, "{:<16} {}", "Collections", o.collections)?;
            writeln!(out, "{:<16} {}", "Documents", o.documents)?;
            writeln!(out, "{:<16} {}", "Bytes scanned", o.bytes_scanned)?;
            if !o.largest.is_empty() {
                writeln!(out)?;
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(["Collection", "Docs", "Bytes"]);
                for ti in &o.largest {
                    t.add_row([&ti.name, &ti.rows.to_string(), &ti.bytes.to_string()]);
                }
                write!(out, "{t}")?;
            }
        }
        DbReport::OverviewMongoDatadir(o) => {
            writeln!(out, "MongoDB datadir: {}", o.path)?;
            for w in &o.warnings {
                writeln!(out, "WARNING: {w}")?;
            }
        }
        DbReport::OverviewSqlite(o) => {
            writeln!(out, "SQLite: {}", o.path)?;
            if !o.tables.is_empty() {
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(["Table", "Rows", "Bytes"]);
                for ti in &o.tables {
                    t.add_row([&ti.name, &ti.rows.to_string(), &ti.bytes.to_string()]);
                }
                write!(out, "{t}")?;
            }
        }
        DbReport::OverviewDynamoDb(o) => {
            writeln!(out, "DynamoDB export: {}", o.path)?;
            writeln!(out, "Format: {}", o.format)?;
            for w in &o.warnings {
                writeln!(out, "WARNING: {w}")?;
            }
            for t in &o.tables {
                writeln!(out, "  {t}")?;
            }
        }
        DbReport::OverviewElasticsearch(o) => {
            writeln!(out, "Elasticsearch snapshot: {}", o.path)?;
            for w in &o.warnings {
                writeln!(out, "WARNING: {w}")?;
            }
            for idx in &o.indices {
                writeln!(out, "  {idx}")?;
            }
        }
        DbReport::OverviewMysqlDatadir(o) => {
            writeln!(out, "MySQL datadir: {}", o.path)?;
            writeln!(out, "{}", "─".repeat(40))?;
            for w in &o.warnings {
                writeln!(out, "WARNING: {w}")?;
            }
            if !o.ibdata_files.is_empty() {
                writeln!(out)?;
                writeln!(out, "InnoDB system / redo:")?;
                for (n, sz) in &o.ibdata_files {
                    writeln!(out, "  {n}  {sz} bytes")?;
                }
            }
            if !o.tablespaces.is_empty() {
                writeln!(out)?;
                writeln!(out, "Tablespaces (.ibd):")?;
                for (n, sz) in &o.tablespaces {
                    writeln!(out, "  {n}  {sz} bytes")?;
                }
            }
        }
        DbReport::Tables { tables } => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(["Table", "Rows", "Bytes"]);
            for ti in tables {
                t.add_row([&ti.name, &ti.rows.to_string(), &ti.bytes.to_string()]);
            }
            write!(out, "{t}")?;
        }
        DbReport::Schema { tables } => {
            for ts in tables {
                writeln!(out, "Table: {}", ts.name)?;
                let mut t = Table::new();
                t.load_preset(UTF8_FULL);
                t.set_header(["Column", "Type", "Nullable"]);
                for c in &ts.columns {
                    let nullable = if c.nullable { "yes" } else { "no" };
                    t.add_row([&c.name, &c.type_name, &nullable.to_string()]);
                }
                write!(out, "{t}")?;
                if !ts.indexes.is_empty() {
                    writeln!(out, "Indexes:")?;
                    let mut it = Table::new();
                    it.load_preset(UTF8_FULL);
                    it.set_header(["Name", "Kind", "Columns", "Unique"]);
                    for idx in &ts.indexes {
                        let unique = if idx.unique { "yes" } else { "no" };
                        it.add_row([
                            &idx.name,
                            &idx.kind,
                            &idx.columns.join(", "),
                            &unique.to_string(),
                        ]);
                    }
                    write!(out, "{it}")?;
                }
                writeln!(out)?;
            }
        }
        DbReport::RedisRdb(s) => {
            writeln!(out, "Redis RDB: {}", s.path)?;
            if let Some(v) = &s.version {
                writeln!(out, "Redis version: {v}")?;
            }
            writeln!(out, "Keys: {}", s.keys)?;
            writeln!(out, "Memory estimate: {} bytes", s.memory_estimate)?;
            if !s.types.is_empty() {
                writeln!(out)?;
                writeln!(out, "By type:")?;
                for (k, v) in &s.types {
                    writeln!(out, "  {k}: {v}")?;
                }
            }
            if !s.patterns.is_empty() {
                writeln!(out)?;
                writeln!(out, "Key patterns:")?;
                for (k, v) in &s.patterns {
                    writeln!(out, "  {k}: {v}")?;
                }
            }
        }
        DbReport::RedisAof(s) => {
            writeln!(out, "Redis AOF: {}", s.path)?;
            writeln!(out, "Commands: {}", s.commands)?;
            if !s.by_command.is_empty() {
                writeln!(out)?;
                writeln!(out, "By command:")?;
                for (k, v) in &s.by_command {
                    writeln!(out, "  {k}: {v}")?;
                }
            }
        }
        DbReport::Samples { items } => {
            let mut t = Table::new();
            t.load_preset(UTF8_FULL);
            t.set_header(["Key", "Kind", "Size"]);
            for i in items {
                t.add_row([&i.key, &i.kind, &i.size.to_string()]);
            }
            write!(out, "{t}")?;
        }
        DbReport::Find { matches } => {
            for m in matches {
                writeln!(out, "{m}")?;
            }
        }
        DbReport::Top { field, items } => {
            writeln!(out, "Top {field}:")?;
            for (k, v) in items {
                writeln!(out, "  {k}: {v}")?;
            }
        }
        DbReport::Query(_) => {}
        DbReport::Text { lines } => {
            for line in lines {
                writeln!(out, "{line}")?;
            }
        }
    }
    Ok(())
}

pub fn write_query_table(report: &QueryResultReport, out: &mut dyn Write) -> Result<()> {
    let mut t = Table::new();
    t.load_preset(UTF8_FULL);
    t.set_header(
        report
            .columns
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    for row in &report.rows {
        t.add_row(row.iter().map(String::as_str).collect::<Vec<_>>());
    }
    write!(out, "{t}")?;
    Ok(())
}
