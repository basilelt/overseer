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

fn lines(app: &App) -> Vec<Value> {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .clone()
}

fn line_of(app: &App, term: &str) -> Option<Value> {
    lines(app)
        .into_iter()
        .find(|l| l["local"]["termId"] == term)
}

#[test]
fn every_agent_keeps_its_titles_and_resumes_across_a_daemon_restart() {
    let mut app = App::new("titles-restart", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let agents = ["claude", "gemini", "opencode", "agy", "copilot"];
    let mut terms = vec![];
    for a in agents {
        let t = app.ok(
            "POST",
            "/v1/terms",
            json!({"agent": a, "cwd": wt, "cols": 100, "rows": 30}),
        );
        let term = t["id"].as_str().unwrap().to_string();
        if a == "gemini" {
            let cmd = t["cmd"].as_array().unwrap();
            let at = cmd.iter().position(|c| c == "--session-id").unwrap();
            let sid = cmd[at + 1].as_str().unwrap();
            let chats = app.fx.base.join("home/.gemini/tmp/repo/chats");
            std::fs::create_dir_all(&chats).unwrap();
            std::fs::write(app.fx.base.join("home/.gemini/tmp/repo/.project_root"), &wt).unwrap();
            std::fs::write(
                chats.join("session-2026-11-19T10-00-aaaaaaaa.jsonl"),
                format!(
                    "{}\n",
                    json!({"sessionId": sid, "startTime": "2026-11-19T10:00:00.000Z", "kind": "main"})
                ),
            )
            .unwrap();
        }
        let l = wait_for(|| line_of(&app, &term).filter(|l| l["line"]["agent"] == a));
        let line = l["line"]["id"].as_str().unwrap().to_string();
        app.ok(
            "PUT",
            &format!("/v1/lines/{line}/title"),
            json!({"title": format!("line {a}")}),
        );
        app.ok(
            "PUT",
            &format!("/v1/terms/{term}/title"),
            json!({"title": format!("term {a}")}),
        );
        terms.push((a, term, line, l["line"]["agentSessionId"].clone()));
    }
    let claude_sid = terms[0].3.as_str().unwrap().to_string();
    let proj = app.fx.base.join("home/.claude/projects");
    let transcript = wait_for(|| {
        std::fs::read_dir(&proj).ok()?.flatten().find_map(|d| {
            d.path()
                .join(format!("{claude_sid}.jsonl"))
                .exists()
                .then(|| d.path().join(format!("{claude_sid}.jsonl")))
        })
    });
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(transcript)
        .unwrap();
    use std::io::Write;
    writeln!(
        f,
        "{}",
        json!({"type": "ai-title", "aiTitle": "Auto claude title", "sessionId": claude_sid})
    )
    .unwrap();
    wait_for(|| line_of(&app, &terms[0].1).filter(|l| l["local"]["title"] == "Auto claude title"));

    for _ in 0..2 {
        app.crash_and_restart();

        for (a, term, line, sid) in &terms {
            let t = wait_for(|| {
                let v = app.ok("GET", "/v1/terms", Value::Null);
                v.as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["id"] == term.as_str())
                    .cloned()
            });
            assert_eq!(t["titleOverride"], format!("term {a}"), "{a}: {t}");
            let l = wait_for(|| line_of(&app, term)).clone();
            assert_eq!(l["line"]["id"], line.as_str(), "{a}: {l}");
            assert_eq!(l["local"]["titleOverride"], format!("line {a}"), "{a}: {l}");
            if sid.is_string() && *a != "opencode" && *a != "agy" && *a != "copilot" {
                let cmd = t["cmd"].to_string();
                assert!(
                    cmd.contains(sid.as_str().unwrap()),
                    "{a} resumes {sid}: {cmd}"
                );
            }
        }
        let l = wait_for(|| line_of(&app, &terms[0].1));
        assert_eq!(l["local"]["title"], "Auto claude title", "{l}");
        std::thread::sleep(std::time::Duration::from_millis(3500));
    }
}
