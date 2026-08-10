//! SQL dump / SQLite preview — overview info, not a full DB toolkit.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::content::{DatabaseContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;
use crate::io::source::open_reader;

pub struct DatabaseDriver;

impl PreviewDriver for DatabaseDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Database
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["sqlite", "db", "sqlite3", "sql"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["application/x-sqlite3"]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.ends_with(".sql")
            || name.ends_with(".sql.gz")
            || name.ends_with(".sql.zst")
            || name.ends_with(".sql.zstd")
            || name.ends_with(".sql.bz2")
            || name.ends_with(".sql.xz")
        {
            return Ok(PreviewContent::Markdown(sql_dump_overview(path, config)?));
        }
        sqlite_preview(path, config)
    }
}

fn sql_dump_overview(path: &Path, config: &OmnicatConfig) -> Result<String> {
    let max = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        8 * 1024 * 1024
    } else {
        config.inspect.max_bytes.max(256 * 1024)
    };
    let reader = open_reader(path, true).context("open sql dump")?;
    let mut buf = Vec::new();
    reader
        .take(max as u64)
        .read_to_end(&mut buf)
        .context("read sql dump")?;
    let text = String::from_utf8_lossy(&buf);

    let mut creates = BTreeSet::new();
    let mut inserts = BTreeSet::new();
    let mut create_stmts = 0u64;
    let mut insert_stmts = 0u64;
    let mut other_stmts = 0u64;

    for stmt in split_sql_statements(&text) {
        let upper = stmt.trim_start().to_ascii_uppercase();
        if upper.starts_with("CREATE TABLE") {
            create_stmts += 1;
            if let Some(name) = extract_table_name(stmt, "CREATE TABLE") {
                creates.insert(name);
            }
        } else if upper.starts_with("INSERT ") {
            insert_stmts += 1;
            if let Some(name) = extract_table_name(stmt, "INSERT") {
                inserts.insert(name);
            }
        } else if !stmt.trim().is_empty() {
            other_stmts += 1;
        }
    }

    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let truncated = buf.len() as u64 >= max as u64;

    let mut md = String::new();
    md.push_str("# SQL dump overview\n\n");
    md.push_str(&format!("- **file**: `{}`\n", path.display()));
    md.push_str(&format!("- **size**: {} bytes\n", size));
    if truncated {
        md.push_str(&format!("- **note**: scanned first {} bytes only\n", max));
    }
    md.push_str(&format!("- **CREATE TABLE**: {create_stmts}\n"));
    md.push_str(&format!("- **INSERT**: {insert_stmts}\n"));
    md.push_str(&format!("- **other statements**: {other_stmts}\n"));

    if !creates.is_empty() {
        md.push_str("\n## Tables (CREATE)\n\n");
        for t in &creates {
            md.push_str(&format!("- `{t}`\n"));
        }
    }
    let insert_only: Vec<_> = inserts.difference(&creates).cloned().collect();
    if !insert_only.is_empty() {
        md.push_str("\n## Tables seen in INSERT only\n\n");
        for t in insert_only {
            md.push_str(&format!("- `{t}`\n"));
        }
    }
    if creates.is_empty() && inserts.is_empty() {
        md.push_str("\n_No CREATE TABLE / INSERT detected — showing as dump metadata only._\n");
    }
    Ok(md)
}

fn split_sql_statements(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            ';' if !in_single && !in_double && !in_backtick => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn extract_table_name(stmt: &str, kind: &str) -> Option<String> {
    let trimmed = stmt.trim_start();
    let rest = if kind == "CREATE TABLE" {
        let upper = trimmed.to_ascii_uppercase();
        let idx = upper.find("CREATE TABLE")?;
        let after = trimmed[idx + "CREATE TABLE".len()..].trim_start();
        let after = after
            .strip_prefix("IF NOT EXISTS")
            .or_else(|| after.strip_prefix("if not exists"))
            .unwrap_or(after)
            .trim_start();
        after
    } else {
        let upper = trimmed.to_ascii_uppercase();
        let idx = upper.find("INTO")?;
        trimmed[idx + 4..].trim_start()
    };
    let token = rest.split_whitespace().next()?;
    let name = token
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches('(')
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn sqlite_preview(path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
    let conn = Connection::open(path).context("open sqlite")?;
    let mut schema = String::new();
    let mut stmt =
        conn.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        schema.push_str(&row?);
        schema.push_str(";\n\n");
    }

    let table_name: String = conn
        .query_row(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "sqlite_master".into());

    let max_rows = config.terminal.data.max_rows.min(50);
    let pragma = format!("PRAGMA table_info({table_name})");
    let mut headers = Vec::new();
    if let Ok(mut info) = conn.prepare(&pragma) {
        let cols = info.query_map([], |row| row.get::<_, String>(1))?;
        for c in cols {
            headers.push(c?);
        }
    }

    let query = format!("SELECT * FROM \"{table_name}\" LIMIT {max_rows}");
    let mut rows = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&query) {
        let col_count = stmt.column_count();
        if headers.is_empty() {
            for i in 0..col_count {
                headers.push(stmt.column_name(i).unwrap_or("?").to_string());
            }
        }
        let mapped = stmt.query_map([], |row| {
            let mut cells = Vec::new();
            for i in 0..col_count {
                let val: rusqlite::types::Value = row.get(i)?;
                cells.push(format_sqlite_value(val));
            }
            Ok(cells)
        })?;
        for r in mapped {
            rows.push(r?);
        }
    }

    Ok(PreviewContent::Database(DatabaseContent {
        schema,
        table_name,
        headers,
        rows,
    }))
}

fn format_sqlite_value(val: rusqlite::types::Value) -> String {
    match val {
        rusqlite::types::Value::Null => "NULL".into(),
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(s) => s,
        rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
    }
}
