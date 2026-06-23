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
    if !availability::gui_available() {
        return Ok(false);
    }
    let content = PreviewOrchestrator::build_resolved(resolved, path, config)?;
    open_preview(path, resolved, config, &content)?;
    Ok(true)
}
