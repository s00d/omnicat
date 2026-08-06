//! Request / trace correlation views.

use crate::log::record::LogRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceLine {
    pub timestamp: Option<String>,
    pub service: Option<String>,
    pub message: String,
    pub duration_ms: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceReport {
    pub id: String,
    pub lines: Vec<TraceLine>,
    pub total_duration_ms: Option<f64>,
}

pub fn build_trace_report(id: &str, records: &[LogRecord<'_>]) -> TraceReport {
    let mut lines = Vec::new();
    let mut first = None;
    let mut last = None;
    for rec in records {
        if let Some(ts) = rec.timestamp {
            first = Some(
                first
                    .map(|f: chrono::DateTime<chrono::Utc>| f.min(ts))
                    .unwrap_or(ts),
            );
            last = Some(
                last.map(|l: chrono::DateTime<chrono::Utc>| l.max(ts))
                    .unwrap_or(ts),
            );
        }
        lines.push(TraceLine {
            timestamp: rec.timestamp.map(|t| t.to_rfc3339()),
            service: rec.service.as_ref().map(|s| s.to_string()),
            message: rec.message.to_string(),
            duration_ms: rec.duration_ms,
        });
    }
    let total = match (first, last) {
        (Some(a), Some(b)) => Some((b - a).num_milliseconds() as f64),
        _ => None,
    };
    TraceReport {
        id: id.to_string(),
        lines,
        total_duration_ms: total,
    }
}

pub fn collect_correlated<'a>(
    id: &str,
    records: impl IntoIterator<Item = LogRecord<'a>>,
) -> Vec<LogRecord<'static>> {
    records
        .into_iter()
        .filter(|r| r.matches_id(id))
        .map(|r| LogRecord {
            timestamp: r.timestamp,
            level: r.level,
            message: r.message.to_string().into(),
            service: r.service.map(|s| s.to_string().into()),
            request_id: r.request_id.map(|s| s.to_string().into()),
            trace_id: r.trace_id.map(|s| s.to_string().into()),
            span_id: r.span_id.map(|s| s.to_string().into()),
            correlation_id: r.correlation_id.map(|s| s.to_string().into()),
            method: r.method.map(|s| s.to_string().into()),
            path: r.path.map(|s| s.to_string().into()),
            status: r.status,
            client_ip: r.client_ip.map(|s| s.to_string().into()),
            duration_ms: r.duration_ms,
            raw_line: r.raw_line.to_string().into(),
            source_file: r.source_file.map(|s| s.to_string().into()),
            format: r.format,
        })
        .collect()
}
