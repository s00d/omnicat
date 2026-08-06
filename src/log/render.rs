//! Log line rendering (color, JSON pretty, stack traces).

use crate::content::{format_log_line, json_log_level};
use crate::inspect::text::sanitize_text;

pub fn render_log_line(line: &str) -> String {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(pretty) = format_log_line(&map) {
            let colored = colorize_by_level_text(&pretty);
            return if line.ends_with('\n') {
                format!("{colored}\n")
            } else {
                colored
            };
        }
    }
    colorize_log_line(line)
}

pub fn render_and_sanitize(line: &str, allow_unsafe: bool) -> String {
    sanitize_text(&render_log_line(line), allow_unsafe)
}

pub fn line_matches_level(line: &str, level: &str) -> bool {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(lvl) = json_log_level(&map) {
            return lvl == *level || lvl.starts_with(level);
        }
    }
    let lower = line.to_ascii_lowercase();
    lower.contains(level)
        || lower.contains(&format!("\"level\":\"{level}\""))
        || lower.contains(&format!("\"level\": \"{level}\""))
        || lower.contains(&format!("[{level}]"))
}

pub fn is_stack_continuation(line: &str) -> bool {
    let raw = line.trim_end_matches(['\r', '\n']);
    if raw.is_empty() {
        return false;
    }
    let trimmed = raw.trim_start();
    if trimmed.starts_with("Caused by:")
        || trimmed.starts_with("Suppressed:")
        || trimmed.starts_with("...")
        || trimmed.starts_with("at ")
        || trimmed.starts_with("\tat ")
    {
        return true;
    }
    (raw.starts_with('\t') || raw.starts_with("    "))
        && (trimmed.starts_with("at ")
            || trimmed.starts_with("...")
            || (trimmed.contains('(') && trimmed.contains(')')))
}

fn colorize_by_level_text(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let (color, reset) = level_color(&lower);
    if color.is_empty() {
        line.to_string()
    } else {
        format!("{color}{line}{reset}")
    }
}

fn colorize_log_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    let (color, reset) = level_color(&lower);
    if color.is_empty() {
        line.to_string()
    } else {
        format!("{color}{line}{reset}")
    }
}

fn level_color(lower: &str) -> (&'static str, &'static str) {
    if lower.contains("error") || lower.contains("fatal") {
        ("\x1b[31m", "\x1b[0m")
    } else if lower.contains("warn") {
        ("\x1b[33m", "\x1b[0m")
    } else if lower.contains(" info")
        || lower.starts_with("info")
        || lower.contains("  info  ")
        || lower.contains("info  ")
    {
        ("\x1b[36m", "\x1b[0m")
    } else if lower.contains("debug") || lower.contains("trace") {
        ("\x1b[90m", "\x1b[0m")
    } else {
        ("", "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_json_log_line() {
        let line = r#"{"ts":"12:01","level":"ERROR","service":"db","msg":"Timeout"}"#;
        let out = render_log_line(line);
        assert!(out.contains("Timeout"));
    }
}
