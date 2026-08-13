use std::path::Path;

use anyhow::Result;

use crate::inspect::report::EncodingReport;
use crate::io;

#[derive(Debug, Clone)]
pub struct TextMeta {
    pub encoding: String,
    pub bom: bool,
    pub line_endings: String,
    pub characters: usize,
    pub lines: usize,
    pub longest_line: usize,
    pub text: String,
}

pub fn analyze_text_file(path: &Path, max_bytes: usize) -> Result<TextMeta> {
    let bytes = io::read_bytes_capped(path, max_bytes, true)?;
    analyze_bytes(&bytes, max_bytes)
}

pub fn analyze_bytes(bytes: &[u8], max_bytes: usize) -> Result<TextMeta> {
    let (encoding, bom, body) = detect_encoding(bytes);
    let limited = if max_bytes > 0 && body.len() > max_bytes {
        &body[..max_bytes]
    } else {
        body
    };
    let text = match encoding.as_str() {
        "UTF-16LE" => decode_utf16(limited, true),
        "UTF-16BE" => decode_utf16(limited, false),
        "Windows-1252" => decode_windows_1252(limited),
        "ISO-8859-1" => decode_latin1(limited),
        _ => String::from_utf8_lossy(limited).into_owned(),
    };

    let mut lines = 0usize;
    let mut longest = 0usize;
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    for line in text.split_inclusive(['\n', '\r']) {
        let raw = line;
        let ending = if raw.ends_with("\r\n") {
            crlf += 1;
            2
        } else if raw.ends_with('\n') {
            lf += 1;
            1
        } else if raw.ends_with('\r') {
            cr += 1;
            1
        } else {
            0
        };
        let content_len = raw.len().saturating_sub(ending);
        longest = longest.max(content_len);
        lines += 1;
    }
    if text.is_empty() {
        lines = 0;
    }

    let line_endings = if crlf >= lf && crlf >= cr && crlf > 0 {
        "CRLF"
    } else if cr > lf && cr > 0 {
        "CR"
    } else if lf > 0 {
        "LF"
    } else {
        "none"
    };

    Ok(TextMeta {
        encoding,
        bom,
        line_endings: line_endings.into(),
        characters: text.chars().count(),
        lines,
        longest_line: longest,
        text,
    })
}

pub fn encoding_report(path: &Path, max_bytes: usize) -> Result<EncodingReport> {
    let meta = analyze_text_file(path, max_bytes)?;
    Ok(EncodingReport {
        path: path.display().to_string(),
        encoding: meta.encoding,
        bom: meta.bom,
        line_endings: meta.line_endings,
        characters: meta.characters,
        lines: meta.lines,
        longest_line: meta.longest_line,
    })
}

fn detect_encoding(bytes: &[u8]) -> (String, bool, &[u8]) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return ("UTF-8".into(), true, &bytes[3..]);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return ("UTF-16LE".into(), true, &bytes[2..]);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return ("UTF-16BE".into(), true, &bytes[2..]);
    }
    if looks_like_utf16_le(bytes) {
        return ("UTF-16LE".into(), false, bytes);
    }
    if std::str::from_utf8(bytes).is_ok() {
        return ("UTF-8".into(), false, bytes);
    }

    // Invalid as UTF-8: score high-bit bytes for Windows-1252 vs ISO-8859-1.
    let sample = &bytes[..bytes.len().min(8 * 1024)];
    let mut high = 0u32;
    let mut cp1252_special = 0u32;
    for &b in sample {
        if b >= 0x80 {
            high += 1;
            // C1 control range is unused in ISO-8859-1 text but common in Windows-1252
            // (smart quotes, euro, etc. live in 0x80–0x9F).
            if (0x80..=0x9F).contains(&b) {
                cp1252_special += 1;
            }
        }
    }

    if high == 0 {
        // NUL / binary-ish but somehow not UTF-8 — still label latin1.
        return ("ISO-8859-1".into(), false, bytes);
    }

    if cp1252_special * 4 >= high {
        ("Windows-1252".into(), false, bytes)
    } else {
        ("ISO-8859-1".into(), false, bytes)
    }
}

fn looks_like_utf16_le(bytes: &[u8]) -> bool {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    let mut nul = 0;
    for chunk in bytes.chunks(2).take(64) {
        if chunk[1] == 0 {
            nul += 1;
        }
    }
    nul > 16
}

fn decode_utf16(bytes: &[u8], le: bool) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| {
            if le {
                u16::from_le_bytes([c[0], c[1]])
            } else {
                u16::from_be_bytes([c[0], c[1]])
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

fn decode_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Decode Windows-1252: same as latin1 except 0x80–0x9F map to printable Unicode.
fn decode_windows_1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| windows_1252_char(b)).collect()
}

fn windows_1252_char(b: u8) -> char {
    match b {
        0x80 => '\u{20AC}', // €
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8A => '\u{0160}',
        0x8B => '\u{2039}',
        0x8C => '\u{0152}',
        0x8E => '\u{017D}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9A => '\u{0161}',
        0x9B => '\u{203A}',
        0x9C => '\u{0153}',
        0x9E => '\u{017E}',
        0x9F => '\u{0178}',
        // 0x81, 0x8D, 0x8F, 0x90, 0x9D undefined → keep as control/private
        other => other as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_utf8() {
        let (enc, bom, _) = detect_encoding(b"hello \xE2\x9C\x93");
        assert_eq!(enc, "UTF-8");
        assert!(!bom);
    }

    #[test]
    fn detects_utf8_bom() {
        let (enc, bom, body) = detect_encoding(&[0xEF, 0xBB, 0xBF, b'a']);
        assert_eq!(enc, "UTF-8");
        assert!(bom);
        assert_eq!(body, b"a");
    }

    #[test]
    fn detects_windows_1252_smart_quotes() {
        // "café" with Windows-1252 smart quotes around — 0x93/0x94 are not valid UTF-8
        let bytes = [0x93, b'h', b'i', 0x94, 0x80]; // “hi”€
        let (enc, _, _) = detect_encoding(&bytes);
        assert_eq!(enc, "Windows-1252");
        let text = decode_windows_1252(&bytes);
        assert!(text.contains('€'));
        assert!(text.contains('\u{201C}'));
    }

    #[test]
    fn detects_iso8859_high_bytes() {
        // Valid latin1 letter é (0xE9) alone is also invalid UTF-8 continuation
        let bytes = [b'c', b'a', b'f', 0xE9];
        let (enc, _, _) = detect_encoding(&bytes);
        assert_eq!(enc, "ISO-8859-1");
    }

    #[test]
    fn analyze_respects_max_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.txt");
        std::fs::write(&p, b"hello world").unwrap();
        let meta = analyze_text_file(&p, 5).unwrap();
        assert_eq!(meta.text, "hello");
    }
}
