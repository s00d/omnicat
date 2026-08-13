use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
use crate::drivers::PreviewDriver;

pub struct PdfDriver;

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
        Ok(convert::preview_markdown(path, config))
    }
}
