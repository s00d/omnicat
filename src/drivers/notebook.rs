use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
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
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let raw = std::fs::read_to_string(path)?;
        let json: Value = serde_json::from_str(&raw).context("invalid ipynb json")?;
        let lang = notebook_language(&json);
        let mut out = String::new();
        if let Some(cells) = json.get("cells").and_then(|c| c.as_array()) {
            for (i, cell) in cells.iter().enumerate() {
                let cell_type = cell
                    .get("cell_type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("cell");
                out.push_str(&format!("## Cell {} ({cell_type})\n\n", i + 1));
                let source = cell.get("source").map(value_to_string).unwrap_or_default();
                match cell_type {
                    "code" => {
                        out.push_str("```");
                        out.push_str(&lang);
                        out.push('\n');
                        out.push_str(source.trim_end());
                        out.push_str("\n```\n\n");
                    }
                    _ => {
                        out.push_str(source.trim_end());
                        out.push_str("\n\n");
                    }
                }
            }
        }
        if out.trim().is_empty() {
            return Ok(PreviewContent::Text(raw));
        }
        let md = convert::truncate_chars(&out, convert::preview_char_limit(config));
        Ok(PreviewContent::Markdown(md))
    }
}

fn notebook_language(json: &Value) -> String {
    json.pointer("/metadata/kernelspec/language")
        .or_else(|| json.pointer("/metadata/language_info/name"))
        .and_then(|v| v.as_str())
        .unwrap_or("python")
        .to_string()
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
