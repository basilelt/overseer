use serde_json::Value;
use std::path::Path;

pub fn load(name: &str) -> Value {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("contract/schemas")
        .join(name);
    serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap()
}

pub fn check(name: &str, v: &Value) {
    let mut errs = vec![];
    validate(&load(name), v, "$", &mut errs);
    assert!(errs.is_empty(), "{name}: {errs:?}\n{v:#}");
}

pub fn valid(schema: &Value, v: &Value) -> bool {
    let mut errs = vec![];
    validate(schema, v, "$", &mut errs);
    errs.is_empty()
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        _ => panic!("unsupported type {t}"),
    }
}

fn pattern_ok(p: &str, s: &str) -> bool {
    match p {
        "^AS-[0-9]+$" => s
            .strip_prefix("AS-")
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())),
        _ => panic!("unsupported pattern {p}"),
    }
}

fn validate(s: &Value, v: &Value, at: &str, errs: &mut Vec<String>) {
    let mut fail = |m: String| errs.push(format!("{at}: {m}"));
    if let Some(r) = s["$ref"].as_str() {
        let sub = load(r);
        return validate(&sub, v, at, errs);
    }
    if let Some(t) = s["type"].as_str() {
        if !type_ok(t, v) {
            return fail(format!("expected {t}, got {v}"));
        }
    }
    if let Some(e) = s["enum"].as_array() {
        if !e.contains(v) {
            fail(format!("{v} not in {e:?}"));
        }
    }
    if let Some(c) = s.get("const") {
        if c != v {
            fail(format!("{v} != {c}"));
        }
    }
    if let (Some(n), Some(x)) = (s["minimum"].as_f64(), v.as_f64()) {
        if x < n {
            fail(format!("{x} < {n}"));
        }
    }
    if let Some(text) = v.as_str() {
        if let Some(n) = s["minLength"].as_u64() {
            if (text.chars().count() as u64) < n {
                fail(format!("shorter than {n}"));
            }
        }
        if let Some(p) = s["pattern"].as_str() {
            if !pattern_ok(p, text) {
                fail(format!("'{text}' does not match {p}"));
            }
        }
        match s["format"].as_str() {
            Some("date-time") if overseer::util::parse_rfc3339(text).is_none() => {
                fail(format!("'{text}' is not a date-time"))
            }
            Some("uri") if !text.contains("://") => fail(format!("'{text}' is not a uri")),
            _ => {}
        }
    }
    if let Some(o) = v.as_object() {
        for r in s["required"].as_array().into_iter().flatten() {
            let k = r.as_str().unwrap();
            if !o.contains_key(k) {
                errs.push(format!("{at}: missing {k}"));
            }
        }
        let props = s["properties"].as_object();
        for (k, x) in o {
            match props.and_then(|p| p.get(k)) {
                Some(ps) => validate(ps, x, &format!("{at}.{k}"), errs),
                None if s["additionalProperties"] == false => {
                    errs.push(format!("{at}: unexpected property {k}"))
                }
                None => {}
            }
        }
    }
    if let (Some(items), Some(a)) = (s.get("items"), v.as_array()) {
        for (i, x) in a.iter().enumerate() {
            validate(items, x, &format!("{at}[{i}]"), errs);
        }
    }
    for sub in s["allOf"].as_array().into_iter().flatten() {
        match (sub.get("if"), sub.get("then")) {
            (Some(c), Some(t)) => {
                if valid(c, v) {
                    validate(t, v, at, errs);
                }
            }
            _ => validate(sub, v, at, errs),
        }
    }
    if let Some(one) = s["oneOf"].as_array() {
        let n = one.iter().filter(|sub| valid(sub, v)).count();
        if n != 1 {
            errs.push(format!("{at}: matches {n} of oneOf"));
        }
    }
}

#[test]
fn validator_rejects_bad_shapes() {
    let line = serde_json::json!({
        "id": "x", "owner": "o", "stationId": "s", "agent": "claude", "agentSessionId": "a",
        "repo": "r", "branch": "b", "worktreePath": "/w", "origin": "dev", "state": "idle",
        "lastActivityAt": "2026-10-28T10:00:00Z",
    });
    assert!(valid(&load("line.schema.json"), &line));
    for (k, bad) in [
        ("state", "sleeping"),
        ("ticketKey", "as-1"),
        ("lastActivityAt", "yesterday"),
        ("extra", "x"),
    ] {
        let mut l = line.clone();
        l[k] = serde_json::json!(bad);
        assert!(!valid(&load("line.schema.json"), &l), "{k}");
    }
}
