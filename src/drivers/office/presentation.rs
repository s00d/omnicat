use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
use crate::drivers::PreviewDriver;

pub struct PresentationDriver;

impl PreviewDriver for PresentationDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Presentation
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pptx", "pptm", "ppsx", "ppsm", "odp", "ppt", "pps", "pot"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "application/vnd.oasis.opendocument.presentation",
            "application/vnd.ms-powerpoint",
        ]
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
