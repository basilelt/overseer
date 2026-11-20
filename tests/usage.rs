#![cfg(target_os = "linux")]
mod common;

use common::{wait_for, App};
use serde_json::{json, Value};

#[test]
fn codex_plan_spent_on_credits_reaches_the_app() {
    let app = App::new("usage-credits", json!({}));
    let dir = app.fx.base.join("home/.codex/sessions/2026/11/09");
    std::fs::create_dir_all(&dir).unwrap();
    let line = json!({
        "timestamp": "2026-11-09T08:45:34.126Z",
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": {"total_token_usage": {"input_tokens": 10, "output_tokens": 2}},
            "rate_limits": {
                "limit_id": "codex",
                "limit_name": null,
                "primary": {"used_percent": 100.0, "window_minutes": 43200, "resets_at": 4_000_000_000_i64},
                "secondary": null,
                "credits": {"has_credits": true, "unlimited": false, "balance": "1272.1821530000"},
                "individual_limit": null,
                "spend_control_reached": null,
                "plan_type": "go",
                "rate_limit_reached_type": null
            }
        }
    });
    std::fs::write(
        dir.join("rollout-2026-11-09T08-43-52-01a1bf7a-68b2-71c2-bc92-0e5477d8ed37.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();

    let limits = wait_for(|| {
        let (code, v) = app.req(
            "GET",
            "/v1/usage?range=today&by=agent&tzOffset=60",
            Value::Null,
        );
        if code != 200 {
            return None;
        }
        v["agents"]
            .as_array()?
            .iter()
            .find(|a| a["agent"] == "codex" && a["limits"]["source"] == "codex")
            .map(|a| a["limits"].clone())
    });
    assert_eq!(limits["plan"], "go");
    assert_eq!(limits["windows"][0]["key"], "mo");
    assert_eq!(limits["windows"][0]["usedPercent"], 100.0);
    assert_eq!(limits["windows"][0]["stale"], false);
    assert_eq!(
        limits["credits"],
        json!({"hasCredits": true, "unlimited": false, "balance": 1272.182153})
    );
    assert_eq!(limits["reached"], Value::Null);
    assert_eq!(limits["hit"], Value::Null);
}

#[test]
fn usage_lists_every_supported_cli() {
    let app = App::new("usage-all-agents", json!({}));
    let v = wait_for(|| {
        let (code, v) = app.req(
            "GET",
            "/v1/usage?range=today&by=agent&tzOffset=0",
            Value::Null,
        );
        (code == 200).then_some(v)
    });
    let ids: Vec<&str> = v["agents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["agent"].as_str().unwrap())
        .collect();
    // Every CLI Overseer supports gets an entry, the shell does not, sorted by id.
    assert_eq!(
        ids,
        ["agy", "amp", "claude", "codex", "copilot", "cursor", "gemini", "opencode", "vibe"]
    );
    for a in v["agents"].as_array().unwrap() {
        assert!(a["today"].is_object(), "{a} has no today totals");
        assert!(a["limits"].is_object(), "{a} has no limits");
    }
    let vibe = v["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["agent"] == "vibe")
        .unwrap();
    assert_eq!(vibe["today"]["total"], 0);
    assert!(vibe["limits"]["source"].is_null());
}
