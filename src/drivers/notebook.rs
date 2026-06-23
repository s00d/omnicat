use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct NotebookDriver;

impl PreviewDriver for NotebookDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Notebook
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ipynb"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["application/x-ipynb+json"]
    }

    fn build(
        &self,
        path: &Path,
        _config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let raw = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&raw).context("invalid ipynb json")?;
        let mut slides = Vec::new();
        if let Some(cells) = json.get("cells").and_then(|c| c.as_array()) {
            for (i, cell) in cells.iter().enumerate() {
                let cell_type = cell
                    .get("cell_type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("cell");
                let mut text = format!("## Cell {} ({cell_type})\n\n", i + 1);
                if let Some(source) = cell.get("source") {
                    text.push_str(&value_to_string(source));
                }
                slides.push(text);
            }
        }
        if slides.is_empty() {
            Ok(PreviewContent::Text(raw))
        } else {
            Ok(PreviewContent::Slides(slides))
        }
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => v.to_string(),
    }
}
