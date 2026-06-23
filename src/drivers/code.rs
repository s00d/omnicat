use std::fs;
use std::io::Write;
use std::path::Path;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::highlight::highlight_source;
use crate::drivers::theme::resolve_theme;
use crate::drivers::PreviewDriver;
use anyhow::{Context, Result};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

pub struct CodeDriver;

impl CodeDriver {
    pub fn render(&self, path: &Path, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
        let source = fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");

        let ps = SyntaxSet::load_defaults_newlines();
        let ts = ThemeSet::load_defaults();
        let syntax = ps
            .find_syntax_by_extension(ext)
            .or_else(|| ps.find_syntax_by_token(ext))
            .unwrap_or_else(|| ps.find_syntax_plain_text());

        let theme_name = config.terminal.code.theme.as_str();
        let theme = resolve_theme(&ts, theme_name);

        let mut h = HighlightLines::new(syntax, theme);
        let show_numbers =
            config.terminal.code.line_numbers && config.terminal.code.style != "plain";

        if config.terminal.plain {
            for (idx, line) in LinesWithEndings::from(&source).enumerate() {
                if show_numbers {
                    let text = line.strip_suffix('\n').unwrap_or(line);
                    write!(out, "{line_no:>4}| {text}", line_no = idx + 1)?;
                } else {
                    write!(out, "{line}")?;
                }
            }
            return Ok(());
        }

        for (idx, line) in LinesWithEndings::from(&source).enumerate() {
            let ranges: Vec<(Style, &str)> =
                h.highlight_line(line, &ps).context("highlight failed")?;
            let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
            if show_numbers {
                write!(
                    out,
                    "{line_no:>4}\x1b[90m|\x1b[0m {escaped}",
                    line_no = idx + 1
                )?;
            } else {
                write!(out, "{escaped}")?;
            }
        }

        Ok(())
    }

    fn code_theme_name(config: &OmnicatConfig) -> &str {
        let gui_theme = config.gui.markdown.code_theme.trim();
        if gui_theme.is_empty() {
            config.terminal.code.theme.as_str()
        } else {
            gui_theme
        }
    }
}

impl PreviewDriver for CodeDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Code
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "py", "js", "ts", "tsx", "jsx", "sh", "zsh", "bash", "rb", "go", "rs", "c", "h",
            "cpp", "hpp", "java", "kt", "swift", "lua", "sql", "html", "css", "scss", "vue",
            "php", "pl", "r", "xml", "svg",
        ]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &[]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let source = fs::read_to_string(path)?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
        let show_numbers =
            config.terminal.code.line_numbers && config.terminal.code.style != "plain";
        let highlighted = highlight_source(
            &source,
            ext,
            Self::code_theme_name(config),
            show_numbers,
        )?;
        Ok(PreviewContent::HighlightedCode(highlighted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::PreviewContent;

    #[test]
    fn render_terminal_no_double_newlines_with_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.py");
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        let mut config = OmnicatConfig::default();
        config.terminal.code.line_numbers = true;
        let mut buf = Vec::new();
        CodeDriver.render(&path, &config, &mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert_eq!(
            out.lines().count(),
            3,
            "expected 3 lines, got extra blanks:\n{out:?}"
        );
    }

    #[test]
    fn build_returns_highlighted_code() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let content = CodeDriver
            .build(&path, &OmnicatConfig::default(), &crate::content::preview_context(&path))
            .unwrap();
        match content {
            PreviewContent::HighlightedCode(code) => {
                assert!(!code.lines.is_empty());
                assert!(code.lines.iter().any(|l| !l.spans.is_empty()));
            }
            other => panic!("expected HighlightedCode, got {other:?}"),
        }
    }
}
