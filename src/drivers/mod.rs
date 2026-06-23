use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{PreviewContent, PreviewContext};
use crate::detect::HandlerKind;

pub mod archive;
pub mod code;
pub mod data;
pub mod database;
pub mod directory;
pub mod ebook;
pub mod email;
pub mod external;
pub mod fallback;
pub mod font;
pub mod highlight;
pub mod image;
pub mod markdown;
mod markdown_table;
pub mod media;
pub mod notebook;
pub mod office;
pub mod pdf;
pub mod plist;
mod theme;

pub trait PreviewDriver: Send + Sync {
    fn kind(&self) -> HandlerKind;
    fn extensions(&self) -> &'static [&'static str];
    fn mime_patterns(&self) -> &'static [&'static str];
    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
    ) -> Result<PreviewContent>;
}

pub fn capture_render<F>(mut f: F) -> Result<String>
where
    F: FnMut(&mut Vec<u8>) -> Result<()>,
{
    let mut buf = Vec::new();
    f(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}
