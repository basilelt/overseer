mod common;

use common::{bin, wait, wait_for, App};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn message(id: &str, role: &str, text: &str) -> Value {
    json!({"type": "message", "id": id, "sessionId": "s", "turnId": "t1", "createdAt": 1_700_000_000_000i64,
        "updatedAt": 1_700_000_000_000i64, "generationStatus": "completed", "role": role,
        "content": [{"type": "text", "text": text}], "source": "user"})
}

fn store(home: &Path, sid: &str, cwd: &str, entries: &[Value]) -> PathBuf {
    let dir = home.join(".vibe/logs/session/unified").join(sid);
    let g = dir.join("generations/0000000000000001");
    for d in [&g, &dir.join("chunks"), &dir.join("journal")] {
        std::fs::create_dir_all(d).unwrap();
    }
    let digest = "a".repeat(64);
    std::fs::write(
        dir.join(format!("chunks/{digest}.json")),
        json!(entries).to_string(),
    )
    .unwrap();
    std::fs::write(
        g.join("projection-state.json"),
        json!({"projection_state_version": 1, "session_id": sid, "snapshot_sequence": 0, "watermark": 0,
            "snapshot": {"history": {"entries": []}}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        g.join("runtime-state.json"),
        json!({"runtime_state_version": 3, "session_id": sid, "identity": {"kind": "root"}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        g.join("manifest.json"),
        json!({"manifest_version": 1, "session_id": sid, "generation": "0000000000000001",
            "snapshot_sequence": 0, "execution_state": "quiescent",
            "runtime_state": {"path": "runtime-state.json", "sha256": "0".repeat(64)},
            "projection_state": {"path": "projection-state.json", "sha256": "0".repeat(64), "chunks": [digest]},
            "recovery_journal_segment": {"path": "journal/0000000000000001.jsonl", "first_sequence": 1}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("journal/0000000000000001.jsonl"), "").unwrap();
    std::fs::write(
        dir.join("CURRENT"),
        format!(
            "{}\n",
            json!({"store_format": "mistral.vibe.unified-session-store/v1", "store_format_minor": 10,
                "session_id": sid, "generation": "0000000000000001", "snapshot_sequence": 0,
                "manifest_sha256": "0".repeat(64)})
        ),
    )
    .unwrap();
    let meta = json!({"session_id": sid, "parent_session_id": null, "start_time": "2026-11-11T10:00:00+00:00",
        "end_time": "2026-11-11T10:05:00+00:00", "git_commit": null, "git_branch": null,
        "environment": {"working_directory": cwd}, "origin_directory": cwd, "username": "u",
        "child_sessions": [], "loops": [], "title": "Fix the login bug", "title_source": "auto",
        "archived_at": null, "config": {"active_model": "mistral-medium-3.5", "model_name": "mistral-medium-latest"}});
    std::fs::write(
        dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();
    dir
}

fn hook(app: &App, event: &str, pid: u64, stdin: Value) {
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    let mut child = c
        .args(["hook", "vibe", event, &pid.to_string()])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.to_string().as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

fn vibe_line(app: &App, sid: &str) -> Option<Value> {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["agent"] == "vibe" && l["line"]["agentSessionId"] == sid)
        .cloned()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_running_vibe_session_is_a_line_with_chat_and_hook_state() {
    let app = App::new("vibe-line", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let home = app.fx.base.join("home");
    let sid = "2aea6685-2327-ee7a-3ac1-0a194fdd8de9";
    let agents = app.ok("GET", "/v1/agents", Value::Null);
    let a = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "vibe")
        .unwrap();
    assert_eq!(a["name"], "Mistral Vibe");
    assert_eq!(a["installed"], true);
    assert_eq!(a["sessions"], true);
    assert_eq!(a["models"], json!([]));

    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "vibe"}));
    assert_eq!(strs(&t["cmd"]), ["vibe", "--auto-approve"]);
    let tid = t["id"].as_str().unwrap().to_string();
    let pid = wait_for(|| {
        app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null)["pid"]
            .as_u64()
            .filter(|p| *p > 0)
    });
    let dir = store(
        &home,
        sid,
        &wt,
        &[
            message("m1", "user", "Fix the login bug"),
            message("m2", "assistant", "On it."),
        ],
    );
    let active = home.join(".vibe/logs/session/active");
    std::fs::create_dir_all(&active).unwrap();
    std::fs::write(
        active.join(format!("{sid}.lock.json")),
        json!({"lease_version": 1, "session_id": sid, "process_id": pid}).to_string(),
    )
    .unwrap();

    let l = wait_for(|| vibe_line(&app, sid).filter(|l| l["local"]["termId"] == tid.as_str()));
    let line = l["line"]["id"].as_str().unwrap().to_string();
    assert_eq!(l["local"]["title"], "Fix the login bug");
    assert_eq!(
        l["local"]["resumeCommand"].as_str().unwrap(),
        format!("cd {wt} && vibe --auto-approve --resume {sid}")
    );

    let chat = app.ok("GET", &format!("/v1/lines/{line}/chat"), Value::Null);
    assert_eq!(chat["model"], "mistral-medium-3.5");
    let e = chat["entries"].as_array().unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[0]["kind"], "user");
    assert_eq!(e[1]["text"], "On it.");
    let cursor = chat["cursor"].as_u64().unwrap();

    let base = json!({"session_id": sid, "parent_session_id": null, "cwd": wt,
        "transcript_path": dir.to_string_lossy()});
    let with = |name: &str, extra: Value| {
        let mut v = base.clone();
        v["hook_event_name"] = json!(name);
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    hook(
        &app,
        "pre_agent",
        pid,
        with(
            "pre_agent",
            json!({"agent_name": "default", "turn_id": "t2", "prompt": "go"}),
        ),
    );
    wait(|| vibe_line(&app, sid).is_some_and(|l| l["local"]["busy"] == true));
    hook(
        &app,
        "notification",
        pid,
        with(
            "notification",
            json!({"notification_type": "approval_requested", "message": "m", "tool_name": "file_system.bash"}),
        ),
    );
    wait(|| vibe_line(&app, sid).is_some_and(|l| l["local"]["needsInput"] == true));

    let rec = json!({"recovery_journal_record_version": 1, "sequence": 1, "previous_record_sha256": null,
        "record_sha256": "0".repeat(64), "type": "projection_delta",
        "payload": {"watermark": 1, "delta": [{"op": "append_entry", "entry": message("m3", "assistant", "Done.")}]}});
    std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("journal/0000000000000001.jsonl"))
        .unwrap()
        .write_all(format!("{rec}\n").as_bytes())
        .unwrap();
    let more = wait_for(|| {
        let c = app.ok(
            "GET",
            &format!("/v1/lines/{line}/chat?after={cursor}"),
            Value::Null,
        );
        (c["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == "m3"))
        .then_some(c)
    });
    assert_eq!(more["reset"], false);

    hook(
        &app,
        "post_agent",
        pid,
        with("post_agent", json!({"agent_name": "default"})),
    );
    wait(|| {
        vibe_line(&app, sid)
            .is_some_and(|l| l["local"]["busy"] == false && l["local"]["needsInput"] == false)
    });
    let act = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
    assert_eq!(act["transcript"], true);
    assert_eq!(act["lastAssistantText"], "Done.");
}

#[test]
fn a_vibe_task_starts_trusted_with_its_prompt_after_the_separator() {
    let app = App::new("vibe-task", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Fix it", "description": "now", "project": wt, "agent": "vibe"}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    let term = app.ok(
        "GET",
        &format!("/v1/terms/{}", s["termId"].as_str().unwrap()),
        Value::Null,
    );
    let cmd = strs(&term["cmd"]);
    assert_eq!(cmd[0], "vibe");
    assert!(cmd.contains(&"--trust".to_string()), "{cmd:?}");
    assert!(cmd.contains(&"--auto-approve".to_string()), "{cmd:?}");
    assert_eq!(cmd[cmd.len() - 2..], ["--", "<prompt>"]);
    let log = || std::fs::read_to_string(app.fx.base.join("stub.log")).unwrap_or_default();
    wait(|| log().contains("-- Fix it"));
    assert!(log().contains("vibe --trust"), "{}", log());
}

#[test]
fn a_resumed_vibe_term_without_a_lease_keeps_its_line() {
    let app = App::new("vibe-resume", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let sid = "5b0e1d2c-3f4a-4b5c-8d6e-7f8091a2b3c4";
    store(
        &app.fx.base.join("home"),
        sid,
        &wt,
        &[message("m1", "user", "Resume me")],
    );
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["vibe", "--resume", sid]}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let l = wait_for(|| vibe_line(&app, sid).filter(|l| l["local"]["termId"] == tid.as_str()));
    assert_eq!(l["line"]["state"], "active");
    assert_eq!(
        l["local"]["resumeCommand"].as_str().unwrap(),
        format!("cd {wt} && vibe --resume {sid}")
    );
}
