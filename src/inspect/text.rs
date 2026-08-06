use std::fs;
use std::io::{self, Write};
use std::path::Path;

use anyhow::{bail, Result};

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::encoding::analyze_text_file;
use crate::orchestrator::registry::DriverRegistry;

pub fn extract_text(path: &Path, kind: HandlerKind, config: &OmnicatConfig) -> Result<String> {
    match kind {
        HandlerKind::Pdf
        | HandlerKind::Document
        | HandlerKind::Ebook
        | HandlerKind::Email
        | HandlerKind::Spreadsheet
        | HandlerKind::Presentation
        | HandlerKind::Notebook
        | HandlerKind::LegacyOffice => {
            let content = DriverRegistry::build(kind, path, config)?;
            Ok(content.plain_text())
        }
        HandlerKind::Image | HandlerKind::Media | HandlerKind::Font | HandlerKind::Archive => {
            bail!("text extraction not supported for {}", kind.name())
        }
        HandlerKind::Directory => bail!("use path to a file for --text"),
        HandlerKind::Database => {
            let content = DriverRegistry::build(kind, path, config)?;
            Ok(content.plain_text())
        }
        _ => {
            let max = if config.inspect.no_limit {
                0
            } else {
                config.inspect.max_bytes
            };
            Ok(analyze_text_file(path, max)?.text)
        }
    }
}

pub fn write_raw(path: &Path, out: &mut dyn Write) -> Result<()> {
    let mut file = fs::File::open(path)?;
    io::copy(&mut file, out)?;
    Ok(())
}

pub fn sanitize_text(text: &str, allow_unsafe: bool) -> String {
    if allow_unsafe {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\t' | '\n' | '\r' => out.push(ch),
            c if c.is_control() => {
                out.push_str(&format!("\\u{{{:04x}}}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}
