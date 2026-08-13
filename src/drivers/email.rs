use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use mail_parser::{HeaderName, MessageParser};

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::convert;
use crate::drivers::html_md;
use crate::drivers::PreviewDriver;

pub struct EmailDriver;

impl PreviewDriver for EmailDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Email
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["eml"]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["message/rfc822"]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let mut raw = Vec::new();
        File::open(path)?.read_to_end(&mut raw)?;
        let msg = MessageParser::default()
            .parse(&raw)
            .context("invalid eml")?;
        let mut out = String::new();
        if let Some(from) = msg.header_raw(HeaderName::From) {
            out.push_str(&format!("**From:** {from}\n\n"));
        }
        if let Some(to) = msg.header_raw(HeaderName::To) {
            out.push_str(&format!("**To:** {to}\n\n"));
        }
        if let Some(subject) = msg.subject() {
            out.push_str(&format!("# {subject}\n\n"));
        }
        if let Some(body) = msg.body_text(0) {
            out.push_str(&body);
        } else if let Some(html) = msg.body_html(0) {
            out.push_str(&html_md::html_to_markdown(&html));
        }
        let md = convert::truncate_chars(&out, convert::preview_char_limit(config));
        Ok(PreviewContent::Markdown(md))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::preview_context;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn parses_simple_eml() {
        let mut f = NamedTempFile::with_suffix(".eml").unwrap();
        write!(
            f,
            "From: sender@example.com\r\n\
             To: recipient@example.com\r\n\
             Subject: Test\r\n\
             \r\n\
             Hello email body\r\n"
        )
        .unwrap();
        let ctx = preview_context(f.path());
        let content = EmailDriver
            .build(f.path(), &OmnicatConfig::default(), &ctx)
            .unwrap();
        let text = content.plain_text();
        assert!(text.contains("sender@example.com"));
        assert!(text.contains("Hello email body"));
    }
}
