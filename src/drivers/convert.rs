//! Convert office/documents to Markdown via [`anydoc`], then preview as MD.

use std::path::Path;

use anyhow::{Context, Result};

use crate::config::OmnicatConfig;
use crate::content::PreviewContent;

/// Convert a supported document to GitHub-Flavored Markdown.
pub fn to_markdown(path: &Path) -> Result<String> {
    anydoc::to_markdown(path).with_context(|| format!("anydoc convert {}", path.display()))
}

/// Truncate Markdown for preview limits (`0` / `usize::MAX` = no limit).
pub fn truncate_chars(s: &str, max: usize) -> String {
    if max == 0 || max == usize::MAX || s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "\n…"
}

pub fn preview_char_limit(config: &OmnicatConfig) -> usize {
    if config.inspect.no_limit {
        return usize::MAX;
    }
    let limit = config.terminal.document.max_chars;
    if limit == 0 {
        usize::MAX
    } else {
        limit
    }
}

/// Convert + apply preview char limit from config.
pub fn to_markdown_preview(path: &Path, config: &OmnicatConfig) -> Result<String> {
    let md = to_markdown(path)?;
    Ok(truncate_chars(&md, preview_char_limit(config)))
}

/// Convert to Markdown preview content. Conversion failures become
/// [`PreviewContent::Unsupported`] instead of dumping raw bytes via cat-fallback.
pub fn preview_markdown(path: &Path, config: &OmnicatConfig) -> PreviewContent {
    match to_markdown_preview(path, config) {
        Ok(md) if !md.trim().is_empty() => PreviewContent::Markdown(md),
        Ok(_) => PreviewContent::Unsupported {
            reason: format!("empty document after convert: {}", path.display()),
            suggestion: "anydoc produced no Markdown for this file.".into(),
        },
        Err(err) => PreviewContent::Unsupported {
            reason: format!("{}", err.root_cause()),
            suggestion: "anydoc could not convert this file.".into(),
        },
    }
}
