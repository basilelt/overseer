mod common;

use common::{wait_for, App};
use serde_json::{json, Value};

fn rollout(dir: &std::path::Path, sid: &str, ts: &str, parent: Option<&str>) -> std::path::PathBuf {
    let day = dir.join("sessions/2026/11/19");
    std::fs::create_dir_all(&day).unwrap();
    let p = day.join(format!("rollout-{ts}-{sid}.jsonl"));
    let source = match parent {
        Some(p) => {
            json!({"subagent": {"thread_spawn": {"parent_thread_id": p}, "agent_role": "worker"}, "thread_source": "subagent", "depth": 2})
        }
        None => json!("vscode"),
    };
    let meta = json!({
        "timestamp": format!("2026-11-19T{ts}Z"),
        "type": "session_meta",
        "payload": {"id": sid, "timestamp": format!("2026-11-19T{ts}Z"), "cwd": dir, "source": source, "thread_source": parent.map(|_| "subagent"), "cli_version": "0.0.0"}
    });
    std::fs::write(&p, format!("{meta}\n")).unwrap();
    p
}

#[test]
fn a_codex_sub_agent_thread_is_not_a_session_and_never_resumed() {
    let mut app = App::new("codex-subagent", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"agent": "codex", "cwd": wt, "worktree": true, "cols": 120, "rows": 32}),
    );
    let term = t["id"].as_str().unwrap().to_string();
    {
        let mut ws = app.ws(&term);
        common::send(&mut ws, "busy\r");
    }
    let line = wait_for(|| {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        v.as_array()
            .unwrap()
            .iter()
            .find(|l| l["local"]["termId"] == term.as_str())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let line_json = wait_for(|| {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        v.as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == line.as_str())
            .cloned()
    });
    let root = line_json["line"]["agentSessionId"]
        .as_str()
        .unwrap()
        .to_string();
    eprintln!("root session id: {root}");

    let codex_home = app.fx.base.join("home/.codex");
    let before_activity = line_json["line"]["lastActivityAt"]
        .as_str()
        .unwrap()
        .to_string();
    rollout(
        &codex_home,
        "subagent-aaaa-0000-4000-8000-0000000000aa",
        "12-00-00",
        Some(&root),
    );

    wait_for(|| {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        let l = v.as_array().unwrap().iter().find(|l| {
            l["line"]["agent"] == "codex" && l["line"]["agentSessionId"] == root.as_str()
        })?;
        (l["line"]["lastActivityAt"].as_str()? != before_activity).then_some(())
    });
    let lines = app.ok("GET", "/v1/lines?all=true", Value::Null);
    let codex: Vec<&Value> = lines
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"]["agent"] == "codex")
        .collect();
    eprintln!("CODEX LINES WITH SUB-AGENT: {codex:?}");
    assert_eq!(
        codex.len(),
        1,
        "sub-agent threads are not separate sessions: {codex:?}"
    );
    assert_eq!(codex[0]["line"]["agentSessionId"], root);
    assert_eq!(codex[0]["local"]["termId"], term.as_str());

    app.crash_and_restart();

    let terms = app.ok("GET", "/v1/terms", Value::Null);
    let restored = terms
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == term.as_str())
        .cloned()
        .expect("term restored");
    let cmd = restored["cmd"].as_array().unwrap();
    eprintln!("RESTORED CMD: {cmd:?}");
    assert!(
        cmd.iter().any(|c| c == &root),
        "the restored term resumes the root session {root}, got {cmd:?}"
    );
    assert!(
        !cmd.iter()
            .any(|c| c.as_str().unwrap_or("").contains("subagent-aaaa")),
        "the restored term must not resume a sub-agent thread, got {cmd:?}"
    );
    let lines = app.ok("GET", "/v1/lines?all=true", Value::Null);
    let codex: Vec<&Value> = lines
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"]["agent"] == "codex")
        .collect();
    eprintln!("CODEX LINES AFTER RESTART: {codex:?}");
    assert_eq!(
        codex.len(),
        1,
        "sub-agent threads are not separate sessions: {codex:?}"
    );
    assert_eq!(codex[0]["line"]["agentSessionId"], root);
}
