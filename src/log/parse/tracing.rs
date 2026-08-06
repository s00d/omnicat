//! OpenTelemetry / Rust tracing style logs.

use std::borrow::Cow;

use crate::log::level::parse_level;
use crate::log::record::{LogFormat, LogRecord};

pub fn looks_like_tracing(line: &str) -> bool {
    let t = line.trim();
    t.contains("trace_id=")
        || t.contains("span_id=")
        || t.contains("traceId=")
        || (t.contains("level=") && t.contains("span"))
}

pub fn parse_line<'a>(line: &'a str) -> Option<LogRecord<'a>> {
    if !looks_like_tracing(line) {
        return None;
    }
    let mut rec = LogRecord::from_line(line);
    rec.format = LogFormat::Tracing;
    for part in line.split_whitespace() {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let key = k.to_ascii_lowercase();
        let val = v.trim_matches('"');
        match key.as_str() {
            "level" | "lvl" => rec.level = parse_level(val),
            "trace_id" | "traceid" => rec.trace_id = Some(Cow::Owned(val.to_string())),
            "span_id" | "spanid" => rec.span_id = Some(Cow::Owned(val.to_string())),
            "request_id" => rec.request_id = Some(Cow::Owned(val.to_string())),
            "msg" | "message" => rec.message = Cow::Owned(val.to_string()),
            _ => {}
        }
    }
    Some(rec)
}
