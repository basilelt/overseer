mod common;

use common::{bin, Reap, SID, T};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[test]
fn hooks_and_polling_drive_live_state() {
    let t = T::new();
    t.start_agent("a", SID, "claude --model opus");
    let l = t.line(SID);
    assert_eq!(l["line"]["state"], "active");
    assert_eq!(l["line"]["ticketKey"], "AS-42");
    assert_eq!(l["line"]["owner"], "dev@example.com");
    assert_eq!(l["local"]["busy"], false);
    assert_eq!(l["local"]["orphan"], true);
    assert!(l["local"]["tmuxPane"].as_str().unwrap().starts_with('%'));
    assert!(l["local"]["rssBytes"].as_u64().unwrap() > 0);
    let pane = l["local"]["tmuxPane"].as_str().unwrap().to_string();
    t.keys(&pane, "busy", true);
    t.wait(|| t.line(SID)["local"]["busy"] == true);
    t.keys(&pane, "ask", true);
    t.wait(|| t.line(SID)["local"]["needsInput"] == true);
    t.keys(&pane, "done", true);
    t.wait(|| t.line(SID)["local"]["needsInput"] == false);
    let spool = std::fs::read_dir(t.base.join("state/spool")).unwrap();
    for f in spool.flatten() {
        assert!(!std::fs::read_to_string(f.path())
            .unwrap()
            .contains("PRIVATE"));
    }
    let st = t.json(&["status", "--json"]);
    assert_eq!(st["activeCount"], 1);
    assert_eq!(st["orphans"].as_array().unwrap().len(), 1);
    assert!(st["memTotalBytes"].as_u64().unwrap() > 0);
}

#[test]
fn hold_refuses_each_unsafe_case() {
    let t = T::new();
    let pane = t.start_agent("a", SID, "claude --model opus");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();

    t.keys(&pane, "half typed", false);
    let (code, v) = t.hold(&id, true);
    assert_eq!((code, T::failed(&v)), (3, vec!["inputEmpty".to_string()]));
    t.tmux(&["send-keys", "-t", &pane, "C-u"]);

    t.keys(&pane, "busy", true);
    t.wait(|| t.line(SID)["local"]["busy"] == true);
    assert_eq!(T::failed(&t.hold(&id, true).1), ["atPrompt"]);
    t.keys(&pane, "done", true);

    t.keys(&pane, "tool", true);
    let (_, v) = t.hold(&id, true);
    assert_eq!(T::failed(&v), ["noToolRunning"]);
    assert_eq!(
        v["predicates"]["noToolRunning"]["detail"],
        "a tool subprocess is running"
    );
    t.keys(&pane, "untool", true);

    t.keys(&pane, "sub", true);
    assert_eq!(T::failed(&t.hold(&id, true).1), ["noToolRunning"]);
    t.keys(&pane, "unsub", true);

    assert!(t.sb(&["pin", &id]).status.success());
    assert_eq!(T::failed(&t.hold(&id, true).1), ["notPinned"]);
    assert!(t.sb(&["pin", &id, "--off"]).status.success());

    t.keys(&pane, "ask", true);
    assert_eq!(T::failed(&t.hold(&id, true).1), ["notNeedsInput"]);
    t.keys(&pane, "done", true);

    let (code, v) = t.hold(&id, false);
    assert_eq!((code, v["action"].as_str()), (0, Some("would-hold")));
    assert_eq!(t.line(SID)["line"]["state"], "active");
    let log = std::fs::read_to_string(t.base.join("state/actions.log")).unwrap();
    assert!(log.lines().count() >= 7 && log.contains("would-hold"));
}

#[test]
fn hold_stub_reboot_and_resume() {
    let t = T::new();
    let pane = t.start_agent("a", SID, "claude --model opus");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    assert!(t
        .sb(&["resume", &id])
        .stderr
        .starts_with(b"overseer: line is already running"));

    let (code, v) = t.hold(&id, true);
    assert_eq!((code, v["action"].as_str()), (0, Some("held")), "{v}");
    let l = t.line(SID);
    assert_eq!(
        (
            l["line"]["state"].as_str(),
            l["local"]["heldReason"].as_str()
        ),
        (Some("held"), Some("user"))
    );
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        id
    );
    t.wait(|| t.screen(&pane).contains("held — AS-42"));
    assert_eq!(l["local"]["tmuxPane"], pane.as_str());

    t.tmux(&["kill-server"]);
    let l = t.line(SID);
    assert_eq!(
        (
            l["line"]["state"].as_str(),
            l["local"]["heldReason"].as_str()
        ),
        (Some("held"), Some("user"))
    );
    assert!(l["local"].get("tmuxPane").is_none());

    assert!(t
        .tmux_cmd()
        .args([
            "new-session",
            "-d",
            "-s",
            "main",
            "-x",
            "160",
            "-y",
            "40",
            "sh"
        ])
        .status()
        .unwrap()
        .success());
    let stub = format!("{} stub {id}", bin());
    let pane = t
        .tmux(&["new-window", "-P", "-F", "#{pane_id}", "-t", "main", &stub])
        .trim()
        .to_string();
    t.tmux(&["set-option", "-p", "-t", &pane, "@sb_held", &id]);
    t.wait(|| t.screen(&pane).contains("press Enter to resume"));
    assert_eq!(t.line(SID)["local"]["tmuxPane"], pane.as_str());

    let out = t.sb(&["resume", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    t.wait(|| t.line(SID)["line"]["state"] == "active");
    let log = std::fs::read_to_string(t.base.join("stub.log")).unwrap();
    assert!(
        log.lines()
            .last()
            .unwrap()
            .ends_with(&format!("--model opus --resume {SID}")),
        "{log}"
    );
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        ""
    );
    assert!(t.line(SID)["local"].get("heldReason").is_none());
}

#[test]
fn stub_enter_resumes_in_place() {
    let t = T::new();
    let pane = t.start_agent("a", SID, "claude --model opus");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    assert_eq!(t.hold(&id, true).1["action"], "held");
    t.wait(|| t.screen(&pane).contains("press Enter to resume"));
    t.keys(&pane, "", true);
    t.wait(|| t.line(SID)["line"]["state"] == "active");
    assert_eq!(t.line(SID)["local"]["tmuxPane"], pane.as_str());
}

#[test]
fn close_and_duplicates() {
    let t = T::new();
    let pane = t.start_agent("a", SID, "claude --model opus");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    t.keys(&pane, "typed", false);
    let out = t.sb(&["close", &id, "--yes", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(t.line(SID)["line"]["state"], "active");
    t.tmux(&["send-keys", "-t", &pane, "C-u"]);

    let out = t.sb(&["close", &id, "--json"]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["action"], "would-close");
    let v: Value =
        serde_json::from_slice(&t.sb(&["close", &id, "--yes", "--json"]).stdout).unwrap();
    assert_eq!(v["action"], "closed", "{v}");
    t.wait(|| t.line(SID)["line"]["state"] == "hung-up");
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        ""
    );

    let other = "dddddddd-0000-4000-8000-000000000002";
    t.start_agent("b", other, "claude");
    t.start_agent("c", other, "claude");
    let l = t.line(other);
    assert_eq!(l["local"]["duplicate"], true);
    let shared: Vec<Value> = t
        .json(&["ls", "--json", "--all"])
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["line"]["agentSessionId"] == other)
        .cloned()
        .collect();
    assert_eq!(
        shared.len(),
        2,
        "two processes on one session stay two lines"
    );
    assert_ne!(shared[0]["line"]["id"], shared[1]["line"]["id"]);
    assert_ne!(shared[0]["local"]["pid"], shared[1]["local"]["pid"]);
    let (_, v) = t.hold(l["line"]["id"].as_str().unwrap(), false);
    assert_eq!(T::failed(&v), ["alive"]);
    let doctor = String::from_utf8_lossy(&t.sb(&["doctor"]).stdout).into_owned();
    assert!(
        doctor.contains("duplicate") && doctor.contains("orphan"),
        "{doctor}"
    );
}

#[test]
fn link_pin_and_resume_print() {
    let t = T::new();
    t.start_agent(
        "a",
        SID,
        "claude --model opus --dangerously-skip-permissions",
    );
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    assert!(t.sb(&["link", &id, "as-7"]).status.success());
    assert_eq!(t.line(SID)["line"]["ticketKey"], "AS-7");
    assert!(!t.sb(&["link", &id, "nope"]).status.success());
    assert!(t.sb(&["link", &id, "none"]).status.success());
    assert_eq!(t.line(SID)["line"]["ticketKey"], "AS-42");
    let out = t.sb(&["resume", &SID[..8], "--print"]);
    assert!(!out.status.success());
}

fn daemon_req(sock: &Path, method: &str, target: &str, body: &str) -> (u16, Value) {
    use std::io::{Read, Write};
    let mut s = std::os::unix::net::UnixStream::connect(sock).unwrap();
    write!(
        s,
        "{method} {target} HTTP/1.1\r\nHost: unix\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    (
        code,
        serde_json::from_str(out.split_once("\r\n\r\n").unwrap().1).unwrap(),
    )
}

#[test]
fn daemon_serves_local_api() {
    let t = T::new();
    t.start_agent("a", SID, "claude");
    let sock = std::env::temp_dir().join(format!("sb-test-{}.sock", t.sock));
    let mut c = Command::new(bin());
    t.env(&mut c);
    let guard = Reap::spawn(
        c.args(["daemon", "--interval", "1"])
            .env("OVERSEER_SOCKET", &sock)
            .stderr(std::process::Stdio::null()),
    );
    t.wait(|| sock.exists());
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&sock).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o600);
    let (code, st) = daemon_req(&sock, "GET", "/v1/status", "");
    assert_eq!((code, st["activeCount"].as_u64()), (200, Some(1)));
    let (_, lines) = daemon_req(&sock, "GET", "/v1/lines?ticketKey=AS-42", "");
    let id = lines[0]["line"]["id"].as_str().unwrap().to_string();
    let (code, v) = daemon_req(
        &sock,
        "POST",
        &format!("/v1/lines/{id}/link"),
        r#"{"ticketKey":"AS-9"}"#,
    );
    assert_eq!((code, v["ticketKey"].as_str()), (200, Some("AS-9")));
    let (code, _) = daemon_req(
        &sock,
        "PUT",
        &format!("/v1/lines/{id}/pin"),
        r#"{"pinned":true}"#,
    );
    assert_eq!(code, 200);
    let (code, v) = daemon_req(&sock, "POST", &format!("/v1/lines/{id}/hold"), "");
    assert_eq!(
        (code, v["predicates"]["notPinned"]["ok"].as_bool()),
        (409, Some(false))
    );
    let (code, v) = daemon_req(&sock, "POST", &format!("/v1/lines/{id}/resume"), "");
    assert_eq!(code, 409, "{v}");
    let (code, _) = daemon_req(
        &sock,
        "POST",
        "/v1/hook-events",
        &format!(
            r#"{{"agent":"claude","event":"UserPromptSubmit","sessionId":"{SID}","at":"2099-01-01T00:00:00Z"}}"#
        ),
    );
    assert_eq!(code, 202);
    t.wait(|| daemon_req(&sock, "GET", "/v1/lines?all=true", "").1[0]["local"]["busy"] == true);
    assert_eq!(daemon_req(&sock, "GET", "/v1/nope", "").0, 404);
    let out = t.sb(&["daemon"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("already running"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    drop(guard);
    let _ = std::fs::remove_file(&sock);
}

#[test]
fn hook_never_fails_and_never_prints() {
    let mut c = Command::new(bin());
    c.env_clear()
        .env("OVERSEER_STATE_DIR", "/proc/forbidden")
        .args(["hook", "claude", "Stop", "1"])
        .stdin(std::process::Stdio::null());
    let out = c.output().unwrap();
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
}

#[test]
fn hooks_installer_dry_run_apply_idempotent_uninstall() {
    let t = T::new();
    let settings = t.base.join("home/.claude/settings.json");
    let original = r#"{
  "model": "opus",
  "hooks": {
    "SessionStart": [{"hooks": [{"type": "command", "command": "~/.claude/hooks/tmux-claude-id.sh"}]}]
  }
}
"#;
    std::fs::write(&settings, original).unwrap();
    std::fs::create_dir_all(t.base.join("home/.codex")).unwrap();
    let out = t.sb(&["hooks", "install"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success()
            && text.contains("dry run")
            && text.contains("+")
            && text.contains("#overseer-hook"),
        "{text}"
    );
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    assert!(!t.base.join("home/.codex/hooks.json").exists());

    assert!(t.sb(&["hooks", "install", "--apply"]).status.success());
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(v["model"], "opus");
    assert_eq!(
        v["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "~/.claude/hooks/tmux-claude-id.sh"
    );
    assert_eq!(v["hooks"]["SessionStart"].as_array().unwrap().len(), 2);
    assert_eq!(v["hooks"].as_object().unwrap().len(), 12);
    let backups = std::fs::read_dir(t.base.join("home/.claude"))
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("settings.json.bak-")
        })
        .count();
    assert_eq!(backups, 1);
    let codex: Value = serde_json::from_str(
        &std::fs::read_to_string(t.base.join("home/.codex/hooks.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(codex["hooks"].as_object().unwrap().len(), 7);

    let again =
        String::from_utf8_lossy(&t.sb(&["hooks", "install", "--apply"]).stdout).into_owned();
    assert!(
        again.matches("nothing to do").count() >= 2 && !again.contains(": written"),
        "{again}"
    );
    let doctor = String::from_utf8_lossy(&t.sb(&["doctor"]).stdout).into_owned();
    assert!(doctor.contains("12/12 events"), "{doctor}");

    let cmd = v["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .to_string();
    let mut sh = Command::new("sh");
    t.env(&mut sh);
    let out = sh
        .args(["-c", &cmd])
        .stdin(std::fs::File::open("/dev/null").unwrap())
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "{}");

    assert!(t.sb(&["hooks", "uninstall", "--apply"]).status.success());
    let back: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert_eq!(back, serde_json::from_str::<Value>(original).unwrap());

    std::fs::write(&settings, "{ broken").unwrap();
    assert!(!t.sb(&["hooks", "install", "--apply"]).status.success());
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ broken");
}
