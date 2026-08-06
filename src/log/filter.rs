//! Filter DSL, time windows, level thresholds.

use chrono::{DateTime, Duration, Utc};

use crate::log::level::{level_matches, parse_level_threshold, LogLevel};
use crate::log::record::LogRecord;

/// Compiled filter for log records.
#[derive(Debug, Default, Clone)]
pub struct LogFilter {
    pub level_threshold: Option<(LogLevel, bool)>,
    pub errors_only: bool,
    pub warnings_only: bool,
    pub where_clauses: Vec<WhereClause>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub status: Option<u16>,
    pub method: Option<String>,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub enum WhereClause {
    FieldEq(String, String),
    FieldContains(String, String),
    FieldGt(String, f64),
    FieldLt(String, f64),
}

impl LogFilter {
    pub fn matches(&self, rec: &LogRecord<'_>) -> bool {
        if self.errors_only && !rec.level.is_some_and(|l| l.is_errorish()) {
            return false;
        }
        if self.warnings_only && !rec.level.is_some_and(|l| l.is_warningish()) {
            return false;
        }
        if let Some((thr, plus)) = self.level_threshold {
            if !level_matches(rec.level, thr, plus) {
                return false;
            }
        }
        if let Some(s) = self.since {
            if rec.timestamp.is_none_or(|t| t < s) {
                return false;
            }
        }
        if let Some(u) = self.until {
            if rec.timestamp.is_none_or(|t| t > u) {
                return false;
            }
        }
        if let Some(st) = self.status {
            if rec.status != Some(st) {
                return false;
            }
        }
        if let Some(ref m) = self.method {
            if !rec.method.as_deref().unwrap_or("").eq_ignore_ascii_case(m) {
                return false;
            }
        }
        if let Some(ref id) = self.request_id {
            if !rec.matches_id(id) {
                return false;
            }
        }
        if let Some(ref id) = self.trace_id {
            if !rec.trace_id.as_deref().unwrap_or("").contains(id.as_str()) {
                return false;
            }
        }
        for clause in &self.where_clauses {
            if !match_clause(clause, rec) {
                return false;
            }
        }
        true
    }
}

fn match_clause(clause: &WhereClause, rec: &LogRecord<'_>) -> bool {
    match clause {
        WhereClause::FieldEq(field, val) => field_value(rec, field)
            .map(|v| v.eq_ignore_ascii_case(val))
            .unwrap_or(false),
        WhereClause::FieldContains(field, val) => field_value(rec, field)
            .map(|v| v.to_ascii_lowercase().contains(&val.to_ascii_lowercase()))
            .unwrap_or(false),
        WhereClause::FieldGt(field, n) => field_value(rec, field)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|x| x > *n),
        WhereClause::FieldLt(field, n) => field_value(rec, field)
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|x| x < *n),
    }
}

fn field_value(rec: &LogRecord<'_>, field: &str) -> Option<String> {
    match field.to_ascii_lowercase().as_str() {
        "level" => rec.level.map(|l| l.name().to_string()),
        "service" => rec.service.as_ref().map(|s| s.to_string()),
        "message" | "msg" => Some(rec.message.to_string()),
        "status" | "status_code" => rec.status.map(|s| s.to_string()),
        "method" => rec.method.as_ref().map(|s| s.to_string()),
        "path" | "url" => rec.path.as_ref().map(|s| s.to_string()),
        "duration" | "duration_ms" => rec.duration_ms.map(|d| d.to_string()),
        "request_id" => rec.request_id.as_ref().map(|s| s.to_string()),
        "trace_id" => rec.trace_id.as_ref().map(|s| s.to_string()),
        _ => None,
    }
}

/// Parse `--where 'level:error service:api'` style expressions.
pub fn parse_where(expr: &str) -> Vec<WhereClause> {
    let mut out = Vec::new();
    for part in expr.split_whitespace() {
        if let Some((k, v)) = part.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().trim_matches('"').to_string();
            if let Some(rest) = val.strip_prefix('>') {
                if let Ok(n) = rest.trim_end_matches('s').parse::<f64>() {
                    let ms = if val.ends_with('s') { n * 1000.0 } else { n };
                    out.push(WhereClause::FieldGt(key, ms));
                }
            } else if let Some(rest) = val.strip_prefix('<') {
                if let Ok(n) = rest.parse::<f64>() {
                    out.push(WhereClause::FieldLt(key, n));
                }
            } else if key.ends_with('~') {
                out.push(WhereClause::FieldContains(
                    key.trim_end_matches('~').to_string(),
                    val,
                ));
            } else {
                out.push(WhereClause::FieldEq(key, val));
            }
        }
    }
    out
}

/// Parse `--since 10m` / `--since 2026-08-06T12:00:00Z`.
pub fn parse_time_bound(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(nd) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(nd.and_utc());
    }
    if let Some(m) = s.strip_suffix('m') {
        if let Ok(n) = m.parse::<i64>() {
            return Some(Utc::now() - Duration::minutes(n));
        }
    }
    if let Some(h) = s.strip_suffix('h') {
        if let Ok(n) = h.parse::<i64>() {
            return Some(Utc::now() - Duration::hours(n));
        }
    }
    None
}

pub fn apply_level_flag(filter: &mut LogFilter, level: &str) {
    if let Some((l, plus)) = parse_level_threshold(level) {
        filter.level_threshold = Some((l, plus));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_level_error() {
        let clauses = parse_where("level:error");
        assert_eq!(clauses.len(), 1);
    }
}
