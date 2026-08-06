//! Elasticsearch snapshot repository (metadata only).

use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::db::options::DbOptions;
use crate::db::report::{DbReport, ElasticsearchSnapshotOverview, TableInfo};

pub fn run(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() || opts.find.is_some() || opts.sample.is_some() {
        bail!("Elasticsearch snapshots support overview and --tables only (no restore/query)");
    }
    let overview = inspect_snapshot(path)?;
    if opts.tables {
        return Ok(DbReport::Tables {
            tables: overview
                .indices
                .iter()
                .map(|name| TableInfo {
                    name: name.clone(),
                    rows: 0,
                    bytes: 0,
                })
                .collect(),
        });
    }
    Ok(DbReport::OverviewElasticsearch(overview))
}

fn inspect_snapshot(path: &Path) -> Result<ElasticsearchSnapshotOverview> {
    let mut indices = Vec::new();
    let mut warnings = vec![
        "Elasticsearch snapshot: metadata only — no index restore or query".into(),
    ];
    let indices_dir = path.join("indices");
    if indices_dir.is_dir() {
        for ent in fs::read_dir(&indices_dir)?.flatten() {
            if ent.path().is_dir() {
                if let Some(name) = ent.file_name().to_str() {
                    indices.push(name.to_string());
                }
            }
        }
    }
    indices.sort();
    if indices.is_empty() {
        warnings.push("no indices/ directory found".into());
    }
    Ok(ElasticsearchSnapshotOverview {
        path: path.display().to_string(),
        indices,
        warnings,
    })
}
