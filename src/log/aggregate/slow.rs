//! Slow operation tracking by duration field.

use crate::log::record::LogRecord;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SlowEntry {
    pub duration_ms: f64,
    pub label: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SlowAgg {
    pub entries: Vec<SlowEntry>,
    pub limit: usize,
}

impl SlowAgg {
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            limit,
        }
    }

    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        let Some(d) = rec.duration_ms else {
            return;
        };
        if self.entries.len() >= self.limit * 4 {
            return;
        }
        let label = if let (Some(m), Some(p)) = (&rec.method, &rec.path) {
            format!("{m} {p}")
        } else {
            rec.message.to_string()
        };
        self.entries.push(SlowEntry {
            duration_ms: d,
            label,
            timestamp: rec.timestamp.map(|t| t.to_rfc3339()),
        });
    }

    pub fn top(&self, n: usize) -> Vec<&SlowEntry> {
        let mut sorted: Vec<_> = self.entries.iter().collect();
        sorted.sort_by(|a, b| b.duration_ms.partial_cmp(&a.duration_ms).unwrap());
        sorted.truncate(n);
        sorted
    }
}
