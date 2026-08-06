//! Context window around a timestamp or error line.

use chrono::{DateTime, Utc};

use crate::log::record::LogRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextLine {
    pub marker: bool,
    pub text: String,
}

pub fn context_around_ts(
    records: &[LogRecord<'_>],
    target: DateTime<Utc>,
    window: usize,
) -> Vec<ContextLine> {
    let _idx = 0usize;
    let mut best = None;
    for (i, rec) in records.iter().enumerate() {
        if let Some(ts) = rec.timestamp {
            let diff = (ts - target).num_seconds().unsigned_abs();
            if best.is_none_or(|(_, d)| diff < d) {
                best = Some((i, diff));
            }
        }
    }
    let Some((center, _)) = best else {
        return Vec::new();
    };
    let start = center.saturating_sub(window);
    let end = (center + window + 1).min(records.len());
    records[start..end]
        .iter()
        .enumerate()
        .map(|(j, rec)| ContextLine {
            marker: start + j == center,
            text: rec.raw_line.to_string(),
        })
        .collect()
}

pub fn context_around_line_index(
    records: &[LogRecord<'_>],
    index: usize,
    window: usize,
) -> Vec<ContextLine> {
    let start = index.saturating_sub(window);
    let end = (index + window + 1).min(records.len());
    records[start..end]
        .iter()
        .enumerate()
        .map(|(j, rec)| ContextLine {
            marker: start + j == index,
            text: rec.raw_line.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn context_window_marks_center() {
        let ts = Utc.with_ymd_and_hms(2026, 8, 6, 12, 42, 17).unwrap();
        let recs: Vec<LogRecord<'_>> = (0..5)
            .map(|i| LogRecord {
                timestamp: Some(ts + chrono::Duration::seconds(i - 2)),
                level: None,
                message: format!("line {i}").into(),
                service: None,
                request_id: None,
                trace_id: None,
                span_id: None,
                correlation_id: None,
                method: None,
                path: None,
                status: None,
                client_ip: None,
                duration_ms: None,
                raw_line: format!("line {i}").into(),
                source_file: None,
                format: crate::log::record::LogFormat::Unknown,
            })
            .collect();
        let lines = context_around_ts(&recs, ts, 1);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.iter().filter(|l| l.marker).count(), 1);
    }
}
