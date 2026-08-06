use std::io::Write;

use anyhow::Result;

use crate::config::OmnicatConfig;
use crate::content::{hex_preview, render_tree_unicode, PreviewContent};
use crate::inspect::text::sanitize_text;
use crate::sinks::styled_table;

pub fn write_content(
    content: &PreviewContent,
    config: &OmnicatConfig,
    out: &mut dyn Write,
) -> Result<()> {
    let safe = !config.inspect.allow_unsafe;
    match content {
        PreviewContent::Text(text) => {
            let t = if safe {
                sanitize_text(text, false)
            } else {
                text.clone()
            };
            write!(out, "{t}")?;
        }
        PreviewContent::Table(table) => {
            styled_table::write_styled_table(
                out,
                table.title.as_deref(),
                &table.headers,
                &table.rows,
                config,
                config.terminal.data.table_border,
            )?;
        }
        PreviewContent::Tree(tree) => {
            if let Some(title) = &tree.title {
                writeln!(out, "{title}")?;
            }
            let icons = config.terminal.directory.icons;
            let mut buf = String::new();
            render_tree_unicode(&tree.root, "", true, icons, &mut buf);
            write!(out, "{buf}")?;
        }
        PreviewContent::Slides(slides) => {
            for (i, slide) in slides.iter().enumerate() {
                if i > 0 {
                    writeln!(out, "\n--- slide {} ---\n", i + 1)?;
                }
                let s = if safe {
                    sanitize_text(slide, false)
                } else {
                    slide.clone()
                };
                write!(out, "{s}")?;
            }
            writeln!(out)?;
        }
        PreviewContent::Markdown(doc) => {
            let t = if safe {
                sanitize_text(doc, false)
            } else {
                doc.clone()
            };
            write!(out, "{t}")?;
        }
        PreviewContent::HighlightedCode(code) => {
            let plain = code.plain_text();
            let t = if safe {
                sanitize_text(&plain, false)
            } else {
                plain
            };
            write!(out, "{t}")?;
        }
        PreviewContent::Hex(hex) => {
            if !hex.metadata.is_empty() {
                writeln!(out, "{}", hex.metadata)?;
            }
            let cols = config.terminal.fallback.hex_cols;
            write!(out, "{}", hex_preview(&hex.bytes, cols))?;
        }
        PreviewContent::Image(_) => {
            writeln!(out, "[image — use terminal image renderer or --preview]")?;
        }
        PreviewContent::MediaInfo(m) => {
            writeln!(out, "{}", m.title)?;
            writeln!(out, "format: {}", m.format)?;
            if let Some(d) = m.duration_secs {
                writeln!(out, "duration: {d:.1}s")?;
            }
            if let Some(c) = &m.codec {
                writeln!(out, "codec: {c}")?;
            }
            for (k, v) in &m.extra {
                writeln!(out, "{k}: {v}")?;
            }
        }
        PreviewContent::FontInfo(f) => {
            writeln!(out, "family: {}", f.family)?;
            writeln!(out, "style: {}", f.style)?;
            writeln!(out, "weight: {}", f.weight)?;
            writeln!(out, "glyphs: {}", f.glyph_count)?;
            writeln!(out)?;
            write!(out, "{}", f.sample)?;
        }
        PreviewContent::Database(d) => {
            write!(out, "{}", d.schema)?;
            writeln!(out)?;
            if !d.headers.is_empty() {
                styled_table::write_styled_table(
                    out,
                    None,
                    &d.headers,
                    &d.rows,
                    config,
                    config.terminal.data.table_border,
                )?;
            }
        }
        PreviewContent::Unsupported { reason, suggestion } => {
            writeln!(out, "{reason}")?;
            writeln!(out, "{suggestion}")?;
        }
    }
    Ok(())
}
