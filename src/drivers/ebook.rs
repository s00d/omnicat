use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct EbookDriver;

/// Extensions routed to the ebook driver (order matters for docs only).
pub const EBOOK_EXTENSIONS: &[&str] = &[
    "epub", "mobi", "azw", "azw1", "azw2", "azw3", "prc", "pdb", "fb2", "fbz", "lit", "djvu",
    "djv", "cbz", "cbr", "opf",
];

impl EbookDriver {
    pub fn render_terminal(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        out: &mut dyn Write,
    ) -> Result<()> {
        let max = terminal_ebook_char_limit(config);
        let text = extract_ebook(path, max)?;
        write!(out, "{text}")?;
        Ok(())
    }
}

pub fn terminal_ebook_char_limit(config: &OmnicatConfig) -> usize {
    let limit = config.terminal.document.max_chars;
    if limit == 0 {
        usize::MAX
    } else {
        limit
    }
}

impl PreviewDriver for EbookDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Ebook
    }

    fn extensions(&self) -> &'static [&'static str] {
        EBOOK_EXTENSIONS
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[
            "application/epub+zip",
            "application/x-mobipocket-ebook",
            "application/vnd.amazon.ebook",
            "application/x-fictionbook+xml",
            "application/x-fictionbook",
            "application/djvu",
            "image/vnd.djvu",
        ]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let max = config.gui.document.max_paragraphs;
        let text = extract_ebook(path, max)?;
        Ok(PreviewContent::Text(text))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EbookFormat {
    Epub,
    Fb2,
    MobiFamily,
    Cbz,
    Cbr,
    Lit,
    Djvu,
    Opf,
}

fn extract_ebook(path: &Path, max_chars: usize) -> Result<String> {
    match detect_format(path) {
        EbookFormat::Epub => extract_epub(path, max_chars),
        EbookFormat::Fb2 => extract_fb2(path, max_chars),
        EbookFormat::MobiFamily => extract_mobi_family(path, max_chars),
        EbookFormat::Cbz => extract_cbz(path, max_chars),
        EbookFormat::Cbr => extract_cbr(path),
        EbookFormat::Lit => extract_lit(path, max_chars),
        EbookFormat::Djvu => extract_djvu(path),
        EbookFormat::Opf => extract_opf(path, max_chars),
    }
}

fn detect_format(path: &Path) -> EbookFormat {
    match extension_lower(path).as_str() {
        "epub" => EbookFormat::Epub,
        "fb2" | "fbz" => EbookFormat::Fb2,
        "mobi" | "azw" | "azw1" | "azw2" | "azw3" | "prc" | "pdb" => EbookFormat::MobiFamily,
        "cbz" => EbookFormat::Cbz,
        "cbr" => EbookFormat::Cbr,
        "lit" => EbookFormat::Lit,
        "djvu" | "djv" => EbookFormat::Djvu,
        "opf" => EbookFormat::Opf,
        _ => EbookFormat::MobiFamily,
    }
}

pub fn is_mobi_family_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mobi" | "azw" | "azw1" | "azw2" | "azw3" | "prc" | "pdb"
    )
}

fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn extract_epub(path: &Path, max_chars: usize) -> Result<String> {
    let mut doc = epub::doc::EpubDoc::new(path).context("failed to open epub")?;
    let mut out = String::new();
    let mut count = 0usize;
    while let Some((_, html)) = doc.get_current_str() {
        let plain = html_to_text(&html);
        out.push_str(&plain);
        out.push_str("\n\n");
        count += plain.len();
        if count >= max_chars {
            break;
        }
        if !doc.go_next() {
            break;
        }
    }
    if out.is_empty() {
        if let Some((_, html)) = doc.get_current_str() {
            out = html_to_text(&html);
        }
    }
    Ok(truncate_chars(&out, max_chars))
}

fn extract_mobi_family(path: &Path, max_chars: usize) -> Result<String> {
    let m = mobi::Mobi::from_path(path).context("open mobi/azw/prc")?;
    let mut out = String::new();
    let title = m.title();
    if !title.is_empty() {
        out.push_str(&title);
        out.push_str("\n\n");
    }
    if let Some(author) = m.author() {
        if !author.is_empty() {
            out.push_str(&author);
            out.push_str("\n\n");
        }
    }
    if let Some(desc) = m.description() {
        let plain = html_to_text(&desc);
        if !plain.is_empty() {
            out.push_str(&plain);
            out.push_str("\n\n");
        }
    }
    let content = m
        .content_as_string()
        .unwrap_or_else(|_| m.content_as_string_lossy());
    let plain = html_to_text(&content);
    out.push_str(&plain);
    Ok(truncate_chars(&out, max_chars))
}

fn extract_cbz(path: &Path, max_entries: usize) -> Result<String> {
    let file = File::open(path).context("open cbz")?;
    let mut archive = ZipArchive::new(file).context("invalid cbz zip")?;
    let total = archive.len();
    let mut out = format!("Comic book archive — {total} file(s)\n\n");
    for i in 0..total.min(max_entries) {
        let entry = archive.by_index(i).context("cbz entry")?;
        let name = entry.name().to_string();
        if is_archive_noise(&name) {
            continue;
        }
        out.push_str(&name);
        if !entry.is_dir() {
            out.push_str(&format!(" ({} bytes)", entry.size()));
        }
        out.push('\n');
    }
    if total > max_entries {
        out.push_str(&format!("\n… {total} entries total\n"));
    }
    Ok(out)
}

fn extract_cbr(path: &Path) -> Result<String> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(format!(
        "RAR comic archive ({size} bytes)\n\nCBR preview requires unrar; extract to CBZ or use calibre."
    ))
}

fn extract_lit(path: &Path, max_chars: usize) -> Result<String> {
    let mut comp = cfb::open(path).context("open lit (OLE)")?;
    let streams: Vec<_> = comp
        .walk()
        .filter(|e| e.is_stream())
        .map(|e| e.path().to_path_buf())
        .collect();
    let mut out = String::new();
    for stream_path in streams {
        let mut stream = comp.open_stream(&stream_path)?;
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes)?;
        append_decoded_snippets(&mut out, &bytes);
        if out.len() >= max_chars {
            break;
        }
    }
    let out = truncate_chars(&out, max_chars);
    if out.len() < 40 {
        anyhow::bail!("no readable text in lit; try calibre ebook-convert");
    }
    Ok(out)
}

fn extract_djvu(path: &Path) -> Result<String> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let kind = std::fs::read(path)
        .ok()
        .and_then(|b| sniff_djvu_kind(&b))
        .unwrap_or_else(|| "DjVu".into());
    Ok(format!(
        "{kind} document ({size} bytes)\n\nText-layer preview is not built in; convert with calibre or pdftotext after export."
    ))
}

fn sniff_djvu_kind(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 16 {
        return None;
    }
    if &bytes[0..4] == b"AT&T" {
        let form = std::str::from_utf8(&bytes[12..16]).ok()?;
        return Some(format!("DjVu ({form})"));
    }
    None
}

fn extract_opf(path: &Path, max_chars: usize) -> Result<String> {
    let xml = std::fs::read_to_string(path).context("read opf")?;
    let mut out = String::from("EPUB package (OPF)\n\n");
    extract_xml_text_nodes(&xml, &mut out, max_chars)?;
    Ok(truncate_chars(&out, max_chars))
}

fn extract_fb2(path: &Path, max_paragraphs: usize) -> Result<String> {
    let xml = read_fb2_xml(path)?;
    extract_fb2_text(&xml, max_paragraphs)
}

fn read_fb2_xml(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(b"PK") || extension_lower(path) == "fbz" {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file).context("fb2 zip archive")?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_ascii_lowercase();
            if name.ends_with(".fb2") || name == "fb2.xml" {
                let mut xml = String::new();
                entry.read_to_string(&mut xml)?;
                return Ok(xml);
            }
        }
        anyhow::bail!("no .fb2 entry in zip");
    }
    String::from_utf8(bytes).context("fb2 is not valid utf-8 xml")
}

fn extract_fb2_text(xml: &str, max_paragraphs: usize) -> Result<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut out = String::new();
    let mut count = 0usize;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        out.push_str(trimmed);
                        out.push('\n');
                        count += 1;
                        if count >= max_paragraphs {
                            break;
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if out.is_empty() {
        anyhow::bail!("no text in fb2");
    }
    Ok(out)
}

fn extract_xml_text_nodes(xml: &str, out: &mut String, limit: usize) -> Result<()> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut count = 0usize;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        out.push_str(trimmed);
                        out.push('\n');
                        count += 1;
                        if count >= limit {
                            break;
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    Ok(())
}

fn html_to_text(html: &str) -> String {
    let prepared = html
        .replace("<mbp:pagebreak/>", "\n\n")
        .replace("<mbp:pagebreak />", "\n\n")
        .replace("<mbp:pagebreak>", "\n\n")
        .replace("</p>", "</p>\n")
        .replace("</div>", "</div>\n")
        .replace("</h1>", "</h1>\n")
        .replace("</h2>", "</h2>\n")
        .replace("</h3>", "</h3>\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("><p", ">\n<p")
        .replace("><P", ">\n<P");

    let mut out = String::new();
    let mut in_tag = false;
    for c in prepared.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            '\n' if !in_tag => out.push('\n'),
            _ if !in_tag => {
                if c.is_whitespace() {
                    if !out.ends_with(' ') && !out.ends_with('\n') && !out.is_empty() {
                        out.push(' ');
                    }
                } else {
                    out.push(c);
                }
            }
            _ => {}
        }
    }
    out.trim_end().to_string()
}

fn append_decoded_snippets(out: &mut String, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
        let utf16: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        if let Ok(s) = String::from_utf16(&utf16) {
            let plain: String = s
                .chars()
                .filter(|c| c.is_alphanumeric() || c.is_whitespace() || ".,;:!?-'\"()".contains(*c))
                .collect();
            if plain.len() > 8 {
                out.push_str(plain.trim());
                out.push('\n');
                return;
            }
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        let plain: String = s
            .chars()
            .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
            .collect();
        if plain.len() > 8 {
            out.push_str(plain.trim());
            out.push('\n');
        }
    }
}

fn is_archive_noise(path: &str) -> bool {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.starts_with("._")
        || base == ".DS_Store"
        || path.starts_with("__MACOSX/")
        || path.contains("/__MACOSX/")
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "\n…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobi_family_extensions() {
        assert!(is_mobi_family_ext("mobi"));
        assert!(is_mobi_family_ext("azw3"));
        assert!(!is_mobi_family_ext("epub"));
    }

    #[test]
    fn fb2_text_extraction() {
        let xml =
            r#"<?xml version="1.0"?><FictionBook><body><p>Hello FB2</p></body></FictionBook>"#;
        let text = extract_fb2_text(xml, 10).unwrap();
        assert!(text.contains("Hello FB2"));
    }

    #[test]
    fn html_to_text_strips_tags() {
        let plain = html_to_text("<p>Hello <b>world</b></p>");
        assert_eq!(plain, "Hello world");
    }

    #[test]
    fn large_fixture_has_many_lines_when_unlimited() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/files/sample-large.mobi");
        if !path.exists() {
            return;
        }
        let text = extract_ebook(&path, usize::MAX).unwrap();
        assert!(
            text.lines().count() > 100,
            "expected 100+ lines from large mobi, got {}",
            text.lines().count()
        );
    }
}
