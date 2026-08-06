pub mod archive;
pub mod bson_stream;
pub mod detect;
pub mod filter;
pub mod inspect;
pub mod metadata;
pub mod overview;
pub mod query;

use std::path::Path;

use anyhow::{bail, Result};

use crate::db::options::DbOptions;
use crate::db::report::DbReport;

pub fn run(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if let Some(q) = &opts.query {
        return query::run_mongo_query(path, opts, q);
    }
    inspect::dispatch(path, opts)
}

pub fn run_datadir(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() || opts.find.is_some() || opts.sample.is_some() {
        bail!("MongoDB WiredTiger datadir supports overview only (no --query/--find)");
    }
    Ok(DbReport::OverviewMongoDatadir(crate::db::report::MongoDatadirOverview {
        path: path.display().to_string(),
        warnings: vec![
            "WiredTiger datadir: metadata only — use mongodump for --query".into(),
        ],
    }))
}
