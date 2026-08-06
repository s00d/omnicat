use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::report::QueryReport;

pub fn build_query(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    query: &str,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    let query = query.trim();
    if query.is_empty() {
        bail!("empty query");
    }
    match kind {
        HandlerKind::Database => query_sqlite(path, display, query, config),
        HandlerKind::Data => query_data(path, display, query, config),
        _ => bail!("query not supported for {}", kind.name()),
    }
}

fn query_sqlite(
    path: &Path,
    display: &str,
    query: &str,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let conn = Connection::open_with_flags(path, flags).context("open sqlite read-only")?;
    let mut stmt = conn.prepare(query).context("prepare SQL")?;
    let col_count = stmt.column_count();
    let headers: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();
    let max_rows = crate::inspect::effective_max_rows(config);
    let mapped = stmt.query_map([], |row| {
        let mut cells = Vec::new();
        for i in 0..col_count {
            let val: rusqlite::types::Value = row.get(i)?;
            cells.push(match val {
                rusqlite::types::Value::Null => "NULL".into(),
                rusqlite::types::Value::Integer(n) => n.to_string(),
                rusqlite::types::Value::Real(f) => f.to_string(),
                rusqlite::types::Value::Text(s) => s,
                rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
            });
        }
        Ok(cells)
    })?;
    let mut rows = Vec::new();
    for (i, r) in mapped.enumerate() {
        if i >= max_rows {
            break;
        }
        rows.push(r?);
    }
    Ok(QueryReport {
        path: display.to_string(),
        query: query.to_string(),
        headers: Some(headers),
        rows: Some(rows),
        json: None,
        text: None,
    })
}

fn query_data(
    path: &Path,
    display: &str,
    query: &str,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "json" => query_json(path, display, query),
        "csv" => query_csv(path, display, query, b',', config),
        "tsv" => query_csv(path, display, query, b'\t', config),
        "parquet" => query_parquet(path, display, query, config),
        "jsonl" | "ndjson" => query_jsonl(path, display, query, config),
        _ => bail!("query not supported for .{ext}"),
    }
}

fn query_json(path: &Path, display: &str, query: &str) -> Result<QueryReport> {
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let result = eval_jq_lite(&value, query)?;
    Ok(QueryReport {
        path: display.to_string(),
        query: query.to_string(),
        headers: None,
        rows: None,
        json: Some(result.clone()),
        text: Some(serde_json::to_string_pretty(&result)?),
    })
}

fn query_jsonl(
    path: &Path,
    display: &str,
    query: &str,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    use crate::io::{FileHandle, OpenOptions};

    let mut out = Vec::new();
    let max = crate::inspect::effective_max_rows(config);
    let handle = FileHandle::open(path, OpenOptions::stream())?;
    handle.for_each_line(|line| {
        if out.len() >= max {
            return Ok(());
        }
        let text = line.as_str().unwrap_or("").trim();
        if text.is_empty() {
            return Ok(());
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
            return Ok(());
        };
        if row_matches_predicate(&value, query)? {
            out.push(value);
        }
        Ok(())
    })?;
    let json = serde_json::Value::Array(out);
    Ok(QueryReport {
        path: display.to_string(),
        query: query.to_string(),
        headers: None,
        rows: None,
        json: Some(json.clone()),
        text: Some(serde_json::to_string_pretty(&json)?),
    })
}

fn query_csv(
    path: &Path,
    display: &str,
    query: &str,
    delim: u8,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    let raw = fs::read_to_string(path)?;
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .delimiter(delim)
        .flexible(true)
        .from_reader(raw.as_bytes());
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let pred = parse_csv_predicate(query, &headers)?;
    let mut rows = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        let cells: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
        if pred.matches(&headers, &cells) {
            rows.push(cells);
            if rows.len() >= crate::inspect::effective_max_rows(config) {
                break;
            }
        }
    }
    Ok(QueryReport {
        path: display.to_string(),
        query: query.to_string(),
        headers: Some(headers),
        rows: Some(rows),
        json: None,
        text: None,
    })
}

fn query_parquet(
    path: &Path,
    display: &str,
    query: &str,
    config: &OmnicatConfig,
) -> Result<QueryReport> {
    // Lite: SELECT * [WHERE col op val] LIMIT n  |  col op val  |  LIMIT n
    let max_rows = crate::inspect::effective_max_rows(config);
    let (predicate, limit) = parse_parquet_query(query, max_rows);

    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = fs::File::open(path)?;
    let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
    let reader = builder.build()?;
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch?;
        if headers.is_empty() {
            headers = (0..batch.num_columns())
                .map(|i| batch.schema().field(i).name().clone())
                .collect();
        }
        let pred = match &predicate {
            Some(raw) => Some(parse_csv_predicate(raw, &headers)?),
            None => None,
        };
        for r in 0..batch.num_rows() {
            if rows.len() >= limit {
                break;
            }
            let cells: Vec<String> = (0..batch.num_columns())
                .map(|c| array_cell(batch.column(c).as_ref(), r))
                .collect();
            if let Some(p) = &pred {
                if !p.matches(&headers, &cells) {
                    continue;
                }
            }
            rows.push(cells);
        }
        if rows.len() >= limit {
            break;
        }
    }
    Ok(QueryReport {
        path: display.to_string(),
        query: query.to_string(),
        headers: Some(headers),
        rows: Some(rows),
        json: None,
        text: None,
    })
}

/// Returns (optional CSV-style predicate, row limit).
fn parse_parquet_query(query: &str, max_rows: usize) -> (Option<String>, usize) {
    let q = query.trim();
    let upper = q.to_ascii_uppercase();

    // SELECT * WHERE age > 18 LIMIT 20
    if let Some(rest) = upper.strip_prefix("SELECT *") {
        let rest_orig = &q[q.len() - rest.len()..];
        let rest_trim = rest_orig.trim();
        let rest_upper = rest_trim.to_ascii_uppercase();
        if rest_trim.is_empty() {
            return (None, max_rows);
        }
        if let Some(after_where) = rest_upper.strip_prefix("WHERE ") {
            let after = &rest_trim[rest_trim.len() - after_where.len()..];
            return split_pred_limit(after, max_rows);
        }
        if let Some(n) = parse_limit(q) {
            return (None, n.min(max_rows));
        }
    }

    if let Some(n) = parse_limit(q) {
        return (None, n.min(max_rows));
    }

    // Plain predicate: age > 18  or  age > 18 LIMIT 10
    split_pred_limit(q, max_rows)
}

fn split_pred_limit(s: &str, max_rows: usize) -> (Option<String>, usize) {
    let upper = s.to_ascii_uppercase();
    if let Some(idx) = upper.rfind(" LIMIT ") {
        let pred = s[..idx].trim();
        let limit_str = s[idx + " LIMIT ".len()..].trim();
        let limit = limit_str.parse::<usize>().unwrap_or(max_rows).min(max_rows);
        let pred = if pred.is_empty() {
            None
        } else {
            Some(pred.to_string())
        };
        return (pred, limit);
    }
    (Some(s.to_string()), max_rows)
}

fn parse_limit(query: &str) -> Option<usize> {
    let q = query.trim();
    let upper = q.to_ascii_uppercase();
    if let Some(rest) = upper.strip_prefix("SELECT * LIMIT ") {
        return rest.trim().parse().ok();
    }
    if let Some(rest) = upper.strip_prefix("LIMIT ") {
        return rest.trim().parse().ok();
    }
    if upper == "SELECT *" {
        return None;
    }
    None
}

fn array_cell(array: &dyn arrow::array::Array, row: usize) -> String {
    use arrow::array::{Array, BooleanArray, Float64Array, Int64Array, StringArray};
    match array.data_type() {
        arrow::datatypes::DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .and_then(|a| {
                if a.is_valid(row) {
                    Some(a.value(row).to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        arrow::datatypes::DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        arrow::datatypes::DataType::Float64 => array
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        arrow::datatypes::DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|a| a.value(row).to_string())
            .unwrap_or_default(),
        other => format!("<{other:?}>"),
    }
}

/// Minimal jq-like: `.`, `.field`, `.field.sub`, `.[]`, `.users[]`, `| select(.age > 18)`
fn eval_jq_lite(value: &serde_json::Value, query: &str) -> Result<serde_json::Value> {
    let mut parts = query.split('|').map(str::trim).filter(|s| !s.is_empty());
    let first = parts.next().unwrap_or(".");
    let mut cur = eval_path(value, first)?;
    for part in parts {
        if let Some(pred) = part
            .strip_prefix("select(")
            .and_then(|s| s.strip_suffix(')'))
        {
            cur = apply_select(cur, pred.trim())?;
        } else {
            cur = eval_path(&cur, part)?;
        }
    }
    Ok(cur)
}

fn eval_path(value: &serde_json::Value, path: &str) -> Result<serde_json::Value> {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Ok(value.clone());
    }
    let mut cur = value.clone();
    let mut rest = path.trim_start_matches('.');
    while !rest.is_empty() {
        if rest.starts_with("[]") {
            rest = rest.trim_start_matches("[]").trim_start_matches('.');
            cur = match cur {
                serde_json::Value::Array(arr) => serde_json::Value::Array(arr),
                other => bail!("cannot iterate non-array: {other}"),
            };
            continue;
        }
        if let Some(field) = rest.strip_suffix("[]") {
            let field = field.trim_end_matches('.');
            if !field.is_empty() {
                cur = cur
                    .get(field)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("missing field {field}"))?;
            }
            cur = match cur {
                serde_json::Value::Array(arr) => serde_json::Value::Array(arr),
                other => bail!("cannot iterate non-array at {field}: {other}"),
            };
            rest = "";
            continue;
        }
        let (field, next) = match rest.find('.') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        let field = field.trim_end_matches("[]");
        if field.is_empty() {
            break;
        }
        cur = cur
            .get(field)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing field {field}"))?;
        rest = next;
    }
    Ok(cur)
}

fn apply_select(value: serde_json::Value, pred: &str) -> Result<serde_json::Value> {
    match value {
        serde_json::Value::Array(arr) => {
            let mut out = Vec::new();
            for item in arr {
                if row_matches_predicate(&item, pred)? {
                    out.push(item);
                }
            }
            Ok(serde_json::Value::Array(out))
        }
        other => {
            if row_matches_predicate(&other, pred)? {
                Ok(other)
            } else {
                Ok(serde_json::Value::Null)
            }
        }
    }
}

/// Match a JSON object against a find/query predicate (`level:error`, `field == "x"`, …).
pub(crate) fn row_matches_predicate(value: &serde_json::Value, pred: &str) -> Result<bool> {
    let pred = pred.trim().trim_start_matches('.');
    // field == "x" | field != "x" | field > N
    let ops = ["==", "!=", ">=", "<=", ">", "<"];
    for op in ops {
        if let Some((left, right)) = pred.split_once(op) {
            let field = left.trim().trim_start_matches('.');
            let rhs = right.trim().trim_matches('"').trim_matches('\'');
            let lhs = value.get(field).cloned().unwrap_or(serde_json::Value::Null);
            return Ok(compare_values(&lhs, op, rhs));
        }
    }
    // field:value shorthand (level:error, service:api)
    if let Some((left, right)) = pred.split_once(':') {
        let field = left.trim();
        let rhs = right.trim();
        if !field.is_empty()
            && !rhs.is_empty()
            && !field.contains(' ')
            && value.get(field).is_some()
        {
            let lhs = value.get(field).cloned().unwrap_or(serde_json::Value::Null);
            return Ok(compare_values(&lhs, "==", rhs));
        }
    }
    // bare field truthiness
    if let Some(v) = value.get(pred) {
        return Ok(match v {
            serde_json::Value::Null => false,
            serde_json::Value::Bool(b) => *b,
            serde_json::Value::String(s) => !s.is_empty(),
            _ => true,
        });
    }
    Ok(false)
}

fn compare_values(lhs: &serde_json::Value, op: &str, rhs: &str) -> bool {
    if let Ok(n) = rhs.parse::<f64>() {
        let l = match lhs {
            serde_json::Value::Number(x) => x.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
            _ => return false,
        };
        return match op {
            "==" => (l - n).abs() < f64::EPSILON,
            "!=" => (l - n).abs() >= f64::EPSILON,
            ">" => l > n,
            "<" => l < n,
            ">=" => l >= n,
            "<=" => l <= n,
            _ => false,
        };
    }
    let l = match lhs {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".into(),
        other => other.to_string(),
    };
    match op {
        "==" => l == rhs,
        "!=" => l != rhs,
        _ => false,
    }
}

struct CsvPredicate {
    col: usize,
    op: String,
    rhs: String,
}

impl CsvPredicate {
    fn matches(&self, _headers: &[String], cells: &[String]) -> bool {
        let lhs = cells.get(self.col).map(String::as_str).unwrap_or("");
        compare_values(&serde_json::json!(lhs), &self.op, &self.rhs)
    }
}

fn parse_csv_predicate(query: &str, headers: &[String]) -> Result<CsvPredicate> {
    let ops = ["==", "!=", ">=", "<=", ">", "<"];
    for op in ops {
        if let Some((left, right)) = query.split_once(op) {
            let col_name = left.trim();
            let col = headers
                .iter()
                .position(|h| h.eq_ignore_ascii_case(col_name))
                .ok_or_else(|| anyhow::anyhow!("unknown column {col_name}"))?;
            return Ok(CsvPredicate {
                col,
                op: op.to_string(),
                rhs: right
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            });
        }
    }
    bail!("unsupported CSV predicate: {query} (expected: col > value)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_colon_shorthand() {
        let v = serde_json::json!({"level": "error", "message": "x"});
        assert!(row_matches_predicate(&v, "level:error").unwrap());
        assert!(!row_matches_predicate(&v, "level:info").unwrap());
    }
}
