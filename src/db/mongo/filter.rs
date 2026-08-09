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
        let field = doc_get_opt(doc, key);
        if !value_matches(field.as_ref(), cond) {
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

fn value_matches(field: Option<&Bson>, cond: &Value) -> bool {
    if let Some(obj) = cond.as_object() {
        if obj.is_empty() {
            return field.is_some_and(|f| !field_is_null(f));
        }
        for (op, val) in obj {
            if !op_matches(field, op, val) {
                return false;
            }
        }
        return true;
    }
    match field {
        Some(f) => json_eq(f, cond),
        None => cond.is_null(),
    }
}

fn op_matches(field: Option<&Bson>, op: &str, val: &Value) -> bool {
    match op {
        "$eq" => field.is_some_and(|f| json_eq(f, val)),
        "$ne" => !field.is_some_and(|f| json_eq(f, val)),
        "$gt" => matches!(compare(field, val), Some(std::cmp::Ordering::Greater)),
        "$gte" => matches!(
            compare(field, val),
            Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
        ),
        "$lt" => matches!(compare(field, val), Some(std::cmp::Ordering::Less)),
        "$lte" => matches!(
            compare(field, val),
            Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
        ),
        "$in" => val
            .as_array()
            .is_some_and(|a| field.is_some_and(|f| a.iter().any(|v| json_eq(f, v)))),
        "$exists" => {
            let want = val.as_bool().unwrap_or(true);
            field.is_some() == want
        }
        "$regex" => {
            let Some(f) = field else {
                return false;
            };
            let s = bson_as_str(f);
            val.as_str()
                .is_some_and(|pat| s.contains(pat) || glob_simple(pat, &s))
        }
        _ => false,
    }
}

fn doc_get_opt(doc: &Document, path: &str) -> Option<Bson> {
    let mut cur = Bson::Document(doc.clone());
    for part in path.split('.') {
        cur = match cur {
            Bson::Document(ref d) => d.get(part).cloned()?,
            _ => return None,
        };
    }
    Some(cur)
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
        (Bson::Double(a), Value::Number(n)) => n
            .as_f64()
            .map(|f| (f - a).abs() < f64::EPSILON)
            .unwrap_or(false),
        (Bson::String(a), Value::String(b)) => a == b,
        (Bson::ObjectId(oid), Value::String(s)) => oid.to_hex() == *s,
        _ => false,
    }
}

fn compare(field: Option<&Bson>, val: &Value) -> Option<std::cmp::Ordering> {
    let field = field?;
    match (field, val) {
        (Bson::Int32(a), Value::Number(n)) => Some((*a as i64).cmp(&n.as_i64()?)),
        (Bson::Int64(a), Value::Number(n)) => Some(a.cmp(&n.as_i64()?)),
        (Bson::Double(a), Value::Number(n)) => a.partial_cmp(&n.as_f64()?),
        (Bson::String(a), Value::String(b)) => Some(a.cmp(b)),
        _ => None,
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

    #[test]
    fn exists_true_matches_present_null() {
        let doc = bson::doc! {"a": Bson::Null};
        let f = serde_json::json!({"a": {"$exists": true}});
        assert!(doc_matches_filter(&doc, &f));
    }

    #[test]
    fn range_rejects_incomparable_types() {
        let doc = bson::doc! {"n": 1i32};
        let f = serde_json::json!({"n": {"$gt": "x"}});
        assert!(!doc_matches_filter(&doc, &f));
    }
}
