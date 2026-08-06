//! DataFusion TableProvider over a MySQL dump table.

use anyhow::{Context, Result};
use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::catalog::TableProvider;
use datafusion::common::Statistics;
use datafusion::logical_expr::{Expr, TableProviderFilterPushDown, TableType};
use datafusion::physical_plan::ExecutionPlan;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::db::mysql::dump::reader::DumpReader;
use crate::db::mysql::dump::scan::{row_filters_from_exprs, DumpScanConfig};
use crate::db::mysql::dump::schema::{parse_create_table, parse_insert_table, RowFilter, TableDef};

pub const BATCH_SIZE: usize = 8192;

#[derive(Debug)]
pub struct MysqlDumpTableProvider {
    path: PathBuf,
    table: String,
    schema: SchemaRef,
    /// Parsed from SQL when DataFusion does not push LIMIT into `scan`.
    sql_fetch: Option<usize>,
}

impl MysqlDumpTableProvider {
    pub fn from_def(path: PathBuf, def: &TableDef, sql_fetch: Option<usize>) -> Self {
        let fields: Vec<Field> = def
            .columns
            .iter()
            .map(|(name, sql_type)| Field::new(name.clone(), sql_type_to_arrow(sql_type), true))
            .collect();
        Self {
            path,
            table: def.name.clone(),
            schema: Arc::new(Schema::new(fields)),
            sql_fetch,
        }
    }
}

#[async_trait]
impl TableProvider for MysqlDumpTableProvider {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> datafusion::common::Result<Arc<dyn ExecutionPlan>> {
        let row_filters = row_filters_from_exprs(&self.schema, filters);
        let fetch = match (limit, self.sql_fetch) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let config = DumpScanConfig {
            path: self.path.clone(),
            table: self.table.clone(),
            schema: self.schema.clone(),
            projection: projection.cloned(),
            filters: row_filters,
            fetch,
        };
        DumpScanConfig::try_new_exec(config)
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> datafusion::common::Result<Vec<TableProviderFilterPushDown>> {
        Ok(filters
            .iter()
            .map(|f| {
                let mut tmp = Vec::new();
                RowFilter::collect_from_expr(f, &self.schema, &mut tmp);
                if tmp.is_empty() {
                    TableProviderFilterPushDown::Unsupported
                } else {
                    TableProviderFilterPushDown::Inexact
                }
            })
            .collect())
    }

    fn statistics(&self) -> Option<Statistics> {
        None
    }
}

pub fn load_table_defs(path: &std::path::Path) -> Result<BTreeMap<String, TableDef>> {
    let mut reader = DumpReader::open(path)?;
    let mut defs = BTreeMap::new();
    while let Some(stmt) = reader.next_statement()? {
        if let Some(def) = parse_create_table(&stmt) {
            defs.insert(def.name.clone(), def);
            continue;
        }
        if parse_insert_table(&stmt).is_some() {
            break;
        }
    }
    Ok(defs)
}

pub async fn register_dump_tables_for_query(
    ctx: &datafusion::prelude::SessionContext,
    path: &std::path::Path,
    sql: &str,
    table_filter: Option<&str>,
) -> Result<BTreeMap<String, TableDef>> {
    use crate::db::mysql::dump::scan::{extract_limit_from_sql, extract_table_names_sql};

    let defs = load_table_defs(path)?;
    let sql_fetch = extract_limit_from_sql(sql)?;
    let mut names = extract_table_names_sql(sql)?;
    if names.is_empty() {
        anyhow::bail!("SQL query must reference at least one table in FROM clause");
    }
    if let Some(t) = table_filter {
        names.retain(|n| n.eq_ignore_ascii_case(t));
        if names.is_empty() {
            anyhow::bail!("--table {t} is not referenced in the query");
        }
    }
    for name in &names {
        let def = defs.get(name).ok_or_else(|| {
            anyhow::anyhow!("table '{name}' not found in dump (check CREATE TABLE statements)")
        })?;
        let provider = MysqlDumpTableProvider::from_def(path.to_path_buf(), def, sql_fetch);
        ctx.register_table(name, Arc::new(provider))?;
    }
    Ok(defs)
}

pub fn sql_type_to_arrow(sql: &str) -> DataType {
    let u = sql.to_ascii_uppercase();
    if u.contains("INT") && !u.contains("POINT") {
        DataType::Int64
    } else if u.contains("DOUBLE") || u.contains("FLOAT") || u.contains("DECIMAL") {
        DataType::Float64
    } else {
        DataType::Utf8
    }
}

pub fn build_batch(schema: &SchemaRef, rows: &[Vec<String>]) -> Result<RecordBatch> {
    let mut arrays: Vec<ArrayRef> = Vec::new();
    for ci in 0..schema.fields().len() {
        match schema.field(ci).data_type() {
            DataType::Int64 => {
                let vals: Vec<Option<i64>> = rows
                    .iter()
                    .map(|r| {
                        r.get(ci).and_then(|s| {
                            if s.eq_ignore_ascii_case("NULL") {
                                None
                            } else {
                                s.parse().ok()
                            }
                        })
                    })
                    .collect();
                arrays.push(Arc::new(Int64Array::from(vals)) as ArrayRef);
            }
            DataType::Float64 => {
                let vals: Vec<Option<f64>> = rows
                    .iter()
                    .map(|r| {
                        r.get(ci).and_then(|s| {
                            if s.eq_ignore_ascii_case("NULL") {
                                None
                            } else {
                                s.parse().ok()
                            }
                        })
                    })
                    .collect();
                arrays.push(Arc::new(Float64Array::from(vals)) as ArrayRef);
            }
            _ => {
                let vals: Vec<Option<String>> = rows
                    .iter()
                    .map(|r| {
                        r.get(ci).and_then(|s| {
                            if s.eq_ignore_ascii_case("NULL") {
                                None
                            } else {
                                Some(s.clone())
                            }
                        })
                    })
                    .collect();
                arrays.push(Arc::new(StringArray::from(vals)) as ArrayRef);
            }
        }
    }
    RecordBatch::try_new(schema.clone(), arrays).context("record batch")
}
