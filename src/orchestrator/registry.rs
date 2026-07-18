use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{preview_context, PreviewContent};
use crate::detect::mime::mime_matches;
use crate::detect::HandlerKind;
use crate::drivers::archive::ArchiveDriver;
use crate::drivers::code::CodeDriver;
use crate::drivers::data::DataDriver;
use crate::drivers::database::DatabaseDriver;
use crate::drivers::directory::DirectoryDriver;
use crate::drivers::ebook::EbookDriver;
use crate::drivers::email::EmailDriver;
use crate::drivers::fallback::FallbackDriver;
use crate::drivers::font::FontDriver;
use crate::drivers::image::ImageDriver;
use crate::drivers::markdown::MarkdownDriver;
use crate::drivers::media::MediaDriver;
use crate::drivers::notebook::NotebookDriver;
use crate::drivers::office::document::DocumentDriver;
use crate::drivers::office::legacy::LegacyOfficeDriver;
use crate::drivers::office::presentation::PresentationDriver;
use crate::drivers::office::spreadsheet::SpreadsheetDriver;
use crate::drivers::pdf::PdfDriver;
use crate::drivers::plist::PlistDriver;
use crate::drivers::PreviewDriver;

static DRIVERS: &[&dyn PreviewDriver] = &[
    &MarkdownDriver,
    &ImageDriver,
    &DataDriver,
    &PdfDriver,
    &ArchiveDriver,
    &CodeDriver,
    &SpreadsheetDriver,
    &DocumentDriver,
    &PresentationDriver,
    &LegacyOfficeDriver,
    &DirectoryDriver,
    &EbookDriver,
    &MediaDriver,
    &FontDriver,
    &DatabaseDriver,
    &EmailDriver,
    &NotebookDriver,
    &PlistDriver,
    &FallbackDriver,
];

pub struct DriverRegistry;

impl DriverRegistry {
    pub fn detect(path: &Path) -> Option<HandlerKind> {
        Self::detect_builtin(path)
    }

    pub fn detect_builtin(path: &Path) -> Option<HandlerKind> {
        if path.is_dir() {
            return Some(HandlerKind::Directory);
        }

        if let Some(ext) = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .filter(|e| !e.is_empty())
        {
            for driver in DRIVERS {
                if driver.extensions().iter().any(|e| *e == ext) {
                    return Some(driver.kind());
                }
            }
        }

        let bytes = fs::read(path).ok()?;
        let guessed = infer::get(&bytes)?.mime_type().to_string();
        for driver in DRIVERS {
            for pattern in driver.mime_patterns() {
                if mime_matches(pattern, &guessed) {
                    return Some(driver.kind());
                }
            }
        }
        None
    }

    pub fn driver_for(kind: HandlerKind) -> Option<&'static dyn PreviewDriver> {
        DRIVERS.iter().copied().find(|d| d.kind() == kind)
    }

    pub fn build(kind: HandlerKind, path: &Path, config: &OmnicatConfig) -> Result<PreviewContent> {
        let driver =
            Self::driver_for(kind).ok_or_else(|| anyhow::anyhow!("no driver for {kind}"))?;
        let ctx = preview_context(path);
        driver.build(path, config, &ctx)
    }

    pub fn extensions_for(kind: HandlerKind) -> Vec<&'static str> {
        DRIVERS
            .iter()
            .find(|d| d.kind() == kind)
            .map(|d| d.extensions().to_vec())
            .unwrap_or_default()
    }

    pub fn all_drivers() -> &'static [&'static dyn PreviewDriver] {
        DRIVERS
    }
}
