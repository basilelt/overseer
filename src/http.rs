use serde_json::{json, Value};
use std::io::Write;

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn q<'a>(r: &'a Request, k: &str) -> Option<&'a str> {
    r.query
        .iter()
        .find(|(a, _)| a == k)
        .map(|(_, v)| v.as_str())
}

pub fn header<'a>(r: &'a Request, k: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|(a, _)| a == k)
        .map(|(_, v)| v.as_str())
}

impl Request {
    pub fn param(&self, k: &str) -> Option<String> {
        q(self, k).map(crate::util::percent_decode)
    }

    pub fn is_true(&self, k: &str) -> bool {
        q(self, k) == Some("true")
    }

    pub fn is_one(&self, k: &str) -> bool {
        q(self, k) == Some("1")
    }

    pub fn not_false(&self, k: &str) -> bool {
        q(self, k) != Some("false")
    }
}

pub fn dry_run(r: &Request) -> bool {
    r.not_false("dryRun")
}

pub(crate) fn err(code: u16, msg: impl Into<String>) -> (u16, Value) {
    (code, json!({ "error": msg.into() }))
}

pub(crate) fn res(code: u16, r: Result<Value, (u16, String)>) -> (u16, Value) {
    r.map(|v| (code, v)).unwrap_or_else(|(c, e)| err(c, e))
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    }
}

pub fn respond_raw(s: &mut (impl Write + ?Sized), code: u16, ctype: &str, body: &[u8]) {
    write_response(s, code, ctype, "", body)
}

pub(crate) fn write_response(
    s: &mut (impl Write + ?Sized),
    code: u16,
    ctype: &str,
    extra: &str,
    body: &[u8],
) {
    let _ = write!(
        s,
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{extra}Connection: close\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = s.write_all(body);
}

pub(crate) fn respond_err(s: &mut (impl Write + ?Sized), code: u16, msg: &str) {
    respond(s, code, &json!({ "error": msg }));
}

pub(crate) fn respond(s: &mut (impl Write + ?Sized), code: u16, v: &Value) {
    respond_raw(
        s,
        code,
        "application/json",
        &serde_json::to_vec(v).unwrap_or_default(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_helpers_keep_one_convention_each() {
        let req = |q: &str| Request {
            method: "GET".into(),
            path: "/".into(),
            query: q
                .split('&')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
            headers: vec![],
            body: vec![],
        };
        let r = req("p=a%2Fb+c&t=true&o=1&f=false&x=yes");
        assert_eq!(r.param("p").as_deref(), Some("a/b c"));
        assert_eq!(r.param("missing"), None);
        assert!(r.is_true("t") && !r.is_true("o") && !r.is_true("missing"));
        assert!(r.is_one("o") && !r.is_one("t") && !r.is_one("missing"));
        assert!(!r.not_false("f") && r.not_false("x") && r.not_false("missing"));
        assert!(dry_run(&req("")) && !dry_run(&req("dryRun=false")));
    }
}
