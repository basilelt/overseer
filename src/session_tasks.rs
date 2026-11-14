use crate::git::Git;
use crate::naming;
use crate::registry::Registry;
use crate::status::{self, Status};
use crate::tasks::{self, Task};
use crate::util::ulid;
use crate::{Ctx, Line};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

pub const ORIGIN: &str = "session";
const DAY_MS: i64 = 86_400_000;
const RECENT_MS: i64 = 120_000;

pub struct Env<'a> {
    pub now_ms: i64,
    pub days: i64,
    pub station: &'a str,
    pub project: &'a dyn Fn(&Line) -> Option<String>,
    pub activity: &'a dyn Fn(&Line) -> Option<Value>,
    pub merged: &'a dyn Fn(&Task) -> bool,
}

pub fn sync(ctx: &Ctx, reg: &Registry, lines: &[Line]) {
    let git = RefCell::new(Git::default());
    let project =
        |l: &Line| tasks::main_root(&mut git.borrow_mut(), path_of(l)).map(|(root, _)| root);
    let activity = |l: &Line| {
        crate::activity::find_transcript(
            &ctx.claude_dir,
            &ctx.codex_dir,
            &ctx.gemini_dir,
            l.agent,
            &l.session_id,
        )
        .and_then(|p| crate::activity::activity(l.agent, &p, Some(l.last_activity_ms - RECENT_MS)))
    };
    let now = ctx.now_ms;
    let fetch = ctx.config.worktree_fetch;
    let merged = |t: &Task| crate::worktrees::branch_landed(&t.project, t.branch.as_deref(), fetch);
    run(
        reg,
        lines,
        &Env {
            now_ms: now,
            days: ctx.config.session_tasks_days,
            station: &ctx.config.station_id,
            project: &project,
            activity: &activity,
            merged: &merged,
        },
    );
}

pub fn run(reg: &Registry, lines: &[Line], env: &Env) {
    let Ok(all) = tasks::all(reg) else {
        return;
    };
    let linked: HashSet<&str> = all.iter().filter_map(|t| t.line_id.as_deref()).collect();
    let by_line: HashMap<&str, &Line> = lines.iter().map(|l| (l.id.as_str(), l)).collect();
    for t in all.iter().filter(|t| t.from_session()) {
        let mut t = t.clone();
        let line = t.line_id.as_deref().and_then(|id| by_line.get(id)).copied();
        // An empty scan proves nothing (unreadable transcripts dir): never close on it.
        if line.is_none() && lines.is_empty() {
            continue;
        }
        if refresh(&mut t, line, env) {
            tasks::logged(
                "session task not saved",
                tasks::store(reg, &mut t, env.station),
            );
        }
    }
    for l in lines {
        if linked.contains(l.id.as_str()) || !eligible(l, env.now_ms, env.days) || runs_a_task(l) {
            continue;
        }
        let Some(project) = (env.project)(l) else {
            continue;
        };
        let mut t = new_task(l, project, (env.activity)(l).as_ref());
        tasks::logged(
            "session task not created",
            tasks::insert(reg, &mut t, env.station),
        );
    }
}

fn runs_a_task(l: &Line) -> bool {
    l.term_id
        .as_deref()
        .and_then(crate::term::get)
        .is_some_and(|t| t.info().task_id.is_some())
}

fn path_of(l: &Line) -> &str {
    if l.worktree_path.is_empty() {
        &l.cwd
    } else {
        &l.worktree_path
    }
}

pub fn eligible(l: &Line, now_ms: i64, days: i64) -> bool {
    days > 0
        && l.state != crate::wire::STATE_HUNG_UP
        && !l.duplicate
        && !l.volatile
        && !l.cwd_missing
        && !l.offline
        && (l.alive() || now_ms - l.last_activity_ms <= days * DAY_MS)
}

pub fn session_state(l: &Line, worked: bool) -> &'static str {
    match l.state {
        crate::wire::STATE_HUNG_UP => "closed",
        crate::wire::STATE_HELD if l.held_reason == Some("user") => "held",
        crate::wire::STATE_HELD => "lost",
        _ if l.needs_input => "input",
        _ if l.busy => "busy",
        _ if !worked => "fresh",
        _ if l.busy_known => "done",
        crate::wire::STATE_ACTIVE => "busy",
        _ => "done",
    }
}

pub fn status_for(
    state: &str,
    current: &str,
    review: bool,
) -> (&'static str, Option<&'static str>) {
    match state {
        "merged" => (status::MERGED, None),
        "closed" => (status::COMPLETED, None),
        "lost" | "done" => match current {
            status::IN_TESTING => (status::IN_TESTING, None),
            status::MERGED => (status::MERGED, None),
            status::IN_PROGRESS if review => (status::IN_TESTING, None),
            _ => (status::COMPLETED, None),
        },
        "held" => (
            match current {
                status::IN_TESTING => status::IN_TESTING,
                status::MERGED => status::MERGED,
                status::COMPLETED => status::COMPLETED,
                _ => status::IN_PROGRESS,
            },
            Some("paused"),
        ),
        "input" | "fresh" => (status::IN_PROGRESS, Some("needs_input")),
        _ => (status::IN_PROGRESS, None),
    }
}

fn order(status: &str) -> usize {
    Status::ALL
        .iter()
        .position(|s| s.as_str() == status)
        .unwrap_or(0)
}

pub fn follow(t: &mut Task, state: &str, review: impl FnOnce() -> bool) -> bool {
    let same = t.manual_status.is_some() && t.session_state.as_deref() == Some(state);
    let review =
        !same && matches!(state, "lost" | "done") && t.status == status::IN_PROGRESS && review();
    let (derived, badge) = status_for(state, &t.status, review);
    let keep = t.manual_status.as_deref().is_some_and(|m| {
        same || (!matches!(state, "busy" | "input") && order(derived) <= order(m))
    });
    let status = if keep {
        t.status.clone()
    } else {
        derived.to_string()
    };
    let manual = if keep { t.manual_status.clone() } else { None };
    let changed = status != t.status
        || manual != t.manual_status
        || t.badge.as_deref() != badge
        || t.session_state.as_deref() != Some(state);
    t.status = status;
    t.manual_status = manual;
    t.badge = badge.map(str::to_string);
    t.session_state = Some(state.to_string());
    changed
}

fn edited(a: Option<Value>) -> bool {
    a.is_none_or(|a| a["editedFiles"].as_array().is_none_or(|f| !f.is_empty()))
}

fn worked(l: &Line, t: &Task) -> bool {
    l.title.is_some()
        || !t.description.is_empty()
        || l.hook
            .as_ref()
            .is_some_and(|h| matches!(h.event.as_str(), "Stop" | "StopFailure"))
}

pub fn summarize(s: &str, max: usize) -> String {
    let first = s
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let flat = first.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => &cut[..i],
        _ => &cut[..],
    };
    format!(
        "{}…",
        cut.trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
    )
}

fn agent_name(agent: &str) -> &str {
    crate::agents::get(agent).map_or(agent, |a| a.name)
}

pub fn fallback_title(agent: &str) -> String {
    format!("New {} session", agent_name(agent))
}

fn some(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

fn text<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    some(a[k].as_str())
}

pub fn title_of(line_title: Option<&str>, a: &Value, agent: &str) -> String {
    if let Some(t) = text(a, "aiTitle")
        .or(some(line_title))
        .or(text(a, "summary"))
    {
        return summarize(t, 80);
    }
    text(a, "goal")
        .and_then(naming::title_from_prompt)
        .or_else(|| text(a, "lastPrompt").and_then(naming::title_from_prompt))
        .unwrap_or_else(|| fallback_title(agent))
}

fn legacy_generated(t: &Task, line_title: Option<&str>, a: &Value) -> bool {
    let name = agent_name(&t.agent);
    t.title.starts_with(&format!("{name} session"))
        || t.title == title_of(line_title, a, &t.agent)
        || text(a, "goal").is_some_and(|g| t.title == summarize(g, 60))
        || [text(a, "aiTitle"), text(a, "summary"), some(line_title)]
            .into_iter()
            .flatten()
            .any(|x| t.title == summarize(x, 80))
}

fn retitle(t: &mut Task, line_title: Option<&str>, a: &Value) -> bool {
    let auto = match t.title_source.as_deref() {
        Some(tasks::TITLE_USER) => false,
        Some(_) => true,
        None => legacy_generated(t, line_title, a),
    };
    let source = if auto {
        tasks::TITLE_AUTO
    } else {
        tasks::TITLE_USER
    };
    let mut changed = t.title_source.as_deref() != Some(source);
    t.title_source = Some(source.into());
    if auto {
        let title = title_of(line_title, a, &t.agent);
        if title != t.title {
            t.title = title;
            changed = true;
        }
    }
    changed
}

fn doing_of(a: &Value) -> Option<String> {
    a["workLog"].as_array()?.iter().rev().find_map(|e| {
        let label = some(e["label"].as_str())?;
        let verb = |from: &str, to: &str| label.strip_prefix(from).map(|f| format!("{to}{f}"));
        match e["kind"].as_str()? {
            "edit" => verb("Edit ", "Editing ")
                .or_else(|| verb("Write ", "Writing "))
                .or_else(|| verb("Delete ", "Deleting "))
                .or_else(|| Some(label.to_string())),
            "read" => verb("Read ", "Reading ").or_else(|| Some(label.to_string())),
            "search" => Some(format!("Searching {label}")),
            "bash" => Some(label.to_string()),
            _ => None,
        }
        .map(|d| naming::shorten(&d, 80))
    })
}

pub fn progress_of(a: &Value) -> Option<Value> {
    let todos = a["todos"].as_array().map(Vec::as_slice).unwrap_or_default();
    let done = todos.iter().filter(|t| t["state"] == "completed").count();
    let note = todos
        .iter()
        .find(|t| t["state"] == "in_progress")
        .and_then(|t| some(t["text"].as_str()))
        .map(str::to_string)
        .or_else(|| doing_of(a));
    if todos.is_empty() && note.is_none() {
        return None;
    }
    let mut p = json!({"done": done, "total": todos.len()});
    if let Some(n) = note {
        p["note"] = json!(n);
    }
    Some(p)
}

fn model_for(l: &Line, a: &Value) -> Option<String> {
    let launch = model_of(&l.launch_args);
    if !launch.is_empty() {
        return Some(launch);
    }
    text(a, "model").map(str::to_string)
}

pub fn model_of(args: &[String]) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(m) = a.strip_prefix("--model=") {
            return m.to_string();
        }
        if a == "--model" || a == "-m" {
            return it.next().cloned().unwrap_or_default();
        }
    }
    String::new()
}

pub fn new_task(l: &Line, project: String, a: Option<&Value>) -> Task {
    let null = Value::Null;
    let a = a.unwrap_or(&null);
    let now = crate::util::now_ms();
    let mut t = Task {
        id: ulid(now),
        title: title_of(l.title.as_deref(), a, l.agent),
        title_source: Some(tasks::TITLE_AUTO.into()),
        description: text(a, "goal").unwrap_or_default().to_string(),
        project,
        status: "pending".into(),
        agent: l.agent.into(),
        model: model_for(l, a).unwrap_or_default(),
        effort: "high".into(),
        ticket_key: l.ticket.clone(),
        line_id: Some(l.id.clone()),
        branch: (!l.branch.is_empty()).then(|| l.branch.clone()),
        worktree_path: Some(path_of(l).to_string()).filter(|p| !p.is_empty()),
        progress: progress_of(a),
        created_ms: now,
        updated_ms: now,
        rank: now as f64,
        origin: Some(ORIGIN.into()),
        ..Default::default()
    };
    let state = session_state(l, worked(l, &t));
    follow(&mut t, state, || true);
    t
}

fn absorb(t: &mut Task, l: &Line, a: &Value) -> bool {
    let mut changed = false;
    if t.description.is_empty() {
        if let Some(g) = text(a, "goal") {
            t.description = g.to_string();
            changed = true;
        }
    }
    changed |= retitle(t, l.title.as_deref(), a);
    if let Some(m) = model_for(l, a).filter(|m| *m != t.model) {
        t.model = m;
        changed = true;
    }
    if let Some(p) = progress_of(a).filter(|p| t.progress.as_ref() != Some(p)) {
        t.progress = Some(p);
        changed = true;
    }
    changed
}

pub fn refresh(t: &mut Task, line: Option<&Line>, env: &Env) -> bool {
    let Some(l) = line else {
        let changed = t.title_source.is_none() && retitle(t, None, &Value::Null);
        let landed = !status::is_done(&t.status) && (env.merged)(t) && follow(t, "merged", || true);
        let gone = !landed && t.status == status::IN_PROGRESS && follow(t, "closed", || true);
        return landed || gone || changed;
    };
    let mut changed = false;
    if l.last_activity_ms > t.updated_ms || t.title_source.is_none() {
        let a = (env.activity)(l).unwrap_or(Value::Null);
        changed |= absorb(t, l, &a);
    }
    if !l.branch.is_empty() && t.branch.as_deref() != Some(l.branch.as_str()) {
        t.branch = Some(l.branch.clone());
        changed = true;
    }
    if l.ticket.is_some() && t.ticket_key != l.ticket {
        t.ticket_key = l.ticket.clone();
        changed = true;
    }
    let mut state = session_state(l, worked(l, t));
    if state == "busy" && t.session_state.as_deref() != Some("busy") {
        if let Some(b) = &t.branch {
            crate::worktrees::forget_landed(b);
        }
    }
    if matches!(state, "lost" | "done") && !status::is_done(&t.status) && (env.merged)(t) {
        state = "merged";
    }
    let branched = t.worktree_path.as_deref().is_some_and(|w| w != t.project);
    follow(t, state, || branched || edited((env.activity)(l))) || changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_800_000_000_000;

    fn line(id: &str, state: &'static str) -> Line {
        Line {
            id: id.into(),
            agent: "claude",
            session_id: format!("s-{id}"),
            state,
            branch: "feat/x".into(),
            worktree_path: "/repo".into(),
            last_activity_ms: NOW - 1000,
            pid: Some(1),
            ..Default::default()
        }
    }

    fn reg(tag: &str) -> (Registry, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "sb-session-tasks-{tag}-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (Registry::open(&dir).unwrap(), dir)
    }

    fn sync_with(reg: &Registry, lines: &[Line], days: i64, a: Option<Value>) {
        let project = |_: &Line| Some("/repo".to_string());
        let activity = move |_: &Line| a.clone();
        let merged = |_: &Task| false;
        run(
            reg,
            lines,
            &Env {
                now_ms: NOW,
                days,
                station: "st",
                project: &project,
                activity: &activity,
                merged: &merged,
            },
        );
    }

    fn session_tasks(reg: &Registry) -> Vec<Task> {
        tasks::all(reg)
            .unwrap()
            .into_iter()
            .filter(Task::from_session)
            .collect()
    }

    #[test]
    fn one_task_per_line_across_scans() {
        let (r, dir) = reg("idem");
        let lines = [line("a", "active"), line("b", "idle")];
        let a = json!({"goal": "Fix the login page", "todos": []});
        sync_with(&r, &lines, 7, Some(a.clone()));
        sync_with(&r, &lines, 7, Some(a.clone()));
        sync_with(&r, &lines[..1], 7, Some(a));
        let got = session_tasks(&r);
        assert_eq!(got.len(), 2);
        let mut ids: Vec<_> = got.iter().filter_map(|t| t.line_id.clone()).collect();
        ids.sort();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(got[0].project, "/repo");
        assert_eq!(got[0].origin.as_deref(), Some("session"));
        assert_eq!(got[0].branch.as_deref(), Some("feat/x"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lines_already_linked_to_a_task_get_none() {
        let (r, dir) = reg("linked");
        let mut own = Task {
            id: "own".into(),
            title: "mine".into(),
            project: "/repo".into(),
            status: "in_progress".into(),
            agent: "claude".into(),
            effort: "high".into(),
            line_id: Some("a".into()),
            ..Default::default()
        };
        tasks::insert(&r, &mut own, "st").unwrap();
        sync_with(&r, &[line("a", "active")], 7, None);
        assert!(session_tasks(&r).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn only_recent_lines_within_the_window() {
        let (r, dir) = reg("window");
        let mut old = line("old", "held");
        old.pid = None;
        old.held_reason = Some("lost");
        old.last_activity_ms = NOW - 8 * DAY_MS;
        let mut recent = old.clone();
        recent.id = "recent".into();
        recent.last_activity_ms = NOW - 6 * DAY_MS;
        let mut alive = line("alive", "idle");
        alive.last_activity_ms = NOW - 30 * DAY_MS;
        sync_with(&r, &[old.clone(), recent.clone(), alive.clone()], 7, None);
        let mut ids: Vec<_> = session_tasks(&r)
            .into_iter()
            .filter_map(|t| t.line_id)
            .collect();
        ids.sort();
        assert_eq!(ids, ["alive", "recent"]);
        assert!(eligible(&old, NOW, 9));
        assert!(!eligible(&recent, NOW, 0));
        let mut closed = recent.clone();
        closed.state = "hung-up";
        assert!(!eligible(&closed, NOW, 7));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn title_fallback_chain() {
        let full =
            json!({"aiTitle": "AI title", "summary": "Summary", "goal": "p", "lastPrompt": "q"});
        assert_eq!(title_of(Some("line"), &full, "claude"), "AI title");
        let a = json!({"summary": "Summary", "goal": "p"});
        assert_eq!(title_of(Some("Line title"), &a, "claude"), "Line title");
        assert_eq!(title_of(None, &a, "claude"), "Summary");
        let a = json!({"goal": "  fix the login\nmore", "lastPrompt": "q"});
        assert_eq!(title_of(None, &a, "claude"), "Fix the login");
        let a = json!({"goal": "<command-name>/clear</command-name>", "lastPrompt": "<system-reminder>x</system-reminder>\nwire the relay timeout. thanks"});
        assert_eq!(title_of(None, &a, "claude"), "Wire the relay timeout");
        let long = "please refactor the whole authentication module so that tokens are refreshed";
        let t = title_of(None, &json!({"goal": long}), "claude");
        assert!(t.chars().count() <= 61, "{t}");
        assert!(t.ends_with('…'));
        assert!(t.starts_with("Please refactor the whole"));
        assert_eq!(
            title_of(None, &json!({"goal": "   "}), "codex"),
            "New Codex session"
        );
        assert_eq!(
            title_of(None, &Value::Null, "claude"),
            "New Claude Code session"
        );
        assert_eq!(title_of(None, &Value::Null, "zzz"), "New zzz session");
    }

    #[test]
    fn retitles_live_but_keeps_user_titles() {
        let mut t = new_task(&line("a", "active"), "/repo".into(), None);
        assert_eq!(t.title, "New Claude Code session");
        assert_eq!(t.title_source.as_deref(), Some(tasks::TITLE_AUTO));
        let l = line("a", "active");
        assert!(absorb(
            &mut t,
            &l,
            &json!({"goal": "add a health endpoint"})
        ));
        assert_eq!(t.title, "Add a health endpoint");
        assert!(absorb(
            &mut t,
            &l,
            &json!({"goal": "add a health endpoint", "aiTitle": "Health endpoint"})
        ));
        assert_eq!(t.title, "Health endpoint");
        t.title_source = Some(tasks::TITLE_USER.into());
        t.title = "Mine".into();
        absorb(&mut t, &l, &json!({"aiTitle": "Something else"}));
        assert_eq!(t.title, "Mine");
    }

    #[test]
    fn heals_legacy_fallback_titles() {
        let mut l = line("a", "active");
        l.branch = "release/next".into();
        let legacy = |title: &str| Task {
            title: title.into(),
            agent: "claude".into(),
            origin: Some(ORIGIN.into()),
            line_id: Some("a".into()),
            ..Default::default()
        };
        let mut t = legacy("Claude Code session in main");
        absorb(&mut t, &l, &Value::Null);
        assert_eq!(t.title, "New Claude Code session");
        assert_eq!(t.title_source.as_deref(), Some(tasks::TITLE_AUTO));
        let mut t = legacy("Claude Code session in main");
        absorb(&mut t, &l, &json!({"aiTitle": "Relay timeout"}));
        assert_eq!(t.title, "Relay timeout");
        let mut t = legacy("My own name");
        absorb(&mut t, &l, &json!({"aiTitle": "Relay timeout"}));
        assert_eq!(t.title, "My own name");
        assert_eq!(t.title_source.as_deref(), Some(tasks::TITLE_USER));
        let mut t = legacy("Old AI title");
        absorb(&mut t, &l, &json!({"aiTitle": "Old AI title", "goal": "x"}));
        assert_eq!(t.title_source.as_deref(), Some(tasks::TITLE_AUTO));
    }

    #[test]
    fn progress_notes_and_model_from_the_transcript() {
        let a = json!({"todos": [], "workLog": [
            {"kind": "read", "label": "Read lib.rs"},
            {"kind": "edit", "label": "Edit daemon.rs"},
            {"kind": "other", "label": "mcp__x__y"}]});
        assert_eq!(
            progress_of(&a),
            Some(json!({"done": 0, "total": 0, "note": "Editing daemon.rs"}))
        );
        let a = json!({"workLog": [{"kind": "search", "label": "fn login"}]});
        assert_eq!(progress_of(&a).unwrap()["note"], "Searching fn login");
        assert_eq!(progress_of(&json!({"todos": []})), None);
        let mut l = line("a", "active");
        let a = json!({"model": "claude-opus-5-5"});
        assert_eq!(new_task(&l, "/r".into(), Some(&a)).model, "claude-opus-5-5");
        l.launch_args = vec!["--model".into(), "sonnet".into()];
        assert_eq!(new_task(&l, "/r".into(), Some(&a)).model, "sonnet");
    }

    #[test]
    fn status_mapping_table() {
        let cases: &[(&str, &str, &str, Option<&str>)] = &[
            ("closed", "in_progress", "completed", None),
            ("merged", "in_testing", "merged", None),
            ("merged", "in_progress", "merged", None),
            ("held", "merged", "merged", Some("paused")),
            ("lost", "in_progress", "in_testing", None),
            ("done", "in_progress", "in_testing", None),
            ("done", "in_testing", "in_testing", None),
            ("done", "pending", "completed", None),
            ("lost", "pending", "completed", None),
            ("done", "completed", "completed", None),
            ("done", "merged", "merged", None),
            ("lost", "merged", "merged", None),
            ("busy", "merged", "in_progress", None),
            ("busy", "in_testing", "in_progress", None),
            ("input", "in_progress", "in_progress", Some("needs_input")),
            ("fresh", "pending", "in_progress", Some("needs_input")),
            ("held", "in_progress", "in_progress", Some("paused")),
            ("held", "pending", "in_progress", Some("paused")),
            ("held", "in_testing", "in_testing", Some("paused")),
            ("held", "completed", "completed", Some("paused")),
        ];
        for (state, current, want, badge) in cases {
            assert_eq!(
                status_for(state, current, true),
                (*want, *badge),
                "{state} from {current}"
            );
        }
        assert_eq!(
            status_for("done", "in_progress", false),
            ("completed", None)
        );
        let mut l = line("a", "active");
        assert_eq!(session_state(&l, false), "fresh");
        l.busy_known = true;
        assert_eq!(session_state(&l, true), "done");
        l.busy = true;
        assert_eq!(session_state(&l, true), "busy");
        l.needs_input = true;
        assert_eq!(session_state(&l, true), "input");
        l = line("a", "active");
        assert_eq!(session_state(&l, true), "busy");
        l.state = "idle";
        assert_eq!(session_state(&l, true), "done");
        l.state = "held";
        l.held_reason = Some("user");
        assert_eq!(session_state(&l, true), "held");
        l.held_reason = Some("lost");
        assert_eq!(session_state(&l, true), "lost");
        l.state = "hung-up";
        assert_eq!(session_state(&l, true), "closed");
    }

    #[test]
    fn manual_moves_hold_until_the_session_works_again() {
        let mut t = Task {
            status: "in_progress".into(),
            ..Default::default()
        };
        let yes = || true;
        assert!(follow(&mut t, "busy", yes));
        assert!(!follow(&mut t, "busy", yes));
        t.status = "completed".into();
        t.manual_status = Some("completed".into());
        assert!(!follow(&mut t, "busy", yes));
        assert_eq!(t.status, "completed");
        follow(&mut t, "done", yes);
        assert_eq!(t.status, "completed");
        assert_eq!(t.manual_status.as_deref(), Some("completed"));
        follow(&mut t, "busy", yes);
        assert_eq!(t.status, "in_progress");
        assert_eq!(t.manual_status, None);
        t.status = "in_progress".into();
        t.manual_status = Some("in_progress".into());
        follow(&mut t, "done", yes);
        assert_eq!(t.status, "in_testing");
        assert_eq!(t.manual_status, None);
        t.status = "in_progress".into();
        t.manual_status = Some("in_progress".into());
        assert!(!follow(&mut t, "done", || unreachable!()));
        assert_eq!(t.status, "in_progress");
        follow(&mut t, "closed", yes);
        assert_eq!(t.status, "completed");
    }

    #[test]
    fn scan_respects_manual_status_and_tracks_progress() {
        let (r, dir) = reg("manual");
        let mut l = line("a", "active");
        l.busy_known = true;
        l.busy = true;
        let a = json!({"goal": "do it", "todos": [
            {"text": "one", "state": "completed"},
            {"text": "two", "state": "in_progress"},
            {"text": "three", "state": "pending"}]});
        sync_with(&r, std::slice::from_ref(&l), 7, Some(a.clone()));
        let mut t = session_tasks(&r).remove(0);
        assert_eq!(t.status, "in_progress");
        assert_eq!(t.title, "Do it");
        assert_eq!(
            t.progress,
            Some(json!({"done": 1, "total": 3, "note": "two"}))
        );
        t.status = "in_testing".into();
        t.manual_status = Some("in_testing".into());
        tasks::store(&r, &mut t, "st").unwrap();
        sync_with(&r, std::slice::from_ref(&l), 7, Some(a.clone()));
        assert_eq!(session_tasks(&r)[0].status, "in_testing");
        l.busy = false;
        sync_with(&r, std::slice::from_ref(&l), 7, Some(a.clone()));
        assert_eq!(session_tasks(&r)[0].status, "in_testing");
        l.state = "hung-up";
        sync_with(&r, &[l], 7, Some(a));
        let t = &session_tasks(&r)[0];
        assert_eq!(t.status, "completed");
        assert_eq!(t.manual_status, None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn idle_session_on_a_landed_branch_moves_to_merged_once() {
        let mut l = line("a", "idle");
        l.busy_known = true;
        let a = json!({"goal": "ship it", "editedFiles": ["/repo/a.rs"]});
        let mut t = new_task(&l, "/repo".into(), Some(&a));
        t.status = "in_testing".into();
        let landed = std::cell::Cell::new(false);
        let project = |_: &Line| Some("/repo".to_string());
        let activity = |_: &Line| Some(a.clone());
        let merged = |_: &Task| landed.get();
        let env = Env {
            now_ms: NOW,
            days: 7,
            station: "st",
            project: &project,
            activity: &activity,
            merged: &merged,
        };
        refresh(&mut t, Some(&l), &env);
        assert_eq!(t.status, "in_testing");
        landed.set(true);
        assert!(refresh(&mut t, Some(&l), &env));
        assert_eq!(t.status, "merged");
        t.status = "in_testing".into();
        t.manual_status = Some("in_testing".into());
        refresh(&mut t, Some(&l), &env);
        assert_eq!(t.status, "in_testing");
        l.busy = true;
        refresh(&mut t, Some(&l), &env);
        assert_eq!(t.status, "in_progress");
    }

    #[test]
    fn a_session_gone_from_the_scan_closes_its_in_progress_task_once() {
        let (r, dir) = reg("gone");
        let mut fresh = line("fresh", "active");
        fresh.busy_known = true;
        let mut review = line("review", "idle");
        review.busy_known = true;
        review.title = Some("Ship it".into());
        let live = line("live", "active");
        sync_with(&r, &[fresh, review, live.clone()], 7, None);
        let by = |id: &str| {
            session_tasks(&r)
                .into_iter()
                .find(|t| t.line_id.as_deref() == Some(id))
                .unwrap()
        };
        assert_eq!(by("fresh").status, "in_progress");
        assert_eq!(by("fresh").badge.as_deref(), Some("needs_input"));
        let mut t = by("review");
        t.status = "in_testing".into();
        tasks::store(&r, &mut t, "st").unwrap();
        sync_with(&r, &[], 7, None);
        assert_eq!(by("fresh").status, "in_progress");
        sync_with(&r, std::slice::from_ref(&live), 7, None);
        let gone = by("fresh");
        assert_eq!(gone.status, "completed");
        assert_eq!(gone.badge, None);
        assert_eq!(gone.session_state.as_deref(), Some("closed"));
        assert_eq!(by("review").status, "in_testing");
        assert_eq!(by("live").status, "in_progress");
        sync_with(&r, std::slice::from_ref(&live), 7, None);
        assert_eq!(by("fresh").updated_ms, gone.updated_ms);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_vanished_session_on_a_landed_branch_still_moves_to_merged() {
        let l = line("a", "active");
        let mut t = new_task(&l, "/repo".into(), None);
        let project = |_: &Line| Some("/repo".to_string());
        let activity = |_: &Line| None;
        let merged = |_: &Task| true;
        let env = Env {
            now_ms: NOW,
            days: 7,
            station: "st",
            project: &project,
            activity: &activity,
            merged: &merged,
        };
        assert!(refresh(&mut t, None, &env));
        assert_eq!(t.status, "merged");
    }

    #[test]
    fn sessions_first_seen_ended_go_straight_to_completed() {
        let (r, dir) = reg("first-seen");
        let mut lost = line("lost", "held");
        lost.pid = None;
        lost.held_reason = Some("lost");
        lost.last_activity_ms = NOW - 2 * DAY_MS;
        let mut idle = line("idle", "idle");
        idle.busy_known = true;
        let a = json!({"goal": "fix it", "editedFiles": ["/repo/a.rs"]});
        sync_with(&r, &[lost.clone(), idle.clone()], 7, Some(a.clone()));
        sync_with(&r, &[lost, idle], 7, Some(a));
        let got = session_tasks(&r);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|t| t.status == "completed"), "{got:?}");
        assert!(got.iter().all(|t| t.manual_status.is_none()));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn observe_end(tag: &str, a: Value, worktree: &str) -> String {
        let (r, dir) = reg(tag);
        let mut l = line("a", "active");
        l.worktree_path = worktree.into();
        l.busy_known = true;
        l.busy = true;
        sync_with(&r, std::slice::from_ref(&l), 7, Some(a.clone()));
        assert_eq!(session_tasks(&r)[0].status, "in_progress");
        l.busy = false;
        l.last_activity_ms += 500;
        sync_with(&r, std::slice::from_ref(&l), 7, Some(a.clone()));
        let status = session_tasks(&r)[0].status.clone();
        let before = session_tasks(&r)[0].updated_ms;
        sync_with(&r, &[l], 7, Some(a));
        assert_eq!(session_tasks(&r)[0].updated_ms, before);
        let _ = std::fs::remove_dir_all(dir);
        status
    }

    #[test]
    fn observed_end_goes_to_review_only_with_changes() {
        let edits = json!({"goal": "fix it", "editedFiles": ["/repo/a.rs"]});
        let none = json!({"goal": "what is it", "editedFiles": []});
        assert_eq!(observe_end("edits", edits, "/repo"), "in_testing");
        assert_eq!(observe_end("readonly", none.clone(), "/repo"), "completed");
        assert_eq!(observe_end("worktree", none, "/repo-wt"), "in_testing");
    }

    #[test]
    fn models_come_from_the_launch_args() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(model_of(&args(&["--model", "opus"])), "opus");
        assert_eq!(model_of(&args(&["-x", "--model=gpt-5"])), "gpt-5");
        assert_eq!(model_of(&args(&["-m", "o3"])), "o3");
        assert_eq!(model_of(&args(&["--resume"])), "");
    }
}
