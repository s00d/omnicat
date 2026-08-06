//! JSONL column ordering — prefer log-shaped TIME/LEVEL/SERVICE/MESSAGE.

/// Display headers + underlying JSON keys (parallel arrays).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlColumns {
    pub headers: Vec<String>,
    pub source_keys: Vec<String>,
}

const TIME_ALIASES: &[&str] = &["timestamp", "time", "ts", "@timestamp", "datetime", "date"];
const LEVEL_ALIASES: &[&str] = &["level", "severity", "sev", "log_level", "loglevel"];
const SERVICE_ALIASES: &[&str] = &[
    "service",
    "logger",
    "component",
    "app",
    "application",
    "module",
    "name",
];
const MESSAGE_ALIASES: &[&str] = &["message", "msg", "text", "body", "event"];

/// Plan column order from discovered JSON object keys.
///
/// When at least two log-ish fields exist, uses fixed labels TIME/LEVEL/SERVICE/MESSAGE
/// and appends remaining keys. Otherwise keeps first-seen order.
pub fn plan_columns(keys: &[String]) -> JsonlColumns {
    let time = find_alias(keys, TIME_ALIASES);
    let level = find_alias(keys, LEVEL_ALIASES);
    let service = find_alias(keys, SERVICE_ALIASES);
    let message = find_alias(keys, MESSAGE_ALIASES);

    let log_hits = [&time, &level, &service, &message]
        .iter()
        .filter(|o| o.is_some())
        .count();

    if log_hits < 2 {
        return JsonlColumns {
            headers: keys.to_vec(),
            source_keys: keys.to_vec(),
        };
    }

    let mut headers = Vec::new();
    let mut source_keys = Vec::new();
    let mut used = std::collections::BTreeSet::new();

    let push = |label: &str,
                key: Option<&String>,
                headers: &mut Vec<String>,
                source_keys: &mut Vec<String>,
                used: &mut std::collections::BTreeSet<String>| {
        if let Some(k) = key {
            headers.push(label.into());
            source_keys.push(k.clone());
            used.insert(k.clone());
        }
    };

    push(
        "TIME",
        time.as_ref(),
        &mut headers,
        &mut source_keys,
        &mut used,
    );
    push(
        "LEVEL",
        level.as_ref(),
        &mut headers,
        &mut source_keys,
        &mut used,
    );
    push(
        "SERVICE",
        service.as_ref(),
        &mut headers,
        &mut source_keys,
        &mut used,
    );
    push(
        "MESSAGE",
        message.as_ref(),
        &mut headers,
        &mut source_keys,
        &mut used,
    );

    for k in keys {
        if used.insert(k.clone()) {
            headers.push(k.clone());
            source_keys.push(k.clone());
        }
    }

    JsonlColumns {
        headers,
        source_keys,
    }
}

fn find_alias(keys: &[String], aliases: &[&str]) -> Option<String> {
    for alias in aliases {
        if let Some(k) = keys.iter().find(|k| k.eq_ignore_ascii_case(alias)) {
            return Some(k.clone());
        }
    }
    None
}

pub fn cell_value(map: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    map.get(key)
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default()
}

/// Format a JSON object as a compact log line (`TIME LEVEL SERVICE MESSAGE …`).
/// Returns `None` when the object is not log-shaped (≥2 log aliases).
pub fn format_log_line(map: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let keys: Vec<String> = map.keys().cloned().collect();
    let cols = plan_columns(&keys);
    let is_log = cols
        .headers
        .iter()
        .any(|h| matches!(h.as_str(), "TIME" | "LEVEL" | "SERVICE" | "MESSAGE"));
    if !is_log {
        return None;
    }
    let parts: Vec<String> = cols
        .source_keys
        .iter()
        .zip(cols.headers.iter())
        .map(|(key, _)| {
            let v = cell_value(map, key);
            if v.is_empty() {
                "-".into()
            } else {
                v
            }
        })
        .collect();
    Some(parts.join("  "))
}

/// Extract normalized log level from a JSON object, if present.
pub fn json_log_level(map: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let keys: Vec<String> = map.keys().cloned().collect();
    let level_key = find_alias(&keys, LEVEL_ALIASES)?;
    let v = cell_value(map, &level_key);
    if v.is_empty() {
        None
    } else {
        Some(v.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_shaped_columns() {
        let keys = vec![
            "extra".into(),
            "msg".into(),
            "level".into(),
            "service".into(),
            "ts".into(),
        ];
        let cols = plan_columns(&keys);
        assert_eq!(
            cols.headers,
            vec!["TIME", "LEVEL", "SERVICE", "MESSAGE", "extra"]
        );
        assert_eq!(
            cols.source_keys,
            vec!["ts", "level", "service", "msg", "extra"]
        );
    }

    #[test]
    fn plain_jsonl_keeps_order() {
        let keys = vec!["id".into(), "name".into(), "age".into()];
        let cols = plan_columns(&keys);
        assert_eq!(cols.headers, keys);
        assert_eq!(cols.source_keys, keys);
    }
}
