use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use cfb::CompoundFile;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct LegacyOfficeDriver;

impl PreviewDriver for LegacyOfficeDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::LegacyOffice
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["doc", "ppt", "xls"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[
            "application/msword",
            "application/vnd.ms-powerpoint",
            "application/vnd.ms-excel",
        ]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let _ = config;
        match extract_ole_text(path) {
            Ok(text) if text.len() > 20 => Ok(PreviewContent::Text(text)),
            _ => Ok(PreviewContent::Unsupported {
                reason: format!(
                    "Legacy Office file: {} ({} bytes)",
                    path.display(),
                    ctx.size
                ),
                suggestion: "Best-effort OLE text extraction failed. \
                    Open in LibreOffice/Microsoft Office or convert to OpenXML (docx/xlsx/pptx)."
                    .into(),
            }),
        }
    }
}

fn extract_ole_text(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut comp = CompoundFile::open(file).context("not a compound file")?;
    let names: Vec<String> = comp
        .walk()
        .filter(|entry| {
            let name = entry.name();
            name.contains("WordDocument")
                || name.contains("PowerPoint Document")
                || name.contains("Workbook")
                || name.ends_with("Contents")
        })
        .map(|entry| entry.name().to_string())
        .collect();

    let mut out = String::new();
    for name in names {
        let mut stream = comp.open_stream(&name)?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf)?;
        out.push_str(&extract_printable(&buf));
        out.push('\n');
    }

    if out.trim().is_empty() {
        anyhow::bail!("no readable streams");
    }
    Ok(out.chars().take(8000).collect())
}

fn extract_printable(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut run = String::new();
    for &b in bytes {
        if b.is_ascii_graphic() || b == b' ' || b == b'\n' || b == b'\t' {
            run.push(b as char);
        } else if run.len() >= 4 {
            out.push_str(&run);
            out.push(' ');
            run.clear();
        } else {
            run.clear();
        }
    }
    if run.len() >= 4 {
        out.push_str(&run);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
