pub mod availability;

use std::path::Path;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::orchestrator::resolve::ResolvedHandler;
use crate::orchestrator::PreviewOrchestrator;
use crate::sinks::gui;

pub use availability::gui_available;

pub fn open_preview(
    path: &Path,
    _resolved: &ResolvedHandler,
    config: &OmnicatConfig,
    content: &crate::content::PreviewContent,
) -> Result<()> {
    if !availability::gui_available() {
        anyhow::bail!("GUI preview unavailable");
    }
    gui::run(path, config, content)
}

pub fn try_open_preview(
    path: &Path,
    resolved: &ResolvedHandler,
    config: &OmnicatConfig,
) -> Result<bool> {
    try_open_preview_with_ctx(
        path,
        resolved,
        config,
        &crate::content::preview_context(path),
    )
}

pub fn try_open_preview_with_ctx(
    path: &Path,
    resolved: &ResolvedHandler,
    config: &OmnicatConfig,
    ctx: &crate::content::PreviewContext,
) -> Result<bool> {
    if !availability::gui_available() {
        return Ok(false);
    }
    let content = PreviewOrchestrator::build_resolved_with_ctx(resolved, path, config, ctx)?;
    open_preview(path, resolved, config, &content)?;
    Ok(true)
}
