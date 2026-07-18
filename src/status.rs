use std::fmt::Write as _;

use anyhow::Result;

use crate::config::{load_config, resolved_config_path, OmnicatConfig};
use crate::detect::HandlerKind;
use crate::drivers::external::{external_status, first_available_command};
use crate::orchestrator::resolve::handler_config_for_builtin;
use crate::orchestrator::{custom_handlers, extensions_for};
use crate::preview::availability::gui_available;
use crate::VERSION;

const APP_NAME: &str = "omnicat";

pub fn print_status() -> Result<()> {
    let config = load_config()?;
    let config_label = resolved_config_path()?
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "bundled defaults".to_string());

    println!("{APP_NAME} {VERSION}");
    println!("config: {config_label}");
    println!(
        "gui: {}",
        if gui_available() {
            "available"
        } else {
            "unavailable"
        }
    );
    println!();

    println!("BUILT-IN HANDLERS");
    println!(
        "{:<14} {:<24} {:<28} EXTENSIONS",
        "TYPE", "BUILTIN", "EXTERNAL"
    );

    for kind in HandlerKind::all() {
        let exts = extensions_for(*kind).join(", ");
        let builtin = format!("{}(+)", kind.renderer_name());
        let external = handler_config_for_builtin(*kind, &config)
            .filter(|h| !h.commands.is_empty())
            .map(|h| external_status(&h.commands))
            .unwrap_or_else(|| "—".into());
        let active = active_renderer_label(*kind, &config);
        println!(
            "{:<14} {:<24} {:<28} {exts}",
            kind.name(),
            builtin,
            if active.is_empty() {
                external
            } else {
                format!("{external}  → {active}")
            },
        );
    }

    let customs = custom_handlers(&config);
    if !customs.is_empty() {
        println!("\nCUSTOM HANDLERS");
        println!(
            "{:<14} {:<24} {:<28} EXTENSIONS",
            "TYPE", "BUILTIN", "EXTERNAL"
        );
        for name in customs {
            if let Some(handler) = config.handlers.get(name) {
                let exts = handler.extensions.join(", ");
                let external = if handler.commands.is_empty() {
                    "—".into()
                } else {
                    external_status(&handler.commands)
                };
                let active = handler
                    .commands
                    .first()
                    .and_then(|_| first_available_command(&handler.commands))
                    .map(|c| c.split_whitespace().next().unwrap_or(c).to_string())
                    .unwrap_or_else(|| "none".into());
                println!(
                    "{:<14} {:<24} {:<28} {exts}",
                    name,
                    "—",
                    format!("{external}  → {active}"),
                );
            }
        }
    }

    println!("\nTERMINAL SETTINGS");
    print_terminal_settings(&config);

    println!("\nGUI SETTINGS");
    print_gui_settings(&config);

    println!("\nBEHAVIOR");
    println!("  preview_fallback: {}", config.behavior.preview_fallback);
    println!("  on_unknown_format: {}", config.behavior.on_unknown_format);
    println!(
        "  external_timeout_secs: {}",
        config.behavior.external_timeout_secs
    );

    Ok(())
}

fn active_renderer_label(kind: HandlerKind, config: &OmnicatConfig) -> String {
    if let Some(handler) = handler_config_for_builtin(kind, config) {
        if let Some(cmd) = first_available_command(&handler.commands) {
            return cmd.split_whitespace().next().unwrap_or(cmd).to_string();
        }
    }
    String::new()
}

fn print_terminal_settings(cfg: &OmnicatConfig) {
    let t = &cfg.terminal;
    let mut buf = String::new();
    let _ = writeln!(buf, "  markdown.wrap_width: {}", t.markdown.wrap_width);
    let _ = writeln!(buf, "  code.line_numbers: {}", t.code.line_numbers);
    let _ = writeln!(buf, "  code.theme: {}", t.code.theme);
    let _ = writeln!(buf, "  data.pretty: {}", t.data.pretty);
    let _ = writeln!(buf, "  image.protocol: {}", t.image.protocol);
    let _ = writeln!(buf, "  fallback.hex_cols: {}", t.fallback.hex_cols);
    let _ = writeln!(
        buf,
        "  paginate.enabled: {}  page_lines: {}",
        t.paginate.enabled, t.paginate.page_lines
    );
    print!("{buf}");
}

fn print_gui_settings(cfg: &OmnicatConfig) {
    let g = &cfg.gui;
    let mut buf = String::new();
    let _ = writeln!(buf, "  window: {}x{}", g.window.width, g.window.height);
    let _ = writeln!(buf, "  theme.mode: {}", g.theme.mode);
    let _ = writeln!(buf, "  theme.font_size: {}", g.theme.font_size);
    let _ = writeln!(buf, "  spreadsheet.max_rows: {}", g.spreadsheet.max_rows);
    let _ = writeln!(
        buf,
        "  document.max_paragraphs: {}",
        g.document.max_paragraphs
    );
    print!("{buf}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::external::command_status_label;
    use crate::orchestrator::resolve::is_builtin_handler_name;

    #[test]
    fn status_includes_markdown() {
        let config = OmnicatConfig::default();
        let mut buf = String::new();
        for kind in HandlerKind::all() {
            buf.push_str(kind.name());
        }
        assert!(buf.contains("markdown"));
        assert_eq!(config.terminal.code.line_numbers, true);
    }

    #[test]
    fn command_status_labels() {
        assert!(matches!(
            command_status_label("definitely-not-a-real-binary-xyz"),
            "(-)"
        ));
    }

    #[test]
    fn builtin_handler_names_detected() {
        assert!(is_builtin_handler_name("markdown"));
        assert!(!is_builtin_handler_name("notebook_custom"));
    }
}
