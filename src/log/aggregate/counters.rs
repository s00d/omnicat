//! Streaming counters for log stats.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::log::record::LogRecord;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct LogCounters {
    pub messages: u64,
    pub first_ts: Option<DateTime<Utc>>,
    pub last_ts: Option<DateTime<Utc>>,
    pub levels: HashMap<String, u64>,
    pub services: HashMap<String, u64>,
    pub status_codes: HashMap<String, u64>,
    pub methods: HashMap<String, u64>,
}

impl LogCounters {
    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        self.messages += 1;
        if let Some(ts) = rec.timestamp {
            self.first_ts = Some(self.first_ts.map(|f| f.min(ts)).unwrap_or(ts));
            self.last_ts = Some(self.last_ts.map(|l| l.max(ts)).unwrap_or(ts));
        }
        if let Some(l) = rec.level {
            *self.levels.entry(l.name().to_string()).or_default() += 1;
        }
        if let Some(ref s) = rec.service {
            *self.services.entry(s.to_string()).or_default() += 1;
        }
        if let Some(st) = rec.status {
            *self.status_codes.entry(st.to_string()).or_default() += 1;
        }
        if let Some(ref m) = rec.method {
            *self.methods.entry(m.to_string()).or_default() += 1;
        }
    }

    pub fn duration_human(&self) -> Option<String> {
        match (self.first_ts, self.last_ts) {
            (Some(a), Some(b)) if b > a => {
                let d = b - a;
                Some(format_duration(d.num_seconds()))
            }
            _ => None,
        }
    }
}

fn format_duration(secs: i64) -> String {
    if secs >= 3600 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}
