//! K-way merge of multiple log files by timestamp.

use std::path::Path;

use anyhow::Result;

use crate::io::{FileHandle, OpenOptions};
use crate::log::parse;
use crate::log::record::{LogFormat, LogRecord};

/// Scan multiple files and yield records sorted by timestamp (best-effort).
pub fn merge_files<F>(paths: &[&Path], mut on_record: F) -> Result<()>
where
    F: FnMut(LogRecord<'_>) -> Result<()>,
{
    if paths.is_empty() {
        return Ok(());
    }
    if paths.len() == 1 {
        return scan_file(paths[0], on_record);
    }

    let mut all: Vec<(i64, usize, LogRecord<'static>)> = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let mut sample = Vec::new();
        let handle = FileHandle::open(*path, OpenOptions::stream())?;
        handle.for_each_line(|line| {
            if sample.len() < 32 {
                sample.push(line.text_lossy());
            }
            Ok(())
        })?;
        let fmt = parse::detect_from_lines(&sample.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        scan_file_collect(path, fmt, i, &mut all)?;
    }
    all.sort_by_key(|(ts, order, _)| (*ts, *order));
    for (_, _, rec) in all {
        on_record(rec)?;
    }
    Ok(())
}

fn scan_file<F>(path: &Path, on_record: F) -> Result<()>
where
    F: FnMut(LogRecord<'_>) -> Result<()>,
{
    let mut sample = Vec::new();
    let handle = FileHandle::open(path, OpenOptions::stream())?;
    handle.for_each_line(|line| {
        if sample.len() < 32 {
            sample.push(line.text_lossy());
        }
        Ok(())
    })?;
    let fmt = parse::detect_from_lines(&sample.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    scan_file_with_format(path, fmt, on_record)
}

pub fn scan_file_with_format<F>(path: &Path, fmt: LogFormat, mut on_record: F) -> Result<()>
where
    F: FnMut(LogRecord<'_>) -> Result<()>,
{
    let path_str = path.to_string_lossy().to_string();
    let handle = FileHandle::open(path, OpenOptions::stream())?;
    handle.for_each_line(|line| {
        let text = line.as_str().unwrap_or("");
        let mut rec = parse::parse_line_with_format(text, fmt);
        rec = rec.with_source(&path_str);
        on_record(rec)
    })
}

fn scan_file_collect(
    path: &Path,
    fmt: LogFormat,
    order: usize,
    out: &mut Vec<(i64, usize, LogRecord<'static>)>,
) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let handle = FileHandle::open(path, OpenOptions::stream())?;
    handle.for_each_line(|line| {
        let text = line.as_str().unwrap_or("");
        let mut rec = parse::parse_line_with_format(text, fmt);
        rec = rec.with_source(&path_str);
        let ts = rec.timestamp.map(|t| t.timestamp()).unwrap_or(0);
        out.push((ts, order, to_owned(rec)));
        Ok(())
    })
}

pub fn to_owned(rec: LogRecord<'_>) -> LogRecord<'static> {
    LogRecord {
        timestamp: rec.timestamp,
        level: rec.level,
        message: rec.message.to_string().into(),
        service: rec.service.map(|s| s.to_string().into()),
        request_id: rec.request_id.map(|s| s.to_string().into()),
        trace_id: rec.trace_id.map(|s| s.to_string().into()),
        span_id: rec.span_id.map(|s| s.to_string().into()),
        correlation_id: rec.correlation_id.map(|s| s.to_string().into()),
        method: rec.method.map(|s| s.to_string().into()),
        path: rec.path.map(|s| s.to_string().into()),
        status: rec.status,
        client_ip: rec.client_ip.map(|s| s.to_string().into()),
        duration_ms: rec.duration_ms,
        raw_line: rec.raw_line.to_string().into(),
        source_file: rec.source_file.map(|s| s.to_string().into()),
        format: rec.format,
    }
}
