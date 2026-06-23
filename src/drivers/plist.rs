use std::path::Path;

use anyhow::{Context, Result};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct PlistDriver;

impl PreviewDriver for PlistDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Plist
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["plist"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        _config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let file = std::fs::File::open(path)?;
        let value: plist::Value = plist::from_reader(file).context("invalid plist")?;
        let text = plist_to_string(&value);
        Ok(PreviewContent::Text(text))
    }
}

fn plist_to_string(value: &plist::Value) -> String {
    match value {
        plist::Value::Dictionary(map) => serde_json::to_string_pretty(map).unwrap_or_default(),
        other => format!("{other:?}"),
    }
}
