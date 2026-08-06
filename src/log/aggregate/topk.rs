//! Top-K counters with message template normalization.

use std::collections::HashMap;

use crate::log::record::LogRecord;

const MAX_TEMPLATES: usize = 10_000;

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct TopKAgg {
    pub field: String,
    pub counts: HashMap<String, u64>,
}

impl TopKAgg {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            counts: HashMap::new(),
        }
    }

    pub fn observe(&mut self, rec: &LogRecord<'_>) {
        if self.counts.len() >= MAX_TEMPLATES {
            return;
        }
        let key = match self.field.as_str() {
            "message" | "msg" => normalize_message(&rec.message),
            "service" => rec
                .service
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            "path" | "endpoint" => rec.path.as_ref().map(|s| s.to_string()).unwrap_or_default(),
            "ip" | "client_ip" => rec
                .client_ip
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            "status" => rec.status.map(|s| s.to_string()).unwrap_or_default(),
            _ => rec.message.to_string(),
        };
        if !key.is_empty() {
            *self.counts.entry(key).or_default() += 1;
        }
    }

    pub fn top(&self, limit: usize) -> Vec<(String, u64)> {
        let mut v: Vec<_> = self.counts.iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by_key(|b| std::cmp::Reverse(b.1));
        v.truncate(limit);
        v
    }
}

/// Normalize dynamic tokens in log messages.
pub fn normalize_message(msg: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let bytes = msg.as_bytes();
    while i < bytes.len() {
        // IPv4
        if let Some(rest) = msg.get(i..) {
            if let Some(m) = simple_ipv4(rest) {
                out.push_str("<IP>");
                i += m;
                continue;
            }
        }
        // UUID
        if msg.get(i..).is_some_and(|s| s.len() >= 36) {
            let slice = &msg[i..i + 36];
            if uuid_like(slice) {
                out.push_str("<UUID>");
                i += 36;
                continue;
            }
        }
        // Long numeric ID
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i - start >= 4 {
                out.push_str("<ID>");
            } else {
                out.push_str(&msg[start..i]);
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn simple_ipv4(s: &str) -> Option<usize> {
    let mut end = 0usize;
    let mut dots = 0u8;
    for (i, c) in s.char_indices() {
        if c.is_ascii_digit() {
            end = i + 1;
        } else if c == '.' && dots < 3 {
            dots += 1;
            end = i + 1;
        } else {
            break;
        }
    }
    if dots != 3 || end == 0 {
        return None;
    }
    let candidate = &s[..end];
    let octets: Vec<&str> = candidate.split('.').collect();
    if octets.len() != 4 {
        return None;
    }
    for o in octets {
        if o.is_empty() || o.parse::<u8>().is_err() {
            return None;
        }
    }
    Some(end)
}

fn uuid_like(s: &str) -> bool {
    let b: Vec<_> = s.chars().collect();
    b.len() == 36 && b[8] == '-' && b[13] == '-' && b[18] == '-' && b[23] == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ip_and_id() {
        let n = normalize_message("request 192.168.1.10 failed for user 12345");
        assert!(n.contains("<IP>"));
        assert!(n.contains("<ID>"));
    }
}
