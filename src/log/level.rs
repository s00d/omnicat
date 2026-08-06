//! Log level normalization and comparison.

use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Notice = 3,
    Warn = 4,
    Error = 5,
    Critical = 6,
    Fatal = 7,
    Panic = 8,
}

impl LogLevel {
    pub fn is_errorish(self) -> bool {
        matches!(
            self,
            LogLevel::Error | LogLevel::Critical | LogLevel::Fatal | LogLevel::Panic
        )
    }

    pub fn is_warningish(self) -> bool {
        self.is_errorish() || self == LogLevel::Warn
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Notice => "NOTICE",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Critical => "CRITICAL",
            Self::Fatal => "FATAL",
            Self::Panic => "PANIC",
        }
    }
}

impl FromStr for LogLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_level(s).ok_or(())
    }
}

/// Parse level string with common aliases.
pub fn parse_level(s: &str) -> Option<LogLevel> {
    match s.trim().to_ascii_uppercase().as_str() {
        "TRACE" | "TRC" => Some(LogLevel::Trace),
        "DEBUG" | "DBG" | "D" => Some(LogLevel::Debug),
        "INFO" | "INF" | "I" => Some(LogLevel::Info),
        "NOTICE" | "NTC" => Some(LogLevel::Notice),
        "WARN" | "WARNING" | "WRN" | "W" => Some(LogLevel::Warn),
        "ERROR" | "ERR" | "E" => Some(LogLevel::Error),
        "CRITICAL" | "CRIT" => Some(LogLevel::Critical),
        "FATAL" | "FTL" => Some(LogLevel::Fatal),
        "PANIC" => Some(LogLevel::Panic),
        _ => None,
    }
}

/// Parse `--level warn+` style threshold.
pub fn parse_level_threshold(s: &str) -> Option<(LogLevel, bool)> {
    let s = s.trim();
    if let Some(base) = s.strip_suffix('+') {
        parse_level(base).map(|l| (l, true))
    } else {
        parse_level(s).map(|l| (l, false))
    }
}

pub fn level_matches(record: Option<LogLevel>, threshold: LogLevel, or_higher: bool) -> bool {
    match record {
        Some(l) if or_higher => l >= threshold,
        Some(l) => l == threshold,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warn_plus_includes_error() {
        let (t, plus) = parse_level_threshold("warn+").unwrap();
        assert!(plus);
        assert!(level_matches(Some(LogLevel::Error), t, plus));
        assert!(!level_matches(Some(LogLevel::Info), t, plus));
    }
}
