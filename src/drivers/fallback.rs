use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{HexContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct FallbackDriver;

impl PreviewDriver for FallbackDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Fallback
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let max = config.terminal.fallback.max_bytes;
        let bytes = std::fs::read(path)?;
        let slice = if bytes.len() > max {
            &bytes[..max]
        } else {
            &bytes[..]
        };

        if let Ok(text) = std::str::from_utf8(slice) {
            if text.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()) {
                return Ok(PreviewContent::Text(text.to_string()));
            }
        }

        let metadata = if config.terminal.fallback.show_metadata {
            format!(
                "file: {}\nsize: {} bytes\nmime: {}",
                path.display(),
                ctx.size,
                ctx.mime.as_deref().unwrap_or("unknown")
            )
        } else {
            String::new()
        };

        Ok(PreviewContent::Hex(HexContent {
            bytes: slice.to_vec(),
            metadata,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;
    use crate::content::preview_context;

    #[test]
    fn binary_fallback_hex() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"\x00\x01SENTINEL").unwrap();
        let cfg = OmnicatConfig::default();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &cfg, &ctx).unwrap();
        match content {
            PreviewContent::Hex(h) => assert!(h.bytes.contains(&0x00)),
            other => panic!("expected hex, got {other:?}"),
        }
    }

    #[test]
    fn text_fallback_for_plain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.txt");
        std::fs::write(&path, "hello plain").unwrap();
        let cfg = OmnicatConfig::default();
        let ctx = preview_context(&path);
        let content = FallbackDriver.build(&path, &cfg, &ctx).unwrap();
        assert!(content.plain_text().contains("hello plain"));
    }
}
