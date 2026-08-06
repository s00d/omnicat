pub mod cli;
pub mod config;
pub mod content;
pub mod db;
pub mod detect;
pub mod drivers;
pub mod editor;
pub mod gate;
pub mod init;
pub mod input;
pub mod inspect;
pub mod io;
pub mod log;
pub mod orchestrator;
pub mod preview;
pub mod sinks;
pub mod status;

mod cat;

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;

use crate::cat::{exec_system_cat, passthrough_cat};
use crate::cli::{Cli, Command, FileOptions};
use crate::config::load_config;
use crate::detect::HandlerKind;
use crate::gate::should_render;
use crate::inspect::InspectOptions;
use crate::log::options::LogOptions;
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
        Command::Open { file, editor } => {
            editor::open_path(Path::new(&file), editor.as_deref())?;
        }
        Command::Log { paths, options } => {
            handle_log(&paths, &options)?;
        }
        Command::Db { source, options } => {
            db::run_db(&source, &options)?;
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

fn handle_log(paths: &[std::path::PathBuf], options: &LogOptions) -> Result<()> {
    log::commands::run_log(paths, options)
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

    let resolved = PreviewOrchestrator::resolve(render_path, &config).or_else(|| {
        if config.behavior.on_unknown_format == "fallback" {
            Some(ResolvedHandler::Builtin(HandlerKind::Fallback))
        } else {
            None
        }
    });

    if let Some(resolved) = resolved {
        if options.preview {
            match preview::try_open_preview(render_path, &resolved, &config) {
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

        let use_pagination = sinks::paginate::pagination_requested(&config, options.paginate)
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
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved(
                &resolved,
                render_path,
                &render_config,
                &mut buf,
            ) {
                eprintln!("{APP_NAME}: {err:#}");
                if let Some(handler) = handler_config_for_resolved(&resolved, &config) {
                    print_hint(handler);
                }
                passthrough_cat(&[path])?;
            } else {
                sinks::paginate::write_paged(&buf, &config.terminal.paginate)?;
            }
        } else {
            let mut stdout = std::io::stdout().lock();
            if let Err(err) = PreviewOrchestrator::render_terminal_resolved(
                &resolved,
                render_path,
                &render_config,
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
