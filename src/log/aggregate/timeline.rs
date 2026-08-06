//! Time-bucket histogram for timeline view.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::log::level::LogLevel;
use crate::log::record::LogRecord;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct TimelineAgg {
    pub interval_secs: i64,
    pub buckets: BTreeMap<i64, BucketCounts>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct BucketCounts {
    pub total: u64,
    pub info: u64,
    pub warn: u64,
    pub error: u64,
}

impl TimelineAgg {
    pub fn new(interval_secs: i64) -> Self {
        Self {
            interval_secs,
            buckets: BTreeMap::new(),
        }
    }

    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        let Some(ts) = rec.timestamp else {
            return;
        };
        let key = bucket_key(ts, self.interval_secs);
        let b = self.buckets.entry(key).or_default();
        b.total += 1;
        match rec.level {
            Some(l) if l.is_errorish() => b.error += 1,
            Some(LogLevel::Warn) => b.warn += 1,
            _ => b.info += 1,
        }
    }

    pub fn render_ascii(&self, width: usize) -> String {
        let max = self
            .buckets
            .values()
            .map(|b| b.total.max(b.error).max(b.warn).max(b.info))
            .max()
            .unwrap_or(1)
            .max(1);
        let mut out = String::new();
        for (k, b) in &self.buckets {
            let info_len = ((b.info as f64 / max as f64) * width as f64).round() as usize;
            let warn_len = ((b.warn as f64 / max as f64) * width as f64).round() as usize;
            let err_len = ((b.error as f64 / max as f64) * width as f64).round() as usize;
            let bar = format!(
                "{}{}{}",
                "░".repeat(info_len),
                "▒".repeat(warn_len),
                "█".repeat(err_len)
            );
            out.push_str(&format!(
                "{k}  {bar}  {} (i{} w{} e{})\n",
                b.total, b.info, b.warn, b.error
            ));
        }
        out
    }
}

fn bucket_key(ts: DateTime<Utc>, interval: i64) -> i64 {
    ts.timestamp() / interval.max(1)
}
