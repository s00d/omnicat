//! PostgreSQL dump (file-only) metadata via `pg_restore --list`.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::db::report::PostgresDumpOverview;

pub fn inspect_postgres_dump(path: &Path) -> Result<PostgresDumpOverview> {
    let format = detect_format(path);
    let mut warnings = Vec::new();
    if format == "directory" {
        warnings.push("directory format: listing via toc.dat when pg_restore unavailable".into());
        return inspect_pg_directory(path, warnings);
    }

    let pg_restore = match which::which("pg_restore") {
        Ok(p) => p,
        Err(_) => {
            warnings.push("pg_restore not found; install PostgreSQL client tools for full TOC".into());
            return Ok(PostgresDumpOverview {
                path: path.display().to_string(),
                format,
                databases: Vec::new(),
                tables: Vec::new(),
                warnings,
            });
        }
    };

    let output = Command::new(pg_restore)
        .arg("--list")
        .arg(path)
        .output()
        .context("pg_restore --list")?;
    if !output.status.success() {
        bail!(
            "pg_restore --list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_toc_list(path, &format, &text, warnings)
}

fn detect_format(path: &Path) -> String {
    if path.is_dir() {
        return "directory".into();
    }
    "custom".into()
}

fn parse_toc_list(
    path: &Path,
    format: &str,
    text: &str,
    mut warnings: Vec<String>,
) -> Result<PostgresDumpOverview> {
    let mut databases = Vec::new();
    let mut tables = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        // TOC lines: id; schema catalog owner desc
        let parts: Vec<&str> = line.splitn(2, ';').collect();
        if parts.len() < 2 {
            continue;
        }
        let desc = parts[1].trim();
        if desc.contains("DATABASE") {
            if let Some(name) = desc.split_whitespace().last() {
                databases.push(name.to_string());
            }
        }
        if desc.contains("TABLE DATA") || desc.contains("TABLE ") {
            let tokens: Vec<_> = desc.split_whitespace().collect();
            if tokens.len() >= 3 {
                let tbl = tokens[tokens.len() - 1].to_string();
                let db = tokens.get(1).unwrap_or(&"public").to_string();
                tables.push((db, tbl));
            }
        }
    }
    databases.sort();
    databases.dedup();
    tables.sort();
    tables.dedup();
    if tables.is_empty() && databases.is_empty() {
        warnings.push("no tables or databases parsed from TOC".into());
    }
    Ok(PostgresDumpOverview {
        path: path.display().to_string(),
        format: format.to_string(),
        databases,
        tables,
        warnings,
    })
}

fn inspect_pg_directory(path: &Path, mut warnings: Vec<String>) -> Result<PostgresDumpOverview> {
    let toc = path.join("toc.dat");
    if !toc.is_file() {
        warnings.push("toc.dat missing; cannot list directory-format dump".into());
        return Ok(PostgresDumpOverview {
            path: path.display().to_string(),
            format: "directory".into(),
            databases: Vec::new(),
            tables: Vec::new(),
            warnings,
        });
    }
    let pg_restore = match which::which("pg_restore") {
        Ok(p) => p,
        Err(_) => {
            warnings.push("pg_restore required to read directory-format dump TOC".into());
            return Ok(PostgresDumpOverview {
                path: path.display().to_string(),
                format: "directory".into(),
                databases: Vec::new(),
                tables: Vec::new(),
                warnings,
            });
        }
    };
    let output = Command::new(pg_restore)
        .arg("--list")
        .arg(path)
        .output()
        .context("pg_restore --list")?;
    if !output.status.success() {
        anyhow::bail!(
            "pg_restore --list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    parse_toc_list(path, "directory", &text, warnings)
}
