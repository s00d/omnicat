//! SQL dump / SQLite preview — overview with structure and sample rows.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::content::{DatabaseContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::sql_dump::{
    is_sql_dump_path, parse_create_table, parse_insert, split_sql_statements, DumpColumn,
};
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
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        if is_sql_dump_path(path) {
            return Ok(PreviewContent::Markdown(sql_dump_preview(
                path,
                config,
                ctx.sql_schema_only,
                ctx.sql_table.as_deref(),
            )?));
        }
        sqlite_preview(path, config)
    }
}

#[derive(Debug, Clone, Default)]
struct DumpTable {
    name: String,
    columns: Vec<DumpColumn>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
struct DumpModel {
    tables: BTreeMap<String, DumpTable>,
    create_stmts: u64,
    insert_stmts: u64,
    other_stmts: u64,
    truncated: bool,
    scan_max: usize,
}

fn sql_dump_preview(
    path: &Path,
    config: &OmnicatConfig,
    schema_only: bool,
    table_filter: Option<&str>,
) -> Result<String> {
    let model = parse_sql_dump(path, config)?;
    render_sql_dump(path, &model, config, schema_only, table_filter)
}

fn parse_sql_dump(path: &Path, config: &OmnicatConfig) -> Result<DumpModel> {
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

    let mut model = DumpModel {
        truncated: buf.len() as u64 >= max as u64,
        scan_max: max,
        ..DumpModel::default()
    };

    for stmt in split_sql_statements(&text) {
        let upper = stmt.trim_start().to_ascii_uppercase();
        if upper.starts_with("CREATE TABLE") {
            model.create_stmts += 1;
            if let Some((name, columns)) = parse_create_table(stmt) {
                let entry = model
                    .tables
                    .entry(name.clone())
                    .or_insert_with(|| DumpTable {
                        name: name.clone(),
                        ..DumpTable::default()
                    });
                entry.name = name;
                entry.columns = columns;
            }
        } else if upper.starts_with("INSERT ") {
            model.insert_stmts += 1;
            if let Some((name, col_names, tuples)) = parse_insert(stmt) {
                let entry = model
                    .tables
                    .entry(name.clone())
                    .or_insert_with(|| DumpTable {
                        name: name.clone(),
                        ..DumpTable::default()
                    });
                if entry.columns.is_empty() {
                    if let Some(cols) = col_names {
                        entry.columns = cols
                            .into_iter()
                            .map(|n| DumpColumn {
                                name: n,
                                type_name: String::new(),
                                notes: String::new(),
                            })
                            .collect();
                    } else if let Some(first) = tuples.first() {
                        entry.columns = (0..first.len())
                            .map(|i| DumpColumn {
                                name: format!("col_{}", i + 1),
                                type_name: String::new(),
                                notes: String::new(),
                            })
                            .collect();
                    }
                }
                let col_count = entry.columns.len();
                for mut tuple in tuples {
                    if col_count > 0 && tuple.len() != col_count {
                        tuple.resize(col_count, String::new());
                    }
                    entry.rows.push(tuple);
                }
            }
        } else if !stmt.trim().is_empty() {
            model.other_stmts += 1;
        }
    }

    Ok(model)
}

fn render_sql_dump(
    path: &Path,
    model: &DumpModel,
    config: &OmnicatConfig,
    schema_only: bool,
    table_filter: Option<&str>,
) -> Result<String> {
    let max_rows = config.terminal.data.max_rows.max(1);

    let selected: Vec<&DumpTable> = if let Some(want) = table_filter {
        let found = model
            .tables
            .values()
            .find(|t| t.name.eq_ignore_ascii_case(want));
        match found {
            Some(t) => vec![t],
            None => {
                let known: Vec<_> = model.tables.keys().cloned().collect();
                bail!(
                    "table `{want}` not found in dump (known: {})",
                    if known.is_empty() {
                        "(none)".into()
                    } else {
                        known.join(", ")
                    }
                );
            }
        }
    } else {
        model.tables.values().collect()
    };

    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut md = String::new();
    md.push_str("# SQL dump overview\n\n");
    md.push_str(&format!("- **file**: `{}`\n", path.display()));
    md.push_str(&format!("- **size**: {} bytes\n", size));
    if model.truncated {
        md.push_str(&format!(
            "- **note**: scanned first {} bytes only\n",
            model.scan_max
        ));
    }
    md.push_str(&format!("- **CREATE TABLE**: {}\n", model.create_stmts));
    md.push_str(&format!("- **INSERT**: {}\n", model.insert_stmts));
    md.push_str(&format!("- **other statements**: {}\n", model.other_stmts));

    if selected.is_empty() {
        md.push_str("\n_No CREATE TABLE / INSERT detected — showing as dump metadata only._\n");
        return Ok(md);
    }

    for table in selected {
        md.push_str(&format!("\n## `{}`\n\n", table.name));

        md.push_str("### Structure\n\n");
        if table.columns.is_empty() {
            md.push_str("_No column definitions found._\n\n");
        } else {
            md.push_str("| column | type | notes |\n");
            md.push_str("| --- | --- | --- |\n");
            for col in &table.columns {
                md.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    escape_md_cell(&col.name),
                    if col.type_name.is_empty() {
                        "—".into()
                    } else {
                        escape_md_cell(&col.type_name)
                    },
                    if col.notes.is_empty() {
                        String::new()
                    } else {
                        escape_md_cell(&col.notes)
                    }
                ));
            }
            md.push('\n');
        }

        if schema_only {
            continue;
        }

        md.push_str("### Data\n\n");
        if table.rows.is_empty() {
            md.push_str("_No INSERT rows found._\n");
            continue;
        }

        let headers: Vec<String> = if table.columns.is_empty() {
            let width = table.rows.iter().map(|r| r.len()).max().unwrap_or(0);
            (0..width).map(|i| format!("col_{}", i + 1)).collect()
        } else {
            table.columns.iter().map(|c| c.name.clone()).collect()
        };

        md.push('|');
        for h in &headers {
            md.push_str(&format!(" {} |", escape_md_cell(h)));
        }
        md.push('\n');
        md.push('|');
        for _ in &headers {
            md.push_str(" --- |");
        }
        md.push('\n');

        let shown = table.rows.len().min(max_rows);
        for row in table.rows.iter().take(shown) {
            md.push('|');
            for i in 0..headers.len() {
                let cell = row.get(i).map(String::as_str).unwrap_or("");
                md.push_str(&format!(" {} |", escape_md_cell(cell)));
            }
            md.push('\n');
        }
        if table.rows.len() > shown {
            md.push_str(&format!(
                "\n_… {} more row(s) not shown (max_rows={max_rows})_\n",
                table.rows.len() - shown
            ));
        }
    }

    Ok(md)
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;
    use std::path::PathBuf;

    fn sample_sql() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo/db/sample.sql")
    }

    #[test]
    fn sample_dump_full_has_structure_and_data() {
        let cfg = OmnicatConfig::default();
        let md = sql_dump_preview(&sample_sql(), &cfg, false, None).unwrap();
        assert!(md.contains("## `orders`"));
        assert!(md.contains("## `users`"));
        assert!(md.contains("### Structure"));
        assert!(md.contains("### Data"));
        assert!(md.contains("`email`"));
        assert!(md.contains("a@example.com"));
        assert!(md.contains("done"));
    }

    #[test]
    fn sample_dump_schema_only_skips_data() {
        let cfg = OmnicatConfig::default();
        let md = sql_dump_preview(&sample_sql(), &cfg, true, None).unwrap();
        assert!(md.contains("### Structure"));
        assert!(!md.contains("### Data"));
        assert!(!md.contains("a@example.com"));
    }

    #[test]
    fn sample_dump_one_table() {
        let cfg = OmnicatConfig::default();
        let md = sql_dump_preview(&sample_sql(), &cfg, false, Some("users")).unwrap();
        assert!(md.contains("## `users`"));
        assert!(!md.contains("## `orders`"));
        assert!(md.contains("a@example.com"));
    }

    #[test]
    fn sample_dump_unknown_table_errors() {
        let cfg = OmnicatConfig::default();
        let err = sql_dump_preview(&sample_sql(), &cfg, false, Some("missing")).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn extract_table_name_create() {
        let n = crate::drivers::sql_dump::extract_table_name(
            "CREATE TABLE `users` (id INT)",
            "CREATE TABLE",
        )
        .unwrap();
        assert_eq!(n, "users");
    }
}
