//! Extract field maps from log / plain-text lines (JSON lines, logfmt / key=value).

use super::pred::json_object_fields;

/// Parse a single line into searchable fields.
///
/// Order:
/// 1. JSON object (line starts with `{`)
/// 2. logfmt / `key=value` pairs (quoted or bare values)
///
/// Returns empty when nothing structured is found (caller falls back to substring).
pub fn line_fields(line: &str) -> Vec<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let mut fields = json_object_fields(&value);
            if !fields.is_empty() {
                normalize_log_fields(&mut fields);
                return fields;
            }
        }
    }

    let mut fields = parse_kv_pairs(trimmed);
    if !fields.is_empty() {
        normalize_log_fields(&mut fields);
    }
    fields
}

fn normalize_log_fields(fields: &mut Vec<(String, String)>) {
    for (k, v) in fields.iter_mut() {
        if is_level_key(k) {
            *v = v.to_ascii_lowercase();
        }
    }
    let has_msg = fields.iter().any(|(k, _)| k.eq_ignore_ascii_case("msg"));
    let has_message = fields
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("message"));
    if has_msg && !has_message {
        if let Some(v) = fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("msg"))
            .map(|(_, v)| v.clone())
        {
            fields.push(("message".into(), v));
        }
    } else if has_message && !has_msg {
        if let Some(v) = fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("message"))
            .map(|(_, v)| v.clone())
        {
            fields.push(("msg".into(), v));
        }
    }
}

fn is_level_key(k: &str) -> bool {
    k.eq_ignore_ascii_case("level")
        || k.eq_ignore_ascii_case("severity")
        || k.eq_ignore_ascii_case("lvl")
        || k.eq_ignore_ascii_case("log_level")
}

fn is_key_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_key_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'/')
}

/// Scan `key=value` / `key="quoted value"` pairs on bytes (no `Vec<char>`).
fn parse_kv_pairs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        if !is_key_start(bytes[i]) {
            while i < len && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }

        let key_start = i;
        i += 1;
        while i < len && is_key_cont(bytes[i]) {
            i += 1;
        }
        let key = String::from_utf8_lossy(&bytes[key_start..i]).into_owned();

        if i >= len || bytes[i] != b'=' {
            continue;
        }
        i += 1;

        if i >= len {
            out.push((key, String::new()));
            break;
        }

        let value = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let mut val = String::new();
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 1;
                    val.push(bytes[i] as char);
                    i += 1;
                } else {
                    val.push(bytes[i] as char);
                    i += 1;
                }
            }
            if i < len && bytes[i] == quote {
                i += 1;
            }
            val
        } else {
            let v_start = i;
            while i < len && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            String::from_utf8_lossy(&bytes[v_start..i]).into_owned()
        };

        out.push((key, value));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{eval_pred, parse_find_pattern};

    #[test]
    fn json_line_fields() {
        let f = line_fields(r#"{"level":"error","service":"db","message":"timeout"}"#);
        assert!(f.iter().any(|(k, v)| k == "level" && v == "error"));
        assert!(f.iter().any(|(k, v)| k == "message" && v == "timeout"));
        assert!(f.iter().any(|(k, v)| k == "msg" && v == "timeout"));
    }

    #[test]
    fn logfmt_fields_and_level_case() {
        let f =
            line_fields(r#"2026-08-06T10:03:03Z level=ERROR span=checkout msg="charge declined""#);
        assert!(f.iter().any(|(k, v)| k == "level" && v == "error"));
        assert!(f.iter().any(|(k, v)| k == "span" && v == "checkout"));
        assert!(f.iter().any(|(k, v)| k == "msg" && v == "charge declined"));
    }

    #[test]
    fn plain_text_no_fields() {
        assert!(line_fields("2026-08-06 ERROR disk full").is_empty());
    }

    #[test]
    fn pred_matches_logfmt() {
        let f = line_fields(r#"ts=1 level=info service=worker msg="ok""#);
        let q = parse_find_pattern("level:info").unwrap();
        assert!(eval_pred(q.pred.as_ref().unwrap(), &f));
    }
}
