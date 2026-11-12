mod common;

use common::{bin, App};
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

const PATH: &str = "/v1/tasks/agent-tasks";

fn mcp(app: &App, own: Option<&str>, input: &[Value]) -> Vec<Value> {
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    if let Some(o) = own {
        c.env("OVERSEER_TASK_ID", o);
    }
    let mut child = c
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for m in input {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn texts(app: &App) -> (String, String) {
    let out = mcp(
        app,
        None,
        &[
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}),
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        ],
    );
    let instructions = out[0]["result"]["instructions"]
        .as_str()
        .unwrap()
        .to_string();
    let create = out[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "create_task")
        .unwrap()["description"]
        .as_str()
        .unwrap()
        .to_string();
    (instructions, create)
}

#[test]
fn the_switch_is_on_by_default_and_roundtrips() {
    let app = App::new("agent-tasks-switch", json!({"autoConcurrency": 0}));
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"enabled": true}));
    assert_eq!(
        app.ok("PUT", PATH, json!({"enabled": false})),
        json!({"enabled": false})
    );
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"enabled": false}));
    let (code, v) = app.req("PUT", PATH, json!({"enabled": "no"}));
    assert_eq!(
        (code, v["error"].clone()),
        (400, json!("enabled must be a boolean"))
    );
    app.ok("PUT", PATH, json!({"enabled": true}));
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"enabled": true}));
}

#[test]
fn the_switch_survives_a_daemon_restart() {
    let mut app = App::new("agent-tasks-restart", json!({"autoConcurrency": 0}));
    app.ok("PUT", PATH, json!({"enabled": false}));
    app.crash_and_restart();
    assert_eq!(app.ok("GET", PATH, Value::Null), json!({"enabled": false}));
}

#[test]
fn new_agents_are_told_to_file_tasks_or_fix_findings_by_the_switch() {
    let app = App::new("agent-tasks-mcp", json!({"autoConcurrency": 0}));
    let (text, create) = texts(&app);
    assert!(
        text.contains("create_task it (with urgent and important)"),
        "{text}"
    );
    assert!(!create.contains("Do not use it"), "{create}");
    app.ok("PUT", PATH, json!({"enabled": false}));
    let (text, create) = texts(&app);
    assert!(text.contains("fix it yourself in your own task"), "{text}");
    assert!(!text.contains("create_task it"), "{text}");
    assert!(
        create.contains("Do not use it for bugs or improvements"),
        "{create}"
    );
}

#[test]
fn agents_can_still_create_a_task_when_asked_with_the_switch_off() {
    let app = App::new("agent-tasks-create", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let own = app.ok("POST", "/v1/tasks", json!({"title": "Mine", "project": wt}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    app.ok("PUT", PATH, json!({"enabled": false}));
    let out = mcp(
        &app,
        Some(&own),
        &[json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                 "params": {"name": "create_task", "arguments": {"title": "Asked for"}}})],
    );
    assert_eq!(out[0]["result"]["isError"], false, "{}", out[0]);
    let made = &out[0]["result"]["structuredContent"];
    assert_eq!(made["parentId"], own.as_str());
    assert_eq!(made["project"], wt.as_str());
}
