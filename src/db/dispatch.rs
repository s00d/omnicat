//! Route detected sources to backend handlers.

use std::io;
use std::path::Path;

use anyhow::{bail, Result};

use crate::db::detect::{detect_source, SourceKind};
use crate::db::export;
use crate::db::mongo;
use crate::db::mysql::datadir::inspect_datadir;
use crate::db::mysql::dump::overview::{
    dump_find, dump_overview, dump_schema, dump_stats, dump_tables,
};
use crate::db::options::DbOptions;
use crate::db::postgres::inspect_postgres_dump;
use crate::db::query::{print_query_from_results, run_query, write_extract, write_query_output, QueryDialect};
use crate::db::redis::aof::{aof_paths_for_source, scan_aof};
use crate::db::redis::rdb::{scan_rdb, RdbScanOptions};
use crate::db::report::DbReport;
use crate::db::sqlite;
use crate::sinks::db_report::write_db_report;
use crate::sinks::progress::ProgressBar;

pub fn run_db(source: &str, opts: &DbOptions) -> Result<()> {
    let src = detect_source(Path::new(source))?;
    let report = dispatch(&src.path, src.kind, opts)?;
    let mut out = io::stdout().lock();
    write_db_report(&report, opts.json, &mut out)?;
    Ok(())
}

pub fn dispatch(path: &Path, kind: SourceKind, opts: &DbOptions) -> Result<DbReport> {
    match kind {
        SourceKind::MysqlDump => dispatch_mysql_dump(path, opts),
        SourceKind::MysqlDatadir => dispatch_mysql_datadir(path, opts),
        SourceKind::RedisRdb => dispatch_redis_rdb(path, opts),
        SourceKind::RedisAof => dispatch_redis_aof(path, opts),
        SourceKind::PostgresDump => dispatch_postgres_dump(path, opts),
        SourceKind::MongoDump => mongo::run(path, opts),
        SourceKind::MongoDatadir => mongo::run_datadir(path, opts),
        SourceKind::MongoExportJson => export::mongo_json::run(path, opts),
        SourceKind::Sqlite => sqlite::run(path, opts),
        SourceKind::DynamoDbExport => export::dynamodb::run(path, opts),
        SourceKind::ElasticsearchSnapshot => export::elasticsearch::run(path, opts),
    }
}

fn dispatch_mysql_dump(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if let Some(sql) = &opts.query {
        return run_sql_query(path, opts, sql, |p, s, t| {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(run_query(p, s, t))
        });
    }
    if let Some(pat) = &opts.find {
        return Ok(DbReport::Find {
            matches: dump_find(path, pat, 100)?,
        });
    }
    if opts.schema {
        let mut tables = dump_schema(path)?;
        if let Some(t) = &opts.table {
            tables.retain(|ts| ts.name.eq_ignore_ascii_case(t));
        }
        return Ok(DbReport::Schema { tables });
    }
    if opts.tables {
        let mut tables = dump_tables(path)?;
        if let Some(t) = &opts.table {
            tables.retain(|ti| ti.name.eq_ignore_ascii_case(t));
        }
        return Ok(DbReport::Tables { tables });
    }
    if opts.stats {
        let mut stats = dump_stats(path)?;
        if let Some(t) = &opts.table {
            stats.tables.retain(|ti| ti.name.eq_ignore_ascii_case(t));
        }
        return Ok(DbReport::StatsMysqlDump(stats));
    }
    Ok(DbReport::OverviewMysqlDump(dump_overview(path)?))
}

pub fn run_sql_query<F>(
    path: &Path,
    opts: &DbOptions,
    sql: &str,
    execute: F,
) -> Result<DbReport>
where
    F: FnOnce(&Path, &str, Option<&str>) -> Result<(crate::db::report::QueryResultReport, Vec<arrow::record_batch::RecordBatch>)>,
{
    let table_filter = opts.table.as_deref();
    let (report, _batches) = execute(path, sql, table_filter)?;

    if opts.print_query {
        print_query_from_results(QueryDialect::Sql, sql, &report)?;
        return Ok(DbReport::Text { lines: Vec::new() });
    }

    let mut out = io::stdout().lock();
    write_query_output(&report, opts.output, &mut out)?;

    if let Some(extract) = &opts.extract {
        write_extract(Path::new(extract), &report, opts.output)?;
        if opts.progress {
            let mut bar = ProgressBar::new("extract");
            bar.tick(report.rows_matched);
            bar.finish();
            eprintln!("wrote {} rows to {}", report.rows_matched, extract);
        }
    } else if opts.progress {
        eprintln!(
            "query: {} rows matched ({} scanned)",
            report.rows_matched, report.rows_scanned
        );
    }

    Ok(DbReport::Text { lines: Vec::new() })
}

fn dispatch_mysql_datadir(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() || opts.sample.is_some() || opts.find.is_some() {
        bail!("MySQL datadir sources support overview only (no --query/--sample/--find)");
    }
    Ok(DbReport::OverviewMysqlDatadir(inspect_datadir(path)?))
}

fn dispatch_postgres_dump(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() {
        bail!("PostgreSQL dump sources do not support --query in file-only mode");
    }
    if opts.find.is_some() || opts.sample.is_some() || opts.top.is_some() {
        bail!("PostgreSQL dump sources support overview, --schema, and --tables only");
    }
    let overview = inspect_postgres_dump(path)?;
    if opts.schema || opts.tables {
        let tables: Vec<_> = overview
            .tables
            .iter()
            .map(|(db, tbl)| crate::db::report::TableSchema {
                name: format!("{db}.{tbl}"),
                columns: Vec::new(),
                indexes: Vec::new(),
            })
            .collect();
        if opts.schema {
            return Ok(DbReport::Schema { tables });
        }
        let table_infos: Vec<_> = overview
            .tables
            .iter()
            .map(|(db, tbl)| crate::db::report::TableInfo {
                name: format!("{db}.{tbl}"),
                rows: 0,
                bytes: 0,
            })
            .collect();
        return Ok(DbReport::Tables { tables: table_infos });
    }
    Ok(DbReport::OverviewPostgresDump(overview))
}

fn dispatch_redis_rdb(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    let scan_opts = RdbScanOptions {
        stats: opts.stats || (!opts.schema && !opts.is_inspect_mode()),
        sample: opts.sample,
        find: opts.find.clone(),
        top: opts.top.clone(),
        top_limit: opts.top_limit,
        schema: opts.schema,
    };
    let result = scan_rdb(path, &scan_opts)?;
    if opts.schema {
        let lines: Vec<String> = result
            .stats
            .patterns
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        return Ok(DbReport::Text { lines });
    }
    if let Some(samples) = result.samples {
        return Ok(DbReport::Samples { items: samples });
    }
    if let Some(matches) = result.find_matches {
        return Ok(DbReport::Find { matches });
    }
    if let Some(top) = result.top {
        return Ok(DbReport::Top {
            field: top.field,
            items: top.items,
        });
    }
    Ok(DbReport::RedisRdb(result.stats))
}

fn dispatch_redis_aof(path: &Path, opts: &DbOptions) -> Result<DbReport> {
    if opts.query.is_some() {
        bail!("Redis AOF sources do not support --query; use --find");
    }
    let paths = aof_paths_for_source(path)?;
    let mut combined: Option<crate::db::redis::aof::AofScanResult> = None;
    for p in paths {
        let part = scan_aof(
            &p,
            opts.stats || !opts.is_inspect_mode(),
            opts.find.as_deref(),
            opts.top.as_deref(),
            opts.top_limit,
        )?;
        combined = Some(match combined {
            None => part,
            Some(acc) => acc.merge(part),
        });
    }
    let result = combined.expect("at least one AOF path");
    if let Some(matches) = result.find_matches {
        return Ok(DbReport::Find { matches });
    }
    if let Some(top) = result.top {
        return Ok(DbReport::Top {
            field: top.field,
            items: top.items,
        });
    }
    Ok(DbReport::RedisAof(result.stats))
}
