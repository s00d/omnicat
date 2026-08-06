//! HTTP access log aggregates.

use std::collections::HashMap;

use crate::log::record::LogRecord;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct HttpAgg {
    pub requests: u64,
    pub s2xx: u64,
    pub s3xx: u64,
    pub s4xx: u64,
    pub s5xx: u64,
    pub methods: HashMap<String, u64>,
}

impl HttpAgg {
    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        let Some(st) = rec.status else {
            return;
        };
        self.requests += 1;
        match st {
            200..=299 => self.s2xx += 1,
            300..=399 => self.s3xx += 1,
            400..=499 => self.s4xx += 1,
            500..=599 => self.s5xx += 1,
            _ => {}
        }
        if let Some(ref m) = rec.method {
            *self.methods.entry(m.to_string()).or_default() += 1;
        }
    }
}
