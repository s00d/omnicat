use std::io::Write;

use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Table};

use crate::inspect::report::InspectReport;

pub fn write_report(report: &InspectReport, json: bool, out: &mut dyn Write) -> Result<()> {
    if json {
        serde_json::to_writer_pretty(&mut *out, report)?;
        writeln!(out)?;
        return Ok(());
    }
    match report {
        InspectReport::Info(r) => {
            writeln!(out, "{}", r.path)?;
            writeln!(out, "{}", "─".repeat(r.path.len().clamp(12, 40)))?;
            writeln!(out, "{:<12} {}", "Type", r.type_label)?;
            if let Some(summary) = r.fields.get("summary").and_then(|v| v.as_str()) {
                writeln!(out, "{summary}")?;
            }
            if let Some(s) = &r.size_human {
                writeln!(out, "{:<12} {}", "Size", s)?;
            }
            if let Some(m) = &r.mime {
                writeln!(out, "{:<12} {}", "MIME", m)?;
            }
            if let Some(d) = &r.detected_mime {
                if r.mime.as_deref() != Some(d.as_str()) {
                    writeln!(out, "{:<12} {}", "Detected", d)?;
                }
            }
            if let Some(e) = &r.extension {
                writeln!(out, "{:<12} .{}", "Extension", e)?;
            }
            if let Some(fs) = &r.fs {
                writeln!(out, "{:<12} {}", "Kind", fs.kind)?;
                if let Some(v) = &fs.created {
                    writeln!(out, "{:<12} {}", "Created", v)?;
                }
                if let Some(v) = &fs.modified {
                    writeln!(out, "{:<12} {}", "Modified", v)?;
                }
                if let Some(v) = &fs.accessed {
                    writeln!(out, "{:<12} {}", "Accessed", v)?;
                }
                if let Some(v) = &fs.permissions {
                    writeln!(out, "{:<12} {}", "Permissions", v)?;
                }
                if let Some(v) = &fs.mode {
                    writeln!(out, "{:<12} {}", "Mode", v)?;
                }
                writeln!(
                    out,
                    "{:<12} {}",
                    "Readonly",
                    if fs.readonly { "yes" } else { "no" }
                )?;
                if let Some(v) = &fs.owner {
                    writeln!(out, "{:<12} {}", "Owner", v)?;
                }
                if let Some(v) = &fs.group {
                    writeln!(out, "{:<12} {}", "Group", v)?;
                }
                if let Some(v) = fs.inode {
                    writeln!(out, "{:<12} {}", "Inode", v)?;
                }
                if let Some(v) = fs.nlink {
                    writeln!(out, "{:<12} {}", "Links", v)?;
                }
                if let Some(v) = &fs.symlink {
                    writeln!(out, "{:<12} {}", "Symlink", v)?;
                }
                if let Some(v) = &fs.canonical {
                    writeln!(out, "{:<12} {}", "Canonical", v)?;
                }
            }
            for (k, v) in &r.fields {
                if k == "summary" {
                    continue;
                }
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                writeln!(out, "{:<12} {}", title_case(k), val)?;
            }
        }
        InspectReport::Type(r) => {
            writeln!(out, "{}", r.label)?;
            if let Some(m) = &r.mime {
                writeln!(out, "MIME: {m}")?;
            }
            if let Some(d) = &r.detected {
                writeln!(out, "Detected: {d}")?;
            }
            if let Some(e) = &r.extension {
                writeln!(out, "Extension: .{e}")?;
            }
        }
        InspectReport::Schema(r) => {
            writeln!(out, "{}", r.path)?;
            writeln!(out, "{}", "─".repeat(28))?;
            if let (Some(rows), Some(cols)) = (r.rows, r.columns) {
                writeln!(out, "Rows       {rows}")?;
                writeln!(out, "Columns    {cols}")?;
                writeln!(out)?;
            }
            if !r.tables.is_empty() {
                for table in &r.tables {
                    writeln!(out, "{}", table.name)?;
                    writeln!(out, "{}", "─".repeat(28))?;
                    for f in &table.fields {
                        let notes = f.notes.as_deref().unwrap_or("");
                        writeln!(out, "{:<16} {} {notes}", f.name, f.type_name)?;
                    }
                    writeln!(out)?;
                }
            } else {
                for f in &r.fields {
                    let notes = f.notes.as_deref().unwrap_or("");
                    writeln!(out, "{:<16} {} {notes}", f.name, f.type_name)?;
                }
            }
        }
        InspectReport::Find(r) => {
            writeln!(out, "Find {:?} in {}", r.query, r.path)?;
            writeln!(out, "{}", "─".repeat(28))?;
            for m in &r.matches {
                let loc = match (&m.location, m.line) {
                    (Some(loc), Some(line)) => format!("{loc}:{line}: "),
                    (Some(loc), None) => format!("{loc}: "),
                    (None, Some(line)) => format!("{line}: "),
                    _ => String::new(),
                };
                writeln!(out, "{loc}{}", m.text)?;
            }
            if r.truncated {
                writeln!(out, "\n… truncated")?;
            }
            writeln!(out, "\n{} matches", r.matches.len())?;
        }
        InspectReport::Stats(r) => {
            writeln!(out, "{}", r.path)?;
            writeln!(out, "{}", "─".repeat(28))?;
            for (k, v) in &r.fields {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                writeln!(out, "{:<14} {}", title_case(k), val)?;
            }
            if !r.groups.is_empty() {
                writeln!(out, "\nTypes")?;
                writeln!(out, "{}", "─".repeat(20))?;
                let mut items: Vec<_> = r.groups.iter().collect();
                items.sort_by(|a, b| b.1.cmp(a.1));
                for (k, v) in items {
                    writeln!(out, "{k:<14} {v}")?;
                }
            }
        }
        InspectReport::Query(r) => {
            if let Some(text) = &r.text {
                writeln!(out, "{text}")?;
            } else if let (Some(headers), Some(rows)) = (&r.headers, &r.rows) {
                write_table(out, headers, rows)?;
                writeln!(out, "\n{} rows", rows.len())?;
            } else if let Some(json) = &r.json {
                writeln!(out, "{}", serde_json::to_string_pretty(json)?)?;
            }
        }
        InspectReport::Diff(r) => {
            writeln!(out, "diff {} {}", r.left, r.right)?;
            writeln!(out, "kind: {}", r.kind)?;
            writeln!(out, "{}", r.summary)?;
            writeln!(out, "{}", "─".repeat(28))?;
            for c in &r.changes {
                writeln!(out, "{c}")?;
            }
        }
        InspectReport::Encoding(r) => {
            writeln!(out, "{}", r.path)?;
            writeln!(out, "{:<14} {}", "Encoding", r.encoding)?;
            writeln!(out, "{:<14} {}", "BOM", if r.bom { "yes" } else { "no" })?;
            writeln!(out, "{:<14} {}", "Line endings", r.line_endings)?;
            writeln!(out, "{:<14} {}", "Characters", r.characters)?;
            writeln!(out, "{:<14} {}", "Lines", r.lines)?;
            writeln!(out, "{:<14} {}", "Longest line", r.longest_line)?;
        }
        InspectReport::Capabilities(r) => {
            if let Some(path) = &r.path {
                writeln!(out, "{path}")?;
            }
            if let Some(h) = &r.handler {
                writeln!(out, "{h}")?;
            }
            writeln!(out, "{}", "─".repeat(20))?;
            for c in &r.capabilities {
                let mark = if c.supported { "✓" } else { "✗" };
                writeln!(out, "{mark} {}", c.name)?;
            }
        }
        InspectReport::Hash(r) => {
            writeln!(out, "{}", r.path)?;
            writeln!(out, "{}", "─".repeat(28))?;
            writeln!(out, "{:<10} {}", "Kind", r.kind)?;
            writeln!(out, "{:<10} {}", "Size", r.size_human)?;
            if r.entries > 0 {
                writeln!(out, "{:<10} {}", "Entries", r.entries)?;
            }
            if !r.md5.is_empty() {
                writeln!(out, "{:<10} {}", "MD5", r.md5)?;
            }
            if !r.sha1.is_empty() {
                writeln!(out, "{:<10} {}", "SHA1", r.sha1)?;
            }
            if !r.sha256.is_empty() {
                writeln!(out, "{:<10} {}", "SHA256", r.sha256)?;
            }
            if !r.sha512.is_empty() {
                writeln!(out, "{:<10} {}", "SHA512", r.sha512)?;
            }
            writeln!(out, "{:<10} {}", "BLAKE3", r.blake3)?;
            if let Some(n) = &r.note {
                writeln!(out, "\n{n}")?;
            }
        }
        InspectReport::Duplicates(r) => {
            writeln!(out, "Duplicate files")?;
            writeln!(out, "{}", "─".repeat(28))?;
            writeln!(out, "Scanned     {}", r.scanned_files)?;
            writeln!(out, "Groups      {}", r.groups.len())?;
            writeln!(out)?;
            for g in &r.groups {
                writeln!(out, "BLAKE3 {}", g.hash)?;
                writeln!(out)?;
                for f in &g.files {
                    writeln!(out, "  {f:<40} {}", g.size_human)?;
                }
                writeln!(out)?;
            }
            writeln!(out, "Potentially reclaimable: {}", r.reclaimable_human)?;
            if let Some(n) = &r.note {
                writeln!(out, "\n{n}")?;
            }
        }
        InspectReport::Text { text, .. } => {
            write!(out, "{text}")?;
            if !text.ends_with('\n') {
                writeln!(out)?;
            }
        }
        InspectReport::Table {
            headers,
            rows,
            total_rows,
            total_cols,
            note,
            ..
        } => {
            write_table(out, headers, rows)?;
            writeln!(out, "\n{total_rows} rows × {total_cols} columns")?;
            if let Some(n) = note {
                writeln!(out, "{n}")?;
            }
        }
    }
    Ok(())
}

fn write_table(out: &mut dyn Write, headers: &[String], rows: &[Vec<String>]) -> Result<()> {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    if !headers.is_empty() {
        table.set_header(headers.to_vec());
    }
    for row in rows {
        table.add_row(row.clone());
    }
    writeln!(out, "{table}")?;
    Ok(())
}

fn title_case(s: &str) -> String {
    let mut out = String::new();
    for (i, part) in s.split('_').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.extend(chars);
        }
    }
    out
}
