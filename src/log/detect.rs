//! Log format auto-detection from sample lines.

use crate::log::parse;
use crate::log::record::LogFormat;

const SAMPLE: usize = 32;

/// Detect dominant log format from first lines of a file.
pub fn detect_format(lines: &[&str]) -> LogFormat {
    let mut scores = [0u32; 6];
    for line in lines.iter().take(SAMPLE) {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if parse::json::looks_like_json(t) {
            scores[LogFormat::Json as usize] += 2;
        }
        if parse::logfmt::looks_like_logfmt(t) {
            scores[LogFormat::Logfmt as usize] += 2;
        }
        if parse::nginx::looks_like_nginx(t) {
            scores[LogFormat::Nginx as usize] += 3;
        }
        if parse::tracing::looks_like_tracing(t) {
            scores[LogFormat::Tracing as usize] += 2;
        }
        if parse::text::looks_like_text(t) {
            scores[LogFormat::Text as usize] += 1;
        }
    }
    let best = scores
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| *s)
        .map(|(i, s)| (i, *s))
        .unwrap_or((0, 0));
    if best.1 == 0 {
        return LogFormat::Unknown;
    }
    match best.0 {
        1 => LogFormat::Json,
        2 => LogFormat::Logfmt,
        3 => LogFormat::Text,
        4 => LogFormat::Nginx,
        5 => LogFormat::Tracing,
        _ => LogFormat::Unknown,
    }
}
