mod common;

use common::{line_json, read_until, send, wait, wait_for, App};
use serde_json::{json, Value};
use std::time::Duration;

#[test]
fn a_pending_hold_survives_a_daemon_crash_and_applies_when_the_turn_ends() {
    let sid = "dddddddd-0000-4000-8000-000000000009";
    let mut app = App::with_env("holdcrash", json!({}), &[("SB_STUB_BOOT", "3")]);
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["claude", "--session-id", sid], "cols": 100, "rows": 30}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid && l["local"]["pid"].is_u64())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    assert_eq!(app.ok("POST", &hold, Value::Null)["action"], "hold-pending");
    wait(|| line_json(&app, &line)["local"]["holdPending"] == true);
    std::thread::sleep(Duration::from_secs(3));
    drop(ws);
    app.crash_and_restart();
    wait(|| line_json(&app, &line)["local"]["pid"].is_u64());
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    wait(|| line_json(&app, &line)["local"]["holdPending"] != true);
}

#[test]
fn a_profile_chip_survives_a_daemon_crash() {
    let mut app = App::new("chipcrash", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "claude", "profile": "economy", "cols": 100, "rows": 30}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let chip = json!({"id": "economy", "name": "Economy", "model": "sonnet", "effort": "low"});
    assert_eq!(t["profile"], chip);
    wait(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["local"]["termId"] == tid.as_str() && l["local"]["pid"].is_u64())
    });
    std::thread::sleep(Duration::from_secs(3));
    app.crash_and_restart();
    let restored = wait_for(|| {
        let (code, v) = app.req("GET", &format!("/v1/terms/{tid}"), Value::Null);
        (code == 200).then_some(v)
    });
    assert_eq!(restored["profile"], chip);
}
