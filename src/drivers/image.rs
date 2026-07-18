use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use crossterm::terminal::size;
use image::GenericImageView;
use viuer::{
    get_kitty_support, is_iterm_supported, print_from_file, Config as ViuerConfig, KittySupport,
};

use crate::config::OmnicatConfig;
use crate::content::{ImageContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;

pub struct ImageDriver;

impl ImageDriver {
    pub fn render_terminal(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        out: &mut dyn Write,
    ) -> Result<()> {
        let conf = build_config(config);
        out.flush()?;
        print_from_file(path, &conf).context("image render failed")?;
        // viuer writes directly to stdout; end on a fresh line for the prompt.
        let _ = writeln!(std::io::stdout());
        Ok(())
    }
}

impl PreviewDriver for ImageDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Image
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "heic", "ico",
        ]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["image/*"]
    }

    fn build(
        &self,
        path: &Path,
        _config: &OmnicatConfig,
        _ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let img = image::open(path)?;
        let (width, height) = img.dimensions();
        let rgba = img.to_rgba8().into_raw();
        Ok(PreviewContent::Image(ImageContent {
            rgba,
            width,
            height,
        }))
    }
}

fn build_config(config: &OmnicatConfig) -> ViuerConfig {
    let mut conf = ViuerConfig {
        width: Some(image_width(config)),
        // Print at the current cursor, not the top-left corner of the terminal.
        absolute_offset: false,
        restore_cursor: true,
        ..ViuerConfig::default()
    };

    apply_protocol(&mut conf, &config.terminal.image.protocol);

    conf
}

fn apply_protocol(conf: &mut ViuerConfig, protocol: &str) {
    match protocol {
        "blocks" => {
            conf.use_kitty = false;
            conf.use_iterm = false;
        }
        "kitty" => {
            conf.use_kitty = true;
            conf.use_iterm = false;
        }
        "iterm2" => {
            conf.use_iterm = true;
            conf.use_kitty = false;
        }
        _ => {
            // auto: prefer reliable paths; avoid kitty capability probes in generic terminals.
            if is_iterm_supported() {
                conf.use_iterm = true;
                conf.use_kitty = false;
            } else if is_likely_kitty_terminal() && get_kitty_support() == KittySupport::Local {
                conf.use_kitty = true;
                conf.use_iterm = false;
            } else {
                conf.use_kitty = false;
                conf.use_iterm = false;
            }
        }
    }
}

fn is_likely_kitty_terminal() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM")
            .map(|t| t.eq_ignore_ascii_case("xterm-kitty") || t.contains("kitty"))
            .unwrap_or(false)
}

fn image_width(config: &OmnicatConfig) -> u32 {
    if config.terminal.image.max_width > 0 {
        return config.terminal.image.max_width as u32;
    }
    size().map(|(w, _)| w as u32).unwrap_or(80).max(20)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_prints_inline_not_at_corner() {
        let conf = build_config(&OmnicatConfig::default());
        assert!(!conf.absolute_offset);
        assert!(conf.restore_cursor);
    }

    #[test]
    fn auto_protocol_disables_remote_kitty() {
        let mut conf = ViuerConfig::default();
        apply_protocol(&mut conf, "auto");
        if !is_iterm_supported() && !is_likely_kitty_terminal() {
            assert!(!conf.use_kitty);
            assert!(!conf.use_iterm);
        }
    }

    #[test]
    fn blocks_protocol_uses_half_blocks_only() {
        let mut conf = ViuerConfig::default();
        apply_protocol(&mut conf, "blocks");
        assert!(!conf.use_kitty);
        assert!(!conf.use_iterm);
    }
}
