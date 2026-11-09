mod common;

use common::{bin, output, read_until, send, wait_for, App};
use serde_json::{json, Value};
use std::process::Command;

fn quorum_app(tag: &str) -> (App, String) {
    let app = App::new(tag, json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    (app, wt)
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn children(app: &App, parent: &str) -> Vec<Value> {
    app.ok("GET", "/v1/tasks", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["parentId"] == parent)
        .cloned()
        .collect()
}

fn messages(app: &App, task: &str) -> Vec<String> {
    app.ok("GET", &format!("/v1/tasks/{task}/messages"), Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["body"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_quorum_is_created_validated_and_edited_before_start() {
    let (app, wt) = quorum_app("quorum-create");
    let q = json!({"workers": ["codex", "gemini"], "reviewers": ["claude", "codex"]});
    let root = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Big job", "project": wt, "agent": "claude", "quorum": q}),
    );
    assert_eq!(root["quorum"], q);
    for (body, msg) in [
        (json!({"quorum": "x"}), "quorum must be an object or null"),
        (
            json!({"quorum": {"workers": [], "reviewers": ["codex"]}}),
            "quorum.workers must be a non-empty array of agent ids",
        ),
        (
            json!({"quorum": {"workers": ["codex"], "reviewers": []}}),
            "quorum.reviewers must list 1 or 2 agent ids",
        ),
        (
            json!({"quorum": {"workers": ["nope"], "reviewers": ["codex"]}}),
            "unknown agent nope",
        ),
        (
            json!({"quorum": {"workers": ["shell"], "reviewers": ["codex"]}}),
            "shell cannot join a quorum",
        ),
        (
            json!({"quorum": {"workers": ["codex", "codex"], "reviewers": ["claude"]}}),
            "quorum.workers lists codex twice",
        ),
        (
            json!({"agent": "gemini", "quorum": {"workers": ["codex"], "reviewers": ["claude"]}}),
            "the quorum orchestrator must be claude or codex",
        ),
        (
            json!({"parentId": id(&root), "agent": "claude"}),
            "agent claude is not a worker of this quorum (workers: codex, gemini)",
        ),
    ] {
        let mut b = body.clone();
        b["title"] = json!("x");
        b["project"] = json!(wt);
        let (code, v) = app.req("POST", "/v1/tasks", b);
        assert_eq!(
            (code, v["error"].as_str().unwrap_or_default()),
            (400, msg),
            "{body}"
        );
    }
    let (code, v) = app.req(
        "POST",
        "/v1/tasks",
        json!({"title": "x", "project": wt, "parentId": id(&root), "agent": "codex",
               "quorum": {"workers": ["codex"], "reviewers": ["claude"]}}),
    );
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (400, "a subtask cannot run a quorum")
    );
    let path = format!("/v1/tasks/{}", id(&root));
    let p = app.ok(
        "PATCH",
        &path,
        json!({"quorum": {"workers": ["gemini"], "reviewers": ["codex"]}}),
    );
    assert_eq!(
        p["quorum"],
        json!({"workers": ["gemini"], "reviewers": ["codex"]})
    );
    let (code, v) = app.req("PATCH", &path, json!({"agent": "gemini"}));
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (400, "the quorum orchestrator must be claude or codex")
    );
    let p = app.ok("PATCH", &path, json!({"quorum": null}));
    assert!(p.get("quorum").is_none());
    app.ok("PATCH", &path, json!({"status": "in_testing"}));
    let (code, v) = app.req("PATCH", &path, json!({"quorum": q}));
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (409, "the quorum can only change before the task starts")
    );
}

#[test]
fn a_ready_subtask_gets_reviewers_and_a_majority_accepts_it() {
    let (app, wt) = quorum_app("quorum-vote");
    let root = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Big job", "project": wt, "agent": "claude",
               "quorum": {"workers": ["codex", "gemini"], "reviewers": ["gemini", "codex"]}}),
    );
    let rid = id(&root);
    let w = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Parser", "description": "write the parser", "project": wt,
               "parentId": rid, "origin": "agent"}),
    );
    assert_eq!(
        (w["agent"].as_str(), w["status"].as_str()),
        (Some("codex"), Some("auto"))
    );
    let wid = id(&w);
    let (code, v) = app.req(
        "POST",
        &format!("/v1/tasks/{wid}/votes"),
        json!({"from": rid, "approve": true, "reason": "ok"}),
    );
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (409, "no vote is open on task #2")
    );

    app.ok(
        "PATCH",
        &format!("/v1/tasks/{wid}"),
        json!({"status": "in_testing"}),
    );
    let w = app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
    assert_eq!(
        w["votes"],
        json!({"round": 1, "voters": 3, "needed": 2, "votes": [], "outcome": null})
    );
    let reviewers = children(&app, &wid);
    assert_eq!(reviewers.len(), 2);
    for (r, agent) in reviewers.iter().zip(["gemini", "codex"]) {
        assert_eq!(r["origin"], "reviewer");
        assert_eq!(r["status"], "auto");
        assert_eq!(r["agent"], agent);
        assert!(r["description"]
            .as_str()
            .unwrap()
            .contains("write the parser"));
        assert!(r["description"]
            .as_str()
            .unwrap()
            .contains("overseer vote approve"));
    }
    let ready = messages(&app, &rid);
    assert!(
        ready[0].starts_with("Quorum: subtask #2 \"Parser\" (codex) is ready for review, round 1"),
        "{ready:?}"
    );

    let vote = |from: &str, approve: bool, reason: &str| {
        app.req(
            "POST",
            &format!("/v1/tasks/{wid}/votes"),
            json!({"from": from, "approve": approve, "reason": reason}),
        )
    };
    let (code, v) = vote(&wid, true, "me");
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (403, "task #2 is not a voter on task #2")
    );
    for (body, msg) in [
        (
            json!({"from": rid, "reason": "x"}),
            "approve must be a boolean",
        ),
        (
            json!({"from": rid, "approve": true, "reason": " "}),
            "reason required",
        ),
        (
            json!({"approve": true, "reason": "x"}),
            "from must be the voter's task id",
        ),
    ] {
        let (code, v) = app.req("POST", &format!("/v1/tasks/{wid}/votes"), body);
        assert_eq!((code, v["error"].as_str().unwrap()), (400, msg));
    }
    let r1 = id(&reviewers[0]);
    let after = vote(&r1, false, "missing tests").1;
    assert_eq!(after["votes"]["outcome"], Value::Null);
    let (code, v) = vote(&r1, true, "again");
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (409, "task #3 already voted")
    );
    vote(&rid, true, "looks right");
    let done = vote(&id(&reviewers[1]), true, "good").1;
    assert_eq!(done["votes"]["outcome"], "accepted");
    assert_eq!(done["votes"]["votes"].as_array().unwrap().len(), 3);
    assert_eq!(done["votes"]["votes"][1]["role"], "orchestrator");
    let verdict = messages(&app, &rid).pop().unwrap();
    assert!(
        verdict.contains("is accepted in round 1 (2 of 3 approve, 2 needed)"),
        "{verdict}"
    );
    assert!(verdict.contains("reject - missing tests"), "{verdict}");
    for r in children(&app, &wid) {
        assert_eq!(r["status"], "completed");
    }
    let (code, v) = vote(&rid, false, "late");
    assert_eq!(
        (code, v["error"].as_str().unwrap()),
        (409, "the vote on task #2 is closed (accepted)")
    );

    app.ok(
        "PATCH",
        &format!("/v1/tasks/{wid}"),
        json!({"status": "pending"}),
    );
    app.ok(
        "PATCH",
        &format!("/v1/tasks/{wid}"),
        json!({"status": "in_testing"}),
    );
    let w = app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
    assert_eq!(w["votes"]["round"], 2);
    assert_eq!(children(&app, &wid).len(), 4);
}

#[test]
fn one_reviewer_and_the_orchestrator_must_both_approve_and_vote_from_the_cli() {
    let (app, wt) = quorum_app("quorum-cli");
    let root = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Job", "project": wt, "agent": "codex",
               "quorum": {"workers": ["gemini"], "reviewers": ["opencode"]}}),
    );
    let w = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Part", "project": wt, "parentId": id(&root), "agent": "gemini"}),
    );
    let wid = id(&w);
    app.ok(
        "PATCH",
        &format!("/v1/tasks/{wid}"),
        json!({"status": "in_testing"}),
    );
    let r = children(&app, &wid).pop().unwrap();
    let vote = |task: &str, args: &[&str]| {
        let mut c = Command::new(bin());
        app.fx.env(&mut c);
        c.env("OVERSEER_TASK_ID", task).arg("vote").args(args);
        output(&mut c, None)
    };
    let o = vote(&id(&r), &["maybe", "x"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("usage: overseer vote"));
    let o = vote(&id(&r), &["reject", "breaks", "the", "build"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        "vote recorded on task #2: rejected"
    );
    let w = app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
    assert_eq!(w["votes"]["votes"][0]["reason"], "breaks the build");
    assert_eq!(w["votes"]["outcome"], "rejected");
    let o = vote(&id(&root), &["approve", "late", "--task", &wid]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("is closed (rejected)"));
    let verdict = app
        .ok(
            "GET",
            &format!("/v1/tasks/{}/messages", id(&root)),
            Value::Null,
        )
        .as_array()
        .unwrap()
        .last()
        .unwrap()["body"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        verdict.contains(&format!("send_message (to: {wid})")),
        "{verdict}"
    );
}

#[test]
fn subtasks_and_reviewers_start_on_their_own_and_a_silent_reviewer_rejects() {
    let app = App::new("quorum-live", json!({"autoConcurrency": 1}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let log = || std::fs::read_to_string(app.fx.base.join("stub.log")).unwrap_or_default();
    let root = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Job", "project": wt, "agent": "claude",
               "quorum": {"workers": ["gemini"], "reviewers": ["opencode"]}}),
    );
    let o = app.ok("POST", &format!("/v1/tasks/{}/start", id(&root)), json!({}));
    let mut ws = app.ws(o["termId"].as_str().unwrap());
    read_until(&mut ws, "Quorum mode: you are the orchestrator");
    let w = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Part", "project": wt, "parentId": id(&root)}),
    );
    let wid = id(&w);
    let w = wait_for(|| {
        let t = app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
        (t["status"] == "in_progress").then_some(t)
    });
    assert_eq!(w["agent"], "gemini");
    let wtp = w["worktreePath"].as_str().unwrap().to_string();
    app.ok(
        "PATCH",
        &format!("/v1/tasks/{wid}"),
        json!({"status": "in_testing"}),
    );
    let line = wait_for(|| {
        log()
            .lines()
            .find(|l| l.contains(" opencode "))
            .map(str::to_string)
    });
    let pid = line.split(' ').next().unwrap();
    let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).unwrap();
    assert_eq!(cwd.to_string_lossy(), wtp);
    let r = children(&app, &wid).pop().unwrap();
    let term = wait_for(|| {
        let t = app.ok("GET", &format!("/v1/tasks/{}", id(&r)), Value::Null);
        t["termId"].as_str().map(str::to_string)
    });
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    let w = wait_for(|| {
        let t = app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
        (t["votes"]["outcome"] == "rejected").then_some(t)
    });
    assert_eq!(w["votes"]["votes"][0]["reason"], "ended without voting");
    assert_eq!(
        app.ok("GET", &format!("/v1/tasks/{}", id(&r)), Value::Null)["status"],
        "completed"
    );
}

#[test]
fn a_rejected_worker_goes_back_to_work_and_its_next_turn_starts_round_two() {
    let app = App::new("quorum-round", json!({"autoConcurrency": 2}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let root = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Job", "project": wt, "agent": "claude",
               "quorum": {"workers": ["claude"], "reviewers": ["opencode"]}}),
    );
    let w = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Part", "project": wt, "parentId": id(&root)}),
    );
    let wid = id(&w);
    let get = || app.ok("GET", &format!("/v1/tasks/{wid}"), Value::Null);
    let term = wait_for(|| get()["termId"].as_str().map(str::to_string));
    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");
    let turn = |ws: &mut _| {
        send(ws, "busy\n");
        send(ws, "done\n");
    };
    turn(&mut ws);
    wait_for(|| (get()["votes"]["round"] == 1).then_some(()));
    let r = children(&app, &wid).pop().unwrap();
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    c.env("OVERSEER_TASK_ID", id(&r))
        .args(["vote", "reject", "no", "tests"]);
    assert!(output(&mut c, None).status.success());
    let w = get();
    assert_eq!(w["votes"]["outcome"], "rejected");
    assert_eq!(w["status"], "in_progress");
    turn(&mut ws);
    wait_for(|| (get()["votes"]["round"] == 2).then_some(()));
    assert_eq!(get()["status"], "in_testing");
    assert_eq!(children(&app, &wid).len(), 2);
}
