//! JSON log line parser.

use std::borrow::Cow;

use chrono::{DateTime, Utc};

use crate::content::json_log_level;
use crate::log::record::{LogFormat, LogRecord};

const TIME_ALIASES: &[&str] = &["timestamp", "time", "ts", "@timestamp", "datetime", "date"];
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

pub fn looks_like_json(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('{') && t.ends_with('}')
}

pub fn parse_line<'a>(line: &'a str) -> Option<LogRecord<'a>> {
    let trimmed = line.trim();
    let v: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    let map = v.as_object()?;
    let mut rec = LogRecord::from_line(line);
    rec.format = LogFormat::Json;

    if let Some(lvl) = json_log_level(map) {
        rec.level = crate::log::level::parse_level(&lvl);
    }
    rec.message = field_str(map, MESSAGE_ALIASES)
        .map(|s| Cow::Owned(s.to_string()))
        .unwrap_or_else(|| Cow::Borrowed(trimmed));
    rec.service = field_str(map, SERVICE_ALIASES).map(|s| Cow::Owned(s.to_string()));
    rec.timestamp = field_str(map, TIME_ALIASES).and_then(parse_ts);

    for (k, val) in map {
        let kl = k.to_ascii_lowercase();
        let vs = value_as_str(val);
        match kl.as_str() {
            "request_id" | "requestid" | "req_id" => rec.request_id = Some(Cow::Owned(vs)),
            "trace_id" | "traceid" => rec.trace_id = Some(Cow::Owned(vs)),
            "span_id" | "spanid" => rec.span_id = Some(Cow::Owned(vs)),
            "correlation_id" | "correlationid" => rec.correlation_id = Some(Cow::Owned(vs)),
            "duration" | "duration_ms" | "latency" | "elapsed" => {
                rec.duration_ms = parse_duration(&vs);
            }
            "status" | "status_code" | "http_status" => rec.status = vs.parse().ok(),
            "method" | "http_method" => rec.method = Some(Cow::Owned(vs)),
            "path" | "url" | "uri" => rec.path = Some(Cow::Owned(vs)),
            "ip" | "client_ip" | "remote_addr" => rec.client_ip = Some(Cow::Owned(vs)),
            _ => {}
        }
    }
    Some(rec)
}

fn field_str<'a>(
    map: &'a serde_json::Map<String, serde_json::Value>,
    aliases: &[&str],
) -> Option<&'a str> {
    for alias in aliases {
        if let Some(v) = map.get(*alias).or_else(|| {
            map.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(alias))
                .map(|(_, v)| v)
        }) {
            return v.as_str();
        }
    }
    None
}

fn value_as_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|n| n.and_utc())
        })
}

fn parse_duration(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    if let Some(num) = s.strip_suffix("ms") {
        return num.parse().ok();
    }
    if let Some(num) = s.strip_suffix('s') {
        return num.parse::<f64>().ok().map(|v| v * 1000.0);
    }
    None
}
