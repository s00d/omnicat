//! Search predicate AST, SQL-WHERE / find-shorthand parsers, and eval.

use anyhow::{bail, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchPred {
    And(Box<SearchPred>, Box<SearchPred>),
    Or(Box<SearchPred>, Box<SearchPred>),
    Not(Box<SearchPred>),
    Cmp {
        col: String,
        op: CmpOp,
        lit: String,
    },
    IsNull {
        col: String,
        negated: bool,
    },
    Like {
        col: String,
        pattern: String,
    },
    In {
        col: String,
        values: Vec<String>,
    },
    /// Bare field truthiness (legacy find: `active`).
    FieldTruthy(String),
}

/// Parsed `--find` pattern: structured predicate and/or substring fallback.
#[derive(Debug, Clone, PartialEq)]
pub struct FindQuery {
    pub pred: Option<SearchPred>,
    pub substring: Option<String>,
}

/// Parse `--find` pattern. Keeps `level:error` shorthand; also accepts SQL WHERE fragments.
pub fn parse_find_pattern(raw: &str) -> Result<FindQuery> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty find query");
    }
    let upper = s.to_ascii_uppercase();
    if upper.starts_with("SELECT ") {
        bail!("full SELECT is not supported with --find; use -q / --query");
    }

    if looks_like_sql_where(s) {
        let pred = parse_sql_where(s)?;
        return Ok(FindQuery {
            pred: Some(pred),
            substring: None,
        });
    }

    if let Some(pred) = parse_find_shorthand(s)? {
        return Ok(FindQuery {
            pred: Some(pred),
            substring: None,
        });
    }

    Ok(FindQuery {
        pred: None,
        substring: Some(s.to_string()),
    })
}

fn looks_like_sql_where(s: &str) -> bool {
    let upper = s.to_ascii_uppercase();
    if upper.contains(" AND ")
        || upper.contains(" OR ")
        || upper.contains(" LIKE ")
        || upper.contains(" IN ")
        || upper.contains(" IS NULL")
        || upper.contains(" IS NOT NULL")
    {
        return true;
    }
    // `col = 'x'` / `col = 1` with spaces around =
    if s.contains('\'') {
        return true;
    }
    let trimmed = s.trim();
    if let Ok(tokens) = tokenize_sql(trimmed) {
        if tokens.len() >= 3 {
            let op = tokens[1].as_str();
            if matches!(op, "=" | "!=" | "<>" | "<" | "<=" | ">" | ">=") {
                return true;
            }
        }
    }
    false
}

fn parse_find_shorthand(s: &str) -> Result<Option<SearchPred>> {
    let q = s.trim();
    let ops = ["==", "!=", ">=", "<=", ">", "<"];
    for op in ops {
        if let Some((left, right)) = q.split_once(op) {
            let field = left.trim().trim_start_matches('.');
            let rhs = right.trim().trim_matches('"').trim_matches('\'');
            if field.is_empty() || rhs.is_empty() || field.contains(' ') {
                continue;
            }
            let cmp = match op {
                "==" => CmpOp::Eq,
                "!=" => CmpOp::Ne,
                ">" => CmpOp::Gt,
                "<" => CmpOp::Lt,
                ">=" => CmpOp::Ge,
                "<=" => CmpOp::Le,
                _ => continue,
            };
            return Ok(Some(SearchPred::Cmp {
                col: field.to_string(),
                op: cmp,
                lit: rhs.to_string(),
            }));
        }
    }
    if let Some((left, right)) = q.split_once(':') {
        let field = left.trim();
        let val = right.trim();
        if !field.is_empty()
            && !val.is_empty()
            && !field.contains(' ')
            && field
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return Ok(Some(SearchPred::Cmp {
                col: field.to_string(),
                op: CmpOp::Eq,
                lit: val.to_string(),
            }));
        }
    }
    // bare word → substring fallback (not field truthiness; that is SQL WHERE only)
    Ok(None)
}

/// Parse a SQL WHERE expression (without the WHERE keyword).
pub fn parse_sql_where(sql: &str) -> Result<SearchPred> {
    let sql = sql.trim();
    let upper = sql.to_ascii_uppercase();
    let body = if upper.starts_with("WHERE ") {
        sql["WHERE ".len()..].trim()
    } else {
        sql
    };
    let tokens = tokenize_sql(body)?;
    let mut i = 0usize;
    let pred = parse_sql_where_tokens(&tokens, &mut i)?;
    if i < tokens.len() {
        bail!("unexpected token `{}` in WHERE", tokens[i]);
    }
    Ok(pred)
}

/// Parse WHERE starting at `i`, advancing past the predicate.
pub fn parse_sql_where_tokens(tokens: &[String], i: &mut usize) -> Result<SearchPred> {
    parse_or_expr(tokens, i)
}

pub fn lookup_col<'a>(row: &'a [(String, String)], col: &str) -> Option<&'a str> {
    row.iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(col))
        .map(|(_, v)| v.as_str())
}

pub fn eval_pred(pred: &SearchPred, row: &[(String, String)]) -> bool {
    match pred {
        SearchPred::And(a, b) => eval_pred(a, row) && eval_pred(b, row),
        SearchPred::Or(a, b) => eval_pred(a, row) || eval_pred(b, row),
        SearchPred::Not(a) => !eval_pred(a, row),
        SearchPred::FieldTruthy(col) => {
            let val = lookup_col(row, col).unwrap_or("");
            !(val.is_empty()
                || val.eq_ignore_ascii_case("NULL")
                || val.eq_ignore_ascii_case("false")
                || val == "0")
        }
        SearchPred::IsNull { col, negated } => {
            let val = lookup_col(row, col).unwrap_or("");
            let is_null = val.is_empty() || val.eq_ignore_ascii_case("NULL");
            if *negated {
                !is_null
            } else {
                is_null
            }
        }
        SearchPred::Like { col, pattern } => {
            let val = lookup_col(row, col).unwrap_or("");
            if val.eq_ignore_ascii_case("NULL") {
                return false;
            }
            like_match(val, pattern)
        }
        SearchPred::In { col, values } => {
            let val = lookup_col(row, col).unwrap_or("");
            values.iter().any(|v| values_equal(val, v))
        }
        SearchPred::Cmp { col, op, lit } => {
            let val = lookup_col(row, col).unwrap_or("");
            cmp_values(val, *op, lit)
        }
    }
}

/// Build a field row from a JSON object (stringifies nested values).
pub fn json_object_fields(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(obj) = value.as_object() else {
        return out;
    };
    for (k, v) in obj {
        let s = match v {
            serde_json::Value::Null => "NULL".into(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        out.push((k.clone(), s));
    }
    out
}

fn parse_or_expr(tokens: &[String], i: &mut usize) -> Result<SearchPred> {
    let mut left = parse_and_expr(tokens, i)?;
    while *i < tokens.len() && eq_kw(tokens[*i].as_str(), "OR") {
        *i += 1;
        let right = parse_and_expr(tokens, i)?;
        left = SearchPred::Or(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_and_expr(tokens: &[String], i: &mut usize) -> Result<SearchPred> {
    let mut left = parse_not_expr(tokens, i)?;
    while *i < tokens.len() && eq_kw(tokens[*i].as_str(), "AND") {
        *i += 1;
        let right = parse_not_expr(tokens, i)?;
        left = SearchPred::And(Box::new(left), Box::new(right));
    }
    Ok(left)
}

fn parse_not_expr(tokens: &[String], i: &mut usize) -> Result<SearchPred> {
    if *i < tokens.len() && eq_kw(tokens[*i].as_str(), "NOT") {
        *i += 1;
        return Ok(SearchPred::Not(Box::new(parse_not_expr(tokens, i)?)));
    }
    parse_primary_pred(tokens, i)
}

fn parse_primary_pred(tokens: &[String], i: &mut usize) -> Result<SearchPred> {
    if *i < tokens.len() && tokens[*i] == "(" {
        *i += 1;
        let inner = parse_or_expr(tokens, i)?;
        expect_tok(tokens, i, ")")?;
        return Ok(inner);
    }
    let col = expect_ident(tokens, i)?;
    if *i >= tokens.len() {
        // bare field at end of expression
        return Ok(SearchPred::FieldTruthy(col));
    }
    if eq_kw(tokens[*i].as_str(), "IS") {
        *i += 1;
        let mut negated = false;
        if *i < tokens.len() && eq_kw(tokens[*i].as_str(), "NOT") {
            *i += 1;
            negated = true;
        }
        expect_kw(tokens, i, "NULL")?;
        return Ok(SearchPred::IsNull { col, negated });
    }
    if eq_kw(tokens[*i].as_str(), "LIKE") {
        *i += 1;
        let pattern = expect_string_lit(tokens, i)?;
        return Ok(SearchPred::Like { col, pattern });
    }
    if eq_kw(tokens[*i].as_str(), "IN") {
        *i += 1;
        expect_tok(tokens, i, "(")?;
        let mut values = Vec::new();
        loop {
            values.push(expect_literal_token(tokens, i)?);
            if *i < tokens.len() && tokens[*i] == "," {
                *i += 1;
                continue;
            }
            break;
        }
        expect_tok(tokens, i, ")")?;
        return Ok(SearchPred::In { col, values });
    }

    let op = match tokens[*i].as_str() {
        "=" | "==" => CmpOp::Eq,
        "!=" | "<>" => CmpOp::Ne,
        "<" => CmpOp::Lt,
        "<=" => CmpOp::Le,
        ">" => CmpOp::Gt,
        ">=" => CmpOp::Ge,
        // not an operator — treat as bare field (caller may have leftover tokens)
        _ => return Ok(SearchPred::FieldTruthy(col)),
    };
    *i += 1;
    let lit = expect_literal_token(tokens, i)?;
    Ok(SearchPred::Cmp { col, op, lit })
}

pub fn tokenize_sql(sql: &str) -> Result<Vec<String>> {
    let bytes = sql.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == '\'' {
            i += 1;
            let mut s = String::from("'");
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        s.push('\'');
                        s.push('\'');
                        i += 2;
                        continue;
                    }
                    s.push('\'');
                    i += 1;
                    break;
                }
                s.push(bytes[i] as char);
                i += 1;
            }
            if !s.ends_with('\'') || s.len() < 2 {
                bail!("unterminated string literal");
            }
            out.push(s);
            continue;
        }
        if c == '`' || c == '"' {
            let quote = bytes[i];
            i += 1;
            let mut s = String::new();
            while i < bytes.len() && bytes[i] != quote {
                s.push(bytes[i] as char);
                i += 1;
            }
            if i >= bytes.len() {
                bail!("unterminated quoted identifier");
            }
            i += 1;
            out.push(s);
            continue;
        }
        if matches!(c, '(' | ')' | ',' | '*') {
            out.push(c.to_string());
            i += 1;
            continue;
        }
        if matches!(c, '=' | '!' | '<' | '>') {
            let mut op = String::new();
            op.push(c);
            i += 1;
            if i < bytes.len() {
                let n = bytes[i] as char;
                if (c == '!' && n == '=')
                    || (c == '<' && (n == '=' || n == '>'))
                    || (c == '>' && n == '=')
                    || (c == '=' && n == '=')
                {
                    op.push(n);
                    i += 1;
                }
            }
            out.push(op);
            continue;
        }
        if c == '+' || c == '-' || c.is_ascii_digit() {
            let start = i;
            if c == '+' || c == '-' {
                i += 1;
            }
            let dig_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i == dig_start {
                bail!("unexpected `{c}`");
            }
            out.push(String::from_utf8_lossy(&bytes[start..i]).into_owned());
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' || c == '$' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(String::from_utf8_lossy(&bytes[start..i]).into_owned());
            continue;
        }
        // colon for legacy — not part of SQL WHERE tokenizer; treat as unexpected
        bail!("unexpected character `{c}` in query");
    }
    Ok(out)
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
        || matches!(
            t.as_str(),
            "=" | "==" | "!=" | "<>" | "<" | "<=" | ">" | ">="
        )
    {
        bail!("expected identifier, got `{t}`");
    }
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

fn expect_string_lit(tokens: &[String], i: &mut usize) -> Result<String> {
    if *i >= tokens.len() || !tokens[*i].starts_with('\'') {
        bail!("expected string literal");
    }
    let raw = &tokens[*i];
    *i += 1;
    Ok(raw[1..raw.len() - 1].replace("''", "'"))
}

fn expect_literal_token(tokens: &[String], i: &mut usize) -> Result<String> {
    if *i >= tokens.len() {
        bail!("expected literal");
    }
    let t = &tokens[*i];
    if t.starts_with('\'') {
        return expect_string_lit(tokens, i);
    }
    if eq_kw(t, "NULL") {
        *i += 1;
        return Ok("NULL".into());
    }
    if t.parse::<f64>().is_ok() {
        let v = t.clone();
        *i += 1;
        return Ok(v);
    }
    // unquoted bare word as string lit (shorthand tolerance in WHERE)
    if t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '-' || c == '.')
    {
        let v = t.clone();
        *i += 1;
        return Ok(v);
    }
    bail!("expected literal, got `{t}`");
}

fn like_match(text: &str, pattern: &str) -> bool {
    like_match_rec(text.as_bytes(), pattern.as_bytes())
}

fn like_match_rec(text: &[u8], pat: &[u8]) -> bool {
    let mut ti = 0usize;
    let mut pi = 0usize;
    let mut star_p = None;
    let mut star_t = 0usize;
    while ti < text.len() {
        if pi < pat.len() && (pat[pi] == b'_' || pat[pi] == text[ti]) {
            ti += 1;
            pi += 1;
        } else if pi < pat.len() && pat[pi] == b'%' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == b'%' {
        pi += 1;
    }
    pi == pat.len()
}

fn cmp_values(left: &str, op: CmpOp, right: &str) -> bool {
    if left.eq_ignore_ascii_case("NULL") || right.eq_ignore_ascii_case("NULL") {
        return matches!(op, CmpOp::Ne)
            && (left.eq_ignore_ascii_case("NULL") != right.eq_ignore_ascii_case("NULL"));
    }
    if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
        return match op {
            CmpOp::Eq => (a - b).abs() < f64::EPSILON,
            CmpOp::Ne => (a - b).abs() >= f64::EPSILON,
            CmpOp::Lt => a < b,
            CmpOp::Le => a <= b,
            CmpOp::Gt => a > b,
            CmpOp::Ge => a >= b,
        };
    }
    match op {
        CmpOp::Eq => left == right,
        CmpOp::Ne => left != right,
        CmpOp::Lt => left < right,
        CmpOp::Le => left <= right,
        CmpOp::Gt => left > right,
        CmpOp::Ge => left >= right,
    }
}

fn values_equal(left: &str, right: &str) -> bool {
    if left.eq_ignore_ascii_case("NULL") && right.eq_ignore_ascii_case("NULL") {
        return true;
    }
    if let (Ok(a), Ok(b)) = (left.parse::<f64>(), right.parse::<f64>()) {
        return (a - b).abs() < f64::EPSILON;
    }
    left == right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_shorthand_level_error() {
        let q = parse_find_pattern("level:error").unwrap();
        match q.pred.unwrap() {
            SearchPred::Cmp { col, op, lit } => {
                assert_eq!(col, "level");
                assert_eq!(op, CmpOp::Eq);
                assert_eq!(lit, "error");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn find_sql_where_and() {
        let q = parse_find_pattern("status = 'ok' AND id > 1").unwrap();
        assert!(matches!(q.pred.unwrap(), SearchPred::And(_, _)));
    }

    #[test]
    fn find_substring() {
        let q = parse_find_pattern("hello world").unwrap();
        assert_eq!(q.substring.as_deref(), Some("hello world"));
        assert!(q.pred.is_none());
    }

    #[test]
    fn find_rejects_select() {
        let err = parse_find_pattern("SELECT * FROM users").unwrap_err();
        assert!(err.to_string().contains("--query") || err.to_string().contains("-q"));
    }

    #[test]
    fn eval_and_like() {
        let pred = parse_sql_where("status = 'ok' AND email LIKE '%example.com'").unwrap();
        let row = vec![
            ("status".into(), "ok".into()),
            ("email".into(), "a@example.com".into()),
        ];
        assert!(eval_pred(&pred, &row));
    }
}
