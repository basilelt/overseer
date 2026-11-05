mod common;
mod fakes;

use common::{recv, wait_for, Api, ADMIN};
use fakes::schema::check;
use fakes::twinfactory::{verify, TwinFactory};
use overseer::sync::{Transport, Ws};
use serde_json::{json, Value};
use std::path::Path;

fn line(id: &str, state: &str, ticket: &str) -> Value {
    json!({
        "id": id, "owner": "feedback-bot", "stationId": "runner-1", "agent": "claude",
        "agentSessionId": format!("sess-{id}"), "repo": "github.com/acme/live",
        "branch": format!("feedback/{ticket}"), "ticketKey": ticket, "worktreePath": "/w/x",
        "origin": "feedback", "state": state, "lastActivityAt": "2026-10-28T10:00:00Z",
        "envRef": "pr-12",
    })
}

#[test]
fn twinfactory_reads_and_events_match_the_contract() {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("tf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let api = Api::start(&base);
    let token = api
        .http(
            "POST",
            "/v1/stations",
            ADMIN,
            Some(json!({"stationId": "runner-1", "kind": "runner"})),
        )
        .1["token"]
        .as_str()
        .unwrap()
        .to_string();
    let tf = TwinFactory::new(&api.addr, &api.user("twinfactory"));
    let hook = tf.webhook_receiver();
    let (code, w) = api.http(
        "POST",
        "/v1/webhooks",
        ADMIN,
        Some(
            json!({"url": hook, "types": ["line.created", "line.state_changed", "station.online"]}),
        ),
    );
    assert_eq!(code, 201, "{w}");
    *tf.secret.lock().unwrap() = w["secret"].as_str().unwrap().to_string();
    tf.subscribe();

    let mut ws = Ws::connect(&api.url(), "runner-1", &token).unwrap();
    ws.send(
        &json!({"kind": "hello", "stationId": "runner-1", "daemonVersion": "t", "lastAckedSeq": 0}),
    )
    .unwrap();
    recv(&mut ws, "ack");
    ws.send(&json!({"kind": "snapshot", "seq": 1, "lines": [line("runner-1-A", "active", "AS-321"), line("runner-1-B", "idle", "AS-9")]}))
        .unwrap();
    recv(&mut ws, "ack");
    ws.send(&json!({"kind": "delta", "seq": 2, "upsert": [line("runner-1-A", "idle", "AS-321")]}))
        .unwrap();
    recv(&mut ws, "ack");
    ws.send(&json!({"kind": "heartbeat", "seq": 2, "activeCount": 0}))
        .unwrap();
    let st = recv(&mut ws, "stations");
    check("sync-message.schema.json", &st);

    for q in ["ticketKey", "ticket"] {
        let (code, rows) = tf.get(&format!("/v1/lines?{q}=AS-321"));
        assert_eq!(code, 200);
        let rows = rows.as_array().unwrap().clone();
        assert_eq!(rows.len(), 1, "{q}");
        assert_eq!(rows[0]["line"]["id"], "runner-1-A");
        assert_eq!(rows[0]["line"]["state"], "idle");
        assert_eq!(rows[0]["stationOnline"], true);
        check("line.schema.json", &rows[0]["line"]);
        assert!(overseer::util::parse_rfc3339(rows[0]["syncedAt"].as_str().unwrap()).is_some());
    }
    let (_, stations) = tf.get("/v1/stations");
    assert_eq!(stations[0]["stationId"], "runner-1");
    assert_eq!(stations[0]["online"], true);

    let events = wait_for(|| {
        let e = tf.events.lock().unwrap().clone();
        e.iter()
            .any(|e| e["type"] == "line.state_changed")
            .then_some(e)
    });
    for e in &events {
        check("event.schema.json", e);
    }
    let changed = events
        .iter()
        .find(|e| e["type"] == "line.state_changed")
        .unwrap();
    assert_eq!(
        (
            changed["data"]["from"].as_str(),
            changed["data"]["to"].as_str()
        ),
        (Some("active"), Some("idle"))
    );

    let got = wait_for(|| {
        let d = tf.deliveries.lock().unwrap().clone();
        (d.iter()
            .filter(|(_, e)| e["type"] == "line.created")
            .count()
            == 2
            && d.iter().any(|(_, e)| e["type"] == "line.state_changed"))
        .then_some(d)
    });
    for (ok, e) in &got {
        assert!(ok, "bad signature for {e}");
        check("event.schema.json", e);
    }
    let secret = tf.secret.lock().unwrap().clone();
    assert!(!verify(&secret, "t=1,v1=00", "{}", 1));
    assert!(!verify("other", "t=1,v1=00", "{}", 1));
}
