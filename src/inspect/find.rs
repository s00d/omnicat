use std::fs;
use std::io::Read;
use std::ops::ControlFlow;
use std::path::Path;

use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use rusqlite::Connection;
use tar::Archive;
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::drivers::sql_dump::{find_in_sql_dump, is_sql_dump_path};
use crate::inspect::report::{FindMatch, FindReport};
use crate::io::{
    detect_compression, file_len, Compression, FileHandle, MmapView, OpenOptions, HUGE_FILE_BYTES,
};
use crate::orchestrator::registry::DriverRegistry;
use crate::search::{eval_pred, json_object_fields, line_fields, parse_find_pattern, FindQuery};
use crate::sinks::progress::ProgressReporter;

pub fn build_find(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    query: &str,
    config: &OmnicatConfig,
    progress: bool,
) -> Result<FindReport> {
    if query.is_empty() {
        bail!("empty find query");
    }
    let find = parse_find_pattern(query)?;
    let max = config.inspect.max_matches;
    let mut matches = Vec::new();
    let mut truncated = false;
    let mut progress = ProgressReporter::new("find", progress);

    match kind {
        HandlerKind::Archive => {
            find_in_archive(path, &find, max, &mut matches, &mut truncated)?;
        }
        HandlerKind::Database => {
            if is_sql_dump_path(path) {
                let (hits, trunc) = find_in_sql_dump(path, &find, max, config, &mut progress)?;
                truncated = trunc;
                for (loc, text) in hits {
                    matches.push(FindMatch {
                        location: Some(loc),
                        line: None,
                        text,
                    });
                }
            } else {
                find_in_sqlite(
                    path,
                    &find,
                    max,
                    &mut matches,
                    &mut truncated,
                    &mut progress,
                )?;
            }
        }
        HandlerKind::Pdf
        | HandlerKind::Document
        | HandlerKind::Presentation
        | HandlerKind::Ebook
        | HandlerKind::Email => {
            let text = extract_preview_text(path, kind, config)?;
            find_in_text(&text, &find, max, None, &mut matches, &mut truncated);
        }
        HandlerKind::Directory => {
            find_in_directory(path, &find, max, &mut matches, &mut truncated)?;
        }
        HandlerKind::Data => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "jsonl" || ext == "ndjson" {
                let max_bytes = effective_max_bytes(config);
                find_in_jsonl(
                    path,
                    &find,
                    max,
                    max_bytes,
                    &mut matches,
                    &mut truncated,
                    &mut progress,
                )?;
            } else if ext == "json" {
                find_in_json_file(path, &find, max, &mut matches, &mut truncated)?;
            } else {
                find_in_file_text(
                    path,
                    &find,
                    max,
                    config,
                    &mut matches,
                    &mut truncated,
                    &mut progress,
                )?;
            }
        }
        _ => {
            find_in_file_text(
                path,
                &find,
                max,
                config,
                &mut matches,
                &mut truncated,
                &mut progress,
            )?;
        }
    }

    progress.finish();

    Ok(FindReport {
        path: display.to_string(),
        query: query.to_string(),
        matches,
        truncated,
    })
}

fn effective_max_bytes(config: &OmnicatConfig) -> usize {
    if config.inspect.no_limit {
        0
    } else {
        config.inspect.max_bytes
    }
}

fn text_matches_find(text: &str, find: &FindQuery) -> bool {
    text_matches_find_needle(text, find, None)
}

fn text_matches_find_needle(text: &str, find: &FindQuery, needle_lower: Option<&str>) -> bool {
    if let Some(pred) = &find.pred {
        let fields = line_fields(text);
        if fields.is_empty() {
            return false;
        }
        return eval_pred(pred, &fields);
    }
    if let Some(q) = needle_lower {
        return ascii_contains_ignore_case(text, q);
    }
    if let Some(q) = &find.substring {
        return ascii_contains_ignore_case(text, &q.to_ascii_lowercase());
    }
    false
}

fn ascii_contains_ignore_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let n = needle_lower.as_bytes();
    haystack
        .as_bytes()
        .windows(n.len())
        .any(|w| eq_ignore_ascii_case(w, n))
}

fn eq_ignore_ascii_case(a: &[u8], b_lower: &[u8]) -> bool {
    a.len() == b_lower.len()
        && a.iter()
            .zip(b_lower.iter())
            .all(|(x, y)| x.to_ascii_lowercase() == *y)
}

fn json_matches_find(value: &serde_json::Value, find: &FindQuery) -> bool {
    json_matches_find_needle(value, find, None)
}

fn json_matches_find_needle(
    value: &serde_json::Value,
    find: &FindQuery,
    needle_lower: Option<&str>,
) -> bool {
    if let Some(pred) = &find.pred {
        let row = json_object_fields(value);
        if !row.is_empty() {
            return eval_pred(pred, &row);
        }
        return false;
    }
    let Some(ql) = needle_lower
        .map(|s| s.to_string())
        .or_else(|| find.substring.as_ref().map(|s| s.to_ascii_lowercase()))
    else {
        return false;
    };
    if let Some(msg) = value.get("message").and_then(|v| v.as_str()) {
        if ascii_contains_ignore_case(msg, &ql) {
            return true;
        }
    }
    if let Some(msg) = value.get("msg").and_then(|v| v.as_str()) {
        if ascii_contains_ignore_case(msg, &ql) {
            return true;
        }
    }
    ascii_contains_ignore_case(&value.to_string(), &ql)
}

fn find_in_file_text(
    path: &Path,
    find: &FindQuery,
    max: usize,
    config: &OmnicatConfig,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
    progress: &mut ProgressReporter,
) -> Result<()> {
    let max_bytes = effective_max_bytes(config);
    let len = file_len(path).unwrap_or(0);
    let total = if len > 0 { Some(len) } else { None };
    let compressed = detect_compression(path) != Compression::None;
    // mmap is only safe for uncompressed plain files
    if max_bytes == 0 && len > HUGE_FILE_BYTES && !compressed {
        find_in_mmap(path, find, max, matches, truncated, progress)?;
        return Ok(());
    }
    let scan_cap = if max_bytes == 0 {
        None
    } else {
        Some(max_bytes as u64)
    };
    let handle = FileHandle::open(
        path,
        OpenOptions {
            max_scan_bytes: scan_cap,
            ..OpenOptions::stream()
        },
    )?;
    let needle = find.substring.as_ref().map(|s| s.to_ascii_lowercase());
    let mut bytes_seen = 0u64;
    handle.for_each_line(|line| {
        if *truncated || matches.len() >= max {
            *truncated = true;
            return Ok(ControlFlow::Break(()));
        }
        let text = match line.as_str() {
            Some(s) => s.to_string(),
            None => line.text_lossy(),
        };
        bytes_seen = bytes_seen.saturating_add(text.len() as u64 + 1);
        progress.tick_bytes(bytes_seen, total);
        if text_matches_find_needle(&text, find, needle.as_deref()) {
            matches.push(FindMatch {
                location: None,
                line: Some(line.line_no),
                text,
            });
            if matches.len() >= max {
                *truncated = true;
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    })
}

fn find_in_mmap(
    path: &Path,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
    progress: &mut ProgressReporter,
) -> Result<()> {
    let view = MmapView::open(path)?;
    let total = Some(view.data.len() as u64);
    let mut bytes_seen = 0u64;
    let needle = find.substring.as_ref().map(|s| s.to_ascii_lowercase());
    for line in view.lines() {
        if *truncated || matches.len() >= max {
            *truncated = true;
            break;
        }
        let text = match line.as_str() {
            Some(s) => s.to_string(),
            None => line.text_lossy(),
        };
        bytes_seen = bytes_seen.saturating_add(text.len() as u64 + 1);
        progress.tick_bytes(bytes_seen, total);
        if text_matches_find_needle(&text, find, needle.as_deref()) {
            matches.push(FindMatch {
                location: None,
                line: Some(line.line_no),
                text,
            });
            if matches.len() >= max {
                *truncated = true;
            }
        }
    }
    Ok(())
}

fn find_in_jsonl(
    path: &Path,
    find: &FindQuery,
    max: usize,
    max_bytes: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
    progress: &mut ProgressReporter,
) -> Result<()> {
    let total = file_len(path).ok();
    let scan_cap = if max_bytes == 0 {
        None
    } else {
        Some(max_bytes as u64)
    };
    let handle = FileHandle::open(
        path,
        OpenOptions {
            max_scan_bytes: scan_cap,
            ..OpenOptions::stream()
        },
    )?;
    let needle = find.substring.as_ref().map(|s| s.to_ascii_lowercase());
    let mut bytes_seen = 0u64;
    handle.for_each_line(|line| {
        if *truncated || matches.len() >= max {
            *truncated = true;
            return Ok(ControlFlow::Break(()));
        }
        let text = match line.as_str() {
            Some(s) => s.to_string(),
            None => line.text_lossy(),
        };
        bytes_seen = bytes_seen.saturating_add(text.len() as u64 + 1);
        progress.tick_bytes(bytes_seen, total);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(ControlFlow::Continue(()));
        }
        let matched = if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            json_matches_find_needle(&value, find, needle.as_deref())
        } else {
            text_matches_find_needle(&text, find, needle.as_deref())
        };
        if matched {
            matches.push(FindMatch {
                location: None,
                line: Some(line.line_no),
                text,
            });
            if matches.len() >= max {
                *truncated = true;
                return Ok(ControlFlow::Break(()));
            }
        }
        Ok(ControlFlow::Continue(()))
    })
}

fn find_in_json_file(
    path: &Path,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let max_bytes = 8 * 1024 * 1024;
    let bytes = crate::io::read_bytes_capped(path, max_bytes, true)?;
    let text = String::from_utf8_lossy(&bytes);
    let value: serde_json::Value = serde_json::from_str(&text)?;
    match &value {
        serde_json::Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                if *truncated || matches.len() >= max {
                    *truncated = true;
                    break;
                }
                if json_matches_find(item, find) {
                    matches.push(FindMatch {
                        location: Some(format!("$[{}]", i)),
                        line: Some((i + 1) as u64),
                        text: item.to_string(),
                    });
                    if matches.len() >= max {
                        *truncated = true;
                    }
                }
            }
        }
        other => {
            if json_matches_find(other, find) {
                matches.push(FindMatch {
                    location: None,
                    line: Some(1),
                    text: other.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn find_in_text(
    text: &str,
    find: &FindQuery,
    max: usize,
    location: Option<&str>,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) {
    for (i, line) in text.lines().enumerate() {
        if text_matches_find(line, find) {
            if matches.len() >= max {
                *truncated = true;
                return;
            }
            matches.push(FindMatch {
                location: location.map(|s| s.to_string()),
                line: Some((i + 1) as u64),
                text: line.to_string(),
            });
        }
    }
}

fn find_in_sqlite(
    path: &Path,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
    progress: &mut ProgressReporter,
) -> Result<()> {
    let conn = Connection::open(path)?;
    let tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let mut rows_seen = 0u64;
    for table in tables {
        let sql = format!("SELECT * FROM \"{table}\" LIMIT 500");
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let col_count = stmt.column_count();
        let headers: Vec<String> = (0..col_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();
        let rows = stmt.query_map([], |row| {
            let mut cells = Vec::new();
            for i in 0..col_count {
                let val: rusqlite::types::Value = row.get(i)?;
                cells.push(match val {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    rusqlite::types::Value::Null => "NULL".into(),
                    rusqlite::types::Value::Blob(b) => format!("<blob {}>", b.len()),
                });
            }
            Ok(cells)
        })?;
        for (row_idx, row) in rows.enumerate() {
            let cells = row?;
            rows_seen += 1;
            progress.tick_count(rows_seen, "rows");
            let row_map: Vec<(String, String)> = headers
                .iter()
                .zip(cells.iter())
                .map(|(h, c)| (h.clone(), c.clone()))
                .collect();
            let matched = if let Some(pred) = &find.pred {
                eval_pred(pred, &row_map)
            } else if let Some(q) = &find.substring {
                let ql = q.to_ascii_lowercase();
                cells.iter().any(|c| c.to_ascii_lowercase().contains(&ql))
            } else {
                false
            };
            if matched {
                if matches.len() >= max {
                    *truncated = true;
                    return Ok(());
                }
                let text = row_map
                    .iter()
                    .map(|(c, v)| format!("{c}={v}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                matches.push(FindMatch {
                    location: Some(format!("{table} row {}", row_idx + 1)),
                    line: None,
                    text,
                });
            }
        }
    }
    Ok(())
}

fn find_in_archive(
    path: &Path,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = find
        .substring
        .as_ref()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if matches!(
        ext.as_str(),
        "zip" | "jar" | "war" | "ear" | "apk" | "ipa" | "xpi" | "whl" | "nupkg" | "epub" | "cbz"
    ) {
        let file = fs::File::open(path)?;
        let mut zip = ZipArchive::new(file)?;
        for i in 0..zip.len() {
            let mut zf = zip.by_index(i)?;
            let entry_name = zf.name().to_string();
            if !q.is_empty() && entry_name.to_ascii_lowercase().contains(&q) {
                push(
                    matches,
                    truncated,
                    max,
                    Some(entry_name.clone()),
                    None,
                    &entry_name,
                );
            }
            if zf.is_dir() || *truncated {
                continue;
            }
            // Search small text-like entries
            if zf.size() > 0 && zf.size() < 512_000 {
                let mut buf = Vec::new();
                if zf.read_to_end(&mut buf).is_ok() {
                    if let Ok(text) = std::str::from_utf8(&buf) {
                        find_in_text(text, find, max, Some(&entry_name), matches, truncated);
                    }
                }
            }
            if *truncated {
                break;
            }
        }
    } else if name.ends_with(".tar.gz")
        || ext == "tgz"
        || ext == "tar"
        || name.ends_with(".tar.zst")
        || name.ends_with(".tar.zstd")
    {
        let file = fs::File::open(path)?;
        if name.ends_with(".tar.gz") || ext == "tgz" {
            let mut archive = Archive::new(GzDecoder::new(file));
            scan_tar(&mut archive, find, max, matches, truncated)?;
        } else if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
            let decoder = zstd::Decoder::new(file)?;
            let mut archive = Archive::new(decoder);
            scan_tar(&mut archive, find, max, matches, truncated)?;
        } else {
            let mut archive = Archive::new(file);
            scan_tar(&mut archive, find, max, matches, truncated)?;
        }
    } else {
        // fallback: search entry names via tree
        let config = OmnicatConfig::default();
        let content = DriverRegistry::build(HandlerKind::Archive, path, &config)?;
        let text = content.plain_text();
        find_in_text(&text, find, max, None, matches, truncated);
    }
    Ok(())
}

fn scan_tar<R: Read>(
    archive: &mut Archive<R>,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = find
        .substring
        .as_ref()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    for ent in archive.entries()? {
        let mut ent = ent?;
        let p = ent.path()?.to_string_lossy().to_string();
        if !q.is_empty() && p.to_ascii_lowercase().contains(&q) {
            push(matches, truncated, max, Some(p.clone()), None, &p);
        }
        if ent.header().entry_type().is_file() {
            let size = ent.header().size().unwrap_or(0);
            if size > 0 && size < 512_000 {
                let mut buf = Vec::new();
                if ent.read_to_end(&mut buf).is_ok() {
                    if let Ok(text) = std::str::from_utf8(&buf) {
                        find_in_text(text, find, max, Some(&p), matches, truncated);
                    }
                }
            }
        }
        if *truncated {
            break;
        }
    }
    Ok(())
}

fn find_in_directory(
    path: &Path,
    find: &FindQuery,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = find
        .substring
        .as_ref()
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    fn walk(
        dir: &Path,
        find: &FindQuery,
        q: &str,
        max: usize,
        matches: &mut Vec<FindMatch>,
        truncated: &mut bool,
        depth: usize,
    ) -> Result<()> {
        if depth > 8 || *truncated {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !q.is_empty() && name.to_ascii_lowercase().contains(q) {
                push(
                    matches,
                    truncated,
                    max,
                    Some(entry.path().display().to_string()),
                    None,
                    &name,
                );
            }
            if entry.file_type()?.is_dir() {
                walk(&entry.path(), find, q, max, matches, truncated, depth + 1)?;
            } else if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) < 256_000 {
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    find_in_text(
                        &text,
                        find,
                        max,
                        Some(&entry.path().display().to_string()),
                        matches,
                        truncated,
                    );
                }
            }
            if *truncated {
                break;
            }
        }
        Ok(())
    }
    walk(path, find, &q, max, matches, truncated, 0)
}

fn extract_preview_text(path: &Path, kind: HandlerKind, config: &OmnicatConfig) -> Result<String> {
    let content = DriverRegistry::build(kind, path, config)?;
    Ok(content.plain_text())
}

fn push(
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
    max: usize,
    location: Option<String>,
    line: Option<u64>,
    text: &str,
) {
    if matches.len() >= max {
        *truncated = true;
        return;
    }
    matches.push(FindMatch {
        location,
        line,
        text: text.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;

    #[test]
    fn jsonl_field_aware_find() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("app.jsonl");
        std::fs::write(
            &p,
            r#"{"level":"info","message":"ok"}
{"level":"error","message":"timeout"}
"#,
        )
        .unwrap();
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "app.jsonl",
            HandlerKind::Data,
            "level:error",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        assert!(report.matches[0].text.contains("timeout"));
    }

    #[test]
    fn jsonl_substring_find_message() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("app.jsonl");
        std::fs::write(
            &p,
            r#"{"level":"error","message":"connection timeout"}
{"level":"info","message":"ready"}
"#,
        )
        .unwrap();
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "app.jsonl",
            HandlerKind::Data,
            "timeout",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
    }

    #[test]
    fn jsonl_sql_where_find() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("app.jsonl");
        std::fs::write(
            &p,
            r#"{"level":"info","message":"ok"}
{"level":"error","message":"timeout"}
"#,
        )
        .unwrap();
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "app.jsonl",
            HandlerKind::Data,
            "level = 'error'",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
    }

    #[test]
    fn sql_dump_find_shorthand_and_where() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/db/sample.sql");
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "sample.sql",
            HandlerKind::Database,
            "status:ok",
            &config,
            false,
        )
        .unwrap();
        assert!(!report.matches.is_empty());
        let report = build_find(
            &p,
            "sample.sql",
            HandlerKind::Database,
            "status = 'ok' AND id > 1",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
    }

    #[test]
    fn find_rejects_full_select() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.txt");
        std::fs::write(&p, "hi").unwrap();
        let config = OmnicatConfig::default();
        let err = build_find(
            &p,
            "x.txt",
            HandlerKind::Fallback,
            "SELECT * FROM users",
            &config,
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("--query") || err.to_string().contains("-q"));
    }

    #[test]
    fn substring_find_on_text() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.txt");
        std::fs::write(&p, "hello\nworld\n").unwrap();
        let config = OmnicatConfig::default();
        let report =
            build_find(&p, "x.txt", HandlerKind::Fallback, "world", &config, false).unwrap();
        assert_eq!(report.matches.len(), 1);
    }

    #[test]
    fn log_json_lines_find() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/log/json.log");
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "json.log",
            HandlerKind::Fallback,
            "level:error",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        assert!(report.matches[0].text.contains("timeout"));
        let report = build_find(
            &p,
            "json.log",
            HandlerKind::Fallback,
            "service = 'db' AND level = 'error'",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
    }

    #[test]
    fn log_logfmt_find() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/log/logfmt.log");
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "logfmt.log",
            HandlerKind::Fallback,
            "level:error",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        let report = build_find(
            &p,
            "logfmt.log",
            HandlerKind::Fallback,
            "job_id > 42",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 2);
    }

    #[test]
    fn log_tracing_level_case_insensitive() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/log/tracing.log");
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "tracing.log",
            HandlerKind::Fallback,
            "level:error",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        assert!(report.matches[0].text.contains("charge declined"));
    }

    #[test]
    fn plain_text_log_substring_only() {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/log/text.log");
        let config = OmnicatConfig::default();
        let report = build_find(
            &p,
            "text.log",
            HandlerKind::Fallback,
            "disk full",
            &config,
            false,
        )
        .unwrap();
        assert_eq!(report.matches.len(), 1);
        // no key=value → structured pred finds nothing
        let report = build_find(
            &p,
            "text.log",
            HandlerKind::Fallback,
            "level:error",
            &config,
            false,
        )
        .unwrap();
        assert!(report.matches.is_empty());
    }
}
