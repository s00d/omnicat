//! Log command dispatch and scanning pipeline.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::Datelike;

use crate::io::{file_len, FileHandle, OpenOptions, ReadMode, DEFAULT_TAIL_LINES, HUGE_FILE_BYTES};
use crate::log::aggregate::Aggregators;
use crate::log::context::context_around_ts;
use crate::log::correlate::build_trace_report;
use crate::log::filter::{apply_level_flag, parse_time_bound, parse_where, LogFilter};
use crate::log::merge::{merge_files, scan_file_with_format, to_owned};
use crate::log::options::LogOptions;
use crate::log::parse;
use crate::log::record::LogFormat;
use crate::log::render::render_and_sanitize;
use crate::log::report::LogReport;
use crate::sinks::log_report::write_log_report;
use crate::sinks::progress::ProgressBar;

pub fn run_log(paths: &[PathBuf], opts: &LogOptions) -> Result<()> {
    if paths.is_empty() {
        bail!("log: at least one file required");
    }

    if opts.follow {
        if paths.len() != 1 {
            bail!("--follow supports a single file");
        }
        return run_follow(&paths[0], opts);
    }

    if opts.wants_context() {
        return run_context(paths, opts);
    }

    if opts.request.is_some() || opts.trace.is_some() {
        return run_correlate(paths, opts);
    }

    if opts.wants_aggregate() {
        return run_aggregate(paths, opts);
    }

    run_print(paths, opts)
}

fn build_filter(opts: &LogOptions) -> LogFilter {
    let mut f = LogFilter::default();
    if opts.errors {
        f.errors_only = true;
    }
    if opts.warnings {
        f.warnings_only = true;
    }
    if let Some(ref lvl) = opts.level {
        apply_level_flag(&mut f, lvl);
    }
    if let Some(ref w) = opts.where_clause {
        f.where_clauses = parse_where(w);
    }
    if let Some(ref s) = opts.since {
        f.since = parse_time_bound(s);
    }
    if let Some(ref u) = opts.until {
        f.until = parse_time_bound(u);
    }
    f.status = opts.status;
    f.method = opts.method.clone();
    f.request_id = opts.request.clone();
    f.trace_id = opts.trace.clone();
    f
}

fn run_print(paths: &[PathBuf], opts: &LogOptions) -> Result<()> {
    let filter = build_filter(opts);
    let mut lines_out = Vec::new();
    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();

    let mut format = LogFormat::Unknown;
    if paths.len() == 1 {
        format = detect_format_file(&paths[0])?;
    }

    let multi = paths.len() > 1;
    let mut scan = |rec: crate::log::record::LogRecord<'_>| -> Result<()> {
        if !filter.matches(&rec) {
            return Ok(());
        }
        let rendered = if opts.json {
            serde_json::to_string(&rec).unwrap_or_else(|_| rec.raw_line.to_string())
        } else {
            let body = render_and_sanitize(&rec.raw_line, opts.allow_unsafe);
            if multi {
                let file = rec
                    .source_file
                    .as_deref()
                    .and_then(|p| Path::new(p).file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
                format!("[{file}] {body}")
            } else {
                body
            }
        };
        lines_out.push(rendered.trim_end_matches('\n').to_string());
        Ok(())
    };

    if paths.len() > 1 {
        merge_files(&path_refs, scan)?;
    } else {
        let mode = pick_read_mode(&paths[0], opts)?;
        if let ReadMode::Tail { lines } = mode {
            let handle = FileHandle::open(&paths[0], OpenOptions::tail(lines))?;
            handle.for_each_line(|line| {
                let text = line.as_str().unwrap_or("");
                let rec = parse::parse_line_with_format(text, format);
                scan(rec)
            })?;
        } else {
            scan_file_with_format(&paths[0], format, scan)?;
        }
    }

    if let Some(n) = opts.head {
        lines_out.truncate(n);
    }
    if let Some(n) = opts.tail {
        let start = lines_out.len().saturating_sub(n);
        lines_out = lines_out[start..].to_vec();
    }

    let report = LogReport::Lines { lines: lines_out };
    let mut stdout = io::stdout().lock();
    write_log_report(&report, opts.json, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}

fn run_aggregate(paths: &[PathBuf], opts: &LogOptions) -> Result<()> {
    let filter = build_filter(opts);
    let mut agg = Aggregators::new(600, opts.slow_limit.max(20));
    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let show_progress = opts.progress
        || paths
            .iter()
            .any(|p| file_len(p).unwrap_or(0) > HUGE_FILE_BYTES);
    let mut progress = if show_progress {
        Some(ProgressBar::new("Scanning log"))
    } else {
        None
    };

    merge_files(&path_refs, |rec| {
        if filter.matches(&rec) {
            agg.observe(&rec);
        }
        if let Some(ref mut pb) = progress {
            pb.tick(agg.counters.messages);
        }
        Ok(())
    })?;

    if let Some(pb) = progress {
        pb.finish();
    }

    let report = if let Some(ref q) = opts.query {
        LogReport::Query {
            result: run_aggregate_query(q, &agg),
        }
    } else {
        LogReport::from_aggregators(&agg, opts)
    };

    let mut stdout = io::stdout().lock();
    write_log_report(&report, opts.json, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}

fn run_correlate(paths: &[PathBuf], opts: &LogOptions) -> Result<()> {
    let id = opts.request.clone().or_else(|| opts.trace.clone()).unwrap();
    let mut collected = Vec::new();
    let path_refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    merge_files(&path_refs, |rec| {
        if rec.matches_id(&id) {
            collected.push(crate::log::merge::to_owned(rec));
        }
        Ok(())
    })?;
    let trace = build_trace_report(&id, &collected);
    let report = LogReport::Trace(trace);
    let mut stdout = io::stdout().lock();
    write_log_report(&report, opts.json, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}

fn run_follow(path: &Path, opts: &LogOptions) -> Result<()> {
    let level = opts.level.as_deref().or(if opts.errors {
        Some("error")
    } else if opts.warnings {
        Some("warn")
    } else {
        None
    });
    let filter = build_filter(opts);
    crate::log::follow::follow_path(path, level, opts.allow_unsafe, &filter)
}

fn run_context(paths: &[PathBuf], opts: &LogOptions) -> Result<()> {
    if paths.len() != 1 {
        bail!("--around/--context supports a single file");
    }
    let around = opts
        .around
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("--around TIME required with --context"))?;
    let window = opts.context.unwrap_or(5);
    let filter = build_filter(opts);
    let mut records = Vec::new();
    scan_file_with_format(&paths[0], detect_format_file(&paths[0])?, |rec| {
        if filter.matches(&rec) {
            records.push(to_owned(rec));
        }
        Ok(())
    })?;
    let lines = match parse_around_time(around).or_else(|| parse_time_bound(around)) {
        Some(target) => context_around_ts(&records, target, window),
        None => bail!("could not parse --around time: {around}"),
    };
    let report = LogReport::Context { lines };
    let mut stdout = io::stdout().lock();
    write_log_report(&report, opts.json, &mut stdout)?;
    stdout.flush()?;
    Ok(())
}

/// Parse `HH:MM:SS` or `HH:MM` for --around (matched against record timestamps).
fn parse_around_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{NaiveDate, NaiveTime, Utc};
    let s = s.trim();
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M:%S") {
        let today = Utc::now().date_naive();
        return NaiveDate::from_ymd_opt(today.year(), today.month(), today.day())
            .and_then(|d| d.and_time(t).and_local_timezone(Utc).single());
    }
    if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
        let today = Utc::now().date_naive();
        return NaiveDate::from_ymd_opt(today.year(), today.month(), today.day())
            .and_then(|d| d.and_time(t).and_local_timezone(Utc).single());
    }
    None
}

fn detect_format_file(path: &Path) -> Result<LogFormat> {
    let mut sample = Vec::new();
    FileHandle::open(path, OpenOptions::stream())?.for_each_line(|line| {
        if sample.len() < 32 {
            sample.push(line.text_lossy());
        }
        Ok(())
    })?;
    Ok(parse::detect_from_lines(
        &sample.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    ))
}

fn pick_read_mode(path: &Path, opts: &LogOptions) -> Result<ReadMode> {
    if let Some(n) = opts.tail {
        return Ok(ReadMode::Tail { lines: n });
    }
    if opts.all {
        return Ok(ReadMode::Stream);
    }
    let len = file_len(path)?;
    if len > HUGE_FILE_BYTES {
        Ok(ReadMode::Tail {
            lines: DEFAULT_TAIL_LINES,
        })
    } else {
        Ok(ReadMode::Stream)
    }
}

fn run_aggregate_query(query: &str, agg: &Aggregators) -> String {
    let q = query.trim();
    let ql = q.to_ascii_lowercase();
    if ql.contains("count") && ql.contains("level") {
        return serde_json::to_string_pretty(&agg.counters.levels).unwrap_or_default();
    }
    if ql.starts_with("top ") {
        let mut field = "message".to_string();
        let mut limit = 20usize;
        let rest = q.strip_prefix("top ").unwrap_or(q).trim();
        if rest.eq_ignore_ascii_case("errors") {
            field = "errors".into();
        } else {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if !parts.is_empty() && !parts[0].eq_ignore_ascii_case("limit") {
                field = parts[0].to_string();
            }
            if let Some(pos) = parts.iter().position(|p| p.eq_ignore_ascii_case("limit")) {
                if let Some(n) = parts.get(pos + 1).and_then(|s| s.parse().ok()) {
                    limit = n;
                }
            }
        }
        let items = match field.as_str() {
            "errors" => agg.top_errors.top(limit),
            "endpoints" | "path" => agg.top_endpoint.top(limit),
            "ips" | "ip" => agg.top_ip.top(limit),
            _ => agg.top_message.top(limit),
        };
        return serde_json::to_string_pretty(
            &items
                .into_iter()
                .map(|(k, c)| crate::log::report::TopItem { key: k, count: c })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
    }
    format!("query not understood: {query}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::aggregate::Aggregators;
    use crate::log::level::LogLevel;
    use crate::log::record::LogFormat;

    #[test]
    fn query_top_message_limit() {
        let mut agg = Aggregators::new(600, 20);
        for i in 0..3 {
            agg.top_message.observe(&crate::log::record::LogRecord {
                timestamp: None,
                level: Some(LogLevel::Info),
                message: format!("err-{i}").into(),
                service: None,
                request_id: None,
                trace_id: None,
                span_id: None,
                correlation_id: None,
                method: None,
                path: None,
                status: None,
                client_ip: None,
                duration_ms: None,
                raw_line: "x".into(),
                source_file: None,
                format: LogFormat::Unknown,
            });
        }
        let out = run_aggregate_query("top message limit 2", &agg);
        assert!(out.contains("err-"));
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }
}
