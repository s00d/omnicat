use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::markdown_table;
use crate::drivers::theme::resolve_theme;
use crate::drivers::PreviewDriver;

pub struct MarkdownDriver;

impl MarkdownDriver {
    pub fn render(&self, path: &Path, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
        let input = fs::read_to_string(path)?;
        render_markdown_str(&input, config, out)
    }
}

struct ListState {
    ordered: bool,
    next: u64,
}

struct MarkdownRenderer<'a> {
    out: &'a mut dyn Write,
    config: &'a OmnicatConfig,
    theme: String,
    code_theme: String,

    in_code_block: bool,
    code_block_lang: String,
    code_block_buf: String,

    in_table: bool,
    in_table_head: bool,
    table_alignments: Vec<Alignment>,
    table_header: Vec<Vec<String>>,
    table_rows: Vec<Vec<String>>,
    table_row: Vec<String>,
    table_cell: String,

    list_stack: Vec<ListState>,
    blockquote_depth: usize,
    in_image: bool,
    image_alt: String,
}

impl<'a> MarkdownRenderer<'a> {
    fn new(out: &'a mut dyn Write, config: &'a OmnicatConfig) -> Self {
        Self {
            out,
            config,
            theme: config.terminal.markdown.theme.clone(),
            code_theme: config.terminal.code.theme.clone(),
            in_code_block: false,
            code_block_lang: String::new(),
            code_block_buf: String::new(),
            in_table: false,
            in_table_head: false,
            table_alignments: Vec::new(),
            table_header: Vec::new(),
            table_rows: Vec::new(),
            table_row: Vec::new(),
            table_cell: String::new(),
            list_stack: Vec::new(),
            blockquote_depth: 0,
            in_image: false,
            image_alt: String::new(),
        }
    }

    fn run(mut self, input: &str) -> Result<()> {
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        opts.insert(Options::ENABLE_TABLES);

        for event in Parser::new_ext(input, opts) {
            self.handle_event(event)?;
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event<'_>) -> Result<()> {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => self.write_text(&text),
            Event::Code(code) => self.write_inline_code(&code),
            Event::SoftBreak => self.soft_break(),
            Event::HardBreak => self.hard_break(),
            Event::Rule => self.rule(),
            Event::Html(html) => self.write_html(&html),
            Event::InlineHtml(html) => self.write_html(&html),
            Event::FootnoteReference(_) | Event::TaskListMarker(_) => Ok(()),
            Event::InlineMath(text) | Event::DisplayMath(text) => self.write_flow(&text),
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) -> Result<()> {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                write!(self.out, "{}", heading_color(level, &self.theme))?;
            }
            Tag::BlockQuote(_) => {
                self.blockquote_depth += 1;
                write!(self.out, "{}", blockquote_color(&self.theme))?;
                self.write_blockquote_prefix()?;
            }
            Tag::CodeBlock(kind) => {
                self.in_code_block = true;
                self.code_block_buf.clear();
                self.code_block_lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
            }
            Tag::List(start) => {
                self.list_stack.push(ListState {
                    ordered: start.is_some(),
                    next: start.unwrap_or(1),
                });
            }
            Tag::Item => self.start_list_item()?,
            Tag::Table(alignments) => {
                self.in_table = true;
                self.table_alignments = alignments;
                self.table_header.clear();
                self.table_rows.clear();
            }
            Tag::TableHead => self.in_table_head = true,
            Tag::TableRow => {
                self.table_row.clear();
            }
            Tag::TableCell => {
                self.table_cell.clear();
            }
            Tag::Emphasis => write!(self.out, "{}", emphasis(&self.theme))?,
            Tag::Strong => write!(self.out, "{}", strong(&self.theme))?,
            Tag::Strikethrough => write!(self.out, "{}", strikethrough(&self.theme))?,
            Tag::Link { .. } => write!(self.out, "{}", link(&self.theme))?,
            Tag::Image { title, .. } => {
                self.in_image = true;
                self.image_alt = title.to_string();
            }
            Tag::HtmlBlock | Tag::FootnoteDefinition(_) | Tag::DefinitionList
            | Tag::DefinitionListTitle | Tag::DefinitionListDefinition | Tag::MetadataBlock(_) => {}
        }
        Ok(())
    }

    fn end_tag(&mut self, tag: TagEnd) -> Result<()> {
        match tag {
            TagEnd::Paragraph => self.end_paragraph()?,
            TagEnd::Heading(_) => {
                writeln!(self.out, "{}", reset())?;
                writeln!(self.out)?;
            }
            TagEnd::BlockQuote(_) => {
                writeln!(self.out, "{}", reset())?;
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                writeln!(self.out)?;
            }
            TagEnd::CodeBlock => self.end_code_block()?,
            TagEnd::List(_) => {
                self.list_stack.pop();
                writeln!(self.out)?;
            }
            TagEnd::Item => writeln!(self.out)?,
            TagEnd::Table => self.flush_table()?,
            TagEnd::TableHead => {
                self.in_table_head = false;
                if !self.table_row.is_empty() {
                    self.table_header.push(self.table_row.clone());
                    self.table_row.clear();
                }
            }
            TagEnd::TableRow => {
                if !self.table_row.is_empty() {
                    self.table_rows.push(self.table_row.clone());
                    self.table_row.clear();
                }
            }
            TagEnd::TableCell => {
                self.table_row.push(self.table_cell.trim().to_string());
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                write!(self.out, "{}", reset())?;
            }
            TagEnd::Link => write!(self.out, "{}", reset())?,
            TagEnd::Image => {
                let label = if self.image_alt.is_empty() {
                    "image"
                } else {
                    self.image_alt.as_str()
                };
                write!(self.out, "[image: {label}]")?;
                self.in_image = false;
                self.image_alt.clear();
            }
            TagEnd::HtmlBlock | TagEnd::FootnoteDefinition | TagEnd::DefinitionList
            | TagEnd::DefinitionListTitle | TagEnd::DefinitionListDefinition
            | TagEnd::MetadataBlock(_) => {}
        }
        Ok(())
    }

    fn write_text(&mut self, text: &str) -> Result<()> {
        if self.in_image {
            if !self.image_alt.is_empty() {
                self.image_alt.push(' ');
            }
            self.image_alt.push_str(text);
            return Ok(());
        }
        if self.in_code_block {
            self.code_block_buf.push_str(text);
            return Ok(());
        }
        if self.in_table {
            self.table_cell.push_str(text);
            return Ok(());
        }
        self.write_flow(text)
    }

    fn write_inline_code(&mut self, code: &str) -> Result<()> {
        if self.in_code_block {
            self.code_block_buf.push_str(code);
            return Ok(());
        }
        if self.in_table {
            self.table_cell.push_str(code);
            return Ok(());
        }
        write!(self.out, "{}", code_color(&self.theme))?;
        write!(self.out, "`{code}`")?;
        write!(self.out, "{}", reset())?;
        Ok(())
    }

    fn write_flow(&mut self, text: &str) -> Result<()> {
        let wrap_width = self.config.terminal.markdown.wrap_width as usize;
        if wrap_width == 0 {
            write!(self.out, "{text}")?;
            return Ok(());
        }
        for word in text.split_whitespace() {
            write!(self.out, "{word}")?;
            write!(self.out, " ")?;
        }
        Ok(())
    }

    fn soft_break(&mut self) -> Result<()> {
        if self.in_code_block {
            self.code_block_buf.push('\n');
            return Ok(());
        }
        if self.in_table {
            self.table_cell.push(' ');
            return Ok(());
        }
        write!(self.out, " ")?;
        Ok(())
    }

    fn hard_break(&mut self) -> Result<()> {
        if self.in_code_block {
            self.code_block_buf.push('\n');
            return Ok(());
        }
        writeln!(self.out)?;
        if self.blockquote_depth > 0 {
            self.write_blockquote_prefix()?;
        }
        Ok(())
    }

    fn end_paragraph(&mut self) -> Result<()> {
        if self.in_code_block || self.in_table {
            return Ok(());
        }
        writeln!(self.out)?;
        writeln!(self.out)?;
        Ok(())
    }

    fn start_list_item(&mut self) -> Result<()> {
        let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
        if let Some(list) = self.list_stack.last_mut() {
            if list.ordered {
                write!(self.out, "{indent}{}. ", list.next)?;
                list.next += 1;
            } else {
                write!(self.out, "{indent}• ")?;
            }
        }
        Ok(())
    }

    fn end_code_block(&mut self) -> Result<()> {
        self.in_code_block = false;
        let code = std::mem::take(&mut self.code_block_buf);
        let lang = std::mem::take(&mut self.code_block_lang);
        render_fenced_code(self.out, &lang, &code, &self.code_theme)?;
        writeln!(self.out)?;
        Ok(())
    }

    fn flush_table(&mut self) -> Result<()> {
        self.in_table = false;
        let header = self.table_header.first().cloned();
        let rows = std::mem::take(&mut self.table_rows);
        let alignments = std::mem::take(&mut self.table_alignments);

        markdown_table::render_table(self.out, header, rows, &alignments, self.config)
    }

    fn write_blockquote_prefix(&mut self) -> Result<()> {
        write!(self.out, "│ ")?;
        Ok(())
    }

    fn rule(&mut self) -> Result<()> {
        writeln!(self.out, "{}", rule(&self.theme))?;
        writeln!(self.out)?;
        Ok(())
    }

    fn write_html(&mut self, html: &str) -> Result<()> {
        if self.in_code_block {
            self.code_block_buf.push_str(html);
        } else if self.in_table {
            self.table_cell.push_str(html);
        } else {
            write!(self.out, "{html}")?;
        }
        Ok(())
    }
}

fn render_markdown_str(input: &str, config: &OmnicatConfig, out: &mut dyn Write) -> Result<()> {
    MarkdownRenderer::new(out, config).run(input)
}

fn render_fenced_code(
    out: &mut dyn Write,
    lang: &str,
    code: &str,
    theme_name: &str,
) -> Result<()> {
    writeln!(out)?;
    if code.trim().is_empty() {
        return Ok(());
    }

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = if lang.is_empty() {
        ps.find_syntax_plain_text()
    } else {
        ps.find_syntax_by_token(lang)
            .or_else(|| ps.find_syntax_by_extension(lang))
            .unwrap_or_else(|| ps.find_syntax_plain_text())
    };
    let theme = resolve_theme(&ts, theme_name);
    let mut h = HighlightLines::new(syntax, theme);

    for line in LinesWithEndings::from(code) {
        let ranges = h.highlight_line(line, &ps).context("highlight failed")?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        write!(out, "  {escaped}")?;
    }
    writeln!(out)?;
    Ok(())
}

fn heading_color(level: pulldown_cmark::HeadingLevel, theme: &str) -> String {
    let _ = theme;
    let color = match level {
        pulldown_cmark::HeadingLevel::H1 => "\x1b[1;36m",
        pulldown_cmark::HeadingLevel::H2 => "\x1b[1;34m",
        pulldown_cmark::HeadingLevel::H3 => "\x1b[1;32m",
        _ => "\x1b[1m",
    };
    let marks = match level {
        pulldown_cmark::HeadingLevel::H1 => "# ",
        pulldown_cmark::HeadingLevel::H2 => "## ",
        pulldown_cmark::HeadingLevel::H3 => "### ",
        pulldown_cmark::HeadingLevel::H4 => "#### ",
        pulldown_cmark::HeadingLevel::H5 => "##### ",
        pulldown_cmark::HeadingLevel::H6 => "###### ",
    };
    format!("{color}{marks}")
}

fn blockquote_color(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[90m"
}

fn code_color(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[38;5;245m"
}

fn emphasis(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[3m"
}

fn strong(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[1m"
}

fn strikethrough(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[9m"
}

fn link(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[4;34m"
}

fn rule(theme: &str) -> &'static str {
    let _ = theme;
    "\x1b[90m────────────────────────────────────────\x1b[0m"
}

fn reset() -> &'static str {
    "\x1b[0m"
}

impl PreviewDriver for MarkdownDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Markdown
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["md", "markdown", "mdown", "mkd", "mkdn"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["text/markdown"]
    }

    fn build(
        &self,
        path: &Path,
        _config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let input = fs::read_to_string(path)?;
        Ok(PreviewContent::Markdown(input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmnicatConfig;
    use crate::content::PreviewContent;
    use crate::drivers::markdown_table;

    fn render(input: &str) -> String {
        let mut out = Vec::new();
        render_markdown_str(input, &OmnicatConfig::default(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn build_returns_markdown_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.md");
        std::fs::write(&path, "# Title\n\nHello.\n").unwrap();
        let content = MarkdownDriver
            .build(&path, &OmnicatConfig::default(), &crate::content::preview_context(&path))
            .unwrap();
        match content {
            PreviewContent::Markdown(text) => {
                assert!(text.contains("Title"));
                assert!(text.contains("Hello."));
            }
            other => panic!("expected Markdown, got {other:?}"),
        }
    }

    #[test]
    fn renders_sentinel_with_ansi() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.md");
        std::fs::write(&path, "# Title\n\nSENTINEL-MD-CONTENT\n").unwrap();
        let mut out = Vec::new();
        MarkdownDriver
            .render(&path, &OmnicatConfig::default(), &mut out)
            .unwrap();
        let rendered = String::from_utf8(out).unwrap();
        assert!(rendered.contains("SENTINEL-MD-CONTENT"));
        assert!(rendered.contains("\x1b["));
    }

    #[test]
    fn paragraphs_are_separated() {
        let rendered = render("First paragraph.\n\nSecond paragraph.\n");
        assert!(rendered.contains("First paragraph."));
        assert!(rendered.contains("Second paragraph."));
        let gap = rendered.find("First paragraph.").unwrap();
        let second = rendered.find("Second paragraph.").unwrap();
        assert!(second > gap);
        let between = &rendered[gap + "First paragraph.".len()..second];
        assert!(between.contains("\n\n"), "expected blank line between paragraphs: {between:?}");
    }

    #[test]
    fn format_matrix_renders_as_table() {
        let mut buf = Vec::new();
        let header = vec![
            "Driver".into(),
            "Extensions".into(),
            "Rust crate".into(),
            "Recommended external".into(),
        ];
        let rows = vec![vec![
            "markdown".into(),
            "md, markdown".into(),
            "pulldown-cmark".into(),
            "glow, mdcat".into(),
        ]];
        let mut config = OmnicatConfig::default();
        config.terminal.markdown.wrap_width = 80;
        markdown_table::render_table(&mut buf, Some(header), rows, &[], &config).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("markdown"));
        assert!(!s.contains("DriverExtensions"));
        assert!(!s.contains("── row"));
    }

    #[test]
    fn wide_format_matrix_is_table_with_borders() {
        let md = "| Driver | Extensions | Rust crate | Recommended external |\n\
                  |--------|------------|------------|----------------------|\n\
                  | markdown | md | pulldown-cmark | glow |\n";
        let rendered = render(md);
        assert!(!rendered.contains("── row 1 ──"));
        assert!(!rendered.contains("Driver:"));
        assert!(
            rendered.contains('│') || rendered.contains('|'),
            "expected table: {rendered}"
        );
    }

    #[test]
    fn truncates_many_rows() {
        let mut buf = Vec::new();
        let header = vec!["A".into(), "B".into()];
        let rows: Vec<Vec<String>> = (0..50).map(|i| vec![format!("r{i}"), "x".into()]).collect();
        let mut config = OmnicatConfig::default();
        config.terminal.data.max_rows = 5;
        markdown_table::render_table(&mut buf, Some(header), rows, &[], &config).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("more row"));
    }

    #[test]
    fn table_uses_borders() {
        let rendered = render(
            "| Col A | Col B |\n|-------|-------|\n| one   | two   |\n| three | four  |\n",
        );
        assert!(rendered.contains("Col A"));
        assert!(rendered.contains("one"));
        assert!(rendered.contains("three"));
        assert!(
            rendered.contains('│')
                || rendered.contains('─')
                || rendered.contains('|'),
            "expected table output: {rendered}"
        );
        assert!(
            rendered.contains('├') || rendered.contains('╌') || rendered.contains('┼'),
            "expected row separators: {rendered}"
        );
        assert!(!rendered.contains("Col Bone"));
    }

    #[test]
    fn list_uses_bullets() {
        let rendered = render("- alpha\n- beta\n");
        assert!(rendered.contains('•'));
        assert!(rendered.contains("alpha"));
        assert!(rendered.contains("beta"));
    }

    #[test]
    fn ordered_list_uses_numbers() {
        let rendered = render("1. first\n2. second\n");
        assert!(rendered.contains("1. first"));
        assert!(rendered.contains("2. second"));
    }

    #[test]
    fn inline_code_has_backticks() {
        let rendered = render("Use `omnicat` here.\n");
        assert!(rendered.contains("`omnicat`"));
    }

    #[test]
    fn readme_like_cli_table_not_mashed() {
        let rendered = render(
            "## CLI\n\n| Command | Description |\n|---------|-------------|\n| `omnicat <file>` | Render when stdout is a TTY |\n",
        );
        assert!(!rendered.contains("CommandDescription"));
        assert!(rendered.contains("Command"));
        assert!(rendered.contains("Description"));
    }
}
