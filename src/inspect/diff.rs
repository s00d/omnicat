use std::fs;
use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::report::DiffReport;
use crate::orchestrator::registry::DriverRegistry;

pub fn build_diff(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
    left_kind: HandlerKind,
    right_kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<DiffReport> {
    if left_kind != right_kind {
        return Ok(DiffReport {
            left: left_display.to_string(),
            right: right_display.to_string(),
            kind: "type-mismatch".into(),
            summary: format!(
                "different handlers: {} vs {}",
                left_kind.name(),
                right_kind.name()
            ),
            changes: vec![],
        });
    }

    match left_kind {
        HandlerKind::Data => diff_data(left, left_display, right, right_display),
        HandlerKind::Database => diff_sqlite(left, left_display, right, right_display),
        HandlerKind::Image | HandlerKind::Media => {
            diff_info(left, left_display, right, right_display, left_kind, config)
        }
        HandlerKind::Markdown => {
            diff_rendered(left, left_display, right, right_display, left_kind, config)
        }
        _ => diff_text(left, left_display, right, right_display),
    }
}

fn diff_data(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
) -> Result<DiffReport> {
    let ext = left
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => diff_json(left, left_display, right, right_display),
        "csv" | "tsv" => diff_csv(
            left,
            left_display,
            right,
            right_display,
            if ext == "tsv" { b'\t' } else { b',' },
        ),
        _ => diff_text(left, left_display, right, right_display),
    }
}

fn diff_json(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
) -> Result<DiffReport> {
    let a: serde_json::Value = serde_json::from_str(&fs::read_to_string(left)?)?;
    let b: serde_json::Value = serde_json::from_str(&fs::read_to_string(right)?)?;
    let mut changes = Vec::new();
    walk_json("", &a, &b, &mut changes, 200);
    let summary = if changes.is_empty() {
        "identical".into()
    } else {
        format!("{} differences", changes.len())
    };
    Ok(DiffReport {
        left: left_display.to_string(),
        right: right_display.to_string(),
        kind: "json".into(),
        summary,
        changes,
    })
}

fn walk_json(
    path: &str,
    a: &serde_json::Value,
    b: &serde_json::Value,
    out: &mut Vec<String>,
    limit: usize,
) {
    if out.len() >= limit {
        return;
    }
    match (a, b) {
        (serde_json::Value::Object(ao), serde_json::Value::Object(bo)) => {
            for (k, av) in ao {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match bo.get(k) {
                    None => out.push(format!("{p}: removed")),
                    Some(bv) => walk_json(&p, av, bv, out, limit),
                }
            }
            for k in bo.keys() {
                if !ao.contains_key(k) {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    out.push(format!("{p}: added"));
                }
            }
        }
        (serde_json::Value::Array(aa), serde_json::Value::Array(ba)) => {
            if aa.len() != ba.len() {
                out.push(format!(
                    "{}: array length {} → {}",
                    if path.is_empty() { "." } else { path },
                    aa.len(),
                    ba.len()
                ));
            }
            for (i, (av, bv)) in aa.iter().zip(ba.iter()).enumerate() {
                walk_json(&format!("{path}[{i}]"), av, bv, out, limit);
            }
        }
        (a, b) if a != b => {
            out.push(format!(
                "{}: {} → {}",
                if path.is_empty() { "." } else { path },
                abbrev(a),
                abbrev(b)
            ));
        }
        _ => {}
    }
}

fn abbrev(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 60 {
        format!("{}…", &s[..57])
    } else {
        s
    }
}

fn diff_csv(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
    delim: u8,
) -> Result<DiffReport> {
    let a = read_csv_rows(left, delim)?;
    let b = read_csv_rows(right, delim)?;
    let mut changes = Vec::new();
    if a.headers != b.headers {
        changes.push(format!("columns: {:?} → {:?}", a.headers, b.headers));
    }
    let a_set: std::collections::BTreeSet<_> = a.rows.iter().collect();
    let b_set: std::collections::BTreeSet<_> = b.rows.iter().collect();
    let only_a = a_set.difference(&b_set).count();
    let only_b = b_set.difference(&a_set).count();
    if only_a > 0 {
        changes.push(format!("-{only_a} rows"));
    }
    if only_b > 0 {
        changes.push(format!("+{only_b} rows"));
    }
    if a.rows.len() != b.rows.len() {
        changes.push(format!("row count: {} → {}", a.rows.len(), b.rows.len()));
    }
    let summary = if changes.is_empty() {
        "identical".into()
    } else {
        format!("{} changes", changes.len())
    };
    Ok(DiffReport {
        left: left_display.to_string(),
        right: right_display.to_string(),
        kind: "csv".into(),
        summary,
        changes,
    })
}

struct CsvData {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

fn read_csv_rows(path: &Path, delim: u8) -> Result<CsvData> {
    let raw = fs::read_to_string(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delim)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let headers = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    for rec in rdr.records() {
        rows.push(rec?.iter().map(|s| s.to_string()).collect());
    }
    Ok(CsvData { headers, rows })
}

fn diff_sqlite(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
) -> Result<DiffReport> {
    let la = sqlite_summary(left)?;
    let rb = sqlite_summary(right)?;
    let schema_l = sqlite_schemas(left)?;
    let schema_r = sqlite_schemas(right)?;

    let mut changes = Vec::new();
    changes.push("Tables".into());
    changes.push("─".repeat(22));

    let all_tables: std::collections::BTreeSet<_> = la.keys().chain(rb.keys()).cloned().collect();
    let mut table_change_count = 0usize;
    for t in &all_tables {
        match (la.get(t), rb.get(t)) {
            (Some(a), Some(b)) if a == b => {
                changes.push(format!("table: {t}\tunchanged"));
            }
            (Some(a), Some(b)) => {
                let delta = *b as i64 - *a as i64;
                let sign = if delta >= 0 { "+" } else { "" };
                changes.push(format!("table: {t}\t{sign}{delta} rows ({a} → {b})"));
                table_change_count += 1;
            }
            (Some(_), None) => {
                changes.push(format!("table: {t}\tremoved"));
                table_change_count += 1;
            }
            (None, Some(_)) => {
                changes.push(format!("table: {t}\tadded"));
                table_change_count += 1;
            }
            _ => {}
        }
    }

    changes.push(String::new());
    changes.push("Schema".into());
    changes.push("─".repeat(22));
    let schema_changes = diff_sqlite_schemas(&schema_l, &schema_r);
    let schema_change_count = schema_changes.len();
    if schema_changes.is_empty() {
        changes.push("schema: (no differences)".into());
    } else {
        changes.extend(schema_changes);
    }

    Ok(DiffReport {
        left: left_display.to_string(),
        right: right_display.to_string(),
        kind: "sqlite".into(),
        summary: format!(
            "{} tables, {table_change_count} table change(s), {schema_change_count} schema change(s)",
            all_tables.len()
        ),
        changes,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ColumnInfo {
    name: String,
    type_name: String,
    notnull: bool,
    pk: bool,
}

impl ColumnInfo {
    fn describe(&self) -> String {
        let mut s = self.type_name.clone();
        if self.pk {
            s.push_str(" PRIMARY KEY");
        }
        if self.notnull {
            s.push_str(" NOT NULL");
        }
        s
    }
}

fn sqlite_summary(path: &Path) -> Result<std::collections::BTreeMap<String, u64>> {
    let conn = Connection::open(path)?;
    let mut map = std::collections::BTreeMap::new();
    for t in sqlite_table_names(&conn)? {
        let c: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0))
            .unwrap_or(0);
        map.insert(t, c as u64);
    }
    Ok(map)
}

fn sqlite_table_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(names)
}

fn sqlite_schemas(path: &Path) -> Result<std::collections::BTreeMap<String, Vec<ColumnInfo>>> {
    let conn = Connection::open(path)?;
    let mut out = std::collections::BTreeMap::new();
    for t in sqlite_table_names(&conn)? {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{t}\")"))?;
        let cols: Vec<ColumnInfo> = stmt
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get::<_, String>(1)?,
                    type_name: row.get::<_, String>(2).unwrap_or_default(),
                    notnull: row.get::<_, i64>(3).unwrap_or(0) != 0,
                    pk: row.get::<_, i64>(5).unwrap_or(0) != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        out.insert(t, cols);
    }
    Ok(out)
}

fn diff_sqlite_schemas(
    left: &std::collections::BTreeMap<String, Vec<ColumnInfo>>,
    right: &std::collections::BTreeMap<String, Vec<ColumnInfo>>,
) -> Vec<String> {
    let mut changes = Vec::new();
    let tables: std::collections::BTreeSet<_> = left.keys().chain(right.keys()).cloned().collect();
    for t in tables {
        match (left.get(&t), right.get(&t)) {
            (None, Some(cols)) => {
                for c in cols {
                    changes.push(format!("schema: {t}.{} added ({})", c.name, c.describe()));
                }
            }
            (Some(cols), None) => {
                for c in cols {
                    changes.push(format!("schema: {t}.{} removed", c.name));
                }
            }
            (Some(lcols), Some(rcols)) => {
                let lmap: std::collections::BTreeMap<_, _> =
                    lcols.iter().map(|c| (c.name.clone(), c)).collect();
                let rmap: std::collections::BTreeMap<_, _> =
                    rcols.iter().map(|c| (c.name.clone(), c)).collect();
                let names: std::collections::BTreeSet<_> =
                    lmap.keys().chain(rmap.keys()).cloned().collect();
                for name in names {
                    match (lmap.get(&name), rmap.get(&name)) {
                        (Some(a), Some(b)) if a == b => {}
                        (Some(a), Some(b)) => {
                            changes.push(format!(
                                "schema: {t}.{name}\n  {} → {}",
                                a.describe(),
                                b.describe()
                            ));
                        }
                        (Some(_), None) => {
                            changes.push(format!("schema: {t}.{name} removed"));
                        }
                        (None, Some(b)) => {
                            changes.push(format!("schema: {t}.{name} added ({})", b.describe()));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    changes
}

fn diff_text(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
) -> Result<DiffReport> {
    let a = fs::read_to_string(left).unwrap_or_default();
    let b = fs::read_to_string(right).unwrap_or_default();
    diff_text_strings(left_display, right_display, &a, &b, "text")
}

/// Diff terminal-rendered content (e.g. markdown preview plain text).
fn diff_rendered(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<DiffReport> {
    let (a, b) = if kind == HandlerKind::Markdown {
        // MarkdownDriver::build keeps source; render like the terminal path.
        let mut render_cfg = config.clone();
        render_cfg.terminal.plain = true;
        let a = render_markdown_plain(left, &render_cfg)?;
        let b = render_markdown_plain(right, &render_cfg)?;
        (a, b)
    } else {
        (
            DriverRegistry::build(kind, left, config)?.plain_text(),
            DriverRegistry::build(kind, right, config)?.plain_text(),
        )
    };
    diff_text_strings(left_display, right_display, &a, &b, kind.name())
}

fn render_markdown_plain(path: &Path, config: &OmnicatConfig) -> Result<String> {
    use crate::drivers::markdown::MarkdownDriver;
    let mut buf = Vec::new();
    MarkdownDriver.render(path, config, &mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn diff_text_strings(
    left_display: &str,
    right_display: &str,
    a: &str,
    b: &str,
    kind: &str,
) -> Result<DiffReport> {
    let a_lines: Vec<&str> = a.lines().collect();
    let b_lines: Vec<&str> = b.lines().collect();
    let mut changes = Vec::new();
    let max = a_lines.len().max(b_lines.len());
    for i in 0..max {
        if changes.len() >= 200 {
            changes.push("… truncated".into());
            break;
        }
        let al = a_lines.get(i).copied().unwrap_or("");
        let bl = b_lines.get(i).copied().unwrap_or("");
        if al != bl {
            if a_lines.get(i).is_none() {
                changes.push(format!("+{}: {bl}", i + 1));
            } else if b_lines.get(i).is_none() {
                changes.push(format!("-{}: {al}", i + 1));
            } else {
                changes.push(format!("~{}: {al} → {bl}", i + 1));
            }
        }
    }
    let summary = if changes.is_empty() {
        "identical".into()
    } else {
        format!("{} line changes", changes.len())
    };
    Ok(DiffReport {
        left: left_display.to_string(),
        right: right_display.to_string(),
        kind: kind.into(),
        summary,
        changes,
    })
}

fn diff_info(
    left: &Path,
    left_display: &str,
    right: &Path,
    right_display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<DiffReport> {
    let a = DriverRegistry::build(kind, left, config)?.plain_text();
    let b = DriverRegistry::build(kind, right, config)?.plain_text();
    if a == b {
        return Ok(DiffReport {
            left: left_display.to_string(),
            right: right_display.to_string(),
            kind: kind.name().into(),
            summary: "identical metadata".into(),
            changes: vec![],
        });
    }
    Ok(DiffReport {
        left: left_display.to_string(),
        right: right_display.to_string(),
        kind: kind.name().into(),
        summary: "metadata differs".into(),
        changes: vec![format!("left:\n{a}"), format!("right:\n{b}")],
    })
}

#[allow(dead_code)]
fn unsupported() -> Result<DiffReport> {
    bail!("diff unsupported")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_schema_diff_detects_column_change() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sqlite");
        let b = dir.path().join("b.sqlite");
        {
            let conn = Connection::open(&a).unwrap();
            conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT);")
                .unwrap();
        }
        {
            let conn = Connection::open(&b).unwrap();
            conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL);")
                .unwrap();
        }
        let report = diff_sqlite(&a, "a", &b, "b").unwrap();
        assert!(report
            .changes
            .iter()
            .any(|c| c.contains("schema: users.email")));
        assert!(report.changes.iter().any(|c| c.contains("NOT NULL")));
    }

    #[test]
    fn markdown_diff_uses_rendered_text() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        fs::write(&a, "# Hello\n\nworld\n").unwrap();
        fs::write(&b, "# Hello\n\nWORLD\n").unwrap();
        let cfg = OmnicatConfig::default();
        let report = diff_rendered(&a, "a.md", &b, "b.md", HandlerKind::Markdown, &cfg).unwrap();
        assert_eq!(report.kind, "markdown");
        assert!(!report.changes.is_empty());
    }

    #[test]
    fn markdown_diff_ignores_markup_only_difference() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.md");
        // Same rendered strong text; only source markup differs.
        fs::write(&a, "Hello **world**\n").unwrap();
        fs::write(&b, "Hello __world__\n").unwrap();
        let cfg = OmnicatConfig::default();
        let report = diff_rendered(&a, "a.md", &b, "b.md", HandlerKind::Markdown, &cfg).unwrap();
        assert_eq!(report.kind, "markdown");
        assert!(report.changes.is_empty(), "{:?}", report.changes);
    }
}
