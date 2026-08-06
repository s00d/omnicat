//! Message rate buckets.

use std::collections::BTreeMap;

use crate::log::record::LogRecord;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct RateAgg {
    pub interval_secs: i64,
    pub buckets: BTreeMap<i64, RateBucket>,
    pub errors_only: bool,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct RateBucket {
    pub count: u64,
}

impl RateAgg {
    pub fn new(interval_secs: i64, errors_only: bool) -> Self {
        Self {
            interval_secs,
            buckets: BTreeMap::new(),
            errors_only,
        }
    }

    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        if self.errors_only && !rec.level.is_some_and(|l| l.is_errorish()) {
            return;
        }
        let Some(ts) = rec.timestamp else {
            return;
        };
        let key = ts.timestamp() / self.interval_secs.max(1);
        self.buckets.entry(key).or_default().count += 1;
    }

    pub fn render(&self) -> String {
        let mut out = String::from("Messages/sec (per bucket)\n");
        for (k, b) in &self.buckets {
            let rate = b.count as f64 / self.interval_secs.max(1) as f64;
            out.push_str(&format!("{k}  {rate:.1}\n"));
        }
        out
    }
}
