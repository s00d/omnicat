//! Redis RDB stats / sample / find via `rdb` crate.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use rdb::filter::Simple;
use rdb::formatter::Formatter;
use rdb::parse;

use crate::db::report::{KeySample, RedisRdbStats};

#[derive(Debug, Clone, Default)]
pub struct RdbScanOptions {
    pub stats: bool,
    pub sample: Option<usize>,
    pub find: Option<String>,
    pub top: Option<String>,
    pub top_limit: usize,
    pub schema: bool,
}

#[derive(Debug)]
pub struct RdbScanResult {
    pub stats: RedisRdbStats,
    pub samples: Option<Vec<KeySample>>,
    pub find_matches: Option<Vec<String>>,
    pub top: Option<TopResult>,
}

#[derive(Debug, Clone)]
pub struct TopResult {
    pub field: String,
    pub items: Vec<(String, u64)>,
}

#[derive(Default)]
struct StatsState {
    types: BTreeMap<String, u64>,
    patterns: BTreeMap<String, u64>,
    memory_estimate: u64,
    keys: u64,
    version: Option<String>,
    samples: Vec<KeySample>,
    find_matches: Vec<String>,
    largest: Vec<(String, u64)>,
}

pub fn scan_rdb(path: &Path, opts: &RdbScanOptions) -> Result<RdbScanResult> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::with_capacity(256 * 1024, file);
    let top_limit = opts.top_limit.max(1);
    let state = Arc::new(Mutex::new(StatsState::default()));
    let fmt = StatsFormatter {
        state: Arc::clone(&state),
        sample_limit: opts.sample.unwrap_or(0),
        find_pat: opts.find.clone(),
        find_limit: 100,
        schema_mode: opts.schema,
        top_limit,
    };
    let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse(reader, fmt, Simple::new())
    }));
    match parse_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e.into()),
        Err(_) => {
            anyhow::bail!(
                "unsupported or corrupt RDB file (Redis 8+ dumps may require a newer parser)"
            );
        }
    }

    let snap = state.lock().expect("stats lock");
    let stats = RedisRdbStats {
        path: path.display().to_string(),
        version: snap.version.clone(),
        keys: snap.keys,
        types: snap.types.clone(),
        memory_estimate: snap.memory_estimate,
        patterns: snap.patterns.clone(),
    };

    let samples = if opts.sample.is_some() {
        Some(snap.samples.clone())
    } else {
        None
    };
    let find_matches = if opts.find.is_some() {
        Some(snap.find_matches.clone())
    } else {
        None
    };
    let top = if opts.top.is_some() {
        let field = opts.top.clone().unwrap_or_else(|| "size".into());
        let mut items = snap.largest.clone();
        items.sort_by_key(|(_, sz)| std::cmp::Reverse(*sz));
        items.truncate(top_limit);
        Some(TopResult { field, items })
    } else {
        None
    };

    Ok(RdbScanResult {
        stats,
        samples,
        find_matches,
        top,
    })
}

struct StatsFormatter {
    state: Arc<Mutex<StatsState>>,
    sample_limit: usize,
    find_pat: Option<String>,
    find_limit: usize,
    schema_mode: bool,
    top_limit: usize,
}

impl StatsFormatter {
    fn note_key(&self, key: &[u8], kind: &str, payload: u64) {
        let mut s = self.state.lock().expect("stats lock");
        s.keys += 1;
        *s.types.entry(kind.to_string()).or_default() += 1;
        s.memory_estimate = s.memory_estimate.saturating_add(key.len() as u64 + payload);

        let key_str = String::from_utf8_lossy(key).into_owned();
        if self.schema_mode {
            let prefix = infer_prefix(&key_str);
            *s.patterns.entry(format!("{prefix} → {kind}")).or_default() += 1;
        }

        if let Some(ref pat) = self.find_pat {
            if glob_match(pat, &key_str) && s.find_matches.len() < self.find_limit {
                s.find_matches.push(key_str.clone());
            }
        }

        if self.sample_limit > 0 && s.samples.len() < self.sample_limit {
            s.samples.push(KeySample {
                key: key_str.clone(),
                kind: kind.to_string(),
                size: key.len() as u64 + payload,
            });
        }

        push_largest(
            &mut s.largest,
            key_str,
            key.len() as u64 + payload,
            self.top_limit,
        );
    }
}

impl Formatter for StatsFormatter {
    fn aux_field(&mut self, key: &[u8], value: &[u8]) {
        if key == b"redis-ver" {
            self.state.lock().expect("stats lock").version =
                Some(String::from_utf8_lossy(value).into_owned());
        }
    }

    fn string(&mut self, key: &[u8], value: &[u8], _expiry: &Option<u64>) {
        self.note_key(key, "string", value.len() as u64);
    }

    fn hash(&mut self, key: &[u8], values: &IndexMap<Vec<u8>, Vec<u8>>, _expiry: &Option<u64>) {
        let payload: u64 = values
            .iter()
            .map(|(k, v)| k.len() as u64 + v.len() as u64)
            .sum();
        self.note_key(key, "hash", payload);
    }

    fn set(&mut self, key: &[u8], values: &[Vec<u8>], _expiry: &Option<u64>) {
        let payload: u64 = values.iter().map(|v| v.len() as u64).sum();
        self.note_key(key, "set", payload);
    }

    fn list(&mut self, key: &[u8], values: &[Vec<u8>], _expiry: &Option<u64>) {
        let payload: u64 = values.iter().map(|v| v.len() as u64).sum();
        self.note_key(key, "list", payload);
    }

    fn sorted_set(&mut self, key: &[u8], values: &[(f64, Vec<u8>)], _expiry: &Option<u64>) {
        let payload: u64 = values.iter().map(|(_, v)| v.len() as u64).sum();
        self.note_key(key, "zset", payload);
    }
}

fn infer_prefix(key: &str) -> String {
    if let Some(i) = key.find(':') {
        format!("{}:*", &key[..i])
    } else if key.len() > 12 {
        format!("{}…", &key[..8])
    } else {
        key.to_string()
    }
}

fn push_largest(top: &mut Vec<(String, u64)>, key: String, size: u64, limit: usize) {
    top.push((key, size));
    top.sort_by_key(|(_, sz)| std::cmp::Reverse(*sz));
    top.truncate(limit);
}

fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(star) = pattern.find('*') {
        let (pre, rest) = pattern.split_at(star);
        let suf = &rest[1..];
        if pre.is_empty() {
            return text.ends_with(suf);
        }
        if suf.is_empty() {
            return text.starts_with(pre);
        }
        return text.starts_with(pre) && text.ends_with(suf) && text.len() >= pre.len() + suf.len();
    }
    text.contains(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_prefix_suffix() {
        assert!(glob_match("user:*", "user:123"));
        assert!(!glob_match("user:*", "other:123"));
    }
}
