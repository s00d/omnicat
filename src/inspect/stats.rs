use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::encoding::analyze_text_file;
use crate::inspect::report::{human_size, StatsReport};
use crate::orchestrator::registry::DriverRegistry;

pub fn build_stats(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<StatsReport> {
    match kind {
        HandlerKind::Directory => stats_directory(path, display),
        HandlerKind::Archive => stats_archive(path, display, config),
        HandlerKind::Markdown => stats_markdown(path, display, config),
        HandlerKind::Code => stats_code(path, display, config),
        HandlerKind::Data | HandlerKind::Fallback | HandlerKind::Plist => {
            stats_text(path, display, kind, config)
        }
        HandlerKind::Pdf
        | HandlerKind::Document
        | HandlerKind::Presentation
        | HandlerKind::Ebook
        | HandlerKind::Email => stats_converted(path, display, kind, config),
        HandlerKind::Database => stats_database(path, display),
        HandlerKind::Image => stats_image(path, display),
        HandlerKind::Media => stats_media(path, display, config),
        _ => stats_text(path, display, kind, config),
    }
}

fn stats_directory(path: &Path, display: &str) -> Result<StatsReport> {
    let mut files = 0u64;
    let mut dirs = 0u64;
    let mut size = 0u64;
    let mut types: BTreeMap<String, u64> = BTreeMap::new();
    walk(path, &mut files, &mut dirs, &mut size, &mut types, 0)?;
    let mut fields = BTreeMap::new();
    fields.insert("files".into(), serde_json::json!(files));
    fields.insert("directories".into(), serde_json::json!(dirs));
    fields.insert("size".into(), serde_json::json!(human_size(size)));
    fields.insert("size_bytes".into(), serde_json::json!(size));
    Ok(StatsReport {
        path: display.to_string(),
        handler: "directory".into(),
        fields,
        groups: types,
    })
}

fn walk(
    path: &Path,
    files: &mut u64,
    dirs: &mut u64,
    size: &mut u64,
    types: &mut BTreeMap<String, u64>,
    depth: usize,
) -> Result<()> {
    if depth > 40 {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            *dirs += 1;
            walk(&entry.path(), files, dirs, size, types, depth + 1)?;
        } else if ft.is_file() {
            *files += 1;
            *size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            let label = type_bucket(&entry.path());
            *types.entry(label).or_default() += 1;
        }
    }
    Ok(())
}

fn type_bucket(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "rs" => "Rust".into(),
        "ts" | "tsx" => "TypeScript".into(),
        "js" | "jsx" | "mjs" | "cjs" => "JavaScript".into(),
        "py" => "Python".into(),
        "md" | "markdown" => "Markdown".into(),
        "json" | "jsonl" | "ndjson" => "JSON".into(),
        "yaml" | "yml" => "YAML".into(),
        "toml" => "TOML".into(),
        "csv" | "tsv" => "CSV".into(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "Images".into(),
        "zip" | "tar" | "gz" | "tgz" | "7z" | "bz2" | "xz" | "zst" | "zstd" => "Archives".into(),
        "pdf" => "PDF".into(),
        "sqlite" | "db" | "sqlite3" => "SQLite".into(),
        "mp3" | "wav" | "flac" | "ogg" | "mp4" | "mkv" | "mov" => "Media".into(),
        "" => "Other".into(),
        other => other.to_ascii_uppercase(),
    }
}

fn stats_archive(path: &Path, display: &str, config: &OmnicatConfig) -> Result<StatsReport> {
    let content = DriverRegistry::build(HandlerKind::Archive, path, config)?;
    let text = content.plain_text();
    let mut types: BTreeMap<String, u64> = BTreeMap::new();
    let mut entries = 0u64;
    for line in text.lines() {
        let name = line.split_whitespace().last().unwrap_or("");
        if name.is_empty() || name.ends_with('/') {
            continue;
        }
        entries += 1;
        let p = Path::new(name);
        *types.entry(type_bucket(p)).or_default() += 1;
    }
    let mut fields = BTreeMap::new();
    fields.insert("entries".into(), serde_json::json!(entries));
    fields.insert(
        "size".into(),
        serde_json::json!(human_size(fs::metadata(path).map(|m| m.len()).unwrap_or(0))),
    );
    Ok(StatsReport {
        path: display.to_string(),
        handler: "archive".into(),
        fields,
        groups: types,
    })
}

fn stats_text(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<StatsReport> {
    let meta = analyze_text_file(path, config.inspect.max_bytes)?;
    let words = meta.text.split_whitespace().count();
    let mut fields = BTreeMap::new();
    fields.insert("lines".into(), serde_json::json!(meta.lines));
    fields.insert("words".into(), serde_json::json!(words));
    fields.insert("characters".into(), serde_json::json!(meta.characters));
    fields.insert("encoding".into(), serde_json::json!(meta.encoding));
    if config.inspect.max_bytes > 0
        && fs::metadata(path).map(|m| m.len()).unwrap_or(0) as usize > config.inspect.max_bytes
    {
        fields.insert(
            "note".into(),
            serde_json::json!(format!(
                "Showing stats for first {} bytes. Use --all to disable limit.",
                config.inspect.max_bytes
            )),
        );
    }
    Ok(StatsReport {
        path: display.to_string(),
        handler: kind.name().into(),
        fields,
        groups: BTreeMap::new(),
    })
}

fn stats_markdown(path: &Path, display: &str, config: &OmnicatConfig) -> Result<StatsReport> {
    let mut report = stats_text(path, display, HandlerKind::Markdown, config)?;
    let text = analyze_text_file(path, config.inspect.max_bytes)?.text;
    let headings = text
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .count();
    let links = text.matches("](").count();
    let code_blocks = text.matches("```").count() / 2;
    report
        .fields
        .insert("headings".into(), serde_json::json!(headings));
    report
        .fields
        .insert("links".into(), serde_json::json!(links));
    report
        .fields
        .insert("code_blocks".into(), serde_json::json!(code_blocks));
    Ok(report)
}

fn stats_code(path: &Path, display: &str, config: &OmnicatConfig) -> Result<StatsReport> {
    let meta = analyze_text_file(path, config.inspect.max_bytes)?;
    let mut code = 0u64;
    let mut comments = 0u64;
    let mut blank = 0u64;
    let mut in_block = false;
    for line in meta.text.lines() {
        let t = line.trim();
        if t.is_empty() {
            blank += 1;
            continue;
        }
        if t.contains("/*") {
            in_block = true;
        }
        if in_block || t.starts_with("//") || t.starts_with('#') || t.starts_with("///") {
            comments += 1;
        } else {
            code += 1;
        }
        if t.contains("*/") {
            in_block = false;
        }
    }
    let mut fields = BTreeMap::new();
    fields.insert("lines".into(), serde_json::json!(meta.lines));
    fields.insert("code".into(), serde_json::json!(code));
    fields.insert("comments".into(), serde_json::json!(comments));
    fields.insert("blank".into(), serde_json::json!(blank));
    let lang = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("source")
        .to_ascii_lowercase();
    fields.insert("language".into(), serde_json::json!(lang));

    let symbols = count_code_symbols(&meta.text, &lang);
    fields.insert("functions".into(), serde_json::json!(symbols.functions));
    if symbols.structs > 0 {
        fields.insert("structs".into(), serde_json::json!(symbols.structs));
    }
    if symbols.classes > 0 {
        fields.insert("classes".into(), serde_json::json!(symbols.classes));
    }
    if symbols.enums > 0 {
        fields.insert("enums".into(), serde_json::json!(symbols.enums));
    }
    if symbols.interfaces > 0 {
        fields.insert("interfaces".into(), serde_json::json!(symbols.interfaces));
    }

    Ok(StatsReport {
        path: display.to_string(),
        handler: "code".into(),
        fields,
        groups: BTreeMap::new(),
    })
}

#[derive(Debug, Default)]
struct CodeSymbols {
    functions: u64,
    structs: u64,
    classes: u64,
    enums: u64,
    interfaces: u64,
}

fn count_code_symbols(text: &str, ext: &str) -> CodeSymbols {
    let mut s = CodeSymbols::default();
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() || t.starts_with("//") || t.starts_with('#') || t.starts_with("/*") {
            continue;
        }
        match ext {
            "rs" => {
                if is_rust_fn(t) {
                    s.functions += 1;
                }
                if starts_item(t, &["struct "]) {
                    s.structs += 1;
                }
                if starts_item(t, &["enum "]) {
                    s.enums += 1;
                }
            }
            "py" => {
                if t.starts_with("def ") || t.starts_with("async def ") {
                    s.functions += 1;
                }
                if t.starts_with("class ") {
                    s.classes += 1;
                }
            }
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => {
                if t.starts_with("function ")
                    || t.starts_with("async function ")
                    || t.starts_with("export function ")
                    || t.starts_with("export async function ")
                    || ((t.contains("const ") || t.contains("let ")) && t.contains(" => "))
                {
                    s.functions += 1;
                }
                if t.contains("class ")
                    && (t.starts_with("class ")
                        || t.starts_with("export class ")
                        || t.starts_with("export default class "))
                {
                    s.classes += 1;
                }
                if t.contains("interface ")
                    && (t.starts_with("interface ") || t.starts_with("export interface "))
                {
                    s.interfaces += 1;
                }
                if t.contains("enum ") && (t.starts_with("enum ") || t.starts_with("export enum "))
                {
                    s.enums += 1;
                }
            }
            "go" => {
                if t.starts_with("func ") {
                    s.functions += 1;
                }
                if t.starts_with("type ") && t.contains(" struct") {
                    s.structs += 1;
                }
            }
            "java" | "kt" | "kts" => {
                if t.contains(" class ")
                    || t.starts_with("class ")
                    || t.starts_with("public class ")
                {
                    s.classes += 1;
                }
                if t.contains(" interface ") || t.starts_with("interface ") {
                    s.interfaces += 1;
                }
                if t.contains(" enum ") || t.starts_with("enum ") {
                    s.enums += 1;
                }
            }
            "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "hh" => {
                if t.starts_with("struct ") {
                    s.structs += 1;
                } else if t.starts_with("enum ") {
                    s.enums += 1;
                } else if t.contains('(')
                    && t.contains(')')
                    && !t.ends_with(';')
                    && !t.starts_with("if ")
                    && !t.starts_with("for ")
                    && !t.starts_with("while ")
                    && !t.starts_with("switch ")
                    && !t.starts_with("typedef ")
                    && (t.contains('{') || !t.contains('='))
                {
                    let first = t.split_whitespace().next().unwrap_or("");
                    if matches!(
                        first,
                        "void"
                            | "int"
                            | "char"
                            | "float"
                            | "double"
                            | "bool"
                            | "static"
                            | "inline"
                            | "auto"
                            | "const"
                            | "unsigned"
                            | "signed"
                            | "long"
                            | "short"
                            | "size_t"
                            | "ssize_t"
                    ) || first.ends_with('*')
                    {
                        s.functions += 1;
                    }
                }
            }
            _ => {
                if t.starts_with("fn ")
                    || t.starts_with("def ")
                    || t.starts_with("function ")
                    || t.starts_with("func ")
                {
                    s.functions += 1;
                }
            }
        }
    }
    s
}

fn starts_item(line: &str, prefixes: &[&str]) -> bool {
    let stripped = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("export ")
        .trim_start_matches("default ")
        .trim_start_matches("async ");
    prefixes.iter().any(|p| stripped.starts_with(p))
}

fn is_rust_fn(line: &str) -> bool {
    let s = line
        .trim_start_matches("pub ")
        .trim_start_matches("pub(crate) ")
        .trim_start_matches("pub(super) ")
        .trim_start_matches("const ")
        .trim_start_matches("async ")
        .trim_start_matches("unsafe ");
    s.starts_with("fn ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_symbol_counts() {
        let src = r#"
pub struct Foo {}
enum Bar { A }
pub async fn hello() {}
fn world() {}
"#;
        let s = count_code_symbols(src, "rs");
        assert_eq!(s.functions, 2);
        assert_eq!(s.structs, 1);
        assert_eq!(s.enums, 1);
    }
}

fn stats_converted(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<StatsReport> {
    let text = DriverRegistry::build(kind, path, config)?.plain_text();
    let mut fields = BTreeMap::new();
    fields.insert("characters".into(), serde_json::json!(text.chars().count()));
    fields.insert(
        "words".into(),
        serde_json::json!(text.split_whitespace().count()),
    );
    fields.insert("lines".into(), serde_json::json!(text.lines().count()));
    Ok(StatsReport {
        path: display.to_string(),
        handler: kind.name().into(),
        fields,
        groups: BTreeMap::new(),
    })
}

fn stats_database(path: &Path, display: &str) -> Result<StatsReport> {
    use rusqlite::Connection;
    let conn = Connection::open(path)?;
    let tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let collected: Vec<String> = rows.filter_map(|r| r.ok()).collect();
        collected
    };
    let mut groups = BTreeMap::new();
    for t in &tables {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0))
            .unwrap_or(0);
        groups.insert(t.clone(), count as u64);
    }
    let mut fields = BTreeMap::new();
    fields.insert("tables".into(), serde_json::json!(tables.len()));
    fields.insert(
        "size".into(),
        serde_json::json!(human_size(fs::metadata(path).map(|m| m.len()).unwrap_or(0))),
    );
    Ok(StatsReport {
        path: display.to_string(),
        handler: "database".into(),
        fields,
        groups,
    })
}

fn stats_image(path: &Path, display: &str) -> Result<StatsReport> {
    use image::GenericImageView;
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut fields = BTreeMap::new();
    fields.insert("size".into(), serde_json::json!(human_size(size)));
    fields.insert("size_bytes".into(), serde_json::json!(size));
    match image::open(path) {
        Ok(img) => {
            let (w, h) = img.dimensions();
            fields.insert("width".into(), serde_json::json!(w));
            fields.insert("height".into(), serde_json::json!(h));
            fields.insert("dimensions".into(), serde_json::json!(format!("{w}×{h}")));
            fields.insert(
                "color".into(),
                serde_json::json!(format!("{:?}", img.color())),
            );
        }
        Err(err) => {
            fields.insert("warning".into(), serde_json::json!(format!("{err:#}")));
        }
    }
    Ok(StatsReport {
        path: display.to_string(),
        handler: "image".into(),
        fields,
        groups: BTreeMap::new(),
    })
}

fn stats_media(path: &Path, display: &str, config: &OmnicatConfig) -> Result<StatsReport> {
    use crate::content::preview_context;
    use crate::drivers::media::MediaDriver;
    use crate::drivers::PreviewDriver;

    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut fields = BTreeMap::new();
    fields.insert("size".into(), serde_json::json!(human_size(size)));
    fields.insert("size_bytes".into(), serde_json::json!(size));

    let content = MediaDriver.build(path, config, &preview_context(path))?;
    if let crate::content::PreviewContent::MediaInfo(info) = content {
        if let Some(c) = info.container {
            fields.insert("container".into(), serde_json::json!(c));
        }
        if let Some(d) = info.duration_secs {
            let h = (d as u64) / 3600;
            let m = ((d as u64) % 3600) / 60;
            let s = d % 60.0;
            fields.insert(
                "duration".into(),
                serde_json::json!(format!("{h:02}:{m:02}:{s:05.2}")),
            );
            fields.insert("duration_secs".into(), serde_json::json!(d));
        }
        if let Some(v) = info.video {
            fields.insert("video".into(), serde_json::json!(v));
        }
        if let Some(a) = info.audio {
            fields.insert("audio".into(), serde_json::json!(a));
        }
        if let Some(c) = info.codec {
            fields.insert("codec".into(), serde_json::json!(c));
        }
        if let Some(b) = info.bitrate {
            fields.insert("bitrate".into(), serde_json::json!(b));
        }
    }

    Ok(StatsReport {
        path: display.to_string(),
        handler: "media".into(),
        fields,
        groups: BTreeMap::new(),
    })
}

#[allow(dead_code)]
fn unsupported(kind: HandlerKind) -> Result<StatsReport> {
    bail!("stats not supported for {}", kind.name())
}
