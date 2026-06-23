use std::io::Write;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::PreviewContent;
use crate::detect::HandlerKind;
use crate::drivers::external;
use crate::drivers::image::ImageDriver;
use crate::drivers::code::CodeDriver;
use crate::drivers::ebook::EbookDriver;
use crate::drivers::markdown::MarkdownDriver;
use crate::drivers::media::MediaDriver;
use crate::sinks::gui;
use crate::sinks::terminal;

pub mod registry;
pub mod resolve;

pub use resolve::ResolvedHandler;

use registry::DriverRegistry;
use resolve::{
    detect_custom, handler_config_for_builtin, handler_config_for_custom,
};

pub struct PreviewOrchestrator;

impl PreviewOrchestrator {
    pub fn resolve(path: &Path, config: &OmnicatConfig) -> Option<ResolvedHandler> {
        if let Some(kind) = DriverRegistry::detect_builtin(path) {
            return Some(ResolvedHandler::Builtin(kind));
        }
        detect_custom(path, config).map(ResolvedHandler::Custom)
    }

    pub fn detect(path: &Path, config: &OmnicatConfig) -> Option<ResolvedHandler> {
        Self::resolve(path, config)
    }

    pub fn build_resolved(
        resolved: &ResolvedHandler,
        path: &Path,
        config: &OmnicatConfig,
    ) -> Result<PreviewContent> {
        let timeout = external_timeout(config);
        match resolved {
            ResolvedHandler::Builtin(kind) => {
                if let Some(handler) = handler_config_for_builtin(*kind, config) {
                    if let Some(content) = external::try_build_content(handler, path, timeout)? {
                        return Ok(content);
                    }
                }
                DriverRegistry::build(*kind, path, config)
            }
            ResolvedHandler::Custom(name) => {
                let handler = handler_config_for_custom(name, config)
                    .ok_or_else(|| anyhow::anyhow!("missing custom handler {name}"))?;
                if let Some(content) = external::try_build_content(handler, path, timeout)? {
                    return Ok(content);
                }
                anyhow::bail!("no external renderer available for custom handler {name}")
            }
        }
    }

    pub fn build(
        kind: HandlerKind,
        path: &Path,
        config: &OmnicatConfig,
    ) -> Result<PreviewContent> {
        Self::build_resolved(&ResolvedHandler::Builtin(kind), path, config)
    }

    pub fn render_terminal_resolved(
        resolved: &ResolvedHandler,
        path: &Path,
        config: &OmnicatConfig,
        out: &mut dyn Write,
    ) -> Result<()> {
        let timeout = external_timeout(config);
        match resolved {
            ResolvedHandler::Builtin(kind) => {
                if let Some(handler) = handler_config_for_builtin(*kind, config) {
                    if external::try_render_terminal(handler, path, timeout, out)? {
                        return Ok(());
                    }
                }
                if *kind == HandlerKind::Image {
                    return ImageDriver.render_terminal(path, config, out);
                }
                if *kind == HandlerKind::Markdown {
                    return MarkdownDriver.render(path, config, out);
                }
                if *kind == HandlerKind::Code {
                    return CodeDriver.render(path, config, out);
                }
                if *kind == HandlerKind::Ebook {
                    return EbookDriver.render_terminal(path, config, out);
                }
                if *kind == HandlerKind::Media {
                    return MediaDriver.render_terminal(path, config, out);
                }
                let content = DriverRegistry::build(*kind, path, config)?;
                terminal::write_content(&content, config, out)
            }
            ResolvedHandler::Custom(name) => {
                let handler = handler_config_for_custom(name, config)
                    .ok_or_else(|| anyhow::anyhow!("missing custom handler {name}"))?;
                if external::try_render_terminal(handler, path, timeout, out)? {
                    return Ok(());
                }
                print_hint(handler);
                anyhow::bail!("no external renderer for custom handler {name}")
            }
        }
    }

    pub fn render_terminal(
        kind: HandlerKind,
        path: &Path,
        config: &OmnicatConfig,
        out: &mut dyn Write,
    ) -> Result<()> {
        Self::render_terminal_resolved(&ResolvedHandler::Builtin(kind), path, config, out)
    }

    pub fn open_gui(
        path: &Path,
        config: &OmnicatConfig,
        content: &PreviewContent,
    ) -> Result<()> {
        gui::run(path, config, content)
    }

    pub fn hint_for(resolved: &ResolvedHandler, config: &OmnicatConfig) -> Option<String> {
        match resolved {
            ResolvedHandler::Builtin(kind) => handler_config_for_builtin(*kind, config)
                .and_then(|h| h.hint.clone()),
            ResolvedHandler::Custom(name) => handler_config_for_custom(name, config)
                .and_then(|h| h.hint.clone()),
        }
    }
}

fn external_timeout(config: &OmnicatConfig) -> Duration {
    Duration::from_secs(config.behavior.external_timeout_secs.max(1))
}

pub fn print_hint(handler: &crate::config::HandlerConfig) {
    if let Some(hint) = &handler.hint {
        eprintln!("omnicat: {hint}");
    }
}

pub fn extensions_for(kind: HandlerKind) -> Vec<&'static str> {
    DriverRegistry::extensions_for(kind)
}

pub fn custom_handlers(config: &OmnicatConfig) -> Vec<&str> {
    config
        .handlers
        .keys()
        .map(String::as_str)
        .filter(|name| !resolve::is_builtin_handler_name(name))
        .collect()
}
