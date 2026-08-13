use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
use crate::drivers::PreviewDriver;

pub struct DocumentDriver;

impl PreviewDriver for DocumentDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Document
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["docx", "docm", "odt", "rtf", "doc"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "application/vnd.oasis.opendocument.text",
            "application/rtf",
            "text/rtf",
            "application/msword",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_demo_docx() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/files/sample.docx");
        if !path.exists() {
            return;
        }
        let cfg = crate::config::OmnicatConfig::default();
        let ctx = crate::content::preview_context(&path);
        let content = DocumentDriver.build(&path, &cfg, &ctx).unwrap();
        assert!(!content.plain_text().trim().is_empty());
    }
}
