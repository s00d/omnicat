//! Log format parsers.

pub mod json;
pub mod logfmt;
pub mod nginx;
pub mod text;
pub mod tracing;

use crate::log::detect;
use crate::log::record::{LogFormat, LogRecord};

/// Parse a line using auto-detected or fixed format.
pub fn parse_line_with_format<'a>(line: &'a str, format: LogFormat) -> LogRecord<'a> {
    match format {
        LogFormat::Json => json::parse_line(line).unwrap_or_else(|| text::parse_line(line)),
        LogFormat::Logfmt => logfmt::parse_line(line).unwrap_or_else(|| text::parse_line(line)),
        LogFormat::Nginx => nginx::parse_line(line).unwrap_or_else(|| text::parse_line(line)),
        LogFormat::Tracing => tracing::parse_line(line).unwrap_or_else(|| text::parse_line(line)),
        LogFormat::Text | LogFormat::Unknown => json::parse_line(line)
            .or_else(|| logfmt::parse_line(line))
            .or_else(|| nginx::parse_line(line))
            .or_else(|| tracing::parse_line(line))
            .unwrap_or_else(|| text::parse_line(line)),
    }
}

/// Detect format from sample and parse.
pub fn parse_line_auto<'a>(line: &'a str, detected: LogFormat) -> LogRecord<'a> {
    parse_line_with_format(line, detected)
}

pub fn detect_from_lines(lines: &[&str]) -> LogFormat {
    detect::detect_format(lines)
}
