//! Streaming DataFusion source over a MySQL dump table.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use datafusion::common::{DataFusionError, Result as DfResult, Statistics};
use datafusion::execution::TaskContext;
use datafusion::logical_expr::Expr;
use arrow::datatypes::SchemaRef;
use arrow::record_batch::{RecordBatch, RecordBatchOptions};
use datafusion::datasource::source::{DataSource, DataSourceExec};
use datafusion::physical_plan::{
    DisplayFormatType, Partitioning, SendableRecordBatchStream,
};
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion_physical_expr::projection::ProjectionExprs;
use datafusion_physical_expr::EquivalenceProperties;

use crate::db::mysql::dump::provider::{build_batch, BATCH_SIZE};
use crate::db::mysql::dump::reader::{DumpFileReader, DumpReader};
use crate::db::mysql::dump::schema::{iter_insert_rows, parse_insert_table, RowFilter};

#[derive(Debug, Clone)]
pub struct DumpScanConfig {
    pub path: PathBuf,
    pub table: String,
    pub schema: SchemaRef,
    pub projection: Option<Vec<usize>>,
    pub filters: Vec<RowFilter>,
    pub fetch: Option<usize>,
}

impl DumpScanConfig {
    pub fn try_new_exec(config: DumpScanConfig) -> DfResult<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        let projected = match &config.projection {
            Some(p) if p.is_empty() => Arc::new(arrow::datatypes::Schema::empty()),
            Some(p) => Arc::new(config.schema.project(p).map_err(|e| {
                DataFusionError::External(format!("project schema: {e}").into())
            })?),
            None => config.schema.clone(),
        };
        let source = DumpScanSource {
            config,
            projected_schema: projected,
        };
        Ok(DataSourceExec::from_data_source(source))
    }
}

#[derive(Debug)]
struct DumpScanSource {
    config: DumpScanConfig,
    projected_schema: SchemaRef,
}

impl DataSource for DumpScanSource {
    fn open(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> DfResult<SendableRecordBatchStream> {
        let reader = DumpReader::open(&self.config.path).map_err(|e| {
            DataFusionError::External(format!("open dump: {e:#}").into())
        })?;
        let full_schema = self.config.schema.clone();
        let projected = self.projected_schema.clone();
        let table = self.config.table.clone();
        let filters = self.config.filters.clone();
        let fetch = self.config.fetch;
        let projection = self.config.projection.clone();

        let state = ScanState {
            reader,
            table,
            row_buf: Vec::new(),
            filters,
            fetch,
            rows_emitted: 0,
            finished: false,
        };

        let stream = futures::stream::try_unfold(state, move |mut st| {
            let fs = full_schema.clone();
            let proj = projection.clone();
            let out_schema = projected.clone();
            async move {
                if st.finished {
                    return Ok(None);
                }
                loop {
                    while st.row_buf.len() < BATCH_SIZE {
                        if st.fetch.is_some_and(|n| st.rows_emitted >= n as u64) {
                            st.finished = true;
                            break;
                        }
                        let Some(stmt) = st.reader.next_statement().map_err(|e| {
                            DataFusionError::External(format!("read dump: {e:#}").into())
                        })? else {
                            st.finished = true;
                            break;
                        };
                        let Some(tname) = parse_insert_table(&stmt) else {
                            continue;
                        };
                        if tname != st.table {
                            continue;
                        }
                        for row in iter_insert_rows(&stmt) {
                            if RowFilter::all_match(&st.filters, &fs, &row) {
                                st.row_buf.push(row);
                            }
                            if st.row_buf.len() >= BATCH_SIZE {
                                break;
                            }
                            if st.fetch.is_some_and(|n| {
                                st.rows_emitted + st.row_buf.len() as u64 >= n as u64
                            }) {
                                break;
                            }
                        }
                        if st.fetch.is_some_and(|n| {
                            st.rows_emitted + st.row_buf.len() as u64 >= n as u64
                        }) {
                            break;
                        }
                    }
                    if st.row_buf.is_empty() {
                        if st.finished {
                            return Ok(None);
                        }
                        continue;
                    }
                    let take = if let Some(limit) = st.fetch {
                        let remain = limit.saturating_sub(st.rows_emitted as usize);
                        if remain == 0 {
                            st.finished = true;
                            continue;
                        }
                        remain.min(st.row_buf.len())
                    } else {
                        st.row_buf.len().min(BATCH_SIZE)
                    };
                    if take == 0 {
                        if st.finished {
                            return Ok(None);
                        }
                        continue;
                    }
                    let chunk: Vec<_> = st.row_buf.drain(..take).collect();
                    st.rows_emitted += chunk.len() as u64;
                    let batch = build_batch(&fs, &chunk).map_err(|e| {
                        DataFusionError::External(format!("batch: {e:#}").into())
                    })?;
                    let batch = match &proj {
                        Some(p) if p.is_empty() => {
                            RecordBatch::try_new_with_options(
                                out_schema.clone(),
                                vec![],
                                &RecordBatchOptions::new().with_row_count(Some(chunk.len())),
                            )
                            .map_err(|e| {
                                DataFusionError::External(format!("empty batch: {e}").into())
                            })?
                        }
                        Some(p) => {
                            let cols: Vec<_> = p.iter().map(|i| batch.column(*i).clone()).collect();
                            RecordBatch::try_new(out_schema.clone(), cols).map_err(|e| {
                                DataFusionError::External(format!("project batch: {e}").into())
                            })?
                        }
                        None => batch,
                    };
                    return Ok(Some((batch, st)));
                }
            }
        });

        Ok(Box::pin(RecordBatchStreamAdapter::new(
            self.projected_schema.clone(),
            stream,
        )))
    }

    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
        match t {
            DisplayFormatType::Default | DisplayFormatType::Verbose => {
                write!(f, "MysqlDumpScan(table={})", self.config.table)
            }
            DisplayFormatType::TreeRender => {
                writeln!(f, "format=mysql_dump")?;
                writeln!(f, "table={}", self.config.table)?;
                Ok(())
            }
        }
    }

    fn output_partitioning(&self) -> Partitioning {
        Partitioning::UnknownPartitioning(1)
    }

    fn eq_properties(&self) -> EquivalenceProperties {
        EquivalenceProperties::new(self.projected_schema.clone())
    }

    fn partition_statistics(
        &self,
        _partition: Option<usize>,
    ) -> DfResult<Arc<Statistics>> {
        Ok(Arc::new(Statistics::new_unknown(&self.projected_schema)))
    }

    fn with_fetch(&self, limit: Option<usize>) -> Option<Arc<dyn DataSource>> {
        let mut config = self.config.clone();
        config.fetch = limit;
        Some(Arc::new(DumpScanSource {
            config,
            projected_schema: self.projected_schema.clone(),
        }))
    }

    fn fetch(&self) -> Option<usize> {
        self.config.fetch
    }

    fn try_swapping_with_projection(
        &self,
        _projection: &ProjectionExprs,
    ) -> DfResult<Option<Arc<dyn DataSource>>> {
        Ok(None)
    }
}

struct ScanState {
    reader: DumpFileReader,
    table: String,
    row_buf: Vec<Vec<String>>,
    filters: Vec<RowFilter>,
    fetch: Option<usize>,
    rows_emitted: u64,
    finished: bool,
}

pub fn row_filters_from_exprs(schema: &SchemaRef, filters: &[Expr]) -> Vec<RowFilter> {
    let mut out = Vec::new();
    for expr in filters {
        RowFilter::collect_from_expr(expr, schema, &mut out);
    }
    out
}

pub fn extract_table_names_sql(sql: &str) -> anyhow::Result<Vec<String>> {
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, sql).map_err(|e| anyhow::anyhow!("parse SQL: {e}"))?;
    let mut tables = Vec::new();
    for stmt in stmts {
        collect_tables_from_statement(&stmt, &mut tables);
    }
    tables.sort();
    tables.dedup();
    Ok(tables)
}

pub fn extract_limit_from_sql(sql: &str) -> anyhow::Result<Option<usize>> {
    use sqlparser::ast::{Expr, LimitClause, Statement, Value};
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, sql).map_err(|e| anyhow::anyhow!("parse SQL: {e}"))?;
    let Some(Statement::Query(q)) = stmts.first() else {
        return Ok(None);
    };
    let limit_expr = match &q.limit_clause {
        Some(LimitClause::LimitOffset { limit, .. }) => limit.as_ref(),
        Some(LimitClause::OffsetCommaLimit { limit, .. }) => Some(limit),
        None => None,
    };
    Ok(limit_expr.and_then(|expr| match expr {
        Expr::Value(v) => match &v.value {
            Value::Number(n, _) => n.parse().ok(),
            _ => None,
        },
        _ => None,
    }))
}

fn collect_tables_from_statement(stmt: &sqlparser::ast::Statement, out: &mut Vec<String>) {
    use sqlparser::ast::*;
    if let Statement::Query(q) = stmt {
        collect_tables_from_query(q, out);
    }
}

fn collect_tables_from_query(q: &sqlparser::ast::Query, out: &mut Vec<String>) {
    use sqlparser::ast::*;
    if let SetExpr::Select(s) = q.body.as_ref() {
        for item in &s.from {
            if let TableFactor::Table { name, .. } = &item.relation {
                out.push(normalize_table_name(name));
            }
        }
    }
}

fn normalize_table_name(name: &sqlparser::ast::ObjectName) -> String {
    name.to_string().trim_matches('`').to_string()
}
