use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Local};
use image::GenericImageView;

use crate::config::OmnicatConfig;
use crate::detect::HandlerKind;
use crate::inspect::encoding::analyze_text_file;
use crate::inspect::report::{human_size, FsInfo, InfoReport, TypeReport};
use crate::orchestrator::registry::DriverRegistry;

pub fn build_info(
    path: &Path,
    display: &str,
    kind: HandlerKind,
    config: &OmnicatConfig,
) -> Result<InfoReport> {
    let fs_info = collect_fs_info(path);
    let size = fs_info
        .as_ref()
        .and_then(|_| fs::metadata(path).ok())
        .map(|m| m.len())
        .or_else(|| fs::symlink_metadata(path).ok().map(|m| m.len()))
        .unwrap_or(0);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let detected_mime = sniff_mime(path);
    let mime = detected_mime
        .clone()
        .or_else(|| extension.as_ref().map(|e| guess_mime_from_ext(e)));

    let mut fields = BTreeMap::new();
    let type_label = match kind {
        HandlerKind::Image => {
            if let Err(err) = fill_image(&mut fields, path) {
                fields.insert("warning".into(), serde_json::json!(format!("{err:#}")));
            }
            format_label("Image", &extension, &detected_mime)
        }
        HandlerKind::Media => {
            fill_media(&mut fields, path, config)?;
            format_label("Media", &extension, &detected_mime)
        }
        HandlerKind::Database => {
            fill_database(&mut fields, path)?;
            "SQLite Database".into()
        }
        HandlerKind::Archive => {
            fill_archive(&mut fields, path, config)?;
            format_label("Archive", &extension, &detected_mime)
        }
        HandlerKind::Directory => {
            fill_directory(&mut fields, path)?;
            "Directory".into()
        }
        HandlerKind::Pdf
        | HandlerKind::Document
        | HandlerKind::Presentation
        | HandlerKind::Ebook
        | HandlerKind::Email => {
            fill_documentish(&mut fields, path, kind, config);
            kind_label(kind, &extension)
        }
        HandlerKind::Font => {
            fill_font(&mut fields, path, config)?;
            format_label("Font", &extension, &detected_mime)
        }
        HandlerKind::Spreadsheet => {
            fill_spreadsheet(&mut fields, path)?;
            "Spreadsheet".into()
        }
        HandlerKind::Data
        | HandlerKind::Code
        | HandlerKind::Markdown
        | HandlerKind::Fallback
        | HandlerKind::Plist => {
            fill_textish(&mut fields, path, config)?;
            kind_label(kind, &extension)
        }
        other => {
            fill_textish(&mut fields, path, config).ok();
            kind_label(other, &extension)
        }
    };

    if let (Some(ext), Some(det)) = (&extension, &detected_mime) {
        let expected = guess_mime_from_ext(ext);
        if !mime_compatible(&expected, det) {
            fields.insert(
                "warning".into(),
                serde_json::json!(format!(
                    "extension .{ext} suggests {expected}, magic bytes: {det}"
                )),
            );
        }
    }

    Ok(InfoReport {
        path: display.to_string(),
        handler: kind.name().to_string(),
        type_label,
        mime,
        detected_mime,
        extension,
        size,
        size_human: Some(human_size(size)),
        fs: fs_info,
        fields,
    })
}

pub fn build_type(path: &Path, display: &str, kind: HandlerKind) -> Result<TypeReport> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let detected = sniff_mime(path);
    let mime = detected
        .clone()
        .or_else(|| extension.as_ref().map(|e| guess_mime_from_ext(e)));
    let label = kind_label(kind, &extension);
    Ok(TypeReport {
        path: display.to_string(),
        label,
        mime,
        detected,
        extension,
        handler: kind.name().to_string(),
    })
}

fn collect_fs_info(path: &Path) -> Option<FsInfo> {
    let symlink_meta = fs::symlink_metadata(path).ok()?;
    let is_symlink = symlink_meta.file_type().is_symlink();
    let meta = if is_symlink {
        fs::metadata(path).unwrap_or(symlink_meta.clone())
    } else {
        symlink_meta.clone()
    };

    let kind = if is_symlink {
        "symlink".into()
    } else if meta.is_dir() {
        "directory".into()
    } else if meta.is_file() {
        "file".into()
    } else {
        "other".into()
    };

    let symlink = if is_symlink {
        fs::read_link(path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    } else {
        None
    };

    let canonical = fs::canonicalize(path)
        .ok()
        .map(|p| p.to_string_lossy().into_owned());

    let mut info = FsInfo {
        kind,
        created: format_system_time(meta.created().ok()),
        modified: format_system_time(meta.modified().ok()),
        accessed: format_system_time(meta.accessed().ok()),
        readonly: meta.permissions().readonly(),
        permissions: None,
        mode: None,
        owner: None,
        group: None,
        inode: None,
        nlink: None,
        device: None,
        blocks: None,
        block_size: None,
        symlink,
        canonical,
    };

    fill_platform_fs(&mut info, &meta);
    Some(info)
}

fn format_system_time(time: Option<SystemTime>) -> Option<String> {
    let time = time?;
    let dt: DateTime<Local> = time.into();
    Some(dt.format("%Y-%m-%d %H:%M:%S %z").to_string())
}

#[cfg(unix)]
fn fill_platform_fs(info: &mut FsInfo, meta: &fs::Metadata) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mode = meta.permissions().mode();
    info.mode = Some(format!("{:04o}", mode & 0o7777));
    info.permissions = Some(mode_string(mode));
    info.owner = Some(meta.uid().to_string());
    info.group = Some(meta.gid().to_string());
    info.inode = Some(meta.ino());
    info.nlink = Some(meta.nlink());
    info.device = Some(meta.dev());
    info.blocks = Some(meta.blocks());
    info.block_size = Some(meta.blksize());
}

#[cfg(unix)]
fn mode_string(mode: u32) -> String {
    let file_type = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o020000 => 'c',
        0o060000 => 'b',
        0o010000 => 'p',
        0o140000 => 's',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(file_type);
    for (bit, ch) in [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ] {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    if mode & 0o4000 != 0 {
        let c = if mode & 0o100 != 0 { 's' } else { 'S' };
        s.replace_range(3..4, &c.to_string());
    }
    if mode & 0o2000 != 0 {
        let c = if mode & 0o010 != 0 { 's' } else { 'S' };
        s.replace_range(6..7, &c.to_string());
    }
    if mode & 0o1000 != 0 {
        let c = if mode & 0o001 != 0 { 't' } else { 'T' };
        s.replace_range(9..10, &c.to_string());
    }
    s
}

#[cfg(windows)]
fn fill_platform_fs(info: &mut FsInfo, meta: &fs::Metadata) {
    info.permissions = Some(if meta.permissions().readonly() {
        "read-only".into()
    } else {
        "read-write".into()
    });
}

#[cfg(not(any(unix, windows)))]
fn fill_platform_fs(info: &mut FsInfo, meta: &fs::Metadata) {
    info.permissions = Some(if meta.permissions().readonly() {
        "read-only".into()
    } else {
        "read-write".into()
    });
}

fn fill_image(fields: &mut BTreeMap<String, serde_json::Value>, path: &Path) -> Result<()> {
    let img = image::open(path)?;
    let (w, h) = img.dimensions();
    let color = img.color();
    fields.insert("dimensions".into(), serde_json::json!(format!("{w} × {h}")));
    fields.insert("width".into(), serde_json::json!(w));
    fields.insert("height".into(), serde_json::json!(h));
    fields.insert("color".into(), serde_json::json!(format!("{color:?}")));
    let channels = color.channel_count().max(1) as u16;
    let bit_depth = color.bits_per_pixel() / channels;
    fields.insert("bit_depth".into(), serde_json::json!(bit_depth));
    Ok(())
}

fn fill_media(
    fields: &mut BTreeMap<String, serde_json::Value>,
    path: &Path,
    config: &OmnicatConfig,
) -> Result<()> {
    use crate::content::preview_context;
    use crate::drivers::media::MediaDriver;
    use crate::drivers::PreviewDriver;

    let content = MediaDriver.build(path, config, &preview_context(path))?;
    if let crate::content::PreviewContent::MediaInfo(info) = content {
        if let Some(summary) = media_one_liner(&info) {
            fields.insert("summary".into(), serde_json::json!(summary));
        }
        fields.insert("format".into(), serde_json::json!(info.format));
        if let Some(c) = info.container {
            fields.insert("container".into(), serde_json::json!(c));
        }
        if let Some(d) = info.duration_secs {
            fields.insert("duration".into(), serde_json::json!(format_duration(d)));
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
        if let (Some(w), Some(h)) = (info.width, info.height) {
            fields.insert("dimensions".into(), serde_json::json!(format!("{w}×{h}")));
            fields.insert("width".into(), serde_json::json!(w));
            fields.insert("height".into(), serde_json::json!(h));
        }
        if let Some(f) = info.fps {
            fields.insert("fps".into(), serde_json::json!(f));
        }
        if let Some(ch) = info.channels {
            fields.insert("channels".into(), serde_json::json!(ch));
        }
        if let Some(sr) = info.sample_rate {
            fields.insert("sample_rate".into(), serde_json::json!(sr));
        }
        if let Some(b) = info.bitrate {
            fields.insert("bitrate".into(), serde_json::json!(b));
            fields.insert(
                "bitrate_human".into(),
                serde_json::json!(if b >= 1_000_000 {
                    format!("{:.1} Mbps", b as f64 / 1_000_000.0)
                } else if b >= 1000 {
                    format!("{:.0} kbps", b as f64 / 1000.0)
                } else {
                    format!("{b} bps")
                }),
            );
        }
        for (k, v) in info.extra {
            if k != "path" && k != "size" {
                fields.insert(k, serde_json::json!(v));
            }
        }
    }
    Ok(())
}

fn media_one_liner(info: &crate::content::MediaInfoContent) -> Option<String> {
    if info.width.is_some() || info.video.is_some() || info.codec.is_some() {
        let codec = info
            .codec
            .as_deref()
            .or(info.video.as_deref())
            .unwrap_or("Video");
        let mut line = format!("Video: {codec}");
        if let (Some(w), Some(h)) = (info.width, info.height) {
            line.push_str(&format!(" {w}×{h}"));
        }
        if let Some(fps) = info.fps {
            line.push_str(&format!(" {fps:.0}fps"));
        }
        return Some(line);
    }
    if info.audio.is_some() || info.sample_rate.is_some() {
        let codec = info
            .codec
            .as_deref()
            .or(info.audio.as_deref())
            .unwrap_or("Audio");
        let mut line = format!("Audio: {codec}");
        if let Some(sr) = info.sample_rate {
            line.push_str(&format!(" {sr}Hz"));
        }
        if let Some(ch) = info.channels {
            line.push_str(&format!(" {ch}ch"));
        }
        return Some(line);
    }
    None
}

fn fill_database(fields: &mut BTreeMap<String, serde_json::Value>, path: &Path) -> Result<()> {
    use rusqlite::Connection;
    let conn = Connection::open(path)?;
    let tables: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let mut row_counts = BTreeMap::new();
    for t in &tables {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{t}\""), [], |r| r.get(0))
            .unwrap_or(0);
        row_counts.insert(t.clone(), count);
    }
    fields.insert("tables".into(), serde_json::json!(tables.len()));
    fields.insert("table_names".into(), serde_json::json!(tables));
    fields.insert("row_counts".into(), serde_json::json!(row_counts));
    Ok(())
}

fn fill_archive(
    fields: &mut BTreeMap<String, serde_json::Value>,
    path: &Path,
    config: &OmnicatConfig,
) -> Result<()> {
    let content = DriverRegistry::build(HandlerKind::Archive, path, config)?;
    if let crate::content::PreviewContent::Tree(tree) = content {
        let count = tree.root.count_nodes().saturating_sub(1);
        fields.insert("entries".into(), serde_json::json!(count));
    }
    Ok(())
}

fn fill_directory(fields: &mut BTreeMap<String, serde_json::Value>, path: &Path) -> Result<()> {
    let mut files = 0u64;
    let mut dirs = 0u64;
    let mut size = 0u64;
    walk_dir_counts(path, &mut files, &mut dirs, &mut size, 0, 32)?;
    fields.insert("files".into(), serde_json::json!(files));
    fields.insert("directories".into(), serde_json::json!(dirs));
    fields.insert("total_size".into(), serde_json::json!(human_size(size)));
    fields.insert("total_size_bytes".into(), serde_json::json!(size));
    Ok(())
}

fn walk_dir_counts(
    path: &Path,
    files: &mut u64,
    dirs: &mut u64,
    size: &mut u64,
    depth: usize,
    max_depth: usize,
) -> Result<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_dir() {
            *dirs += 1;
            walk_dir_counts(&entry.path(), files, dirs, size, depth + 1, max_depth)?;
        } else if ft.is_file() {
            *files += 1;
            *size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok(())
}

fn fill_font(
    fields: &mut BTreeMap<String, serde_json::Value>,
    path: &Path,
    config: &OmnicatConfig,
) -> Result<()> {
    use crate::content::preview_context;
    use crate::drivers::font::FontDriver;
    use crate::drivers::PreviewDriver;

    let content = FontDriver.build(path, config, &preview_context(path))?;
    if let crate::content::PreviewContent::FontInfo(info) = content {
        fields.insert("family".into(), serde_json::json!(info.family));
        fields.insert("style".into(), serde_json::json!(info.style));
        fields.insert("weight".into(), serde_json::json!(info.weight));
        fields.insert("glyphs".into(), serde_json::json!(info.glyph_count));
    }
    Ok(())
}

fn fill_spreadsheet(fields: &mut BTreeMap<String, serde_json::Value>, path: &Path) -> Result<()> {
    use calamine::{open_workbook_auto, Reader};
    let wb = open_workbook_auto(path)?;
    let sheets = wb.sheet_names().to_vec();
    fields.insert("sheets".into(), serde_json::json!(sheets.len()));
    fields.insert("sheet_names".into(), serde_json::json!(sheets));
    Ok(())
}

fn fill_documentish(
    fields: &mut BTreeMap<String, serde_json::Value>,
    path: &Path,
    kind: HandlerKind,
    config: &OmnicatConfig,
) {
    match DriverRegistry::build(kind, path, config) {
        Ok(content) => {
            let text = content.plain_text();
            fields.insert("characters".into(), serde_json::json!(text.chars().count()));
            fields.insert(
                "words".into(),
                serde_json::json!(text.split_whitespace().count()),
            );
            fields.insert("lines".into(), serde_json::json!(text.lines().count()));
        }
        Err(err) => {
            fields.insert("warning".into(), serde_json::json!(format!("{err:#}")));
        }
    }
}

fn fill_textish(
    fields: &mut BTreeMap<String, serde_json::Value>,
    path: &Path,
    config: &OmnicatConfig,
) -> Result<()> {
    let max = config.inspect.max_bytes;
    let meta = analyze_text_file(path, max)?;
    fields.insert("encoding".into(), serde_json::json!(meta.encoding));
    fields.insert(
        "bom".into(),
        serde_json::json!(if meta.bom { "yes" } else { "no" }),
    );
    fields.insert("line_endings".into(), serde_json::json!(meta.line_endings));
    fields.insert("characters".into(), serde_json::json!(meta.characters));
    fields.insert("lines".into(), serde_json::json!(meta.lines));
    fields.insert("longest_line".into(), serde_json::json!(meta.longest_line));
    Ok(())
}

pub fn sniff_mime(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let head = if bytes.len() > 8192 {
        &bytes[..8192]
    } else {
        &bytes
    };
    infer::get(head).map(|t| t.mime_type().to_string())
}

pub fn guess_mime_from_ext(ext: &str) -> String {
    mime_guess::from_ext(ext)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

fn mime_compatible(expected: &str, detected: &str) -> bool {
    expected == detected
        || (expected.starts_with("image/") && detected.starts_with("image/"))
        || (expected.contains("zip") && detected.contains("zip"))
}

fn format_label(base: &str, ext: &Option<String>, mime: &Option<String>) -> String {
    if let Some(e) = ext {
        format!("{base} ({})", e.to_ascii_uppercase())
    } else if let Some(m) = mime {
        format!("{base} ({m})")
    } else {
        base.into()
    }
}

fn kind_label(kind: HandlerKind, ext: &Option<String>) -> String {
    match kind {
        HandlerKind::Markdown => "Markdown".into(),
        HandlerKind::Code => format_label("Source", ext, &None),
        HandlerKind::Data => format_label("Data", ext, &None),
        HandlerKind::Spreadsheet => "Spreadsheet".into(),
        HandlerKind::Document => "Document".into(),
        HandlerKind::Pdf => "PDF".into(),
        HandlerKind::Presentation => "Presentation".into(),
        HandlerKind::Ebook => "Ebook".into(),
        HandlerKind::Email => "Email".into(),
        HandlerKind::Fallback => "Binary/Text".into(),
        other => other.name().to_string(),
    }
}

fn format_duration(secs: f64) -> String {
    let total = secs.floor() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_includes_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        fs::write(&path, "hello\n").unwrap();
        let report = build_info(
            &path,
            path.to_str().unwrap(),
            HandlerKind::Fallback,
            &OmnicatConfig::default(),
        )
        .unwrap();
        let fs = report.fs.expect("fs meta");
        assert!(fs.modified.is_some(), "modified missing");
        assert_eq!(fs.kind, "file");
        assert!(fs.canonical.is_some());
    }
}
