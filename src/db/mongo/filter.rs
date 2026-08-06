//! Match MongoDB filter documents against BSON docs (V1 subset).

use bson::{Bson, Document};
use serde_json::Value;

pub fn doc_matches_filter(doc: &Document, filter: &Value) -> bool {
    if filter.is_null() || filter.as_object().is_some_and(|o| o.is_empty()) {
        return true;
    }
    let Some(obj) = filter.as_object() else {
        return false;
    };
    for (key, cond) in obj {
        if key.starts_with('$') {
            if !match_top_operator(doc, key, cond) {
                return false;
            }
            continue;
        }
        let field = doc_get(doc, key);
        if !value_matches(&field, cond) {
            return false;
        }
    }
    true
}

fn match_top_operator(doc: &Document, op: &str, val: &Value) -> bool {
    match op {
        "$and" => val
            .as_array()
            .is_some_and(|a| a.iter().all(|f| doc_matches_filter(doc, f))),
        "$or" => val
            .as_array()
            .is_some_and(|a| a.iter().any(|f| doc_matches_filter(doc, f))),
        _ => false,
    }
}

fn value_matches(field: &Bson, cond: &Value) -> bool {
    if let Some(obj) = cond.as_object() {
        if obj.is_empty() {
            return !field_is_null(field);
        }
        for (op, val) in obj {
            if !op_matches(field, op, val) {
                return false;
            }
        }
        return true;
    }
    json_eq(field, cond)
}

fn op_matches(field: &Bson, op: &str, val: &Value) -> bool {
    match op {
        "$eq" => json_eq(field, val),
        "$ne" => !json_eq(field, val),
        "$gt" => compare(field, val) == std::cmp::Ordering::Greater,
        "$gte" => compare(field, val) != std::cmp::Ordering::Less,
        "$lt" => compare(field, val) == std::cmp::Ordering::Less,
        "$lte" => compare(field, val) != std::cmp::Ordering::Greater,
        "$in" => val
            .as_array()
            .is_some_and(|a| a.iter().any(|v| json_eq(field, v))),
        "$exists" => {
            let want = val.as_bool().unwrap_or(true);
            field_is_null(field) != want
        }
        "$regex" => {
            let s = bson_as_str(field);
            val.as_str()
                .is_some_and(|pat| s.contains(pat) || glob_simple(pat, &s))
        }
        _ => false,
    }
}

fn doc_get(doc: &Document, path: &str) -> Bson {
    let mut cur = Bson::Document(doc.clone());
    for part in path.split('.') {
        cur = match cur {
            Bson::Document(ref d) => d.get(part).cloned().unwrap_or(Bson::Null),
            _ => Bson::Null,
        };
    }
    cur
}

fn field_is_null(b: &Bson) -> bool {
    matches!(b, Bson::Null)
}

fn json_eq(field: &Bson, val: &Value) -> bool {
    match (field, val) {
        (Bson::Null, Value::Null) => true,
        (Bson::Boolean(a), Value::Bool(b)) => a == b,
        (Bson::Int32(a), Value::Number(n)) => n.as_i64() == Some(*a as i64),
        (Bson::Int64(a), Value::Number(n)) => n.as_i64() == Some(*a),
        (Bson::Double(a), Value::Number(n)) => n.as_f64().map(|f| (f - a).abs() < f64::EPSILON).unwrap_or(false),
        (Bson::String(a), Value::String(b)) => a == b,
        (Bson::ObjectId(oid), Value::String(s)) => oid.to_hex() == *s,
        _ => false,
    }
}

fn compare(field: &Bson, val: &Value) -> std::cmp::Ordering {
    match (field, val) {
        (Bson::Int32(a), Value::Number(n)) if let Some(b) = n.as_i64() => (*a as i64).cmp(&b),
        (Bson::Int64(a), Value::Number(n)) if let Some(b) = n.as_i64() => a.cmp(&b),
        (Bson::Double(a), Value::Number(n)) if let Some(b) = n.as_f64() => {
            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Bson::String(a), Value::String(b)) => a.cmp(b),
        _ => std::cmp::Ordering::Equal,
    }
}

fn bson_as_str(b: &Bson) -> String {
    match b {
        Bson::String(s) => s.clone(),
        Bson::ObjectId(oid) => oid.to_hex(),
        other => format!("{other}"),
    }
}

fn glob_simple(pat: &str, text: &str) -> bool {
    if let Some(star) = pat.find('*') {
        let (pre, rest) = pat.split_at(star);
        let suf = &rest[1..];
        return text.starts_with(pre) && text.ends_with(suf);
    }
    text.contains(pat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eq_filter() {
        let doc = bson::doc! {"status": "failed", "id": 1i32};
        let f = serde_json::json!({"status": "failed"});
        assert!(doc_matches_filter(&doc, &f));
    }
}
