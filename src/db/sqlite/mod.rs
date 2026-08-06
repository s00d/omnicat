//! SQLite file support under `omnicat db`.

use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::db::dispatch::run_sql_query;
use crate::db::options::DbOptions;
use crate::db::report::{DbReport, MysqlDumpStats, TableInfo, TableSchema};
use crate::inspect::query::build_query;
use crate::detect::HandlerKind;

pub fn run(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if let Some(sql) = &opts.query {
        return run_sqlite_query(path, opts, sql);
    }
    if opts.find.is_some() {
        bail!("SQLite sources do not support --find; use --query");
    }
    if opts.sample.is_some() {
        bail!("SQLite sources do not support --sample");
    }
    if opts.schema {
        let mut tables = list_schemas(path)?;
        if let Some(t) = &opts.table {
            tables.retain(|ts| ts.name.eq_ignore_ascii_case(t));
        }
        return Ok(DbReport::Schema { tables });
    }
    if opts.tables || opts.stats {
        let mut tables = list_tables(path)?;
        if let Some(t) = &opts.table {
            tables.retain(|ti| ti.name.eq_ignore_ascii_case(t));
        }
        if opts.stats {
            return Ok(DbReport::StatsMysqlDump(MysqlDumpStats {
                path: path.display().to_string(),
                bytes_scanned: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
                tables,
            }));
        }
        return Ok(DbReport::Tables { tables });
    }
    Ok(DbReport::OverviewSqlite(crate::db::report::SqliteOverview {
        path: path.display().to_string(),
        tables: list_tables(path)?,
    }))
}

fn run_sqlite_query(path: &Path, opts: &DbOptions, sql: &str) -> Result<DbReport> {
    let config = OmnicatConfig::default();
    let display = path.display().to_string();
    run_sql_query(path, opts, sql, |_, s, _| {
        let report = build_query(path, &display, HandlerKind::Database, s, &config)?;
        let rows = report.rows.unwrap_or_default();
        let headers = report.headers.unwrap_or_default();
        Ok((
            crate::db::report::QueryResultReport {
                columns: headers,
                rows: rows.clone(),
                rows_scanned: rows.len() as u64,
                rows_matched: rows.len() as u64,
            },
            Vec::new(),
        ))
    })
}

fn open_readonly(path: &Path) -> Result<Connection> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
    Ok(Connection::open_with_flags(path, flags)?)
}

fn list_tables(path: &Path) -> Result<Vec<TableInfo>> {
    let conn = open_readonly(path)?;
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    let mut out = Vec::new();
    for name in names {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{name}\""), [], |r| r.get(0))
            .unwrap_or(0);
        out.push(TableInfo {
            name,
            rows: count.max(0) as u64,
            bytes: 0,
        });
    }
    Ok(out)
}

fn list_schemas(path: &Path) -> Result<Vec<TableSchema>> {
    let conn = open_readonly(path)?;
    let mut stmt = conn.prepare(
        "SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for r in rows {
        let (name, sql) = r?;
        let columns = parse_create_table_columns(&sql);
        out.push(TableSchema {
            name,
            columns,
            indexes: Vec::new(),
        });
    }
    Ok(out)
}

fn parse_create_table_columns(sql: &str) -> Vec<crate::db::report::ColumnInfo> {
    let upper = sql.to_ascii_uppercase();
    let Some(start) = upper.find('(') else {
        return Vec::new();
    };
    let Some(end) = sql.rfind(')') else {
        return Vec::new();
    };
    let body = &sql[start + 1..end];
    body.split(',')
        .filter_map(|part| {
            let p = part.trim();
            if p.is_empty()
                || p.starts_with("PRIMARY KEY")
                || p.starts_with("UNIQUE")
                || p.starts_with("FOREIGN KEY")
                || p.starts_with("CONSTRAINT")
            {
                return None;
            }
            let mut words = p.split_whitespace();
            let name = words.next()?.trim_matches('"').trim_matches('`').to_string();
            let type_name = words.collect::<Vec<_>>().join(" ");
            Some(crate::db::report::ColumnInfo {
                name,
                type_name,
                nullable: !p.to_ascii_uppercase().contains("NOT NULL"),
            })
        })
        .collect()
}
