mod code;
mod tree;

use std::path::PathBuf;

pub use code::{ColoredSpan, HighlightedCode, HighlightedLine, RgbColor};
pub use tree::{build_tree_from_entries, render_tree_unicode, FileTree, TreeNode};

#[derive(Debug, Clone)]
pub struct TableContent {
    pub title: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct HexContent {
    pub bytes: Vec<u8>,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct ImageContent {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct MediaInfoContent {
    pub title: String,
    pub format: String,
    pub duration_secs: Option<f64>,
    pub codec: Option<String>,
    pub bitrate: Option<u64>,
    pub extra: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct FontInfoContent {
    pub family: String,
    pub style: String,
    pub weight: u16,
    pub glyph_count: u16,
    pub sample: String,
}

#[derive(Debug, Clone)]
pub struct DatabaseContent {
    pub schema: String,
    pub table_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub enum PreviewContent {
    Text(String),
    Markdown(String),
    HighlightedCode(HighlightedCode),
    Table(TableContent),
    Tree(FileTree),
    Image(ImageContent),
    Hex(HexContent),
    Slides(Vec<String>),
    MediaInfo(MediaInfoContent),
    FontInfo(FontInfoContent),
    Database(DatabaseContent),
    Unsupported { reason: String, suggestion: String },
}

impl PreviewContent {
    pub fn plain_text(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Markdown(s) => s.clone(),
            Self::HighlightedCode(code) => code.plain_text(),
            Self::Table(t) => {
                let mut out = String::new();
                if let Some(title) = &t.title {
                    out.push_str(title);
                    out.push('\n');
                }
                if !t.headers.is_empty() {
                    out.push_str(&t.headers.join("\t"));
                    out.push('\n');
                }
                for row in &t.rows {
                    out.push_str(&row.join("\t"));
                    out.push('\n');
                }
                out
            }
            Self::Tree(tree) => {
                let mut out = String::new();
                if let Some(title) = &tree.title {
                    out.push_str(title);
                    out.push('\n');
                }
                render_tree_unicode(&tree.root, "", true, false, &mut out);
                out
            }
            Self::Slides(slides) => slides.join("\n\n---\n\n"),
            Self::Hex(h) => format!("{}\n{}", h.metadata, hex_preview(&h.bytes, 16)),
            Self::Image(_) => "[image preview]".to_string(),
            Self::MediaInfo(m) => {
                let mut out = format!("{}\nformat: {}", m.title, m.format);
                if let Some(d) = m.duration_secs {
                    out.push_str(&format!("\nduration: {d:.1}s"));
                }
                if let Some(c) = &m.codec {
                    out.push_str(&format!("\ncodec: {c}"));
                }
                for (k, v) in &m.extra {
                    out.push_str(&format!("\n{k}: {v}"));
                }
                out
            }
            Self::FontInfo(f) => {
                format!(
                    "family: {}\nstyle: {}\nweight: {}\nglyphs: {}\n\n{}",
                    f.family, f.style, f.weight, f.glyph_count, f.sample
                )
            }
            Self::Database(d) => {
                let mut out = d.schema.clone();
                out.push_str("\n\n");
                if !d.headers.is_empty() {
                    out.push_str(&d.headers.join("\t"));
                    out.push('\n');
                }
                for row in &d.rows {
                    out.push_str(&row.join("\t"));
                    out.push('\n');
                }
                out
            }
            Self::Unsupported { reason, suggestion } => {
                format!("{reason}\n{suggestion}")
            }
        }
    }
}

pub fn hex_preview(bytes: &[u8], cols: usize) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(cols).enumerate() {
        let offset = i * cols;
        out.push_str(&format!("{offset:08x}  "));
        for (j, b) in chunk.iter().enumerate() {
            if j > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{b:02x}"));
        }
        let pad = (cols.saturating_sub(chunk.len())) * 3;
        out.push_str(&" ".repeat(pad));
        out.push_str("  |");
        for b in chunk {
            let c = if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            };
            out.push(c);
        }
        out.push_str("|\n");
    }
    out
}

#[derive(Debug, Clone)]
pub struct PreviewContext {
    pub path: PathBuf,
    pub mime: Option<String>,
    pub size: u64,
}

pub fn preview_context(path: &std::path::Path) -> PreviewContext {
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mime = if meta.as_ref().is_some_and(|m| m.is_file()) {
        std::fs::read(path)
            .ok()
            .and_then(|b| infer::get(&b).map(|t| t.mime_type().to_string()))
    } else {
        None
    };
    PreviewContext {
        path: path.to_path_buf(),
        mime,
        size,
    }
}
