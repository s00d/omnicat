use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::content::{ColoredSpan, HighlightedCode, HighlightedLine, RgbColor};
use crate::drivers::theme::resolve_theme;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

pub fn highlight_source(
    source: &str,
    lang_or_ext: &str,
    theme_name: &str,
    line_numbers: bool,
) -> Result<HighlightedCode> {
    let ps = &SYNTAX_SET;
    let ts = &THEME_SET;
    let syntax = if lang_or_ext.is_empty() {
        ps.find_syntax_plain_text()
    } else {
        ps.find_syntax_by_token(lang_or_ext)
            .or_else(|| ps.find_syntax_by_extension(lang_or_ext))
            .unwrap_or_else(|| ps.find_syntax_plain_text())
    };
    let theme = resolve_theme(ts, theme_name);
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut lines = Vec::new();
    for (idx, line) in LinesWithEndings::from(source).enumerate() {
        let ranges = highlighter
            .highlight_line(line, ps)
            .context("highlight failed")?;
        let spans = ranges
            .into_iter()
            .map(|(style, text)| style_to_span(style, text))
            .collect();
        lines.push(HighlightedLine {
            line_no: if line_numbers {
                Some(idx + 1)
            } else {
                None
            },
            spans,
        });
    }

    Ok(HighlightedCode {
        lang: lang_or_ext.to_string(),
        lines,
    })
}

fn style_to_span(style: Style, text: &str) -> ColoredSpan {
    ColoredSpan {
        text: text.to_string(),
        foreground: color_to_rgb(style.foreground),
        background: if style.background.a > 0 {
            Some(color_to_rgb(style.background))
        } else {
            None
        },
    }
}

fn color_to_rgb(color: Color) -> RgbColor {
    RgbColor {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_produces_multiple_colors() {
        let code = "fn main() {\n    let x = 1;\n}\n";
        let highlighted = highlight_source(code, "rs", "InspiredGitHub", false).unwrap();
        let colors: Vec<RgbColor> = highlighted
            .lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|s| s.foreground))
            .collect();
        let distinct: std::collections::HashSet<_> = colors
            .iter()
            .map(|c| (c.r, c.g, c.b))
            .collect();
        assert!(
            distinct.len() >= 2,
            "expected multiple syntax colors, got {distinct:?}"
        );
    }
}
