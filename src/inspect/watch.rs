use std::io::{self, Write};
use std::path::Path;
use std::thread;
use std::time::Duration;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::inspect::InspectOptions;
use crate::orchestrator::PreviewOrchestrator;

/// Re-render preview (or info) when the file changes.
pub fn watch_file(path: &Path, options: &InspectOptions, config: &OmnicatConfig) -> Result<()> {
    let mut last = file_stamp(path);
    render_once(path, options, config)?;
    loop {
        thread::sleep(Duration::from_millis(500));
        let stamp = file_stamp(path);
        if stamp != last {
            last = stamp;
            // clear screen
            print!("\x1b[2J\x1b[H");
            let _ = io::stdout().flush();
            render_once(path, options, config)?;
        }
    }
}

fn file_stamp(path: &Path) -> (u64, Option<std::time::SystemTime>) {
    match std::fs::metadata(path) {
        Ok(m) => (m.len(), m.modified().ok()),
        Err(_) => (0, None),
    }
}

fn render_once(path: &Path, options: &InspectOptions, config: &OmnicatConfig) -> Result<()> {
    if options.info
        || options.schema
        || options.stats
        || options.type_only
        || options.mime_only
        || options.encoding
    {
        // Avoid re-entering watch/follow.
        let mut opts = options.clone();
        opts.watch = false;
        opts.follow = false;
        crate::inspect::run_inspect_path(path, &opts, config)?;
    } else {
        let resolved = PreviewOrchestrator::resolve(path, config).unwrap_or(
            crate::orchestrator::ResolvedHandler::Builtin(crate::detect::HandlerKind::Fallback),
        );
        let mut stdout = io::stdout().lock();
        PreviewOrchestrator::render_terminal_resolved(&resolved, path, config, &mut stdout)?;
        stdout.flush()?;
    }
    Ok(())
}
