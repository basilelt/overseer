#![cfg(target_os = "linux")]
mod common;

use common::{wait_for, App};
use serde_json::{json, Value};
use std::io::Write;

fn assistant(sid: &str, id: &str, model: &str, ts: &str, out: u64) -> String {
    json!({"type": "assistant", "sessionId": sid, "timestamp": ts, "message": {"id": id, "model": model,
        "content": [{"type": "text", "text": "done"}],
        "usage": {"input_tokens": 10, "cache_creation_input_tokens": 1000, "cache_read_input_tokens": 50_000,
                  "output_tokens": out, "cache_creation": {"ephemeral_1h_input_tokens": 0}}}})
    .to_string()
}

#[test]
fn a_session_cost_reaches_its_activity_and_its_task() {
    let app = App::new("cost", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let sid = "ffffffff-0000-4000-8000-000000000001";
    let ts = overseer::util::format_rfc3339(overseer::util::now_ms() - 60_000);
    let dir = app.fx.base.join("home/.claude/projects/p");
    let path = dir.join(format!("{sid}.jsonl"));
    let lines = [
        json!({"type": "user", "sessionId": sid, "cwd": wt, "timestamp": ts,
               "message": {"role": "user", "content": "Price the session"}})
        .to_string(),
        assistant(sid, "m1", "claude-opus-5-5", &ts, 100),
        assistant(sid, "m1", "claude-opus-5-5", &ts, 100),
    ];
    std::fs::write(&path, lines.join("\n") + "\n").unwrap();
    let subs = dir.join(sid).join("subagents");
    std::fs::create_dir_all(&subs).unwrap();
    std::fs::write(
        subs.join("agent-x.jsonl"),
        assistant(sid, "s1", "claude-haiku-5-5", &ts, 100) + "\n",
    )
    .unwrap();

    let task = wait_for(|| {
        app.ok("GET", "/v1/tasks", Value::Null)
            .as_array()?
            .iter()
            .find(|t| t["origin"] == "session" && t["cost"].is_object())
            .cloned()
    });
    let line = task["lineId"].as_str().unwrap().to_string();
    let opus = (10.0 * 4.0 + 1000.0 * 5.0 + 50_000.0 * 0.2 + 100.0 * 20.0) / 1e6;
    let haiku = (10.0 * 0.1 + 1000.0 * 0.125 + 50_000.0 * 0.01 + 100.0 * 0.5) / 1e6;
    let c = &task["cost"];
    assert_eq!(c["input"], 20);
    assert_eq!(c["cacheWrite"], 2000);
    assert_eq!(c["cacheRead"], 100_000);
    assert_eq!(c["output"], 200);
    assert_eq!(c["model"], "claude-opus-5-5");
    assert_eq!(c["sessions"], 1);
    assert!((c["cost"].as_f64().unwrap() - (opus + haiku)).abs() < 1e-4);

    let a = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
    let cost = &a["cost"];
    assert_eq!(cost["model"], "claude-opus-5-5");
    assert_eq!(cost["partial"], false);
    assert_eq!(cost["models"][0]["model"], "claude-opus-5-5");
    assert_eq!(cost["models"][1]["model"], "claude-haiku-5-5");
    assert_eq!(cost["switches"], json!([]));
    assert!((cost["cost"].as_f64().unwrap() - (opus + haiku)).abs() < 1e-4);

    let later = overseer::util::format_rfc3339(overseer::util::now_ms());
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        f,
        "{}",
        assistant(sid, "m2", "claude-sonnet-5-5", &later, 100)
    )
    .unwrap();
    drop(f);
    let a = wait_for(|| {
        let a = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
        (a["cost"]["switches"].as_array()?.len() == 1).then_some(a)
    });
    assert_eq!(a["cost"]["switches"][0]["from"], "claude-opus-5-5");
    assert_eq!(a["cost"]["switches"][0]["to"], "claude-sonnet-5-5");
    let id = task["id"].as_str().unwrap();
    let t = wait_for(|| {
        let t = app.ok("GET", &format!("/v1/tasks/{id}"), Value::Null);
        (t["cost"]["model"] == "claude-sonnet-5-5").then_some(t)
    });
    assert_eq!(t["cost"]["output"], 300);
}

#[test]
fn a_session_without_token_counts_has_no_cost() {
    let app = App::new("cost-none", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let sid = "ffffffff-0000-4000-8000-000000000002";
    let ts = overseer::util::format_rfc3339(overseer::util::now_ms() - 60_000);
    std::fs::write(
        app.fx
            .base
            .join(format!("home/.claude/projects/p/{sid}.jsonl")),
        json!({"type": "user", "sessionId": sid, "cwd": wt, "timestamp": ts,
               "message": {"role": "user", "content": "No usage here"}})
        .to_string()
            + "\n",
    )
    .unwrap();
    let task = wait_for(|| {
        app.ok("GET", "/v1/tasks", Value::Null)
            .as_array()?
            .iter()
            .find(|t| t["origin"] == "session")
            .cloned()
    });
    let line = task["lineId"].as_str().unwrap();
    let a = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
    assert_eq!(a["transcript"], true);
    assert!(a.get("cost").is_none());
    assert!(task.get("cost").is_none());
}
