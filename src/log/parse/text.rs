//! Heuristic text/syslog log parser.

use std::borrow::Cow;

use chrono::{DateTime, Utc};

use crate::log::level::{parse_level, LogLevel};
use crate::log::record::{LogFormat, LogRecord};

pub fn looks_like_text(line: &str) -> bool {
    let t = line.trim();
    (t.len() > 19 && t.as_bytes().get(4) == Some(&b'-'))
        || t.contains(" ERROR ")
        || t.contains(" WARN ")
        || t.contains(" INFO ")
        || t.starts_with("ERROR ")
        || t.starts_with("WARN ")
}

pub fn parse_line<'a>(line: &'a str) -> LogRecord<'a> {
    let mut rec = LogRecord::from_line(line);
    rec.format = LogFormat::Text;
    let t = line.trim();

    // ISO-ish timestamp prefix (RFC3339 or `YYYY-MM-DD HH:MM:SS`)
    if let Some(sp) = t.find(|c: char| c.is_ascii_whitespace()) {
        let prefix = &t[..sp];
        if let Ok(dt) = DateTime::parse_from_rfc3339(prefix) {
            rec.timestamp = Some(dt.with_timezone(&Utc));
        } else if let Ok(nd) = chrono::NaiveDateTime::parse_from_str(prefix, "%Y-%m-%d %H:%M:%S") {
            rec.timestamp = Some(nd.and_utc());
        }
    } else if t.len() >= 19 {
        if let Ok(dt) = DateTime::parse_from_rfc3339(&t[..19.min(t.len())]) {
            rec.timestamp = Some(dt.with_timezone(&Utc));
        } else if let Ok(nd) = chrono::NaiveDateTime::parse_from_str(
            &t.chars().take(19).collect::<String>(),
            "%Y-%m-%d %H:%M:%S",
        ) {
            rec.timestamp = Some(nd.and_utc());
        }
    }

    for token in t.split_whitespace() {
        if let Some(lvl) = parse_level(token.trim_matches(|c| c == '[' || c == ']' || c == ':')) {
            rec.level = Some(lvl);
            break;
        }
    }

    if rec.level.is_none() {
        let upper = t.to_ascii_uppercase();
        for (pat, lvl) in [
            (" ERROR ", LogLevel::Error),
            (" FATAL ", LogLevel::Fatal),
            (" WARN ", LogLevel::Warn),
            (" INFO ", LogLevel::Info),
            (" DEBUG ", LogLevel::Debug),
            (" TRACE ", LogLevel::Trace),
        ] {
            if upper.contains(pat) {
                rec.level = Some(lvl);
                break;
            }
        }
    }

    rec.message = Cow::Borrowed(t);
    rec
}
