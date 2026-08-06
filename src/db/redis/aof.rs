//! Redis AOF command scanner (streaming, read-only).

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::db::report::RedisAofStats;
use crate::db::redis::rdb::TopResult;
use crate::io::source::open_reader;

const FIND_LIMIT: usize = 100;
const CHUNK: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub struct AofScanResult {
    pub stats: RedisAofStats,
    pub find_matches: Option<Vec<String>>,
    pub top: Option<TopResult>,
}

impl AofScanResult {
    pub fn merge(mut self, other: AofScanResult) -> Self {
        self.stats.commands += other.stats.commands;
        for (k, v) in other.stats.by_command {
            *self.stats.by_command.entry(k).or_default() += v;
        }
        if let Some(m) = other.find_matches {
            let acc = self.find_matches.get_or_insert_with(Vec::new);
            acc.extend(m);
            acc.truncate(FIND_LIMIT);
        }
        if other.top.is_some() {
            self.top = other.top;
        }
        self
    }
}

pub fn aof_paths_for_source(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_dir() {
        let manifest = path.join("appendonly.aof.manifest");
        if manifest.is_file() {
            return parse_manifest(&manifest);
        }
        let single = path.join("appendonly.aof");
        if single.is_file() {
            return Ok(vec![single]);
        }
        bail_if_missing(path)?;
    }
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".manifest"))
    {
        return parse_manifest(path);
    }
    Ok(vec![path.to_path_buf()])
}

fn bail_if_missing(path: &Path) -> Result<Vec<PathBuf>> {
    anyhow::bail!("no AOF file found under {}", path.display())
}

fn parse_manifest(path: &Path) -> Result<Vec<PathBuf>> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut files = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("file ") {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            files.push(dir.join(parts[1]));
        }
    }
    if files.is_empty() {
        bail_if_missing(dir)?;
    }
    Ok(files)
}

pub fn scan_aof(
    path: &Path,
    stats: bool,
    find: Option<&str>,
    top: Option<&str>,
    top_limit: usize,
) -> Result<AofScanResult> {
    let _ = stats;
    let mut reader = open_reader(path, true)?;
    let mut buf = Vec::with_capacity(CHUNK);
    let mut carry = Vec::new();
    let mut by_command = BTreeMap::new();
    let mut commands = 0u64;
    let mut find_matches = Vec::new();
    let mut top_counts: BTreeMap<String, u64> = BTreeMap::new();
    let top_field = top.unwrap_or("command");
    let top_limit = top_limit.max(1);
    let mut first_line: Option<String> = None;
    let mut skipped_preamble = false;

    loop {
        buf.resize(CHUNK, 0);
        let n = reader.read(&mut buf).context("read AOF")?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        if !skipped_preamble && carry.len() >= 5 && &carry[..5] == b"REDIS" {
            if let Some(pos) = memchr::memchr(b'*', &carry[5..]) {
                carry.drain(..5 + pos);
                skipped_preamble = true;
            }
        }
        while let Some(cmd) = parse_next_command(&carry) {
            if first_line.is_none() {
                first_line = Some(format!("*{}", cmd.parts.len()));
            }
            if let Some(name) = cmd.parts.first() {
                let upper = name.to_ascii_uppercase();
                commands += 1;
                *by_command.entry(upper.clone()).or_default() += 1;
                if top_field == "command" {
                    *top_counts.entry(upper).or_default() += 1;
                }
                if let Some(pat) = find {
                    if cmd.parts.iter().skip(1).any(|p| p.contains(pat))
                        && find_matches.len() < FIND_LIMIT
                    {
                        find_matches.push(cmd.parts.join(" "));
                    }
                }
            }
            carry.drain(..cmd.consumed);
        }
    }

    let stats = RedisAofStats {
        path: path.display().to_string(),
        commands,
        by_command,
        first_line,
        last_line: None,
    };

    let find_matches = if find.is_some() {
        Some(find_matches)
    } else {
        None
    };
    let top = if top.is_some() {
        let mut items: Vec<_> = top_counts.into_iter().collect();
        items.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
        items.truncate(top_limit);
        Some(TopResult {
            field: top_field.to_string(),
            items,
        })
    } else {
        None
    };

    Ok(AofScanResult {
        stats,
        find_matches,
        top,
    })
}

struct ParsedCommand {
    parts: Vec<String>,
    consumed: usize,
}

fn parse_next_command(buf: &[u8]) -> Option<ParsedCommand> {
    let text = String::from_utf8_lossy(buf);
    let normalized = text.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.is_empty() || !lines[0].starts_with('*') {
        return None;
    }
    let count: usize = lines[0][1..].trim().parse().ok()?;
    let mut parts = Vec::new();
    let mut line_idx = 1usize;
    let mut byte_offset = lines[0].len() + 1;
    for _ in 0..count {
        if line_idx >= lines.len() {
            return None;
        }
        let len_line = lines[line_idx];
        if !len_line.starts_with('$') {
            return None;
        }
        byte_offset += len_line.len() + 1;
        line_idx += 1;
        if line_idx >= lines.len() {
            return None;
        }
        let len: usize = len_line[1..].trim().parse().ok()?;
        let payload = lines[line_idx];
        parts.push(payload[..len.min(payload.len())].to_string());
        byte_offset += payload.len() + 1;
        line_idx += 1;
    }
    Some(ParsedCommand {
        parts,
        consumed: byte_offset.min(buf.len()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_set_commands() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("appendonly.aof");
        fs::write(
            &p,
            "*3\n$3\nSET\n$4\nuser\n$2\nok\n*3\n$3\nSET\n$4\nuser\n$2\nno\n",
        )
        .unwrap();
        let s = scan_aof(&p, true, None, None, 10).unwrap();
        assert_eq!(s.stats.commands, 2);
        assert_eq!(s.stats.by_command.get("SET"), Some(&2));
    }

    #[test]
    fn find_outputs_matches() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("appendonly.aof");
        fs::write(&p, "*3\n$3\nSET\n$8\nuser:123\n$2\nok\n").unwrap();
        let s = scan_aof(&p, true, Some("user:"), None, 10).unwrap();
        let m = s.find_matches.unwrap();
        assert_eq!(m.len(), 1);
        assert!(m[0].contains("user:123"));
    }
}
