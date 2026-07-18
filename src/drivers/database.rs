use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::content::{DatabaseContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct DatabaseDriver;

impl PreviewDriver for DatabaseDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Database
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["sqlite", "db", "sqlite3"]
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
