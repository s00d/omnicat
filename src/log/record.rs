//! Parsed log record — minimal superset for app + access logs.

use std::borrow::Cow;

use chrono::{DateTime, Utc};

use crate::log::level::LogLevel;

/// A single parsed log entry (fields borrowed from line when possible).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct LogRecord<'a> {
    pub timestamp: Option<DateTime<Utc>>,
    pub level: Option<LogLevel>,
    pub message: Cow<'a, str>,
    pub service: Option<Cow<'a, str>>,
    pub request_id: Option<Cow<'a, str>>,
    pub trace_id: Option<Cow<'a, str>>,
    pub span_id: Option<Cow<'a, str>>,
    pub correlation_id: Option<Cow<'a, str>>,
    pub method: Option<Cow<'a, str>>,
    pub path: Option<Cow<'a, str>>,
    pub status: Option<u16>,
    pub client_ip: Option<Cow<'a, str>>,
    pub duration_ms: Option<f64>,
    pub raw_line: Cow<'a, str>,
    pub source_file: Option<Cow<'a, str>>,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub enum LogFormat {
    #[default]
    Unknown,
    Json,
    Logfmt,
    Text,
    Nginx,
    Tracing,
}

impl<'a> LogRecord<'a> {
    pub fn from_line(line: &'a str) -> Self {
        Self {
            message: Cow::Borrowed(line),
            raw_line: Cow::Borrowed(line),
            ..Default::default()
        }
    }

    pub fn with_source(mut self, path: &str) -> Self {
        self.source_file = Some(Cow::Owned(path.to_string()));
        self
    }

    pub fn matches_id(&self, id: &str) -> bool {
        let id = id.to_ascii_lowercase();
        [
            self.request_id.as_deref(),
            self.trace_id.as_deref(),
            self.correlation_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|v| v.to_ascii_lowercase().contains(&id))
    }
}
