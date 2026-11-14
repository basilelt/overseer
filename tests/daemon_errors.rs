mod common;

use common::{wait, wait_for, App};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn golden() -> Value {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/daemon_errors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn expect(app: &App, key: &str, id: Option<&str>, method: &str, path: &str, body: Value) {
    let g = &golden()[key];
    let mut message = g["message"].as_str().unwrap().to_string();
    if let Some(id) = id {
        message = message.replace("{id}", id);
    }
    let (code, v) = app.req(method, path, body);
    assert_eq!(
        (code as u64, v["error"].as_str().unwrap_or("")),
        (g["status"].as_u64().unwrap(), message.as_str()),
        "{key}: {method} {path} -> {v}"
    );
}

#[test]
fn every_error_the_mock_copies_is_what_the_daemon_sends() {
    let app = App::new("daemon-errors", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut check = |key: &str, id: Option<&str>, method: &str, path: &str, body: Value| {
        expect(&app, key, id, method, path, body);
        covered.insert(key.to_string());
    };

    check(
        "noTerm",
        Some("nope"),
        "POST",
        "/v1/terms/nope/kill",
        json!({"confirm": true}),
    );
    check(
        "noTerm",
        Some("nope"),
        "DELETE",
        "/v1/terms/nope",
        Value::Null,
    );

    let id = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/cat"], "cols": 80, "rows": 24}),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    let term = format!("/v1/terms/{id}");
    check("termStillRunning", None, "DELETE", &term, Value::Null);
    check(
        "killNeedsConfirm",
        None,
        "POST",
        &format!("{term}/kill"),
        json!({}),
    );
    let resize = format!("{term}/resize");
    check("termSizeRequired", None, "POST", &resize, json!({}));
    check(
        "termSize",
        None,
        "POST",
        &resize,
        json!({"cols": 0, "rows": 20}),
    );
    check("termSize", None, "POST", &resize, json!({"cols": 80}));

    app.ok("POST", &format!("{term}/kill"), json!({"confirm": true}));
    wait(|| app.ok("GET", &term, Value::Null)["exited"] == true);
    check(
        "termExited",
        None,
        "POST",
        &format!("{term}/kill"),
        json!({"confirm": true}),
    );
    check(
        "termExited",
        None,
        "POST",
        &resize,
        json!({"cols": 100, "rows": 30}),
    );

    let quorum = |agent: &str, workers: Value| {
        json!({"title": "Q", "project": wt, "agent": agent,
               "quorum": {"workers": workers, "reviewers": ["codex"]}})
    };
    check(
        "shellInQuorum",
        None,
        "POST",
        "/v1/tasks",
        quorum("claude", json!(["shell"])),
    );
    check(
        "quorumOrchestrator",
        None,
        "POST",
        "/v1/tasks",
        quorum("gemini", json!(["claude"])),
    );

    let sid = "eeeeeeee-0000-4000-8000-000000000002";
    let ts = overseer::util::format_rfc3339(overseer::util::now_ms() - 60_000);
    std::fs::write(
        app.fx
            .base
            .join(format!("home/.claude/projects/p/{sid}.jsonl")),
        format!(
            "{}\n",
            json!({"type": "user", "sessionId": sid, "cwd": wt, "timestamp": ts,
                   "message": {"role": "user", "content": "Fix the flaky login test please"}})
        ),
    )
    .unwrap();
    let task = wait_for(|| {
        app.ok("GET", "/v1/tasks", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["origin"] == "session")
            .cloned()
    });
    check(
        "sessionTaskStatus",
        None,
        "PATCH",
        &format!("/v1/tasks/{}", task["id"].as_str().unwrap()),
        json!({"status": "pending"}),
    );

    check(
        "bodyTooLarge",
        None,
        "POST",
        "/v1/tasks",
        json!({"title": "x".repeat(1 << 20)}),
    );
    let exact = format!("{{\"title\":\"{}\"}}", "x".repeat((1 << 20) - 12));
    assert_eq!(exact.len(), 1 << 20);
    let (code, _) = app.fx.req("POST", "/v1/tasks", &exact);
    assert_ne!(code, 413, "a body of exactly 1 MiB is still read");

    let keys: BTreeSet<String> = golden().as_object().unwrap().keys().cloned().collect();
    assert_eq!(covered, keys, "every golden entry is triggered over HTTP");
}
