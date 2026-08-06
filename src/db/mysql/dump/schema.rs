//! Parse CREATE TABLE statements into table schemas.

use std::collections::BTreeMap;

use arrow::datatypes::SchemaRef;
use datafusion::logical_expr::{BinaryExpr, Expr, Operator};

use crate::db::report::{ColumnInfo, IndexInfo, TableSchema};

#[derive(Debug, Clone)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<(String, String)>,
    pub indexes: Vec<IndexInfo>,
}

#[derive(Debug, Clone)]
pub enum RowFilter {
    Eq {
        col_idx: usize,
        value: String,
    },
    NotEq {
        col_idx: usize,
        value: String,
    },
    Gt {
        col_idx: usize,
        value: String,
    },
    Gte {
        col_idx: usize,
        value: String,
    },
    Lt {
        col_idx: usize,
        value: String,
    },
    Lte {
        col_idx: usize,
        value: String,
    },
    IsNull {
        col_idx: usize,
    },
    IsNotNull {
        col_idx: usize,
    },
}

impl RowFilter {
    pub fn collect_from_expr(expr: &Expr, schema: &SchemaRef, out: &mut Vec<Self>) {
        match expr {
            Expr::BinaryExpr(BinaryExpr { left, op, right }) if *op == Operator::And => {
                Self::collect_from_expr(left, schema, out);
                Self::collect_from_expr(right, schema, out);
            }
            Expr::BinaryExpr(BinaryExpr { left, op, right }) => {
                if let Some(f) = Self::from_binary(left, *op, right, schema) {
                    out.push(f);
                }
            }
            Expr::IsNull(e) => {
                if let Some(idx) = column_index(e, schema) {
                    out.push(Self::IsNull { col_idx: idx });
                }
            }
            Expr::IsNotNull(e) => {
                if let Some(idx) = column_index(e, schema) {
                    out.push(Self::IsNotNull { col_idx: idx });
                }
            }
            _ => {}
        }
    }

    fn from_binary(left: &Expr, op: Operator, right: &Expr, schema: &SchemaRef) -> Option<Self> {
        let col_idx = column_index(left, schema)?;
        let lit = literal_string(right)?;
        Some(match op {
            Operator::Eq => Self::Eq {
                col_idx,
                value: lit,
            },
            Operator::NotEq => Self::NotEq {
                col_idx,
                value: lit,
            },
            Operator::Gt => Self::Gt {
                col_idx,
                value: lit,
            },
            Operator::GtEq => Self::Gte {
                col_idx,
                value: lit,
            },
            Operator::Lt => Self::Lt {
                col_idx,
                value: lit,
            },
            Operator::LtEq => Self::Lte {
                col_idx,
                value: lit,
            },
            _ => return None,
        })
    }

    pub fn all_match(filters: &[Self], schema: &SchemaRef, row: &[String]) -> bool {
        filters.iter().all(|f| f.matches(schema, row))
    }

    fn matches(&self, _schema: &SchemaRef, row: &[String]) -> bool {
        let cell = |idx: usize| row.get(idx).map(String::as_str).unwrap_or("NULL");
        match self {
            Self::Eq { col_idx, value } => cell(*col_idx) == value.as_str(),
            Self::NotEq { col_idx, value } => cell(*col_idx) != value.as_str(),
            Self::IsNull { col_idx } => {
                cell(*col_idx).eq_ignore_ascii_case("NULL") || cell(*col_idx).is_empty()
            }
            Self::IsNotNull { col_idx } => {
                !cell(*col_idx).eq_ignore_ascii_case("NULL") && !cell(*col_idx).is_empty()
            }
            Self::Gt { col_idx, value } => compare(cell(*col_idx), value) == std::cmp::Ordering::Greater,
            Self::Gte { col_idx, value } => {
                compare(cell(*col_idx), value) != std::cmp::Ordering::Less
            }
            Self::Lt { col_idx, value } => compare(cell(*col_idx), value) == std::cmp::Ordering::Less,
            Self::Lte { col_idx, value } => {
                compare(cell(*col_idx), value) != std::cmp::Ordering::Greater
            }
        }
    }
}

fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    if let (Ok(a), Ok(b)) = (a.parse::<f64>(), b.parse::<f64>()) {
        return a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal);
    }
    a.cmp(b)
}

fn column_index(expr: &Expr, schema: &SchemaRef) -> Option<usize> {
    match expr {
        Expr::Column(c) => schema.index_of(&c.name).ok(),
        _ => None,
    }
}

fn literal_string(expr: &Expr) -> Option<String> {
    use datafusion::scalar::ScalarValue;
    match expr {
        Expr::Literal(ScalarValue::Utf8(Some(s)), _)
        | Expr::Literal(ScalarValue::Utf8View(Some(s)), _)
        | Expr::Literal(ScalarValue::LargeUtf8(Some(s)), _) => Some(s.clone()),
        Expr::Literal(ScalarValue::Int64(Some(n)), _) => Some(n.to_string()),
        Expr::Literal(ScalarValue::Float64(Some(n)), _) => Some(n.to_string()),
        _ => None,
    }
}

pub fn parse_create_table(stmt: &str) -> Option<TableDef> {
    let stmt = strip_leading_sql_noise(stmt);
    let upper = stmt.trim().to_ascii_uppercase();
    if !upper.starts_with("CREATE TABLE") {
        return None;
    }
    let rest = stmt.trim();
    let after = rest
        .strip_prefix("CREATE TABLE")
        .or_else(|| rest.get(12..).and_then(|s| s.strip_prefix(' ')))?;
    let after = after.trim_start();
    let (raw_name, body_start) = parse_table_name(after)?;
    let body = extract_paren_body(after, body_start)?;
    let (columns, indexes) = parse_column_defs(body);
    Some(TableDef {
        name: raw_name,
        columns,
        indexes,
    })
}

pub fn schemas_from_create(stmts: &BTreeMap<String, TableDef>) -> Vec<TableSchema> {
    let mut out = Vec::new();
    for (name, def) in stmts {
        out.push(TableSchema {
            name: name.clone(),
            columns: def
                .columns
                .iter()
                .map(|(n, t)| ColumnInfo {
                    name: n.clone(),
                    type_name: t.clone(),
                    nullable: !t.to_ascii_uppercase().contains("NOT NULL"),
                })
                .collect(),
            indexes: def.indexes.clone(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn strip_leading_sql_noise(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map(|(_, tail)| tail).unwrap_or("");
            continue;
        }
        if s.starts_with("/*!") {
            if let Some(end) = s.find("*/") {
                s = &s[end + 2..];
                continue;
            }
        }
        break;
    }
    s.trim_start()
}

fn parse_table_name(s: &str) -> Option<(String, usize)> {
    let s = s.trim_start();
    if let Some(rest) = s.strip_prefix('`') {
        let end = rest.find('`')? + 1;
        return Some((rest[..end - 1].to_string(), end + 1));
    }
    let end = s.find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')?;
    Some((s[..end].to_string(), end))
}

fn extract_paren_body(s: &str, start: usize) -> Option<&str> {
    let rest = s[start..].trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&rest[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_column_defs(body: &str) -> (Vec<(String, String)>, Vec<IndexInfo>) {
    let mut columns = Vec::new();
    let mut indexes = Vec::new();
    for part in split_top_level_commas(body) {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let upper = p.to_ascii_uppercase();
        if upper.starts_with("PRIMARY KEY") {
            if let Some(cols) = parse_index_columns(p) {
                indexes.push(IndexInfo {
                    name: "PRIMARY".into(),
                    columns: cols,
                    unique: true,
                    kind: "PRIMARY KEY".into(),
                });
            }
            continue;
        }
        if upper.starts_with("UNIQUE KEY") || upper.starts_with("UNIQUE INDEX") {
            if let Some((name, cols)) = parse_named_index(p) {
                indexes.push(IndexInfo {
                    name,
                    columns: cols,
                    unique: true,
                    kind: "UNIQUE".into(),
                });
            }
            continue;
        }
        if upper.starts_with("KEY ") || upper.starts_with("INDEX ") {
            if let Some((name, cols)) = parse_named_index(p) {
                indexes.push(IndexInfo {
                    name,
                    columns: cols,
                    unique: false,
                    kind: "INDEX".into(),
                });
            }
            continue;
        }
        if upper.starts_with("CONSTRAINT ") || upper.starts_with("FULLTEXT ") {
            continue;
        }
        let mut words = p.split_whitespace();
        let Some(col_name_raw) = words.next() else {
            continue;
        };
        let col_name = col_name_raw.trim_matches('`').to_string();
        let type_name = words.collect::<Vec<_>>().join(" ");
        if col_name.is_empty() || type_name.is_empty() {
            continue;
        }
        columns.push((col_name, type_name));
    }
    (columns, indexes)
}

fn parse_index_columns(s: &str) -> Option<Vec<String>> {
    let start = s.find('(')?;
    let body = extract_paren_body(s, start)?;
    Some(
        split_top_level_commas(body)
            .iter()
            .map(|c| c.trim().trim_matches('`').to_string())
            .filter(|c| !c.is_empty())
            .collect(),
    )
}

fn parse_named_index(s: &str) -> Option<(String, Vec<String>)> {
    let paren = s.find('(')?;
    let head = s[..paren].trim();
    let name = head
        .split_whitespace()
        .last()
        .unwrap_or("idx")
        .trim_matches('`')
        .to_string();
    let cols = parse_index_columns(s)?;
    Some((name, cols))
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

pub fn parse_insert_table(stmt: &str) -> Option<String> {
    let stmt = strip_leading_sql_noise(stmt);
    let upper = stmt.trim().to_ascii_uppercase();
    let rest = if upper.strip_prefix("INSERT IGNORE INTO").is_some() {
        stmt.trim()[18..].trim_start()
    } else if upper.strip_prefix("REPLACE INTO").is_some() {
        stmt.trim()[12..].trim_start()
    } else if upper.starts_with("INSERT INTO") {
        stmt.trim()[11..].trim_start()
    } else {
        return None;
    };
    parse_table_name(rest).map(|(n, _)| n)
}

pub fn parse_insert_values(stmt: &str) -> Option<Vec<Vec<String>>> {
    let stmt = strip_leading_sql_noise(stmt);
    let upper = stmt.trim().to_ascii_uppercase();
    if !upper.contains("VALUES") {
        return None;
    }
    let values_pos = upper.find("VALUES")?;
    Some(parse_value_groups(&stmt[values_pos + 6..]))
}

/// Count rows in an INSERT without materializing cell values.
pub fn count_insert_rows(stmt: &str) -> u64 {
    let stmt = strip_leading_sql_noise(stmt);
    let upper = stmt.trim().to_ascii_uppercase();
    let Some(values_pos) = upper.find("VALUES") else {
        return 0;
    };
    count_value_groups(&stmt[values_pos + 6..])
}

/// Stream rows from a single INSERT statement (one row at a time).
pub fn iter_insert_rows(stmt: &str) -> InsertRowIter<'_> {
    let stmt = strip_leading_sql_noise(stmt);
    let upper = stmt.trim().to_ascii_uppercase();
    let tail = upper
        .find("VALUES")
        .map(|pos| &stmt[pos + 6..])
        .unwrap_or("");
    InsertRowIter { s: tail, done: tail.is_empty() }
}

pub struct InsertRowIter<'a> {
    s: &'a str,
    done: bool,
}

impl<'a> Iterator for InsertRowIter<'a> {
    type Item = Vec<String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let bytes = self.s.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            self.done = true;
            return None;
        }
        let (row, consumed) = parse_tuple(&self.s[i..])?;
        self.s = &self.s[i + consumed..];
        let mut j = 0usize;
        while j < self.s.len() && (self.s.as_bytes()[j] == b',' || self.s.as_bytes()[j].is_ascii_whitespace())
        {
            j += 1;
        }
        self.s = &self.s[j..];
        if self.s.is_empty() {
            self.done = true;
        }
        Some(row)
    }
}

fn count_value_groups(s: &str) -> u64 {
    let mut count = 0u64;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            break;
        }
        let Some((_, next)) = parse_tuple(&s[i..]) else {
            break;
        };
        count += 1;
        i += next;
        while i < bytes.len() && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
    }
    count
}

fn parse_value_groups(s: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'(' {
            break;
        }
        if let Some((row, next)) = parse_tuple(&s[i..]) {
            out.push(row);
            i += next;
        } else {
            break;
        }
        while i < bytes.len() && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
    }
    out
}

fn parse_tuple(s: &str) -> Option<(Vec<String>, usize)> {
    if !s.starts_with('(') {
        return None;
    }
    let mut vals = Vec::new();
    let mut i = 1usize;
    loop {
        while i < s.len() && s.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= s.len() {
            return None;
        }
        if s.as_bytes()[i] == b')' {
            return Some((vals, i + 1));
        }
        let (val, consumed) = parse_value(&s[i..])?;
        vals.push(val);
        i += consumed;
        while i < s.len() && s.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i < s.len() && s.as_bytes()[i] == b',' {
            i += 1;
        }
    }
}

fn parse_value(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if s.starts_with("NULL") && (s.len() == 4 || !s.as_bytes()[4].is_ascii_alphanumeric()) {
        return Some(("NULL".into(), 4));
    }
    if b.len() >= 2 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        let mut i = 2usize;
        while i < b.len() && b[i].is_ascii_hexdigit() {
            i += 1;
        }
        return Some((format!("<hex {} bytes>", (i - 2) / 2), i));
    }
    if b[0] == b'\'' {
        let mut out = String::new();
        let mut i = 1usize;
        let mut escape = false;
        while i < b.len() {
            let c = b[i];
            if escape {
                out.push(c as char);
                escape = false;
                i += 1;
                continue;
            }
            if c == b'\\' {
                escape = true;
                i += 1;
                continue;
            }
            if c == b'\'' {
                return Some((out, i + 1));
            }
            out.push(c as char);
            i += 1;
        }
        return None;
    }
    if b[0] == b'"' {
        let mut out = String::new();
        let mut i = 1usize;
        while i < b.len() {
            if b[i] == b'"' {
                return Some((out, i + 1));
            }
            out.push(b[i] as char);
            i += 1;
        }
        return None;
    }
    let mut i = 0usize;
    while i < b.len() && !matches!(b[i], b',' | b')') {
        i += 1;
    }
    Some((s[..i].trim().to_string(), i))
}

pub fn fuzzy_column_hint(name: &str, columns: &[String]) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    columns
        .iter()
        .min_by_key(|c| levenshtein(&lower, &c.to_ascii_lowercase()))
        .filter(|c| levenshtein(&lower, &c.to_ascii_lowercase()) <= 3)
        .cloned()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let mut dp = vec![0usize; b.len() + 1];
    for (j, val) in dp.iter_mut().enumerate().skip(1) {
        *val = j;
    }
    for (i, ca) in a.iter().enumerate() {
        let mut prev = i;
        dp[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            let cur = (dp[j + 1] + 1).min(prev + 1).min(dp[j] + cost);
            dp[j + 1] = cur;
            prev = cur;
        }
    }
    dp[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_create_and_insert() {
        let stmt = "CREATE TABLE `users` (`id` INT NOT NULL, `email` VARCHAR(255), PRIMARY KEY (`id`));";
        let t = parse_create_table(stmt).unwrap();
        assert_eq!(t.name, "users");
        assert_eq!(t.columns.len(), 2);
        assert_eq!(t.indexes.len(), 1);
        let ins = "INSERT INTO `users` VALUES (1,'a@b.com'),(2,'c@d.com');";
        assert_eq!(parse_insert_table(ins).as_deref(), Some("users"));
        let rows = parse_insert_values(ins).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][1], "a@b.com");
    }

    #[test]
    fn count_and_iter_insert_rows() {
        let ins = "INSERT INTO `users` VALUES (1,'a@b.com'),(2,'c@d.com');";
        assert_eq!(count_insert_rows(ins), 2);
        let rows: Vec<_> = iter_insert_rows(ins).collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][1], "a@b.com");
    }

    #[test]
    fn parses_replace_and_ignore() {
        assert_eq!(
            parse_insert_table("REPLACE INTO t VALUES (1);").as_deref(),
            Some("t")
        );
        assert_eq!(
            parse_insert_table("INSERT IGNORE INTO t VALUES (1);").as_deref(),
            Some("t")
        );
    }
}
