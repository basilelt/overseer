use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

pub struct FeedbackHub {
    pub addr: String,
    pub token: String,
}

pub const TERMINAL: [&str; 3] = ["succeeded", "refused", "failed"];

impl FeedbackHub {
    pub fn http(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let mut s = TcpStream::connect(&self.addr).unwrap();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.token,
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = out.split_once("\r\n\r\n").unwrap().1;
        (code, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    pub fn open_line(&self, key: &str, ticket: &str, prompt: &str, dry_run: Option<bool>) -> Value {
        let mut b = json!({
            "owner": "feedback-bot", "repo": "github.com/acme/live",
            "branch": format!("feedback/{ticket}"), "agent": "claude", "origin": "feedback",
            "ticketKey": ticket, "prompt": prompt, "idempotencyKey": key,
        });
        if let Some(d) = dry_run {
            b["dryRun"] = json!(d);
        }
        let (code, v) = self.http("POST", "/v1/lines", Some(b));
        assert_eq!(code, 202, "{v}");
        v
    }

    pub fn relaunch(&self, line: &str, id: &str, message: &str, dry_run: bool) -> Value {
        let (code, v) = self.http(
            "POST",
            &format!("/v1/lines/{line}/commands"),
            Some(json!({"id": id, "kind": "relaunch", "args": {"message": message, "dryRun": dry_run}})),
        );
        assert_eq!(code, 202, "{v}");
        self.outcome(line, id)
    }

    pub fn outcome(&self, line: &str, id: &str) -> Value {
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let (_, v) = self.http("GET", &format!("/v1/lines/{line}/commands"), None);
            if let Some(c) =
                v.as_array().into_iter().flatten().find(|c| {
                    c["id"] == id && TERMINAL.contains(&c["status"].as_str().unwrap_or(""))
                })
            {
                return c.clone();
            }
            assert!(Instant::now() < end, "no outcome for {id}: {v}");
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}
