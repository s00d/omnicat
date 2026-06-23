use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use zip::ZipArchive;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct DocumentDriver;

impl PreviewDriver for DocumentDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Document
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["docx", "odt", "rtf"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let text = match ext.as_str() {
            "docx" => extract_docx(path, config.gui.document.max_paragraphs)?,
            "rtf" => std::fs::read_to_string(path).unwrap_or_default(),
            _ => extract_odt_text(path, config.gui.document.max_paragraphs)?,
        };

        Ok(PreviewContent::Text(text))
    }
}

fn extract_docx(path: &Path, max_paragraphs: usize) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid docx zip")?;
    let mut doc = archive
        .by_name("word/document.xml")
        .context("missing document.xml")?;
    let mut xml = String::new();
    doc.read_to_string(&mut xml)?;

    let mut paragraphs = Vec::new();
    let mut current = String::new();
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"w:p" => {
                if !current.is_empty() {
                    paragraphs.push(std::mem::take(&mut current));
                    if paragraphs.len() >= max_paragraphs {
                        break;
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    current.push_str(&s);
                    current.push(' ');
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }

    Ok(paragraphs.join("\n\n"))
}

fn extract_odt_text(path: &Path, max_paragraphs: usize) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid odt zip")?;
    let mut content = archive.by_name("content.xml").context("missing content.xml")?;
    let mut xml = String::new();
    content.read_to_string(&mut xml)?;
    let mut out = String::new();
    let mut count = 0;
    let mut reader = Reader::from_str(&xml);
    loop {
        match reader.read_event() {
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    out.push_str(&s);
                    out.push('\n');
                    count += 1;
                    if count >= max_paragraphs {
                        break;
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.into()),
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_minimal_docx(path: &std::path::Path, text: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("word/document.xml", options).unwrap();
        write!(
            zip,
            r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"#
        )
        .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_docx_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        write_minimal_docx(&path, "SENTINEL-DOCX");
        let cfg = crate::config::OmnicatConfig::default();
        let ctx = crate::content::preview_context(&path);
        let content = DocumentDriver.build(&path, &cfg, &ctx).unwrap();
        assert!(content.plain_text().contains("SENTINEL-DOCX"));
    }
}
