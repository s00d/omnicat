use std::path::Path;

use anyhow::Result;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

use crate::config::OmnicatConfig;
use crate::content::{HexContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::theme::resolve_theme;
use crate::drivers::PreviewDriver;

pub struct FallbackDriver;

impl PreviewDriver for FallbackDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Fallback
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let max = config.terminal.fallback.max_bytes;
        let bytes = std::fs::read(path)?;
        let slice = if bytes.len() > max {
            &bytes[..max]
        } else {
            &bytes[..]
        };

        // Unknown, but readable as text: show it as text and try to
        // syntax-highlight it based on what the content looks like.
        if let Some(text) = decode_text(slice) {
            if config.terminal.plain {
                return Ok(PreviewContent::Text(text));
            }
            if let Some(syntax_name) = sniff_syntax(&text) {
                if let Ok(highlighted) = highlight_to_ansi(&text, syntax_name, config) {
                    return Ok(PreviewContent::Text(highlighted));
                }
            }
            return Ok(PreviewContent::Text(text));
        }

        // Binary / undetermined: lead with metadata gathered from the file,
        // then a hex dump of the leading bytes.
        let metadata = if config.terminal.fallback.show_metadata {
            let mime = ctx
                .mime
                .clone()
                .or_else(|| infer::get(&bytes).map(|kind| kind.mime_type().to_string()))
                .unwrap_or_else(|| "unknown (binary data)".to_string());
            format!(
                "file: {}\nsize: {} bytes\nmime: {}",
                path.display(),
                ctx.size,
                mime
            )
        } else {
            String::new()
        };

        Ok(PreviewContent::Hex(HexContent {
            bytes: slice.to_vec(),
            metadata,
        }))
    }
}

/// Decode `slice` as UTF-8 text, tolerating a multi-byte character truncated by
/// the read cap, and rejecting content that looks binary (NUL bytes or many
/// control characters).
fn decode_text(slice: &[u8]) -> Option<String> {
    if slice.is_empty() {
        return Some(String::new());
    }

    let text = match std::str::from_utf8(slice) {
        Ok(text) => text,
        Err(err) => {
            // Accept only when the sole invalid part is a character truncated at
            // the very end of the capped slice (a few trailing bytes).
            let valid = err.valid_up_to();
            if err.error_len().is_none() && slice.len() - valid <= 3 && valid > 0 {
                std::str::from_utf8(&slice[..valid]).ok()?
            } else {
                return None;
            }
        }
    };

    if text.contains('\u{0}') {
        return None;
    }
    let total = text.chars().count();
    if total == 0 {
        return Some(text.to_string());
    }
    let control = text
        .chars()
        .filter(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        .count();
    if control * 100 / total > 30 {
        return None;
    }
    Some(text.to_string())
}

/// Guess a syntect syntax name from the content of an unknown text file.
fn sniff_syntax(text: &str) -> Option<&'static str> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let first = trimmed.as_bytes()[0];
    if first == b'{' || first == b'[' {
        return Some("JSON");
    }
    if first == b'<' {
        let lower = trimmed[..trimmed.len().min(256)].to_ascii_lowercase();
        if lower.contains("<!doctype html") || lower.contains("<html") {
            return Some("HTML");
        }
        return Some("XML");
    }
    if trimmed.starts_with("---") {
        return Some("YAML");
    }

    let mut config_like = 0usize;
    let mut yaml_like = 0usize;
    let mut considered = 0usize;
    for line in text.lines().take(40) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        considered += 1;
        if line.starts_with('#') || line.starts_with(';') {
            config_like += 1;
            continue;
        }
        if (line.starts_with('[') && line.ends_with(']')) || is_key_value(line, '=') {
            config_like += 1;
        } else if is_key_value(line, ':') {
            yaml_like += 1;
        }
    }
    if considered == 0 {
        return None;
    }
    if config_like * 2 >= considered {
        return Some("INI");
    }
    if yaml_like * 2 >= considered {
        return Some("YAML");
    }
    None
}

/// True when `line` looks like `key<sep>value` with a plausible identifier key.
fn is_key_value(line: &str, sep: char) -> bool {
    match line.split_once(sep) {
        Some((key, _)) => {
            let key = key.trim();
            !key.is_empty()
                && key.len() <= 64
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        }
        None => false,
    }
}

fn highlight_to_ansi(text: &str, syntax_name: &str, config: &OmnicatConfig) -> Result<String> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps
        .find_syntax_by_name(syntax_name)
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    let theme = resolve_theme(&ts, &config.terminal.code.theme);
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut out = String::new();
    for line in LinesWithEndings::from(text) {
        let ranges = highlighter.highlight_line(line, &ps)?;
        out.push_str(&as_24_bit_terminal_escaped(&ranges[..], false));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;
    use crate::content::preview_context;

    fn plain_config() -> OmnicatConfig {
        let mut cfg = OmnicatConfig::default();
        cfg.terminal.plain = true;
        cfg
    }

    #[test]
    fn binary_fallback_hex() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"\x00\x01SENTINEL").unwrap();
        let cfg = OmnicatConfig::default();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &cfg, &ctx).unwrap();
        match content {
            PreviewContent::Hex(h) => {
                assert!(h.bytes.contains(&0x00));
                assert!(h.metadata.contains("size:"));
            }
            other => panic!("expected hex, got {other:?}"),
        }
    }

    #[test]
    fn text_fallback_for_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.txt");
        std::fs::write(&path, "hello plain").unwrap();
        let cfg = OmnicatConfig::default();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &cfg, &ctx).unwrap();
        assert!(content.plain_text().contains("hello plain"));
    }

    #[test]
    fn utf8_text_is_not_hex() {
        // A .env-style file with non-ASCII (UTF-8) content must render as text.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dotenv");
        std::fs::write(&path, "API_KEY=secret\n# Настройки\nCITY=Zürich\n").unwrap();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &plain_config(), &ctx).unwrap();
        match content {
            PreviewContent::Text(text) => {
                assert!(text.contains("API_KEY=secret"));
                assert!(text.contains("Zürich"));
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn json_content_detected_as_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mystery.unknownext");
        std::fs::write(&path, "{\"name\":\"probe\",\"n\":1}\n").unwrap();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &plain_config(), &ctx).unwrap();
        assert!(content.plain_text().contains("\"name\""));
    }

    #[test]
    fn sniff_detects_common_text_formats() {
        assert_eq!(sniff_syntax("{\"a\":1}"), Some("JSON"));
        assert_eq!(sniff_syntax("[1,2,3]"), Some("JSON"));
        assert_eq!(sniff_syntax("<note><to>x</to></note>"), Some("XML"));
        assert_eq!(
            sniff_syntax("<!DOCTYPE html><html><body>hi</body></html>"),
            Some("HTML")
        );
        assert_eq!(sniff_syntax("KEY=value\nOTHER=1\n# comment\n"), Some("INI"));
        assert_eq!(sniff_syntax("name: probe\nport: 8080\n"), Some("YAML"));
        assert_eq!(sniff_syntax("just some prose without structure"), None);
    }

    #[test]
    fn decode_rejects_binary_but_accepts_utf8() {
        assert!(decode_text(b"\x00\x01\x02binary").is_none());
        assert!(decode_text("héllo".as_bytes()).is_some());
        // A multi-byte char truncated at the cap is still treated as text.
        let s = "abcé".as_bytes();
        assert!(decode_text(&s[..s.len() - 1]).is_some());
    }
}
