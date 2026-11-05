mod common;

use common::{bin, config, err, out, recv, sb_stdin, wait_for, Api, Reap, SID, T};
use overseer::sync::{Transport, Ws};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;

const REMOTE_ID: &str = "vm2-01JREMOTE00000000000000AB";

fn remote_station(api: &Api, token: &str) -> Ws {
    let mut ws = Ws::connect(&api.url(), "vm2", token).unwrap();
    ws.send(
        &json!({"kind": "hello", "stationId": "vm2", "daemonVersion": "test", "lastAckedSeq": 0}),
    )
    .unwrap();
    recv(&mut ws, "ack");
    let line = json!({
        "id": REMOTE_ID, "owner": "dev@example.com", "stationId": "vm2", "agent": "claude",
        "agentSessionId": "eeeeeeee-0000-4000-8000-000000000009", "repo": "github.com/acme/live",
        "branch": "feature/AS-77-remote", "ticketKey": "AS-77", "worktreePath": "/w/live",
        "origin": "dev", "state": "idle", "lastActivityAt": "2099-01-01T00:00:00Z",
    });
    ws.send(&json!({"kind": "snapshot", "seq": 1, "lines": [line]}))
        .unwrap();
    recv(&mut ws, "ack");
    ws
}

fn synced_station(t: &T, api: &Api, accept: bool) -> (String, String, Reap) {
    config(
        t,
        json!({"stationId": "st1", "owner": "dev@example.com", "acceptRemoteCommands": accept}),
    );
    let station_token = api.enrol("st1");
    let user = api.user("bob");
    let o = sb_stdin(t, &["enroll", "--api", &api.url()], &station_token);
    assert!(o.status.success(), "{}", err(&o));
    let mut c = Command::new(bin());
    t.env(&mut c);
    let sync = Reap::spawn(c.arg("sync").stdout(Stdio::null()).stderr(Stdio::null()));
    let id = wait_for(|| {
        let (_, v) = api.http("GET", "/v1/lines?stationId=st1", &user, None);
        v.as_array()?
            .iter()
            .find(|r| r["line"]["agentSessionId"] == SID && r["stationOnline"] == true)
            .map(|r| r["line"]["id"].as_str().unwrap().to_string())
    });
    (id, user, sync)
}

fn relay(api: &Api, user: &str, line: &str, cmd: &str, args: Value) -> Value {
    let (code, v) = api.http(
        "POST",
        &format!("/v1/lines/{line}/commands"),
        user,
        Some(json!({"id": cmd, "kind": "hold", "args": args})),
    );
    assert_eq!(code, 202, "{v}");
    wait_for(|| {
        let (_, v) = api.http("GET", &format!("/v1/lines/{line}/commands"), user, None);
        v.as_array()?
            .iter()
            .find(|c| {
                c["id"] == cmd
                    && ["succeeded", "refused", "failed"]
                        .contains(&c["status"].as_str().unwrap_or(""))
            })
            .cloned()
    })
}

#[test]
fn relayed_hold_honours_predicates_and_is_audited() {
    let t = T::new();
    let api = Api::start(&t.base);
    let pane = t.start_agent("a", SID, "claude --model opus");
    let (id, user, _sync) = synced_station(&t, &api, true);

    t.keys(&pane, "half typed", false);
    let c = relay(&api, &user, &id, "cmd-hold-0", json!({"dryRun": false}));
    assert_eq!(c["status"], "refused", "{c}");
    assert!(
        c["reason"].as_str().unwrap().starts_with("inputEmpty"),
        "{c}"
    );
    assert_eq!(t.line(SID)["line"]["state"], "active");
    t.tmux(&["send-keys", "-t", &pane, "C-u"]);

    let c = relay(&api, &user, &id, "cmd-hold-d", json!({}));
    assert_eq!(
        (c["status"].as_str(), c["reason"].as_str()),
        (Some("succeeded"), Some("would-hold")),
        "{c}"
    );
    assert!(t.line(SID)["local"]["pid"].is_u64());

    let c = relay(&api, &user, &id, "cmd-hold-1", json!({"dryRun": false}));
    assert_eq!(
        (c["status"].as_str(), c["reason"].as_str()),
        (Some("succeeded"), Some("held")),
        "{c}"
    );
    assert_eq!(c["requestedBy"], "bob");
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        id
    );
    let l = t.line(SID);
    assert_eq!(l["local"]["heldReason"], "user");
    let log = std::fs::read_to_string(t.base.join("state/actions.log")).unwrap();
    let audit: Vec<Value> = log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["action"] == "remote-command")
        .collect();
    assert_eq!(audit.len(), 3, "{log}");
    assert_eq!(audit[2]["commandId"], "cmd-hold-1");
    assert_eq!(audit[2]["requestedBy"], "bob");
    assert!(log.contains(r#""action":"hold""#));
    assert!(std::fs::read_dir(t.base.join("state/commands/inbox"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
fn relayed_commands_are_refused_when_disabled() {
    let t = T::new();
    let api = Api::start(&t.base);
    let pane = t.start_agent("a", SID, "claude");
    let (id, user, _sync) = synced_station(&t, &api, false);
    let c = relay(&api, &user, &id, "cmd-off-1", json!({"dryRun": false}));
    assert_eq!(c["status"], "refused", "{c}");
    assert!(c["reason"].as_str().unwrap().starts_with("disabled"), "{c}");
    assert_eq!(t.line(SID)["line"]["state"], "active");
    assert!(t.line(SID)["local"]["pid"].is_u64());
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        ""
    );
}

#[test]
fn ls_and_resume_across_stations() {
    let t = T::new();
    let api = Api::start(&t.base);
    config(
        &t,
        json!({"stationId": "st1", "owner": "dev@example.com", "ssh": {"vm2": "dev@vm2.internal"}}),
    );
    let vm2 = api.enrol("vm2");
    let user = api.user("bob");
    let ws = remote_station(&api, &vm2);

    let o = sb_stdin(&t, &["login", "--api", &api.url()], "not-a-valid-token\n");
    assert!(!o.status.success());
    assert!(!t.base.join("state/user.json").exists());
    let o = sb_stdin(&t, &["login", "--api", &api.url()], &format!("{user}\n"));
    assert!(o.status.success(), "{}", err(&o));
    let mode = std::fs::metadata(t.base.join("state/user.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    let rows = t.json(&["ls", "--json"]);
    let remote = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["line"]["id"] == REMOTE_ID)
        .expect("remote line listed");
    assert_eq!(remote["stationOnline"], true);
    assert!(!t
        .json(&["ls", "--json", "--local"])
        .to_string()
        .contains(REMOTE_ID));
    let table = out(&t.sb(&["ls", "--ticket", "as-77"]));
    assert!(
        table.contains("vm2") && table.contains(REMOTE_ID),
        "{table}"
    );

    let o = t.sb(&["resume", "01jremote", "--print"]);
    assert_eq!(
        out(&o).trim(),
        format!("ssh -t dev@vm2.internal -- overseer resume {REMOTE_ID} --attach"),
        "{}",
        err(&o)
    );
    let o = t.sb(&["resume", REMOTE_ID]);
    assert!(o.status.success(), "{}", err(&o));
    let o = t.sb(&["open", &format!("overseer://resume/{REMOTE_ID}")]);
    assert!(!o.status.success() && err(&o).contains("--yes"));
    assert!(!t
        .sb(&["open", "overseer://hold/x", "--yes"])
        .status
        .success());
    let o = t.sb(&["open", &format!("overseer://resume/{REMOTE_ID}"), "--yes"]);
    assert!(o.status.success(), "{}", err(&o));
    let log = std::fs::read_to_string(t.base.join("ssh.log")).unwrap();
    let expected = format!("-t dev@vm2.internal -- overseer resume {REMOTE_ID} --attach");
    assert_eq!(log.lines().collect::<Vec<_>>(), [expected.as_str(); 2]);

    config(&t, json!({"stationId": "st1", "owner": "dev@example.com"}));
    let o = t.sb(&["resume", REMOTE_ID, "--print"]);
    assert!(out(&o).starts_with("ssh -t vm2 -- "), "{}", out(&o));
    assert!(!t.sb(&["resume", "zzzz"]).status.success());

    drop(ws);
    api.offset.fetch_add(60_000, Ordering::SeqCst);
    api.app.sweep();
    let o = t.sb(&["resume", REMOTE_ID]);
    assert!(!o.status.success());
    assert!(err(&o).contains("vm2, which is offline"), "{}", err(&o));
    assert_eq!(
        std::fs::read_to_string(t.base.join("ssh.log"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    let table = out(&t.sb(&["ls"]));
    assert!(table.contains("OFFLINE"), "{table}");

    std::fs::write(
        t.base.join("state/user.json"),
        json!({"api": "http://127.0.0.1:9", "token": "x"}).to_string(),
    )
    .unwrap();
    let o = t.sb(&["ls"]);
    assert!(
        o.status.success() && err(&o).contains("api unreachable"),
        "{}",
        err(&o)
    );
}

#[test]
fn resume_attach_targets_the_running_pane() {
    let t = T::new();
    t.start_agent("a", SID, "claude");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    let o = t.sb(&["resume", &id, "--attach"]);
    assert!(o.status.success(), "{}", err(&o));
    let text = out(&o);
    assert!(
        text.contains("(running %") && text.contains("tmux attach -t main"),
        "{text}"
    );
}
