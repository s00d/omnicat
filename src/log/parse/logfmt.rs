//! logfmt parser (key=value pairs).

use std::borrow::Cow;

use crate::log::level::parse_level;
use crate::log::record::{LogFormat, LogRecord};

pub fn looks_like_logfmt(line: &str) -> bool {
    let t = line.trim();
    !t.starts_with('{')
        && t.contains('=')
        && t.split_whitespace().filter(|p| p.contains('=')).count() >= 2
}

pub fn parse_line<'a>(line: &'a str) -> Option<LogRecord<'a>> {
    if !looks_like_logfmt(line) {
        return None;
    }
    let mut rec = LogRecord::from_line(line);
    rec.format = LogFormat::Logfmt;
    let mut msg = None;
    for part in split_logfmt(line) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = unquote(v.trim());
        match key.as_str() {
            "level" | "lvl" | "severity" => rec.level = parse_level(&val),
            "msg" | "message" => msg = Some(val),
            "service" | "app" | "component" => rec.service = Some(Cow::Owned(val)),
            "request_id" | "req_id" => rec.request_id = Some(Cow::Owned(val)),
            "trace_id" => rec.trace_id = Some(Cow::Owned(val)),
            "span_id" => rec.span_id = Some(Cow::Owned(val)),
            "correlation_id" => rec.correlation_id = Some(Cow::Owned(val)),
            "duration" | "duration_ms" | "latency" => {
                rec.duration_ms = val
                    .parse()
                    .ok()
                    .or_else(|| val.strip_suffix("ms").and_then(|n| n.parse().ok()));
            }
            "status" | "status_code" => rec.status = val.parse().ok(),
            "method" => rec.method = Some(Cow::Owned(val)),
            "path" | "url" => rec.path = Some(Cow::Owned(val)),
            _ => {}
        }
    }
    if let Some(m) = msg {
        rec.message = Cow::Owned(m);
    }
    Some(rec)
}

fn split_logfmt(line: &str) -> Vec<&str> {
    // Simple split respecting quoted values
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if !in_quote => in_quote = true,
            b'"' if in_quote => in_quote = false,
            b' ' if !in_quote => {
                if i > start {
                    out.push(&line[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < line.len() {
        out.push(&line[start..]);
    }
    out
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
