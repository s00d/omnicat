//! SQL dump helpers: path detect, CREATE/INSERT parse, streaming query.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::OmnicatConfig;
use crate::io::source::open_reader;
use crate::search::{eval_pred, lookup_col, parse_sql_where_tokens, tokenize_sql, SearchPred};

#[derive(Debug, Clone)]
pub(crate) struct DumpColumn {
    pub name: String,
    pub type_name: String,
    pub notes: String,
}

pub(crate) fn is_sql_dump_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".sql")
        || name.ends_with(".sql.gz")
        || name.ends_with(".sql.zst")
        || name.ends_with(".sql.zstd")
        || name.ends_with(".sql.bz2")
        || name.ends_with(".sql.xz")
}

#[derive(Debug, Clone)]
pub enum SelectColumns {
    All,
    List(Vec<String>),
}

#[derive(Debug, Clone)]
pub enum SelectKind {
    Columns(SelectColumns),
    CountAll,
    CountCol(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct DumpSelect {
    pub kind: SelectKind,
    pub table: String,
    pub where_pred: Option<SearchPred>,
    pub order: Vec<(String, SortDir)>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Parse a dump SELECT:
/// `SELECT *|cols|COUNT(*)|COUNT(col) FROM t [WHERE …] [ORDER BY …] [LIMIT n] [OFFSET m]`
pub fn parse_dump_select(sql: &str) -> Result<DumpSelect> {
    let sql = sql.trim().trim_end_matches(';').trim();
    let tokens = tokenize_sql(sql)?;
    if tokens.is_empty() {
        bail!("empty query");
    }
    let mut i = 0usize;
    expect_kw(&tokens, &mut i, "SELECT")?;

    let kind = parse_select_kind(&tokens, &mut i)?;
    expect_kw(&tokens, &mut i, "FROM")?;
    let table = expect_ident(&tokens, &mut i)?;

    // Reject JOIN / multi-table immediately after FROM table.
    if i < tokens.len() {
        let t = tokens[i].as_str();
        if eq_kw(t, "JOIN")
            || eq_kw(t, "INNER")
            || eq_kw(t, "LEFT")
            || eq_kw(t, "RIGHT")
            || eq_kw(t, "FULL")
            || eq_kw(t, "CROSS")
            || t == ","
        {
            bail!("SQL dumps support only single-table SELECT (no JOIN); got near `{t}`");
        }
    }

    let mut where_pred = None;
    let mut order = Vec::new();
    let mut limit = None;
    let mut offset = None;

    while i < tokens.len() {
        if eq_kw(tokens[i].as_str(), "WHERE") {
            i += 1;
            where_pred = Some(parse_sql_where_tokens(&tokens, &mut i)?);
        } else if eq_kw(tokens[i].as_str(), "ORDER") {
            i += 1;
            expect_kw(&tokens, &mut i, "BY")?;
            loop {
                let col = expect_ident(&tokens, &mut i)?;
                let mut dir = SortDir::Asc;
                if i < tokens.len() {
                    if eq_kw(tokens[i].as_str(), "ASC") {
                        i += 1;
                        dir = SortDir::Asc;
                    } else if eq_kw(tokens[i].as_str(), "DESC") {
                        i += 1;
                        dir = SortDir::Desc;
                    }
                }
                order.push((col, dir));
                if i < tokens.len() && tokens[i] == "," {
                    i += 1;
                    continue;
                }
                break;
            }
        } else if eq_kw(tokens[i].as_str(), "LIMIT") {
            i += 1;
            limit = Some(expect_usize(&tokens, &mut i)?);
        } else if eq_kw(tokens[i].as_str(), "OFFSET") {
            i += 1;
            offset = Some(expect_usize(&tokens, &mut i)?);
        } else if eq_kw(tokens[i].as_str(), "GROUP")
            || eq_kw(tokens[i].as_str(), "HAVING")
            || eq_kw(tokens[i].as_str(), "UNION")
            || eq_kw(tokens[i].as_str(), "WITH")
        {
            bail!(
                "unsupported clause `{}` in SQL dump SELECT",
                tokens[i].to_ascii_uppercase()
            );
        } else {
            bail!("unexpected token `{}` in dump SELECT", tokens[i]);
        }
    }

    if matches!(kind, SelectKind::CountAll | SelectKind::CountCol(_)) && !order.is_empty() {
        bail!("ORDER BY is not supported with COUNT on SQL dumps");
    }
    if !order.is_empty() && limit.is_none() {
        bail!("ORDER BY on SQL dumps requires LIMIT");
    }

    Ok(DumpSelect {
        kind,
        table,
        where_pred,
        order,
        limit,
        offset,
    })
}

fn parse_select_kind(tokens: &[String], i: &mut usize) -> Result<SelectKind> {
    if *i >= tokens.len() {
        bail!("missing select list");
    }
    if tokens[*i] == "*" {
        *i += 1;
        return Ok(SelectKind::Columns(SelectColumns::All));
    }
    if eq_kw(tokens[*i].as_str(), "COUNT") {
        *i += 1;
        expect_tok(tokens, i, "(")?;
        if *i < tokens.len() && tokens[*i] == "*" {
            *i += 1;
            expect_tok(tokens, i, ")")?;
            return Ok(SelectKind::CountAll);
        }
        let col = expect_ident(tokens, i)?;
        expect_tok(tokens, i, ")")?;
        return Ok(SelectKind::CountCol(col));
    }

    let mut cols = Vec::new();
    loop {
        let col = expect_ident(tokens, i)?;
        if *i < tokens.len() && eq_kw(tokens[*i].as_str(), "AS") {
            bail!("column aliases (AS) are not supported on SQL dumps");
        }
        cols.push(col);
        if *i < tokens.len() && tokens[*i] == "," {
            *i += 1;
            continue;
        }
        break;
    }
    Ok(SelectKind::Columns(SelectColumns::List(cols)))
}

fn eq_kw(tok: &str, kw: &str) -> bool {
    tok.eq_ignore_ascii_case(kw)
}

fn expect_kw(tokens: &[String], i: &mut usize, kw: &str) -> Result<()> {
    if *i >= tokens.len() || !eq_kw(tokens[*i].as_str(), kw) {
        bail!("expected `{kw}`");
    }
    *i += 1;
    Ok(())
}

fn expect_tok(tokens: &[String], i: &mut usize, tok: &str) -> Result<()> {
    if *i >= tokens.len() || tokens[*i] != tok {
        bail!("expected `{tok}`");
    }
    *i += 1;
    Ok(())
}

fn expect_ident(tokens: &[String], i: &mut usize) -> Result<String> {
    if *i >= tokens.len() {
        bail!("expected identifier");
    }
    let t = &tokens[*i];
    if t.starts_with('\'')
        || t == "("
        || t == ")"
        || t == ","
        || t == "*"
        || matches!(t.as_str(), "=" | "!=" | "<>" | "<" | "<=" | ">" | ">=")
    {
        bail!("expected identifier, got `{t}`");
    }
    // Numbers alone aren't idents for columns/tables in our dialect.
    if t.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '+' || c == '-')
        && t.chars()
            .all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == '.')
    {
        bail!("expected identifier, got number `{t}`");
    }
    *i += 1;
    Ok(t.clone())
}

fn expect_usize(tokens: &[String], i: &mut usize) -> Result<usize> {
    if *i >= tokens.len() {
        bail!("expected number");
    }
    let n: usize = tokens[*i]
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid number `{}`", tokens[*i]))?;
    *i += 1;
    Ok(n)
}

#[derive(Debug)]
pub struct DumpQueryResult {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Stream a SQL dump and answer a dump SELECT with early-stop when possible.
pub fn query_sql_dump(
    path: &Path,
    select: &DumpSelect,
    config: &OmnicatConfig,
    max_rows: usize,
    progress: &mut crate::sinks::progress::ProgressReporter,
) -> Result<DumpQueryResult> {
    let offset = select.offset.unwrap_or(0);
    let limit_cap = select.limit.unwrap_or(usize::MAX).min(max_rows.max(1));

    let max_bytes = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes.max(256 * 1024) as u64)
    };

    let total = crate::io::file_len(path).ok();
    let reader = open_reader(path, true).context("open sql dump")?;
    let mut stream = DumpScanner::new(reader, max_bytes);

    let mut columns: Vec<DumpColumn> = Vec::new();
    let mut saw_table = false;
    let mut insert_active = false;

    let is_count = matches!(select.kind, SelectKind::CountAll | SelectKind::CountCol(_));
    let use_topk = !select.order.is_empty() && !is_count;

    let topk_size = if use_topk {
        offset.saturating_add(limit_cap)
    } else {
        0
    };

    let mut headers: Option<Vec<String>> = None;
    let mut header_idx: Option<Vec<usize>> = None;
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut skipped = 0usize;
    let mut count_val: u64 = 0;
    let mut topk: Vec<(Vec<SortKey>, Vec<String>)> = Vec::new();
    let mut row_buf = Vec::new();

    while let Some(ev) = stream.next_event()? {
        progress.tick_bytes(stream.bytes_read, total);
        match ev {
            DumpEvent::Create(stmt) => {
                if let Some((name, cols)) = parse_create_table(&stmt) {
                    if name.eq_ignore_ascii_case(&select.table) {
                        saw_table = true;
                        columns = cols;
                    }
                }
            }
            DumpEvent::InsertBegin {
                table,
                columns: col_names,
            } => {
                insert_active = table.eq_ignore_ascii_case(&select.table);
                if !insert_active {
                    stream.finish_insert()?;
                    continue;
                }
                saw_table = true;
                if columns.is_empty() {
                    if let Some(cols) = col_names {
                        columns = cols
                            .into_iter()
                            .map(|n| DumpColumn {
                                name: n,
                                type_name: String::new(),
                                notes: String::new(),
                            })
                            .collect();
                    }
                }
                if !is_count && headers.is_none() {
                    if let SelectKind::Columns(cols) = &select.kind {
                        headers = Some(resolve_headers(cols, &columns)?);
                        if let Some(h) = headers.as_ref() {
                            header_idx = header_indices(h, &columns);
                        }
                    }
                }
            }
            DumpEvent::InsertTuple(tuple) => {
                if !insert_active {
                    continue;
                }
                if columns.is_empty() && !tuple.is_empty() {
                    columns = (0..tuple.len())
                        .map(|i| DumpColumn {
                            name: format!("col_{}", i + 1),
                            type_name: String::new(),
                            notes: String::new(),
                        })
                        .collect();
                    if !is_count && headers.is_none() {
                        if let SelectKind::Columns(cols) = &select.kind {
                            headers = Some(resolve_headers(cols, &columns)?);
                            if let Some(h) = headers.as_ref() {
                                header_idx = header_indices(h, &columns);
                            }
                        }
                    }
                }

                let needs_map = select.where_pred.is_some()
                    || use_topk
                    || matches!(select.kind, SelectKind::CountCol(_))
                    || (matches!(select.kind, SelectKind::Columns(_)) && header_idx.is_none());
                if needs_map {
                    fill_row_map(&mut row_buf, &columns, &tuple);
                }
                if let Some(pred) = &select.where_pred {
                    if !eval_pred(pred, &row_buf) {
                        continue;
                    }
                }

                match &select.kind {
                    SelectKind::CountAll => {
                        count_val += 1;
                    }
                    SelectKind::CountCol(col) => {
                        let val = lookup_col(&row_buf, col).unwrap_or("");
                        if !val.is_empty() && !val.eq_ignore_ascii_case("NULL") {
                            count_val += 1;
                        }
                    }
                    SelectKind::Columns(_) => {
                        let Some(header_list) = headers.as_ref() else {
                            continue;
                        };
                        let projected = if let Some(idx) = header_idx.as_ref() {
                            project_row_by_index(idx, &tuple)
                        } else {
                            project_row(header_list, &row_buf)?
                        };
                        if use_topk {
                            if !needs_map {
                                fill_row_map(&mut row_buf, &columns, &tuple);
                            }
                            let keys = sort_keys(&select.order, &row_buf);
                            push_topk(&mut topk, topk_size, keys, projected, &select.order);
                        } else {
                            if skipped < offset {
                                skipped += 1;
                                continue;
                            }
                            rows.push(projected);
                            if rows.len() >= limit_cap {
                                stream.finish_insert()?;
                                return Ok(DumpQueryResult {
                                    headers: headers.unwrap_or_default(),
                                    rows,
                                });
                            }
                        }
                    }
                }
            }
            DumpEvent::InsertEnd => {
                insert_active = false;
            }
        }
    }

    if !saw_table {
        bail!("table `{}` not found in dump", select.table);
    }

    if is_count {
        return Ok(DumpQueryResult {
            headers: vec!["count".into()],
            rows: vec![vec![count_val.to_string()]],
        });
    }

    let headers = headers.unwrap_or_else(|| match &select.kind {
        SelectKind::Columns(SelectColumns::All) => columns.iter().map(|c| c.name.clone()).collect(),
        SelectKind::Columns(SelectColumns::List(cols)) => cols.clone(),
        _ => vec!["count".into()],
    });

    if use_topk {
        topk.sort_by(|a, b| cmp_sort_keys(&a.0, &b.0, &select.order));
        let rows: Vec<Vec<String>> = topk
            .into_iter()
            .skip(offset)
            .take(limit_cap)
            .map(|(_, r)| r)
            .collect();
        return Ok(DumpQueryResult { headers, rows });
    }

    Ok(DumpQueryResult { headers, rows })
}

/// Stream a SQL dump for `--find`: match SearchPred / substring against INSERT rows.
pub(crate) fn find_in_sql_dump(
    path: &Path,
    find: &crate::search::FindQuery,
    max: usize,
    config: &OmnicatConfig,
    progress: &mut crate::sinks::progress::ProgressReporter,
) -> Result<(Vec<(String, String)>, bool)> {
    let max_bytes = if config.inspect.no_limit || config.inspect.max_bytes == 0 {
        None
    } else {
        Some(config.inspect.max_bytes.max(256 * 1024) as u64)
    };
    let total = crate::io::file_len(path).ok();
    let reader = open_reader(path, true).context("open sql dump")?;
    let mut stream = DumpScanner::new(reader, max_bytes);

    let mut table_cols: std::collections::BTreeMap<String, Vec<DumpColumn>> =
        std::collections::BTreeMap::new();
    let mut table_row_n: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    let mut out = Vec::new();
    let mut truncated = false;
    let substr = find.substring.as_ref().map(|s| s.to_ascii_lowercase());

    let mut active_table: Option<String> = None;
    let mut active_key = String::new();
    let mut row_buf = Vec::new();

    while let Some(ev) = stream.next_event()? {
        progress.tick_bytes(stream.bytes_read, total);
        if truncated || out.len() >= max {
            truncated = out.len() >= max;
            if stream.in_insert_values() {
                stream.finish_insert()?;
            }
            break;
        }
        match ev {
            DumpEvent::Create(stmt) => {
                if let Some((name, cols)) = parse_create_table(&stmt) {
                    table_cols.insert(name.to_ascii_lowercase(), cols);
                }
            }
            DumpEvent::InsertBegin {
                table,
                columns: col_names,
            } => {
                active_key = table.to_ascii_lowercase();
                active_table = Some(table);
                table_cols.entry(active_key.clone()).or_insert_with(|| {
                    if let Some(cols) = col_names {
                        cols.into_iter()
                            .map(|n| DumpColumn {
                                name: n,
                                type_name: String::new(),
                                notes: String::new(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                });
            }
            DumpEvent::InsertTuple(tuple) => {
                let Some(name) = active_table.as_ref() else {
                    continue;
                };
                let columns = table_cols.get_mut(&active_key).unwrap();
                if columns.is_empty() && !tuple.is_empty() {
                    *columns = (0..tuple.len())
                        .map(|i| DumpColumn {
                            name: format!("col_{}", i + 1),
                            type_name: String::new(),
                            notes: String::new(),
                        })
                        .collect();
                }
                let row_n = table_row_n.entry(active_key.clone()).or_insert(0);
                *row_n += 1;
                let row_n = *row_n;
                fill_row_map(&mut row_buf, columns, &tuple);
                let matched = if let Some(pred) = &find.pred {
                    eval_pred(pred, &row_buf)
                } else if let Some(q) = &substr {
                    row_buf.iter().any(|(_, v)| {
                        v.as_bytes()
                            .windows(q.len())
                            .any(|w| w.eq_ignore_ascii_case(q.as_bytes()))
                    })
                } else {
                    false
                };
                if matched {
                    let text = row_buf
                        .iter()
                        .map(|(c, v)| format!("{c}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push((format!("{name} row {row_n}"), text));
                    if out.len() >= max {
                        truncated = true;
                        stream.finish_insert()?;
                        break;
                    }
                }
            }
            DumpEvent::InsertEnd => {
                active_table = None;
            }
        }
    }

    Ok((out, truncated))
}

#[derive(Debug, Clone, PartialEq)]
enum SortKey {
    Null,
    Num(f64),
    Str(String),
}

fn resolve_headers(sel: &SelectColumns, columns: &[DumpColumn]) -> Result<Vec<String>> {
    match sel {
        SelectColumns::All => {
            if columns.is_empty() {
                bail!("no columns known for table yet");
            }
            Ok(columns.iter().map(|c| c.name.clone()).collect())
        }
        SelectColumns::List(cols) => {
            for c in cols {
                if !columns.is_empty() && !columns.iter().any(|dc| dc.name.eq_ignore_ascii_case(c))
                {
                    bail!("unknown column `{c}`");
                }
            }
            Ok(cols.clone())
        }
    }
}

/// Build a field map; reuse column name Strings across rows (index-aligned).
fn fill_row_map(buf: &mut Vec<(String, String)>, columns: &[DumpColumn], tuple: &[String]) {
    if columns.is_empty() {
        if buf.len() == tuple.len() {
            for (i, (_, v)) in buf.iter_mut().enumerate() {
                *v = tuple[i].clone();
            }
            return;
        }
        buf.clear();
        for (i, v) in tuple.iter().enumerate() {
            buf.push((format!("col_{}", i + 1), v.clone()));
        }
        return;
    }
    if buf.len() == columns.len()
        && buf
            .iter()
            .zip(columns.iter())
            .all(|((n, _), c)| n.eq_ignore_ascii_case(&c.name))
    {
        for (i, (_, v)) in buf.iter_mut().enumerate() {
            *v = tuple.get(i).cloned().unwrap_or_default();
        }
        return;
    }
    buf.clear();
    for (i, c) in columns.iter().enumerate() {
        buf.push((c.name.clone(), tuple.get(i).cloned().unwrap_or_default()));
    }
}

fn project_row(headers: &[String], row_map: &[(String, String)]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(headers.len());
    for h in headers {
        let val = row_map
            .iter()
            .find(|(c, _)| c.eq_ignore_ascii_case(h))
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        out.push(val);
    }
    Ok(out)
}

/// Project by column index into `row_map` (same order as `fill_row_map`).
fn project_row_by_index(indices: &[usize], tuple: &[String]) -> Vec<String> {
    indices
        .iter()
        .map(|&i| tuple.get(i).cloned().unwrap_or_default())
        .collect()
}

fn header_indices(headers: &[String], columns: &[DumpColumn]) -> Option<Vec<usize>> {
    if columns.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(headers.len());
    for h in headers {
        let idx = columns
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(h))?;
        out.push(idx);
    }
    Some(out)
}

fn sort_keys(order: &[(String, SortDir)], row: &[(String, String)]) -> Vec<SortKey> {
    order
        .iter()
        .map(|(col, _)| {
            let val = lookup_col(row, col).unwrap_or("");
            if val.is_empty() || val.eq_ignore_ascii_case("NULL") {
                SortKey::Null
            } else if let Ok(n) = val.parse::<f64>() {
                SortKey::Num(n)
            } else {
                SortKey::Str(val.to_string())
            }
        })
        .collect()
}

fn cmp_sort_keys(a: &[SortKey], b: &[SortKey], order: &[(String, SortDir)]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    for (i, (_, dir)) in order.iter().enumerate() {
        let ord = match (a.get(i), b.get(i)) {
            (Some(SortKey::Null), Some(SortKey::Null)) => Ordering::Equal,
            (Some(SortKey::Null), _) => Ordering::Less,
            (_, Some(SortKey::Null)) => Ordering::Greater,
            (Some(SortKey::Num(x)), Some(SortKey::Num(y))) => {
                x.partial_cmp(y).unwrap_or(Ordering::Equal)
            }
            (Some(SortKey::Str(x)), Some(SortKey::Str(y))) => x.cmp(y),
            (Some(SortKey::Num(_)), Some(SortKey::Str(_))) => Ordering::Less,
            (Some(SortKey::Str(_)), Some(SortKey::Num(_))) => Ordering::Greater,
            _ => Ordering::Equal,
        };
        let ord = match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    Ordering::Equal
}

fn push_topk(
    heap: &mut Vec<(Vec<SortKey>, Vec<String>)>,
    capacity: usize,
    keys: Vec<SortKey>,
    row: Vec<String>,
    order: &[(String, SortDir)],
) {
    if capacity == 0 {
        return;
    }
    if heap.len() < capacity {
        heap.push((keys, row));
        return;
    }
    let mut last = 0usize;
    for i in 1..heap.len() {
        if cmp_sort_keys(&heap[i].0, &heap[last].0, order) == std::cmp::Ordering::Greater {
            last = i;
        }
    }
    if cmp_sort_keys(&keys, &heap[last].0, order) == std::cmp::Ordering::Less {
        heap[last] = (keys, row);
    }
}

/// Streaming dump events: CREATE buffered; INSERT tuples without retaining the full statement.
#[derive(Debug)]
enum DumpEvent {
    Create(String),
    InsertBegin {
        table: String,
        columns: Option<Vec<String>>,
    },
    InsertTuple(Vec<String>),
    InsertEnd,
}

#[derive(Debug)]
enum Phase {
    Idle,
    /// Buffer until `;` (CREATE TABLE).
    CollectCreate,
    /// Discard until `;`.
    SkipStmt,
    /// Collect INSERT … VALUES header (capped).
    InsertHeader,
    InsertValues {
        table: String,
        columns: Option<Vec<String>>,
        sent_begin: bool,
    },
}

struct DumpScanner<R: Read> {
    reader: R,
    buf: Vec<u8>,
    start: usize,
    bytes_read: u64,
    max_bytes: Option<u64>,
    eof: bool,
    phase: Phase,
}

/// Hard cap on buffered window (header or incomplete tuple), not whole mega-INSERT.
const MAX_STATEMENT_BYTES: usize = 32 * 1024 * 1024;

impl<R: Read> DumpScanner<R> {
    fn new(reader: R, max_bytes: Option<u64>) -> Self {
        Self {
            reader,
            buf: Vec::with_capacity(64 * 1024),
            start: 0,
            bytes_read: 0,
            max_bytes,
            eof: false,
            phase: Phase::Idle,
        }
    }

    fn compact(&mut self) {
        if self.start > 0 {
            self.buf.drain(..self.start);
            self.start = 0;
        }
    }

    fn window(&self) -> &[u8] {
        &self.buf[self.start..]
    }

    fn window_len(&self) -> usize {
        self.buf.len().saturating_sub(self.start)
    }

    fn ensure_cap(&self) -> Result<()> {
        if self.window_len() > MAX_STATEMENT_BYTES {
            bail!(
                "SQL statement window exceeds {} bytes; use smaller INSERTs or raise limits",
                MAX_STATEMENT_BYTES
            );
        }
        Ok(())
    }

    fn fill_more(&mut self) -> Result<()> {
        self.compact();
        if let Some(max) = self.max_bytes {
            if self.bytes_read >= max {
                self.eof = true;
                return Ok(());
            }
        }
        let mut chunk = [0u8; 64 * 1024];
        let to_read = if let Some(max) = self.max_bytes {
            ((max - self.bytes_read) as usize).min(chunk.len())
        } else {
            chunk.len()
        };
        let n = self
            .reader
            .read(&mut chunk[..to_read])
            .context("read sql dump")?;
        if n == 0 {
            self.eof = true;
        } else {
            self.bytes_read += n as u64;
            self.buf.extend_from_slice(&chunk[..n]);
        }
        Ok(())
    }

    fn skip_ws(&mut self) {
        while self.start < self.buf.len() && self.buf[self.start].is_ascii_whitespace() {
            self.start += 1;
        }
    }

    fn next_event(&mut self) -> Result<Option<DumpEvent>> {
        loop {
            match &mut self.phase {
                Phase::Idle => {
                    self.skip_ws();
                    if self.window().is_empty() {
                        if self.eof {
                            return Ok(None);
                        }
                        self.fill_more()?;
                        continue;
                    }
                    let w = self.window();
                    if starts_with_bytes_ci(w, b"INSERT") {
                        self.phase = Phase::InsertHeader;
                        continue;
                    }
                    if starts_with_bytes_ci(w, b"CREATE") {
                        self.phase = Phase::CollectCreate;
                        continue;
                    }
                    self.phase = Phase::SkipStmt;
                }
                Phase::CollectCreate => {
                    if let Some(rel) = find_semicolon_unquoted(self.window()) {
                        let end = self.start + rel;
                        let stmt = String::from_utf8_lossy(&self.buf[self.start..end]).into_owned();
                        self.start = end + 1;
                        self.phase = Phase::Idle;
                        self.compact();
                        if !stmt.trim().is_empty() {
                            return Ok(Some(DumpEvent::Create(stmt)));
                        }
                        continue;
                    }
                    if self.eof {
                        let rest = String::from_utf8_lossy(self.window()).into_owned();
                        self.start = self.buf.len();
                        self.phase = Phase::Idle;
                        self.compact();
                        if rest.trim().is_empty() {
                            return Ok(None);
                        }
                        return Ok(Some(DumpEvent::Create(rest)));
                    }
                    self.fill_more()?;
                    self.ensure_cap()?;
                }
                Phase::SkipStmt => {
                    if let Some(rel) = find_semicolon_unquoted(self.window()) {
                        self.start += rel + 1;
                        self.phase = Phase::Idle;
                        self.compact();
                        continue;
                    }
                    if self.eof {
                        self.start = self.buf.len();
                        self.phase = Phase::Idle;
                        self.compact();
                        return Ok(None);
                    }
                    // Discard unread skip window to bound RAM.
                    self.start = self.buf.len();
                    self.compact();
                    self.fill_more()?;
                }
                Phase::InsertHeader => {
                    let w = self.window();
                    if let Some(vidx) = find_keyword_ci_bytes(w, b"VALUES") {
                        let header_end = vidx + 6;
                        let header = std::str::from_utf8(&w[..header_end])
                            .context("INSERT header is not UTF-8")?;
                        let Some((table, columns, _)) = parse_insert_header(header) else {
                            // Malformed: skip to ';'
                            self.phase = Phase::SkipStmt;
                            continue;
                        };
                        self.start += header_end;
                        self.phase = Phase::InsertValues {
                            table,
                            columns,
                            sent_begin: false,
                        };
                        self.compact();
                        continue;
                    }
                    if find_semicolon_unquoted(w).is_some() {
                        self.phase = Phase::SkipStmt;
                        continue;
                    }
                    if self.eof {
                        self.phase = Phase::Idle;
                        self.start = self.buf.len();
                        self.compact();
                        return Ok(None);
                    }
                    self.fill_more()?;
                    self.ensure_cap()?;
                }
                Phase::InsertValues { .. } => match self.next_insert_values_event()? {
                    Some(ev) => return Ok(Some(ev)),
                    None => continue,
                },
            }
        }
    }

    fn next_insert_values_event(&mut self) -> Result<Option<DumpEvent>> {
        let (need_begin, table, columns) = match &mut self.phase {
            Phase::InsertValues {
                table,
                columns,
                sent_begin,
            } => {
                if !*sent_begin {
                    *sent_begin = true;
                    return Ok(Some(DumpEvent::InsertBegin {
                        table: table.clone(),
                        columns: columns.clone(),
                    }));
                }
                (false, (), ())
            }
            _ => return Ok(None),
        };
        let _ = (need_begin, table, columns);

        self.skip_ws();
        while self.start < self.buf.len() && self.buf[self.start] == b',' {
            self.start += 1;
            self.skip_ws();
        }
        if self.start < self.buf.len() && self.buf[self.start] == b';' {
            self.start += 1;
            self.phase = Phase::Idle;
            self.compact();
            return Ok(Some(DumpEvent::InsertEnd));
        }
        if self.window().is_empty() {
            if self.eof {
                self.phase = Phase::Idle;
                return Ok(Some(DumpEvent::InsertEnd));
            }
            self.fill_more()?;
            return Ok(None);
        }
        if self.buf[self.start] != b'(' {
            self.phase = Phase::SkipStmt;
            return Ok(None);
        }
        match find_matching_paren_end(self.window()) {
            Some(end) => {
                let body = &self.window()[1..end];
                let body_s = std::str::from_utf8(body).unwrap_or("");
                let cells: Vec<String> = split_top_level_commas(body_s)
                    .into_iter()
                    .map(|c| normalize_sql_value(c.trim()))
                    .collect();
                self.start += end + 1;
                self.compact();
                Ok(Some(DumpEvent::InsertTuple(cells)))
            }
            None => {
                if self.eof {
                    self.phase = Phase::Idle;
                    return Ok(Some(DumpEvent::InsertEnd));
                }
                self.fill_more()?;
                self.ensure_cap()?;
                Ok(None)
            }
        }
    }

    /// After early-stop mid-INSERT: discard remaining tuples until `;` without allocating cells.
    fn finish_insert(&mut self) -> Result<()> {
        if !matches!(self.phase, Phase::InsertValues { .. }) {
            return Ok(());
        }
        loop {
            if let Some(rel) = find_semicolon_unquoted(self.window()) {
                self.start += rel + 1;
                self.phase = Phase::Idle;
                self.compact();
                return Ok(());
            }
            if self.eof {
                self.start = self.buf.len();
                self.phase = Phase::Idle;
                self.compact();
                return Ok(());
            }
            self.start = self.buf.len();
            self.compact();
            self.fill_more()?;
        }
    }

    fn in_insert_values(&self) -> bool {
        matches!(self.phase, Phase::InsertValues { .. })
    }
}

fn starts_with_bytes_ci(hay: &[u8], keyword: &[u8]) -> bool {
    hay.len() >= keyword.len() && hay[..keyword.len()].eq_ignore_ascii_case(keyword)
}

fn find_keyword_ci_bytes(hay: &[u8], keyword: &[u8]) -> Option<usize> {
    if keyword.is_empty() || hay.len() < keyword.len() {
        return None;
    }
    let first_l = keyword[0].to_ascii_lowercase();
    let first_u = keyword[0].to_ascii_uppercase();
    let mut i = 0usize;
    while i + keyword.len() <= hay.len() {
        let b = hay[i];
        if (b == first_l || b == first_u) && hay[i..i + keyword.len()].eq_ignore_ascii_case(keyword)
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_semicolon_unquoted(bytes: &[u8]) -> Option<usize> {
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' if !in_double && !in_backtick => {
                if in_single && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = !in_single;
            }
            b'"' if !in_single && !in_backtick => in_double = !in_double,
            b'`' if !in_single && !in_double => in_backtick = !in_backtick,
            b';' if !in_single && !in_double && !in_backtick => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// End index of matching `)` for a slice that starts with `(`.
fn find_matching_paren_end(bytes: &[u8]) -> Option<usize> {
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\'' if !in_double && !in_backtick => {
                if in_single && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = !in_single;
            }
            b'"' if !in_single && !in_backtick => in_double = !in_double,
            b'`' if !in_single && !in_double => in_backtick = !in_backtick,
            b'(' if !in_single && !in_double && !in_backtick => depth += 1,
            b')' if !in_single && !in_double && !in_backtick => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Case-insensitive ASCII keyword search without allocating an uppercase copy.
fn find_keyword_ci(hay: &str, keyword_upper: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let k = keyword_upper.as_bytes();
    if k.is_empty() || h.len() < k.len() {
        return None;
    }
    let first_l = k[0].to_ascii_lowercase();
    let first_u = k[0].to_ascii_uppercase();
    let mut i = 0usize;
    while i + k.len() <= h.len() {
        let b = h[i];
        if (b == first_l || b == first_u) && h[i..i + k.len()].eq_ignore_ascii_case(k) {
            return Some(i);
        }
        i += 1;
    }
    None
}

pub(crate) fn split_sql_statements(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double && !in_backtick => {
                if in_single && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = !in_single;
            }
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            ';' if !in_single && !in_double && !in_backtick => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

pub(crate) fn extract_table_name(stmt: &str, kind: &str) -> Option<String> {
    let trimmed = stmt.trim_start();
    let rest = if kind == "CREATE TABLE" {
        let idx = find_keyword_ci(trimmed, "CREATE TABLE")?;
        let after = trimmed[idx + "CREATE TABLE".len()..].trim_start();
        let after = after
            .strip_prefix("IF NOT EXISTS")
            .or_else(|| after.strip_prefix("if not exists"))
            .unwrap_or(after)
            .trim_start();
        after
    } else {
        let idx = find_keyword_ci(trimmed, "INTO")?;
        trimmed[idx + 4..].trim_start()
    };
    let token = rest.split_whitespace().next()?;
    let name = token
        .trim_matches('`')
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches('(')
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub(crate) fn parse_create_table(stmt: &str) -> Option<(String, Vec<DumpColumn>)> {
    let name = extract_table_name(stmt, "CREATE TABLE")?;
    let body = table_paren_body(stmt)?;
    let mut columns = Vec::new();
    for part in split_top_level_commas(body) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let upper = part.to_ascii_uppercase();
        if upper.starts_with("CONSTRAINT")
            || upper.starts_with("PRIMARY KEY")
            || upper.starts_with("UNIQUE")
            || upper.starts_with("KEY ")
            || upper.starts_with("INDEX ")
            || upper.starts_with("FOREIGN KEY")
            || upper.starts_with("CHECK")
        {
            continue;
        }
        let (col_name, rest) = split_ident(part)?;
        let rest = rest.trim_start();
        let (type_name, flags) = split_type_and_flags(rest);
        let mut notes = Vec::new();
        let flags_upper = flags.to_ascii_uppercase();
        if flags_upper.contains("PRIMARY KEY") {
            notes.push("PRIMARY KEY");
        }
        if flags_upper.contains("NOT NULL") {
            notes.push("NOT NULL");
        }
        if flags_upper.contains("UNIQUE") && !flags_upper.contains("PRIMARY KEY") {
            notes.push("UNIQUE");
        }
        columns.push(DumpColumn {
            name: col_name,
            type_name,
            notes: notes.join(" "),
        });
    }
    Some((name, columns))
}

type InsertParse = (String, Option<Vec<String>>, Vec<Vec<String>>);

pub(crate) fn parse_insert(stmt: &str) -> Option<InsertParse> {
    let (name, cols, values_part) = parse_insert_header(stmt)?;
    let mut tuples = Vec::new();
    for_each_value_tuple(values_part, |t| {
        tuples.push(t.to_vec());
        std::ops::ControlFlow::Continue(())
    });
    Some((name, cols, tuples))
}

/// Parse INSERT header; returns table name, optional column list, and the VALUES payload slice.
fn parse_insert_header(stmt: &str) -> Option<(String, Option<Vec<String>>, &str)> {
    let name = extract_table_name(stmt, "INSERT")?;
    let values_idx = find_keyword_ci(stmt, "VALUES")?;
    let before = stmt[..values_idx].trim_end();
    let after_name = {
        let into_idx = find_keyword_ci(before, "INTO")?;
        before[into_idx + 4..].trim_start()
    };
    let after_table = skip_ident(after_name)?.trim_start();
    let col_names = if after_table.starts_with('(') {
        let cols_body = matching_paren_inner(after_table)?;
        Some(
            split_top_level_commas(cols_body)
                .into_iter()
                .filter_map(|c| {
                    let t = c.trim();
                    if t.is_empty() {
                        None
                    } else {
                        Some(unquote_ident(t))
                    }
                })
                .collect(),
        )
    } else {
        None
    };

    let values_part = stmt[values_idx + "VALUES".len()..].trim_start();
    Some((name, col_names, values_part))
}

fn for_each_value_tuple<F>(s: &str, mut f: F)
where
    F: FnMut(&[String]) -> std::ops::ControlFlow<()>,
{
    // Allocate each tuple's cells once; do not retain prior tuples.
    let mut rest = s.trim_start();
    while let Some(inner) = find_next_tuple(rest) {
        let (body, after) = inner;
        let cells: Vec<String> = split_top_level_commas(body)
            .into_iter()
            .map(|c| normalize_sql_value(c.trim()))
            .collect();
        if matches!(f(&cells), std::ops::ControlFlow::Break(())) {
            return;
        }
        rest = after.trim_start().trim_start_matches(',').trim_start();
        if rest.is_empty() || rest.starts_with(';') {
            break;
        }
    }
}

fn table_paren_body(stmt: &str) -> Option<&str> {
    let bytes = stmt.as_bytes();
    let mut i = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '(' if !in_single && !in_double && !in_backtick => {
                return matching_paren_inner(&stmt[i..]);
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn matching_paren_inner(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double && !in_backtick => {
                if in_single && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = !in_single;
            }
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '(' if !in_single && !in_double && !in_backtick => depth += 1,
            ')' if !in_single && !in_double && !in_backtick => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[1..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '\'' if !in_double && !in_backtick => {
                if in_single && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    i += 2;
                    continue;
                }
                in_single = !in_single;
            }
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '(' if !in_single && !in_double && !in_backtick => depth += 1,
            ')' if !in_single && !in_double && !in_backtick => depth -= 1,
            ',' if !in_single && !in_double && !in_backtick && depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if start <= s.len() {
        out.push(&s[start..]);
    }
    out
}

fn split_ident(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    match bytes[0] {
        b'`' | b'"' | b'\'' => {
            let quote = bytes[0];
            let mut i = 1usize;
            while i < bytes.len() {
                if bytes[i] == quote {
                    let name = unquote_ident(&s[..=i]);
                    return Some((name, &s[i + 1..]));
                }
                i += 1;
            }
            None
        }
        _ => {
            let mut i = 0usize;
            while i < bytes.len() {
                let c = bytes[i] as char;
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            if i == 0 {
                return None;
            }
            Some((s[..i].to_string(), &s[i..]))
        }
    }
}

fn skip_ident(s: &str) -> Option<&str> {
    split_ident(s).map(|(_, rest)| rest)
}

fn unquote_ident(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if (b[0] == b'`' && b[b.len() - 1] == b'`')
            || (b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\'')
        {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

fn split_type_and_flags(rest: &str) -> (String, &str) {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return (String::new(), "");
    }
    let bytes = rest.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            i += 1;
        } else {
            break;
        }
    }
    if i < bytes.len() && bytes[i] == b'(' {
        let mut depth = 0i32;
        while i < bytes.len() {
            match bytes[i] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
    }
    (rest[..i].trim().to_string(), rest[i..].trim_start())
}

fn find_next_tuple(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'(' {
        return None;
    }
    let body = matching_paren_inner(&s[i..])?;
    let end = i + 1 + body.len() + 1;
    Some((body, &s[end..]))
}

fn normalize_sql_value(s: &str) -> String {
    if s.eq_ignore_ascii_case("NULL") {
        return "NULL".into();
    }
    let t = s.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if b[0] == b'\'' && b[b.len() - 1] == b'\'' {
            return t[1..t.len() - 1].replace("''", "'");
        }
        if b[0] == b'"' && b[b.len() - 1] == b'"' {
            return t[1..t.len() - 1].replace("\"\"", "\"");
        }
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_sql() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo/db/sample.sql")
    }

    fn q(sql: &str) -> DumpQueryResult {
        let cfg = OmnicatConfig::default();
        let sel = parse_dump_select(sql).unwrap();
        let mut progress = crate::sinks::progress::ProgressReporter::new("test", false);
        query_sql_dump(&sample_sql(), &sel, &cfg, 1000, &mut progress).unwrap()
    }

    #[test]
    fn parse_select_star_limit() {
        let s = parse_dump_select("SELECT * FROM users LIMIT 5").unwrap();
        assert_eq!(s.table, "users");
        assert!(matches!(s.kind, SelectKind::Columns(SelectColumns::All)));
        assert_eq!(s.limit, Some(5));
    }

    #[test]
    fn parse_select_where() {
        let s = parse_dump_select("SELECT email FROM users WHERE status = 'ok'").unwrap();
        assert_eq!(s.table, "users");
        assert!(s.where_pred.is_some());
        match s.kind {
            SelectKind::Columns(SelectColumns::List(c)) => assert_eq!(c, vec!["email"]),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn reject_join() {
        let err = parse_dump_select("SELECT * FROM a JOIN b").unwrap_err();
        assert!(err.to_string().contains("JOIN") || err.to_string().contains("single-table"));
    }

    #[test]
    fn order_by_requires_limit() {
        let err = parse_dump_select("SELECT * FROM users ORDER BY id").unwrap_err();
        assert!(err.to_string().contains("requires LIMIT"));
    }

    #[test]
    fn query_users_limit() {
        let out = q("SELECT * FROM users LIMIT 5");
        assert_eq!(out.headers, vec!["id", "email", "status"]);
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.rows[0][1], "a@example.com");
    }

    #[test]
    fn query_where_and() {
        let out = q("SELECT email FROM users WHERE status = 'ok' AND id > 1");
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0][0], "c@example.com");
    }

    #[test]
    fn query_where_in_and_like() {
        let out = q("SELECT id FROM users WHERE status IN ('ok', 'failed')");
        assert_eq!(out.rows.len(), 3);
        let out = q("SELECT email FROM users WHERE email LIKE '%example.com'");
        assert_eq!(out.rows.len(), 3);
    }

    #[test]
    fn query_order_desc_limit() {
        let out = q("SELECT id FROM users ORDER BY id DESC LIMIT 1");
        assert_eq!(out.rows, vec![vec!["3".to_string()]]);
    }

    #[test]
    fn query_offset() {
        let out = q("SELECT id FROM users LIMIT 1 OFFSET 1");
        assert_eq!(out.rows, vec![vec!["2".to_string()]]);
    }

    #[test]
    fn query_count() {
        let out = q("SELECT COUNT(*) FROM users WHERE status = 'ok'");
        assert_eq!(out.headers, vec!["count"]);
        assert_eq!(out.rows, vec![vec!["2".to_string()]]);
    }

    #[test]
    fn query_where_and_limit_one() {
        let out = q("SELECT email FROM users WHERE status = 'ok'");
        assert_eq!(out.rows.len(), 2);
        let out = q("SELECT * FROM users LIMIT 1");
        assert_eq!(out.rows.len(), 1);
    }

    #[test]
    fn query_missing_table() {
        let cfg = OmnicatConfig::default();
        let sel = parse_dump_select("SELECT * FROM nope").unwrap();
        let mut progress = crate::sinks::progress::ProgressReporter::new("test", false);
        let err = query_sql_dump(&sample_sql(), &sel, &cfg, 1000, &mut progress).unwrap_err();
        assert!(err.to_string().contains("not found"));
        assert!(!err.to_string().contains("not a database"));
    }

    #[test]
    fn like_patterns_via_pred() {
        let pred = crate::search::parse_sql_where("email LIKE '%example.com'").unwrap();
        let row = vec![("email".into(), "a@example.com".into())];
        assert!(crate::search::eval_pred(&pred, &row));
    }

    #[test]
    fn for_each_value_tuple_stops_early() {
        let values = "(1,'a'),(2,'b'),(3,'c'),(4,'d')";
        let mut seen = 0usize;
        for_each_value_tuple(values, |_| {
            seen += 1;
            if seen >= 2 {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        });
        assert_eq!(seen, 2);
    }

    #[test]
    fn query_limit_stops_after_enough_rows() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big.sql");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "CREATE TABLE t (id INT);").unwrap();
        for i in 0..5_000 {
            writeln!(f, "INSERT INTO t VALUES ({i});").unwrap();
        }
        let mut cfg = OmnicatConfig::default();
        cfg.inspect.no_limit = true;
        cfg.inspect.max_bytes = 0;
        let sel = parse_dump_select("SELECT * FROM t LIMIT 1").unwrap();
        let mut progress = crate::sinks::progress::ProgressReporter::new("test", false);
        let out = query_sql_dump(&p, &sel, &cfg, 1000, &mut progress).unwrap();
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0][0], "0");
    }

    #[test]
    fn find_keyword_ci_no_full_upper() {
        let s = "insert into `Users` values (1)";
        assert_eq!(find_keyword_ci(s, "INTO"), Some(7));
        assert_eq!(find_keyword_ci(s, "VALUES"), Some(20));
        assert!(starts_with_bytes_ci(s.as_bytes(), b"INSERT"));
    }

    #[test]
    fn mega_insert_streams_tuples_with_limit() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mega.sql");
        let mut f = std::fs::File::create(&p).unwrap();
        write!(f, "CREATE TABLE t (id INT);\nINSERT INTO t VALUES ").unwrap();
        for i in 0..20_000 {
            if i > 0 {
                write!(f, ",").unwrap();
            }
            write!(f, "({i})").unwrap();
        }
        writeln!(f, ";").unwrap();
        let mut cfg = OmnicatConfig::default();
        cfg.inspect.no_limit = true;
        cfg.inspect.max_bytes = 0;
        let sel = parse_dump_select("SELECT * FROM t LIMIT 3").unwrap();
        let mut progress = crate::sinks::progress::ProgressReporter::new("test", false);
        let out = query_sql_dump(&p, &sel, &cfg, 1000, &mut progress).unwrap();
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.rows[0][0], "0");
        assert_eq!(out.rows[2][0], "2");
    }
}
