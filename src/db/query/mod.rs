//! Execute SQL queries against dump-backed tables via DataFusion.

pub mod print;

use std::path::Path;

use anyhow::Result;

pub use print::{print_query_from_results, render_mongo_inserts, render_sql_inserts, QueryDialect};
use arrow::array::{Array, AsArray};
use arrow::datatypes::DataType;
use datafusion::prelude::SessionContext;

use crate::db::mysql::dump::provider::register_dump_tables_for_query;
use crate::db::mysql::dump::schema::fuzzy_column_hint;
use crate::db::options::DbOutputFormat;
use crate::db::report::QueryResultReport;

pub async fn run_query(
    path: &Path,
    sql: &str,
    table_filter: Option<&str>,
) -> Result<(QueryResultReport, Vec<arrow::record_batch::RecordBatch>)> {
    let ctx = SessionContext::new();
    let defs = register_dump_tables_for_query(&ctx, path, sql, table_filter).await?;
    let df = ctx.sql(sql).await.map_err(|e| enrich_sql_error(e, &defs))?;
    let batches = df
        .collect()
        .await
        .map_err(|e| enrich_sql_error(e, &defs))?;
    let report = batches_to_report(&batches)?;
    Ok((report, batches))
}

fn enrich_sql_error(
    e: datafusion::error::DataFusionError,
    defs: &std::collections::BTreeMap<String, crate::db::mysql::dump::schema::TableDef>,
) -> anyhow::Error {
    let msg = e.to_string();
    if let Some(unknown) = msg
        .split("Schema error: No field named ")
        .nth(1)
        .and_then(|s| s.split('.').next())
        .map(|s| s.trim_matches('"').trim_matches('`').to_string())
    {
        for def in defs.values() {
            let cols: Vec<String> = def.columns.iter().map(|(n, _)| n.clone()).collect();
            if let Some(hint) = fuzzy_column_hint(&unknown, &cols) {
                return anyhow::anyhow!("{msg}\nHint: did you mean '{hint}'?");
            }
        }
    }
    anyhow::anyhow!("{msg}")
}

pub fn batches_to_report(
    batches: &[arrow::record_batch::RecordBatch],
) -> Result<QueryResultReport> {
    if batches.is_empty() {
        return Ok(QueryResultReport {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_scanned: 0,
            rows_matched: 0,
        });
    }
    let schema = batches[0].schema();
    let columns: Vec<String> = schema.fields().iter().map(|f| f.name().clone()).collect();
    let mut rows = Vec::new();
    for batch in batches {
        for row_idx in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(columns.len());
            for col_idx in 0..batch.num_columns() {
                row.push(array_value_at(batch.column(col_idx), row_idx));
            }
            rows.push(row);
        }
    }
    let matched = rows.len() as u64;
    Ok(QueryResultReport {
        columns,
        rows,
        rows_scanned: matched,
        rows_matched: matched,
    })
}

fn array_value_at(col: &dyn Array, idx: usize) -> String {
    if col.is_null(idx) {
        return "NULL".into();
    }
    match col.data_type() {
        DataType::Utf8 => col.as_string::<i32>().value(idx).to_string(),
        DataType::LargeUtf8 => col.as_string::<i64>().value(idx).to_string(),
        DataType::Int64 => col
            .as_primitive::<arrow::datatypes::Int64Type>()
            .value(idx)
            .to_string(),
        DataType::Float64 => col
            .as_primitive::<arrow::datatypes::Float64Type>()
            .value(idx)
            .to_string(),
        other => format!("{other:?}"),
    }
}

pub fn write_query_output(
    report: &QueryResultReport,
    format: DbOutputFormat,
    out: &mut dyn std::io::Write,
) -> Result<()> {
    match format {
        DbOutputFormat::Json | DbOutputFormat::Jsonl => {
            for row in &report.rows {
                let obj: serde_json::Map<String, serde_json::Value> = report
                    .columns
                    .iter()
                    .zip(row.iter())
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                if format == DbOutputFormat::Jsonl {
                    serde_json::to_writer(&mut *out, &obj)?;
                    writeln!(out)?;
                } else {
                    serde_json::to_writer_pretty(&mut *out, &obj)?;
                    writeln!(out)?;
                }
            }
        }
        DbOutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(out);
            wtr.write_record(&report.columns)?;
            for row in &report.rows {
                wtr.write_record(row)?;
            }
            wtr.flush()?;
        }
        DbOutputFormat::Table => {
            crate::sinks::db_report::write_query_table(report, out)?;
        }
    }
    Ok(())
}

pub fn write_extract(
    path: &std::path::Path,
    report: &QueryResultReport,
    format: DbOutputFormat,
) -> Result<()> {
    let mut file = std::fs::File::create(path)?;
    write_query_output(report, format, &mut file)
}
