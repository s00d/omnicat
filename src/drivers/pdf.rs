use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::{capture_render, PreviewDriver};

pub struct PdfDriver;

impl PdfDriver {
    pub fn render(&self, path: &Path, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
        let bytes = fs::read(path)?;
        let text =
            pdf_extract::extract_text_from_mem(&bytes).context("pdf text extraction failed")?;

        if config.terminal.pdf.page_separator {
            let pages: Vec<&str> = text.split('\x0c').collect();
            for (i, page) in pages.iter().enumerate() {
                if i > 0 {
                    writeln!(out, "\n--- page {} ---\n", i + 1)?;
                }
                write!(out, "{}", page.trim())?;
            }
            writeln!(out)?;
        } else {
            write!(out, "{text}")?;
        }

        Ok(())
    }
}

impl PreviewDriver for PdfDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Pdf
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["application/pdf"]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let text = capture_render(|buf| self.render(path, config, buf))?;
        Ok(PreviewContent::Text(text))
    }
}
