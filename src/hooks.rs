use crate::procs::{now_ms, Probe};
use crate::registry::{HookState, Registry};
use crate::sys::OpenOptionsExt;
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "StopFailure",
    "Notification",
    "PermissionRequest",
    "Elicitation",
    "PostToolUse",
    "PostToolUseFailure",
    "SubagentStart",
    "SubagentStop",
    "TaskCreated",
    "TaskCompleted",
    "SessionEnd",
];

pub const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "PermissionRequest",
    "PostToolUse",
    "SubagentStart",
    "SubagentStop",
    "SessionEnd",
];

pub const GEMINI_EVENTS: &[&str] = &[
    "SessionStart",
    "BeforeAgent",
    "AfterAgent",
    "Notification",
    "SessionEnd",
];

pub const COPILOT_EVENTS: &[&str] = &[
    "sessionStart",
    "userPromptSubmitted",
    "agentStop",
    "notification",
    "sessionEnd",
];

pub const VIBE_EVENTS: &[&str] = &[
    "session_start",
    "pre_agent",
    "post_agent",
    "post_tool",
    "notification",
    "session_end",
];

pub const VIBE_MAIN_ONLY: &[&str] = &["pre_agent", "post_agent"];

pub const CURSOR_EVENTS: &[&str] = &["sessionStart", "beforeSubmitPrompt", "stop", "sessionEnd"];

const MAX_STDIN: u64 = 1 << 20;

pub fn canonical<'a>(event: &'a str, notification_type: Option<&str>) -> &'a str {
    match event {
        "BeforeAgent" | "userPromptSubmitted" | "beforeSubmitPrompt" | "pre_agent" => {
            "UserPromptSubmit"
        }
        "AfterAgent" | "agentStop" | "stop" | "post_agent" => "Stop",
        "sessionStart" | "session_start" => "SessionStart",
        "sessionEnd" | "session_end" => "SessionEnd",
        "post_tool" => "PostToolUse",
        "errorOccurred" => "StopFailure",
        "notification" | "Notification" => match notification_type {
            Some("agent_idle" | "agent_completed" | "turn_completed") => "Stop",
            Some("turn_failed") => "StopFailure",
            Some("approval_requested") => "PermissionRequest",
            Some("question_asked") => "Elicitation",
            _ => "Notification",
        },
        e => e,
    }
}

pub fn spool_record(
    agent: &str,
    event: &str,
    stdin: &[u8],
    pid: Option<u32>,
    env: impl Fn(&str) -> Option<String>,
    at_ms: i64,
) -> Value {
    let v: Value = serde_json::from_slice(stdin).unwrap_or(Value::Null);
    let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty());
    let raw = s("hook_event_name").unwrap_or(event);
    let event = canonical(raw, s("notification_type"));
    let mut rec = json!({ "agent": agent, "event": event, "at": at_ms });
    for (key, val) in [
        ("sessionId", s("session_id").map(str::to_string)),
        ("cwd", s("cwd").map(str::to_string)),
        (
            "notificationType",
            s("notification_type").map(str::to_string),
        ),
        ("tmuxPane", env("TMUX_PANE")),
        ("termId", env("OVERSEER_TERM_ID")),
    ] {
        if let Some(val) = val {
            rec[key] = json!(val);
        }
    }
    if let Some(p) = pid {
        rec["pid"] = json!(p);
    }
    rec
}

pub fn append_spool(state_dir: &Path, rec: &Value) -> std::io::Result<()> {
    let dir = state_dir.join("spool");
    crate::mkdir_private(&dir)?;
    let name = match rec["pid"].as_u64() {
        Some(p) => format!("{p}.jsonl"),
        None => "nopid.jsonl".into(),
    };
    let mut line = serde_json::to_vec(rec)?;
    line.push(b'\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(dir.join(name))?
        .write_all(&line)
}

pub fn run_hook(state_dir: &Path, agent: &str, event: &str, pid_arg: Option<&str>) {
    let mut buf = vec![];
    let _ = std::io::stdin().take(MAX_STDIN).read_to_end(&mut buf);
    let pid = pid_arg.and_then(|p| p.parse().ok());
    let rec = spool_record(agent, event, &buf, pid, |k| std::env::var(k).ok(), now_ms());
    let _ = append_spool(state_dir, &rec);
}

pub fn apply(h: &mut HookState, ev: &Value) {
    let event = ev["event"].as_str().unwrap_or("");
    h.event = event.to_string();
    h.at_ms = h.at_ms.max(ev["at"].as_i64().unwrap_or(0));
    if let Some(p) = ev["pid"].as_u64() {
        h.pid = Some(p as u32);
    }
    if let Some(p) = ev["tmuxPane"].as_str() {
        h.pane = Some(p.to_string());
    }
    match event {
        "SessionStart" => {
            h.busy = false;
            h.needs_input = false;
            h.subagents = 0;
            h.tasks = 0;
        }
        "UserPromptSubmit" => {
            h.busy = true;
            h.needs_input = false;
        }
        "Stop" | "StopFailure" => {
            h.busy = false;
            h.needs_input = false;
        }
        // The session is gone: a lost SubagentStop must not outlive it.
        "SessionEnd" => {
            h.busy = false;
            h.needs_input = false;
            h.subagents = 0;
            h.tasks = 0;
        }
        "Notification" => {
            h.needs_input = !matches!(
                ev["notificationType"].as_str(),
                Some("idle_prompt" | "auth_success")
            )
        }
        "PermissionRequest" | "Elicitation" => h.needs_input = true,
        "PostToolUse" | "PostToolUseFailure" => h.needs_input = false,
        "SubagentStart" => h.subagents += 1,
        "SubagentStop" => h.subagents = (h.subagents - 1).max(0),
        "TaskCreated" => h.tasks += 1,
        "TaskCompleted" => h.tasks = (h.tasks - 1).max(0),
        _ => {}
    }
}

pub fn session_key(agent: &str, ev: &Value) -> Option<String> {
    let term = ev["termId"].as_str().filter(|t| !t.is_empty());
    let by_session = crate::agents::get(agent).is_some_and(|a| a.sessions());
    match (ev["sessionId"].as_str(), term) {
        (Some(s), _) if by_session => Some(s.to_string()),
        (_, Some(t)) => Some(term_key(t)),
        (Some(s), None) => Some(s.to_string()),
        (None, None) => None,
    }
}

pub fn term_key(term_id: &str) -> String {
    format!("term:{term_id}")
}

pub fn ingest(state_dir: &Path, reg: &Registry, probe: &dyn Probe) -> Result<usize, String> {
    let Ok(dir) = std::fs::read_dir(state_dir.join("spool")) else {
        return Ok(0);
    };
    let mut events: Vec<Value> = vec![];
    for e in dir.flatten() {
        let path = e.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if !name.ends_with(".jsonl") {
            continue;
        }
        let Ok(mut f) = std::fs::File::open(&path) else {
            continue;
        };
        let off = reg.spool_offset(&name)?;
        let mut buf = vec![];
        if f.seek(SeekFrom::Start(off)).is_err() || f.read_to_end(&mut buf).is_err() {
            continue;
        }
        let complete = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        events.extend(
            buf[..complete]
                .split(|b| *b == b'\n')
                .filter_map(|l| serde_json::from_slice::<Value>(l).ok()),
        );
        let end = off + complete as u64;
        let pid = name
            .strip_suffix(".jsonl")
            .and_then(|p| p.parse::<u32>().ok());
        let finished = complete == buf.len() && pid.is_some_and(|p| probe.info(p).is_none());
        if finished {
            let _ = std::fs::remove_file(&path);
            reg.set_spool_offset(&name, None)?;
        } else if complete > 0 {
            reg.set_spool_offset(&name, Some(end))?;
        }
    }
    events.sort_by_key(|e| e["at"].as_i64().unwrap_or(0));
    let n = events.len();
    for ev in events {
        let Some(agent) = ev["agent"].as_str() else {
            continue;
        };
        let Some(sid) = session_key(agent, &ev) else {
            continue;
        };
        let sid = sid.as_str();
        let mut h = reg.hook_state(agent, sid)?.unwrap_or_default();
        apply(&mut h, &ev);
        reg.put_hook_state(agent, sid, &h)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_keeps_only_allowed_fields() {
        let stdin = br#"{"session_id":"s1","cwd":"/w","hook_event_name":"UserPromptSubmit","prompt":"SECRET","transcript_path":"/t","message":"SECRET"}"#;
        let rec = spool_record(
            "claude",
            "X",
            stdin,
            Some(42),
            |k| (k == "TMUX_PANE").then(|| "%3".into()),
            7,
        );
        let s = rec.to_string();
        assert!(!s.contains("SECRET"));
        assert_eq!(rec["event"], "UserPromptSubmit");
        assert_eq!(rec["sessionId"], "s1");
        assert_eq!(rec["tmuxPane"], "%3");
        assert_eq!(rec["pid"], 42);
        let rec = spool_record("codex", "Stop", b"not json", None, |_| None, 7);
        assert_eq!(rec["event"], "Stop");
        assert!(rec.get("sessionId").is_none());
    }

    #[test]
    fn vibe_events_map_to_line_states_by_session() {
        let rec =
            |stdin: &[u8]| spool_record("vibe", "x", stdin, Some(9), |_| Some("T1".into()), 1);
        let ev = |name: &str, extra: &str| {
            rec(format!(r#"{{"session_id":"v1","parent_session_id":null,"cwd":"/w","transcript_path":"/h/unified/v1","hook_event_name":"{name}"{extra}}}"#).as_bytes())
        };
        let mut h = HookState::default();
        let start = ev(
            "session_start",
            r#","source":"startup","agent_name":"default","model":"m""#,
        );
        assert_eq!(start["event"], "SessionStart");
        assert_eq!(session_key("vibe", &start).as_deref(), Some("v1"));
        let pre = ev(
            "pre_agent",
            r#","agent_name":"default","turn_id":"t","prompt":"hi""#,
        );
        assert_eq!(pre["event"], "UserPromptSubmit");
        apply(&mut h, &pre);
        assert!(h.busy);
        let ask = ev(
            "notification",
            r#","notification_type":"approval_requested","message":"m","tool_name":"file_system.bash""#,
        );
        assert_eq!(ask["event"], "PermissionRequest");
        apply(&mut h, &ask);
        assert!(h.needs_input);
        let tool = ev(
            "post_tool",
            r#","tool_name":"file_system.bash","tool_status":"success""#,
        );
        apply(&mut h, &tool);
        assert!(!h.needs_input && h.busy);
        let q = ev("notification", r#","notification_type":"question_asked""#);
        assert_eq!(q["event"], "Elicitation");
        let done = ev("notification", r#","notification_type":"turn_completed""#);
        assert_eq!(done["event"], "Stop");
        let failed = ev("notification", r#","notification_type":"turn_failed""#);
        assert_eq!(failed["event"], "StopFailure");
        let post = ev("post_agent", r#","agent_name":"default""#);
        assert_eq!(post["event"], "Stop");
        apply(&mut h, &post);
        assert!(!h.busy);
        assert_eq!(
            ev("session_end", r#","reason":"exit""#)["event"],
            "SessionEnd"
        );
    }

    #[test]
    fn native_events_map_to_line_states() {
        let rec = |agent: &str, ev: &str, stdin: &[u8]| {
            spool_record(
                agent,
                ev,
                stdin,
                Some(9),
                |k| (k == "OVERSEER_TERM_ID").then(|| "T1".into()),
                1,
            )
        };
        let g = rec(
            "gemini",
            "AfterAgent",
            br#"{"session_id":"g1","hook_event_name":"AfterAgent"}"#,
        );
        assert_eq!(g["event"], "Stop");
        assert_eq!(session_key("gemini", &g).as_deref(), Some("g1"));
        let c = rec("copilot", "userPromptSubmitted", br#"{"sessionId":"c1"}"#);
        assert_eq!(c["event"], "UserPromptSubmit");
        assert_eq!(session_key("copilot", &c).as_deref(), Some("term:T1"));
        let n = rec(
            "copilot",
            "notification",
            br#"{"hook_event_name":"Notification","notification_type":"permission_prompt"}"#,
        );
        assert_eq!(n["event"], "Notification");
        let done = rec(
            "copilot",
            "notification",
            br#"{"hook_event_name":"Notification","notification_type":"agent_completed"}"#,
        );
        assert_eq!(done["event"], "Stop");
        let k = rec(
            "cursor",
            "stop",
            br#"{"hook_event_name":"stop","conversation_id":"x"}"#,
        );
        assert_eq!(k["event"], "Stop");
        let o = rec(
            "opencode",
            "PermissionRequest",
            br#"{"session_id":"ses_1"}"#,
        );
        assert_eq!(session_key("opencode", &o).as_deref(), Some("term:T1"));
        let mut h = HookState::default();
        apply(&mut h, &rec("cursor", "beforeSubmitPrompt", b"{}"));
        assert!(h.busy);
        apply(&mut h, &o);
        assert!(h.needs_input);
        apply(&mut h, &k);
        assert!(!h.busy && !h.needs_input);
        let none = spool_record("amp", "Stop", b"{}", None, |_| None, 1);
        assert_eq!(session_key("amp", &none), None);
    }

    #[test]
    fn a_lost_subagent_stop_is_cleared_by_stop_never_but_by_session_end_and_start() {
        // Given a session whose turn ended with a subagent and a task still counted
        // When Stop arrives, then SessionEnd, and separately SessionStart
        // Then Stop keeps the counts and both SessionEnd and SessionStart reset them
        let ev = |e: &str| json!({"event": e, "at": 1});
        let mut h = HookState::default();
        for e in ["UserPromptSubmit", "SubagentStart", "TaskCreated", "Stop"] {
            apply(&mut h, &ev(e));
        }
        assert!(!h.busy && h.subagents == 1 && h.tasks == 1);
        let mut again = h.clone();
        apply(&mut again, &ev("SessionStart"));
        assert!(again.subagents == 0 && again.tasks == 0);
        apply(&mut h, &ev("SessionEnd"));
        assert!(h.subagents == 0 && h.tasks == 0);
    }

    #[test]
    fn events_drive_state() {
        let mut h = HookState::default();
        let ev = |e: &str, at: i64| json!({"event": e, "at": at});
        apply(&mut h, &ev("SessionStart", 1));
        apply(&mut h, &ev("UserPromptSubmit", 2));
        assert!(h.busy);
        apply(&mut h, &ev("SubagentStart", 3));
        apply(&mut h, &ev("PermissionRequest", 4));
        assert!(h.needs_input && h.subagents == 1);
        apply(&mut h, &ev("SubagentStop", 5));
        apply(&mut h, &ev("Stop", 6));
        assert!(!h.busy && !h.needs_input && h.subagents == 0);
        apply(
            &mut h,
            &json!({"event": "Notification", "at": 7, "notificationType": "idle_prompt"}),
        );
        assert!(!h.needs_input);
        apply(&mut h, &ev("Notification", 8));
        assert!(h.needs_input);
        assert_eq!(h.at_ms, 8);
    }

    #[test]
    fn answered_permission_clears_needs_input_while_turn_runs() {
        let ev = |e: &str| json!({"event": e, "at": 1});
        for after in ["PostToolUse", "PostToolUseFailure"] {
            let mut h = HookState::default();
            apply(&mut h, &ev("UserPromptSubmit"));
            apply(&mut h, &ev("PermissionRequest"));
            apply(
                &mut h,
                &json!({"event": "Notification", "at": 1, "notificationType": "permission_prompt"}),
            );
            assert!(h.needs_input);
            apply(&mut h, &ev(after));
            assert!(h.busy && !h.needs_input, "{after}");
        }
    }

    #[test]
    fn codex_installs_post_tool_use_to_clear_permission_waits() {
        assert!(CODEX_EVENTS.contains(&"PostToolUse"));

        let mut h = HookState::default();
        apply(&mut h, &json!({"event": "UserPromptSubmit", "at": 1}));
        apply(&mut h, &json!({"event": "PermissionRequest", "at": 2}));
        assert!(h.needs_input);

        apply(&mut h, &json!({"event": "PostToolUse", "at": 3}));
        assert!(h.busy && !h.needs_input);
    }

    #[test]
    fn claude_hooks_report_tool_completion() {
        assert!(CLAUDE_EVENTS.contains(&"PostToolUse"));
        assert!(CLAUDE_EVENTS.contains(&"PostToolUseFailure"));
    }
}
