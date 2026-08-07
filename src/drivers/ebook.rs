//! Ebook preview: anydoc for EPUB, mobi+htmd, thin FB2, stubs elsewhere.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
use crate::drivers::html_md;
use crate::drivers::PreviewDriver;

pub struct EbookDriver;

/// Extensions routed to the ebook driver (cbz lives under Archive).
pub const EBOOK_EXTENSIONS: &[&str] = &[
    "epub", "mobi", "azw", "azw1", "azw2", "azw3", "prc", "pdb", "fb2", "fbz", "lit", "djvu",
    "djv", "cbr", "opf",
];

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
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        match detect_format(path) {
            EbookFormat::Epub => Ok(convert::preview_markdown(path, config)),
            EbookFormat::MobiFamily => Ok(extract_mobi_markdown(path, config)?),
            EbookFormat::Fb2 => Ok(extract_fb2_markdown(path, config)?),
            EbookFormat::Unsupported(kind) => Ok(PreviewContent::Unsupported {
                reason: format!("{kind}: {} ({} bytes)", path.display(), ctx.size),
                suggestion: unsupported_hint(kind).into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EbookFormat {
    Epub,
    Fb2,
    MobiFamily,
    Unsupported(&'static str),
}

fn detect_format(path: &Path) -> EbookFormat {
    match extension_lower(path).as_str() {
        "epub" => EbookFormat::Epub,
        "fb2" | "fbz" => EbookFormat::Fb2,
        "mobi" | "azw" | "azw1" | "azw2" | "azw3" | "prc" | "pdb" => EbookFormat::MobiFamily,
        "lit" => EbookFormat::Unsupported("Microsoft Reader LIT"),
        "djvu" | "djv" => EbookFormat::Unsupported("DjVu"),
        "cbr" => EbookFormat::Unsupported("RAR comic (CBR)"),
        "opf" => EbookFormat::Unsupported("EPUB package (OPF)"),
        _ => EbookFormat::MobiFamily,
    }
}

fn unsupported_hint(kind: &str) -> &'static str {
    match kind {
        "Microsoft Reader LIT" => "Convert with calibre (ebook-convert) to EPUB.",
        "DjVu" => "Convert with calibre or djvutxt after export.",
        "RAR comic (CBR)" => "Extract to CBZ or use unrar, then preview the CBZ archive.",
        "EPUB package (OPF)" => "Open the surrounding EPUB archive instead of the bare OPF.",
        _ => "Convert externally to EPUB or Markdown.",
    }
}

fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn extract_mobi_markdown(path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
    let m = mobi::Mobi::from_path(path).context("open mobi/azw/prc")?;
    let mut out = String::new();
    let title = m.title();
    if !title.is_empty() {
        out.push_str("# ");
        out.push_str(&title);
        out.push_str("\n\n");
    }
    if let Some(author) = m.author() {
        if !author.is_empty() {
            out.push_str("**Author:** ");
            out.push_str(&author);
            out.push_str("\n\n");
        }
    }
    if let Some(desc) = m.description() {
        let md = html_md::html_to_markdown(&desc);
        if !md.trim().is_empty() {
            out.push_str(&md);
            out.push_str("\n\n");
        }
    }
    let content = m
        .content_as_string()
        .unwrap_or_else(|_| m.content_as_string_lossy());
    out.push_str(&html_md::html_to_markdown(&content));
    let md = convert::truncate_chars(&out, convert::preview_char_limit(config));
    Ok(PreviewContent::Markdown(md))
}

fn element_plain_text(node: roxmltree::Node<'_, '_>) -> String {
    // In roxmltree, Element::text() already returns concatenated child text;
    // only walk Text nodes so we do not double-count.
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_fb2_markdown(path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
    let xml = read_fb2_xml(path)?;
    let doc = roxmltree::Document::parse(&xml).context("parse fb2 xml")?;
    let mut paras = Vec::new();
    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }
        let tag = node.tag_name().name();
        if tag != "p" && tag != "v" && tag != "subtitle" && tag != "text-author" {
            continue;
        }
        let text = element_plain_text(node);
        if !text.is_empty() {
            paras.push(text);
        }
    }
    if paras.is_empty() {
        anyhow::bail!("no text in fb2");
    }
    let mut out = String::new();
    if let Some(title) = doc.descendants().find(|n| n.has_tag_name("book-title")) {
        let t = element_plain_text(title);
        if !t.is_empty() {
            out.push_str("# ");
            out.push_str(&t);
            out.push_str("\n\n");
        }
    }
    out.push_str(&paras.join("\n\n"));
    let md = convert::truncate_chars(&out, convert::preview_char_limit(config));
    Ok(PreviewContent::Markdown(md))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ebook_formats() {
        assert_eq!(detect_format(Path::new("x.mobi")), EbookFormat::MobiFamily);
        assert_eq!(detect_format(Path::new("x.azw3")), EbookFormat::MobiFamily);
        assert_eq!(detect_format(Path::new("x.epub")), EbookFormat::Epub);
        assert_eq!(detect_format(Path::new("x.fb2")), EbookFormat::Fb2);
        assert!(matches!(
            detect_format(Path::new("x.djvu")),
            EbookFormat::Unsupported(_)
        ));
    }

    #[test]
    fn fb2_text_extraction() {
        let xml = r#"<?xml version="1.0"?>
        <FictionBook>
          <description><title-info><book-title>Demo</book-title></title-info></description>
          <body><p>Hello FB2</p></body>
        </FictionBook>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let paras: Vec<_> = doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "p")
            .map(|n| n.descendants().filter_map(|c| c.text()).collect::<String>())
            .collect();
        assert!(paras.iter().any(|p| p.contains("Hello FB2")));
    }

    #[test]
    fn fb2_demo_fixture_no_dup_text() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/files/sample.fb2");
        if !path.exists() {
            return;
        }
        let content = extract_fb2_markdown(&path, &OmnicatConfig::default()).unwrap();
        let text = content.plain_text();
        assert!(text.contains("FictionBook2 sample"));
        assert!(
            !text.contains("FictionBook2 sample for ebook preview.FictionBook2"),
            "paragraph text should not be duplicated: {text}"
        );
    }

    #[test]
    fn large_fixture_has_many_lines_when_unlimited() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/files/sample-large.mobi");
        if !path.exists() {
            return;
        }
        let mut cfg = OmnicatConfig::default();
        cfg.terminal.document.max_chars = 0;
        cfg.inspect.no_limit = true;
        let content = extract_mobi_markdown(&path, &cfg).unwrap();
        assert!(
            content.plain_text().lines().count() > 100,
            "expected 100+ lines from large mobi, got {}",
            content.plain_text().lines().count()
        );
    }
}
