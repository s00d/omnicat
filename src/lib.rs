pub mod cli;
pub mod config;
pub mod content;
pub mod detect;
pub mod drivers;
pub mod gate;
pub mod init;
pub mod orchestrator;
pub mod preview;
pub mod sinks;
pub mod status;

mod cat;

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;

use crate::cat::{exec_system_cat, passthrough_cat};
use crate::cli::{Cli, Command, FileOptions};
use crate::config::load_config;
use crate::detect::HandlerKind;
use crate::gate::should_render;
use crate::orchestrator::resolve::ResolvedHandler;
use crate::orchestrator::{print_hint, PreviewOrchestrator};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "omnicat";

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Version => {
            println!("{APP_NAME} {VERSION}");
        }
        Command::Help => {
            print!("{}", Cli::help_text());
        }
        Command::Init { shell } => {
            init::print_init(&shell)?;
        }
        Command::Status => {
            status::print_status()?;
        }
        Command::Native { args } => {
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            exec_system_cat(&refs)?;
        }
        Command::File { path, options } => {
            handle_file(&path, &options)?;
        }
    }

    Ok(())
}

pub fn run_main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{APP_NAME}: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn handle_file(path: &str, options: &FileOptions) -> Result<()> {
    if !should_render(path) {
        passthrough_cat(&[path])?;
        return Ok(());
    }

    let path = Path::new(path);
    let config = load_config()?;

    let resolved = PreviewOrchestrator::resolve(path, &config).or_else(|| {
        if config.behavior.on_unknown_format == "fallback" {
            Some(ResolvedHandler::Builtin(HandlerKind::Fallback))
        } else {
            None
        }
    });

    if let Some(resolved) = resolved {
        if options.preview {
            match preview::try_open_preview(path, &resolved, &config) {
                Ok(true) => {
                    if options.preview_only {
                        return Ok(());
                    }
                }
                Ok(false) => {
                    eprintln!(
                        "{APP_NAME}: GUI preview unavailable (no display); falling back to terminal render"
                    );
                    if config.behavior.preview_fallback == "cat" {
                        passthrough_cat(&[path.to_string_lossy().as_ref()])?;
                        return Ok(());
                    }
                }
                Err(err) => {
                    eprintln!("{APP_NAME}: preview failed: {err:#}; falling back");
                }
            }
        }

        if options.preview_only {
            return Ok(());
        }

        let use_pagination = sinks::paginate::pagination_requested(&config, options.paginate)
            && !sinks::paginate::skips_pagination(&resolved);

        let render_config = if use_pagination {
            let mut c = config.clone();
            // Full text for the pager; screen size limits display, not extraction.
            c.terminal.document.max_chars = 0;
            c.terminal.plain = true;
            c
        } else {
            config.clone()
        };

        if use_pagination {
            let mut buf = Vec::new();
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved(
                &resolved,
                path,
                &render_config,
                &mut buf,
            )
            {
                eprintln!("{APP_NAME}: {err:#}");
                if let Some(handler) = handler_config_for_resolved(&resolved, &config) {
                    print_hint(handler);
                }
                passthrough_cat(&[path.to_string_lossy().as_ref()])?;
            } else {
                sinks::paginate::write_paged(&buf, &config.terminal.paginate)?;
            }
        } else {
            let mut stdout = io::stdout().lock();
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved(
                &resolved,
                path,
                &render_config,
                &mut stdout,
            )
            {
                drop(stdout);
                eprintln!("{APP_NAME}: {err:#}");
                if let Some(handler) = handler_config_for_resolved(&resolved, &config) {
                    print_hint(handler);
                }
                passthrough_cat(&[path.to_string_lossy().as_ref()])?;
            } else {
                stdout.flush()?;
            }
        }
    } else {
        passthrough_cat(&[path.to_string_lossy().as_ref()])?;
    }

    Ok(())
}

fn handler_config_for_resolved<'a>(
    resolved: &ResolvedHandler,
    config: &'a crate::config::OmnicatConfig,
) -> Option<&'a crate::config::HandlerConfig> {
    match resolved {
        ResolvedHandler::Builtin(kind) => {
            crate::orchestrator::resolve::handler_config_for_builtin(*kind, config)
        }
        ResolvedHandler::Custom(name) => {
            crate::orchestrator::resolve::handler_config_for_custom(name, config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_constant_matches_cargo() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }
}
