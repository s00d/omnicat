//! `--print-query`: emit INSERT / insertMany from matched rows (copy-paste into a live DB).

use std::io::{self, Write};

use anyhow::{Context, Result};

use crate::db::mysql::dump::scan::extract_table_names_sql;
use crate::db::report::QueryResultReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryDialect {
    Sql,
    Mongo,
}

/// Print runnable INSERT statements built from --query result rows (stdout only).
pub fn print_query_from_results(
    dialect: QueryDialect,
    source: &str,
    report: &QueryResultReport,
) -> Result<()> {
    let text = match dialect {
        QueryDialect::Sql => render_sql_inserts(source, report)?,
        QueryDialect::Mongo => render_mongo_inserts(source, report),
    };
    let mut out = io::stdout().lock();
    write!(out, "{text}")?;
    if !text.ends_with('\n') {
        writeln!(out)?;
    }
    out.flush().ok();
    Ok(())
}

/// `INSERT INTO t (cols) VALUES (...), (...)` — same values as the result table.
pub fn render_sql_inserts(source_sql: &str, report: &QueryResultReport) -> Result<String> {
    let tables = extract_table_names_sql(source_sql).context("parse source SQL")?;
    let table = tables
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("could not resolve table name from query"))?;

    if report.rows.is_empty() {
        return Ok(format!("-- no rows matched for `{table}`\n"));
    }

    if report.columns.is_empty() {
        return Ok("-- no columns in result\n".into());
    }

    let col_list = report
        .columns
        .iter()
        .map(|c| format!("`{c}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let value_rows = report
        .rows
        .iter()
        .map(|row| {
            let vals = row
                .iter()
                .map(|v| sql_literal(v.as_deref()))
                .collect::<Vec<_>>()
                .join(", ");
            format!("({vals})")
        })
        .collect::<Vec<_>>()
        .join(",\n");

    Ok(format!(
        "INSERT INTO `{table}` ({col_list}) VALUES\n{value_rows};\n"
    ))
}

/// `db.coll.insertMany([{...}, ...])` — documents from result rows.
pub fn render_mongo_inserts(collection: &str, report: &QueryResultReport) -> String {
    if report.rows.is_empty() {
        return format!("// no rows matched for db.{collection}\n");
    }

    let docs: Vec<String> = report
        .rows
        .iter()
        .map(|row| {
            let parts: Vec<String> = report
                .columns
                .iter()
                .zip(row.iter())
                .filter_map(|(col, val)| {
                    // Omit absent fields so insertMany does not invent nulls.
                    let Some(v) = val else {
                        return None;
                    };
                    Some(format!("{col:?}: {}", json_literal(v)))
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        })
        .collect();

    if docs.len() == 1 {
        format!("db.{collection}.insertOne({});\n", docs[0])
    } else {
        format!(
            "db.{collection}.insertMany([\n  {},\n]);\n",
            docs.join(",\n  ")
        )
    }
}

fn sql_literal(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "NULL".into();
    };
    // Keep textual values quoted — do not infer numbers from display strings
    // ("001" must stay a string; the string "NULL" is not SQL NULL).
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn json_literal(value: &str) -> String {
    // Prefer JSON-shaped values (bool/array/object/number) when the cell was
    // already serialized JSON; otherwise emit a JSON string.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(value) {
        return v.to_string();
    }
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_insert_matches_table_values() {
        let report = QueryResultReport {
            columns: vec!["email".into()],
            rows: vec![vec![Some("a@example.com".into())]],
            rows_scanned: 1,
            rows_matched: 1,
        };
        let sql = render_sql_inserts("SELECT email FROM users LIMIT 1", &report).unwrap();
        assert!(sql.contains("INSERT INTO `users` (`email`) VALUES"));
        assert!(sql.contains("'a@example.com'"));
        assert!(!sql.contains("SELECT"));
    }

    #[test]
    fn mongo_insert_many() {
        let report = QueryResultReport {
            columns: vec!["_id".into(), "status".into()],
            rows: vec![
                vec![Some("1".into()), Some("ok".into())],
                vec![Some("2".into()), Some("failed".into())],
            ],
            rows_scanned: 2,
            rows_matched: 2,
        };
        let m = render_mongo_inserts("sample.users", &report);
        assert!(m.contains("insertMany"));
        assert!(m.contains("\"failed\""));
    }
}
