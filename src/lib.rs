pub mod cli;
pub mod config;
pub mod content;
pub mod detect;
pub mod drivers;
pub mod gate;
pub mod init;
pub mod input;
pub mod inspect;
pub mod io;
pub mod orchestrator;
pub mod preview;
pub mod search;
pub mod sinks;
pub mod status;

#[doc(hidden)]
pub mod bench_support;

mod cat;

use std::io::Write;
use std::process::ExitCode;

use anyhow::Result;

use crate::cat::{exec_system_cat, passthrough_cat};
use crate::cli::{Cli, Command, FileOptions};
use crate::config::load_config;
use crate::detect::HandlerKind;
use crate::gate::should_render;
use crate::inspect::InspectOptions;
use crate::orchestrator::resolve::ResolvedHandler;
use crate::orchestrator::{print_hint, PreviewOrchestrator};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "omnicat";

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
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
        Command::Inspect { path, options } => {
            handle_inspect(path.as_deref(), &options)?;
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

fn handle_inspect(path: Option<&str>, options: &InspectOptions) -> Result<()> {
    let config = load_config()?;
    inspect::run_inspect(path, options, &config)
}

fn handle_file(path: &str, options: &FileOptions) -> Result<()> {
    let input = match input::InputRef::parse(path) {
        Ok(input) => input,
        Err(_) => {
            passthrough_cat(&[path])?;
            return Ok(());
        }
    };

    let render_path = input.path_for_ops();
    let is_virtual = input.virtual_path.is_some();

    // Plain preview still requires TTY; virtual archive paths always render.
    if !is_virtual && !should_render(path) {
        passthrough_cat(&[path])?;
        return Ok(());
    }

    let config = load_config()?;
    let mut config = config;
    if options.allow_unsafe {
        config.inspect.allow_unsafe = true;
    }
    if options.all {
        config.inspect.no_limit = true;
        config.inspect.max_bytes = 0;
        config.inspect.max_rows = 0;
    }

    let mut preview_ctx = content::preview_context(render_path);
    preview_ctx.sql_schema_only = options.schema_only;
    preview_ctx.sql_table = options.table.clone();
    preview_ctx.progress = options.progress;

    let resolved = PreviewOrchestrator::resolve(render_path, &config).or_else(|| {
        if config.behavior.on_unknown_format == "fallback" {
            Some(ResolvedHandler::Builtin(HandlerKind::Fallback))
        } else {
            None
        }
    });

    if let Some(resolved) = resolved {
        if options.preview {
            match preview::try_open_preview_with_ctx(render_path, &resolved, &config, &preview_ctx)
            {
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
                        passthrough_cat(&[path])?;
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

        let use_pagination = sinks::paginate::pagination_requested(options.paginate, options.page)
            && !sinks::paginate::skips_pagination(&resolved);

        let render_config = if use_pagination {
            let mut c = config.clone();
            c.terminal.document.max_chars = 0;
            c.terminal.plain = true;
            c
        } else {
            config.clone()
        };

        if use_pagination {
            let mut buf = Vec::new();
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved_with_ctx(
                &resolved,
                render_path,
                &render_config,
                &preview_ctx,
                &mut buf,
            ) {
                eprintln!("{APP_NAME}: {err:#}");
                if let Some(handler) = handler_config_for_resolved(&resolved, &config) {
                    print_hint(handler);
                }
                passthrough_cat(&[path])?;
            } else {
                let page = options.page.unwrap_or(1);
                sinks::paginate::write_page(&buf, &config.terminal.paginate, page)?;
            }
        } else {
            let mut stdout = std::io::stdout().lock();
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved_with_ctx(
                &resolved,
                render_path,
                &render_config,
                &preview_ctx,
                &mut stdout,
            ) {
                drop(stdout);
                eprintln!("{APP_NAME}: {err:#}");
                if let Some(handler) = handler_config_for_resolved(&resolved, &config) {
                    print_hint(handler);
                }
                passthrough_cat(&[path])?;
            } else {
                stdout.flush()?;
            }
        }
    } else {
        passthrough_cat(&[path])?;
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
