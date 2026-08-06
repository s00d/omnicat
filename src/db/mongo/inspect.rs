//! MongoDB inspect dispatch.

use std::path::Path;

use anyhow::Result;

use crate::db::mongo::metadata::schema_for_collection;
use crate::db::mongo::overview::{dump_find, dump_overview, dump_stats, dump_tables, sample_collection};
use crate::db::mongo::detect::list_collections;
use crate::db::options::DbOptions;
use crate::db::report::{DbReport, KeySample};

pub fn dispatch(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if let Some(pat) = &opts.find {
        return Ok(DbReport::Find {
            matches: dump_find(path, pat, 100)?,
        });
    }
    if let Some(n) = opts.sample {
        let samples: Vec<KeySample> = sample_collection(path, opts.table.as_deref(), n)?
            .into_iter()
            .enumerate()
            .map(|(i, line)| KeySample {
                key: format!("doc-{i}"),
                kind: "document".into(),
                size: line.len() as u64,
            })
            .collect();
        return Ok(DbReport::Samples { items: samples });
    }
    if opts.schema {
        let mut tables = Vec::new();
        for c in list_collections(path)? {
            if let Some(t) = &opts.table {
                if !c.name.eq_ignore_ascii_case(t) && !c.full_name().eq_ignore_ascii_case(t) {
                    continue;
                }
            }
            tables.push(schema_for_collection(&c)?);
        }
        return Ok(DbReport::Schema { tables });
    }
    if opts.tables {
        let mut tables = dump_tables(path)?;
        if let Some(t) = &opts.table {
            tables.retain(|ti| ti.name.eq_ignore_ascii_case(t) || ti.name.ends_with(&format!(".{t}")));
        }
        return Ok(DbReport::Tables { tables });
    }
    if opts.stats {
        return Ok(DbReport::StatsMongoDump(dump_stats(path)?));
    }
    Ok(DbReport::OverviewMongoDump(dump_overview(path)?))
}
