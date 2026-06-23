use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::OmnicatConfig;
use crate::content::{FontInfoContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct FontDriver;

impl PreviewDriver for FontDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Font
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ttf", "otf", "woff", "woff2"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["font/*"]
    }

    fn build(
        &self,
        path: &Path,
        _config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let data = fs::read(path).context("read font")?;
        let face = ttf_parser::Face::parse(&data, 0).context("parse font")?;
        let family = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FULL_NAME)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| path.display().to_string());
        let style = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::SUBFAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Regular".into());
        let weight = face.weight().to_number();
        let glyph_count = face.number_of_glyphs();
        let sample = "The quick brown fox jumps over the lazy dog.\n\
                      ABCDEFGHIJKLMNOPQRSTUVWXYZ\n\
                      abcdefghijklmnopqrstuvwxyz\n\
                      0123456789 !@#$%^&*()";

        Ok(PreviewContent::FontInfo(FontInfoContent {
            family,
            style,
            weight,
            glyph_count,
            sample: sample.to_string(),
        }))
    }
}
