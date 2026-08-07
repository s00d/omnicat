use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use rusqlite::Connection;
use tar::Archive;
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::query::row_matches_predicate;
use crate::inspect::report::{FindMatch, FindReport};
use crate::io::{file_len, FileHandle, MmapView, OpenOptions, HUGE_FILE_BYTES};
use crate::orchestrator::registry::DriverRegistry;

pub fn build_find(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    query: &str,
    config: &OmnicatConfig,
) -> Result<FindReport> {
    if query.is_empty() {
        bail!("empty find query");
    }
    let max = config.inspect.max_matches;
    let mut matches = Vec::new();
    let mut truncated = false;

    match kind {
        HandlerKind::Archive => {
            find_in_archive(path, query, max, &mut matches, &mut truncated)?;
        }
        HandlerKind::Database => {
            find_in_sqlite(path, query, max, &mut matches, &mut truncated)?;
        }
        HandlerKind::Pdf | HandlerKind::Document | HandlerKind::Presentation | HandlerKind::Ebook | HandlerKind::Email => {
            let text = extract_preview_text(path, kind, config)?;
            find_in_text(&text, query, max, None, &mut matches, &mut truncated);
        }
        HandlerKind::Directory => {
            find_in_directory(path, query, max, &mut matches, &mut truncated)?;
        }
        HandlerKind::Data => {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext == "jsonl" || ext == "ndjson" {
                let max_bytes = effective_max_bytes(config);
                find_in_jsonl(path, query, max, max_bytes, &mut matches, &mut truncated)?;
            } else if ext == "json" {
                find_in_json_file(path, query, max, &mut matches, &mut truncated)?;
            } else {
                find_in_file_text(path, query, max, config, &mut matches, &mut truncated)?;
            }
        }
        _ => {
            find_in_file_text(path, query, max, config, &mut matches, &mut truncated)?;
        }
    }

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

fn find_in_file_text(
    path: &Path,
    query: &str,
    max: usize,
    config: &OmnicatConfig,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let max_bytes = effective_max_bytes(config);
    let len = file_len(path).unwrap_or(0);
    if max_bytes == 0 && len > HUGE_FILE_BYTES {
        find_in_mmap(path, query, max, matches, truncated)?;
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
    let q = query.to_ascii_lowercase();
    handle.for_each_line(|line| {
        if *truncated || matches.len() >= max {
            *truncated = true;
            return Ok(());
        }
        let text = line.text_lossy();
        if text.to_ascii_lowercase().contains(&q) {
            matches.push(FindMatch {
                location: None,
                line: Some(line.line_no),
                text,
            });
            if matches.len() >= max {
                *truncated = true;
            }
        }
        Ok(())
    })
}

fn find_in_mmap(
    path: &Path,
    query: &str,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let view = MmapView::open(path)?;
    let q = query.as_bytes();
    let q_lower = query.to_ascii_lowercase();
    for line in view.lines() {
        if *truncated || matches.len() >= max {
            *truncated = true;
            break;
        }
        let bytes = line.bytes;
        let hay = if bytes.iter().any(|b| b.is_ascii_uppercase()) {
            line.text_lossy()
        } else {
            String::new()
        };
        let matched = if hay.is_empty() {
            memchr::memmem::find(bytes, q).is_some()
                || memchr::memmem::find(bytes, q_lower.as_bytes()).is_some()
        } else {
            hay.to_ascii_lowercase().contains(&q_lower)
        };
        if matched {
            let text = if hay.is_empty() {
                line.text_lossy()
            } else {
                hay
            };
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

fn is_structured_find_query(query: &str) -> bool {
    let q = query.trim();
    if ["==", "!=", ">=", "<=", ">", "<"]
        .iter()
        .any(|op| q.contains(op))
    {
        return true;
    }
    if let Some((left, right)) = q.split_once(':') {
        let field = left.trim();
        let val = right.trim();
        return !field.is_empty()
            && !val.is_empty()
            && !field.contains(' ')
            && field
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    }
    false
}

fn json_line_matches(value: &serde_json::Value, query: &str) -> Result<bool> {
    if is_structured_find_query(query) {
        return row_matches_predicate(value, query);
    }
    let q = query.to_ascii_lowercase();
    if let Some(msg) = value.get("message").and_then(|v| v.as_str()) {
        if msg.to_ascii_lowercase().contains(&q) {
            return Ok(true);
        }
    }
    if let Some(msg) = value.get("msg").and_then(|v| v.as_str()) {
        if msg.to_ascii_lowercase().contains(&q) {
            return Ok(true);
        }
    }
    Ok(value.to_string().to_ascii_lowercase().contains(&q))
}

fn find_in_jsonl(
    path: &Path,
    query: &str,
    max: usize,
    max_bytes: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
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
    handle.for_each_line(|line| {
        if *truncated || matches.len() >= max {
            *truncated = true;
            return Ok(());
        }
        let text = line.text_lossy();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let matched = if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            json_line_matches(&value, query)?
        } else {
            text.to_ascii_lowercase()
                .contains(&query.to_ascii_lowercase())
        };
        if matched {
            matches.push(FindMatch {
                location: None,
                line: Some(line.line_no),
                text,
            });
            if matches.len() >= max {
                *truncated = true;
            }
        }
        Ok(())
    })
}

fn find_in_json_file(
    path: &Path,
    query: &str,
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
                if json_line_matches(item, query)? {
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
            if json_line_matches(other, query)? {
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
    query: &str,
    max: usize,
    location: Option<&str>,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) {
    let q = query.to_ascii_lowercase();
    for (i, line) in text.lines().enumerate() {
        if line.to_ascii_lowercase().contains(&q) {
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
    query: &str,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let conn = Connection::open(path)?;
    let tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let collected: Vec<String> = rows.filter_map(|r| r.ok()).collect();
        collected
    };
    let q = query.to_ascii_lowercase();
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
                    rusqlite::types::Value::Null => String::new(),
                    rusqlite::types::Value::Blob(b) => format!("<blob {}>", b.len()),
                });
            }
            Ok(cells)
        })?;
        for (row_idx, row) in rows.enumerate() {
            let cells = row?;
            for (ci, cell) in cells.iter().enumerate() {
                if cell.to_ascii_lowercase().contains(&q) {
                    if matches.len() >= max {
                        *truncated = true;
                        return Ok(());
                    }
                    matches.push(FindMatch {
                        location: Some(format!(
                            "{table}.{} row {}",
                            headers.get(ci).map(String::as_str).unwrap_or("?"),
                            row_idx + 1
                        )),
                        line: None,
                        text: cell.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn find_in_archive(
    path: &Path,
    query: &str,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = query.to_ascii_lowercase();
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
            if entry_name.to_ascii_lowercase().contains(&q) {
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
                        find_in_text(text, query, max, Some(&entry_name), matches, truncated);
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
            scan_tar(&mut archive, query, max, matches, truncated)?;
        } else if name.ends_with(".tar.zst") || name.ends_with(".tar.zstd") {
            let decoder = zstd::Decoder::new(file)?;
            let mut archive = Archive::new(decoder);
            scan_tar(&mut archive, query, max, matches, truncated)?;
        } else {
            let mut archive = Archive::new(file);
            scan_tar(&mut archive, query, max, matches, truncated)?;
        }
    } else {
        // fallback: search entry names via tree
        let config = OmnicatConfig::default();
        let content = DriverRegistry::build(HandlerKind::Archive, path, &config)?;
        let text = content.plain_text();
        find_in_text(&text, query, max, None, matches, truncated);
    }
    Ok(())
}

fn scan_tar<R: Read>(
    archive: &mut Archive<R>,
    query: &str,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = query.to_ascii_lowercase();
    for ent in archive.entries()? {
        let mut ent = ent?;
        let p = ent.path()?.to_string_lossy().to_string();
        if p.to_ascii_lowercase().contains(&q) {
            push(matches, truncated, max, Some(p.clone()), None, &p);
        }
        if ent.header().entry_type().is_file() {
            let size = ent.header().size().unwrap_or(0);
            if size > 0 && size < 512_000 {
                let mut buf = Vec::new();
                if ent.read_to_end(&mut buf).is_ok() {
                    if let Ok(text) = std::str::from_utf8(&buf) {
                        find_in_text(text, query, max, Some(&p), matches, truncated);
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
    query: &str,
    max: usize,
    matches: &mut Vec<FindMatch>,
    truncated: &mut bool,
) -> Result<()> {
    let q = query.to_ascii_lowercase();
    fn walk(
        dir: &Path,
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
            if name.to_ascii_lowercase().contains(q) {
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
                walk(&entry.path(), q, max, matches, truncated, depth + 1)?;
            } else if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) < 256_000 {
                if let Ok(text) = fs::read_to_string(entry.path()) {
                    find_in_text(
                        &text,
                        q,
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
    walk(path, &q, max, matches, truncated, 0)
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
        let report =
            build_find(&p, "app.jsonl", HandlerKind::Data, "level:error", &config).unwrap();
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
        let report = build_find(&p, "app.jsonl", HandlerKind::Data, "timeout", &config).unwrap();
        assert_eq!(report.matches.len(), 1);
    }
}
