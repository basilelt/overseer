#![cfg(target_os = "linux")]
mod common;

use common::App;
use serde_json::json;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

#[test]
fn system_snapshot_history_agents_and_stream() {
    let app = App::new("system", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh", "-c", "sleep 30"]}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let v = app.ok("GET", "/v1/system", serde_json::Value::Null);
    assert_eq!(v["intervalMs"], 2000);
    assert_eq!(v["windowMs"], 600_000);
    assert_eq!(v["running"], true);
    let cur = &v["current"];
    assert_eq!(cur["supported"], true);
    assert_eq!(cur["platform"], "linux");
    assert!(cur["mem"]["total"].as_u64().unwrap() > 0);
    assert!(!cur["top"]["mem"].as_array().unwrap().is_empty());
    assert!(cur["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["termId"] == id.as_str() && a["procs"].as_u64().unwrap() >= 1));
    let last = v["history"].as_array().unwrap().last().unwrap()["t"]
        .as_i64()
        .unwrap();
    let v = app.ok(
        "GET",
        &format!("/v1/system?since={last}"),
        serde_json::Value::Null,
    );
    assert!(v["history"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["t"].as_i64().unwrap() > last));
    let v = app.ok(
        "GET",
        &format!("/v1/system?since={}", last - 1),
        serde_json::Value::Null,
    );
    let times: Vec<i64> = v["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["t"].as_i64().unwrap())
        .collect();
    assert!(times.contains(&last), "{times:?}");
    assert!(times.iter().all(|t| *t > last - 1), "{times:?}");

    let mut s = UnixStream::connect(&app.fx.sock).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    s.write_all(b"GET /v1/system/stream HTTP/1.1\r\nHost: unix\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    let end = Instant::now() + Duration::from_secs(10);
    while !buf.contains("event: system.sample\ndata: ") && Instant::now() < end {
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
    assert!(buf.starts_with("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream"));
    assert!(buf.contains("event: system.sample\ndata: "), "{buf}");
}
