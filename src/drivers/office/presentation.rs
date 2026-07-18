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

pub struct PresentationDriver;

impl PreviewDriver for PresentationDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Presentation
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pptx", "odp"]
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

        let slides = match ext.as_str() {
            "pptx" => extract_pptx_slides(path)?,
            _ => extract_odp_slides(path)?,
        };
        let _ = config;
        Ok(PreviewContent::Slides(slides))
    }
}

fn extract_pptx_slides(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid pptx zip")?;
    let mut slides = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("pptx entry")?;
        let name = entry.name().to_string();
        if !name.starts_with("ppt/slides/slide") || !name.ends_with(".xml") {
            continue;
        }
        let mut xml = String::new();
        entry.read_to_string(&mut xml)?;
        slides.push(extract_text_from_xml(&xml));
    }

    Ok(slides)
}

fn extract_odp_slides(path: &Path) -> Result<Vec<String>> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("invalid odp zip")?;
    let mut content = archive
        .by_name("content.xml")
        .context("missing content.xml")?;
    let mut xml = String::new();
    content.read_to_string(&mut xml)?;
    Ok(vec![extract_text_from_xml(&xml)])
}

fn extract_text_from_xml(xml: &str) -> String {
    let mut out = String::new();
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event() {
            Ok(Event::Text(t)) => {
                if let Ok(s) = t.decode() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        out.push_str(trimmed);
                        out.push('\n');
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    out
}
