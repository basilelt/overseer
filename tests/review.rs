mod common;

use common::{bin, read_until, wait, wait_for, Fx, Reap};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

struct App {
    fx: Fx,
    _d: Reap,
}

impl App {
    fn new(tag: &str) -> App {
        let fx = Fx::new(tag, 0);
        let cfg = json!({"stationId": "vm-local", "autoConcurrency": 0,
                         "worktreeDir": fx.base.join("worktrees")});
        std::fs::write(fx.state.join("config.json"), cfg.to_string()).unwrap();
        let mut c = Command::new(bin());
        fx.env(&mut c);
        c.env("SB_BIN", bin())
            .env("SB_STUB_LOG", fx.base.join("stub.log"))
            .env("SHELL", "/bin/sh")
            .args(["daemon", "--interval", "1"]);
        let d = Reap::spawn(c.stderr(Stdio::null()));
        wait(|| UnixStream::connect(&fx.sock).is_ok());
        App { fx, _d: d }
    }

    fn req(&self, method: &str, target: &str, body: Value) -> (u16, Value) {
        let b = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        let (code, out) = self.fx.req(method, target, &b);
        (code, serde_json::from_str(&out).unwrap_or(Value::Null))
    }

    fn ok(&self, method: &str, target: &str, body: Value) -> Value {
        let (code, v) = self.req(method, target, body);
        assert!((200..300).contains(&code), "{method} {target}: {code} {v}");
        v
    }

    fn ws(&self, id: &str) -> WebSocket<UnixStream> {
        let s = UnixStream::connect(&self.fx.sock).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        tungstenite::client(format!("ws://localhost/v1/terms/{id}/io"), s)
            .unwrap()
            .0
    }

    fn thread(&self, task: &str, id: &str) -> Value {
        self.ok(
            "GET",
            &format!("/v1/review/comments?task={task}"),
            Value::Null,
        )
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .cloned()
        .unwrap()
    }
}

fn events(sock: &Path) -> mpsc::Receiver<Value> {
    let (tx, rx) = mpsc::channel();
    let mut s = UnixStream::connect(sock).unwrap();
    write!(s, "GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(s).lines() {
            let Ok(l) = l else { return };
            if let Some(d) = l.strip_prefix("data: ") {
                if let Ok(v) = serde_json::from_str::<Value>(d) {
                    if tx.send(v).is_err() {
                        return;
                    }
                }
            }
        }
    });
    rx
}

fn next_event(rx: &mpsc::Receiver<Value>, f: impl Fn(&Value) -> bool) -> Value {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let e = rx.recv_timeout(left).expect("event timed out");
        if f(&e) {
            return e;
        }
    }
}

fn mcp(app: &App, input: &[Value]) -> Vec<Value> {
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
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

fn tool(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": name, "arguments": args}})
}

#[test]
fn live_review_comments_reach_the_agent_and_come_back() {
    let app = App::new("review");
    let ev = events(&app.fx.sock);
    let project = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "review me", "project": project}),
    );
    let task = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{task}/start"), json!({}));
    let wt = s["worktreePath"].as_str().unwrap().to_string();
    let term = s["termId"].as_str().unwrap().to_string();
    let line = s["lineId"].as_str().unwrap().to_string();
    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");

    let w = |p: &str, c: &str| std::fs::write(Path::new(&wt).join(p), c).unwrap();
    w("a.txt", "one\ntwo\nthree\n");
    common::git(Path::new(&wt), &["commit", "-qam", "more"]);
    w("a.txt", "one\ntwo\nthree\nfour\n");
    w("new.txt", "hello\nworld\n");

    let r = app.ok("GET", &format!("/v1/review?task={task}"), Value::Null);
    assert_eq!(r["worktree"], wt.as_str());
    assert_eq!(r["branch"], "task/1-review-me");
    assert_eq!(r["base"], "feature/AS-42-board");
    let files: Vec<(&str, &str, u64)> = r["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap(),
                f["status"].as_str().unwrap(),
                f["additions"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        files,
        [("a.txt", "modified", 3), ("new.txt", "untracked", 2)]
    );
    let by_line = app.ok("GET", &format!("/v1/review?line={line}"), Value::Null);
    assert_eq!(by_line["revision"], r["revision"]);
    let f = app.ok(
        "GET",
        &format!("/v1/review/file?task={task}&path=a.txt"),
        Value::Null,
    );
    assert_eq!(f["lines"], json!(["one", "two", "three", "four"]));
    assert_eq!(
        app.req(
            "GET",
            &format!("/v1/review/file?task={task}&path=..%2Fx"),
            Value::Null
        )
        .0,
        400
    );
    assert_eq!(app.req("GET", "/v1/review", Value::Null).0, 400);
    assert_eq!(r["triage"]["byKind"], json!({"logic": 5}));
    assert_eq!(r["files"][0]["hunks"][0]["tags"][0]["kind"], "logic");
    assert_eq!(r["files"][0]["risk"], 100);
    let g = app.ok("GET", &format!("/v1/review/graph?task={task}"), Value::Null);
    assert_eq!(g["revision"], r["revision"]);
    assert_eq!(g["order"], json!(["a.txt", "new.txt"]));

    let hash = r["files"][1]["hash"].as_str().unwrap();
    app.ok(
        "PUT",
        "/v1/review/viewed",
        json!({"taskId": task, "path": "new.txt", "hash": hash, "viewed": true}),
    );
    let r = app.ok("GET", &format!("/v1/review?task={task}"), Value::Null);
    assert_eq!(r["files"][1]["viewed"], true);
    assert_eq!(r["files"][0]["viewed"], false);

    let t1 = app.ok(
        "POST",
        "/v1/review/comments",
        json!({"taskId": task, "path": "a.txt", "line": 2, "endLine": 3,
               "body": "Spell it out.\n```suggestion\nTWO\nTHREE\n```"}),
    );
    assert_eq!(t1["status"], "pending");
    assert_eq!(t1["code"], "two\nthree");
    assert_eq!(t1["comments"][0]["pending"], true);
    let id1 = t1["id"].as_str().unwrap().to_string();
    let t2 = app.ok(
        "POST",
        "/v1/review/comments",
        json!({"worktree": wt, "path": "new.txt", "line": 1, "body": "Say hi instead."}),
    );
    let id2 = t2["id"].as_str().unwrap().to_string();
    assert_eq!(t2["taskId"], task.as_str());
    let reply = app.ok(
        "POST",
        "/v1/review/comments",
        json!({"threadId": id1, "body": "Also keep the order."}),
    );
    assert_eq!(reply["comments"].as_array().unwrap().len(), 2);
    let draft = reply["comments"][1]["id"].as_str().unwrap().to_string();
    app.ok(
        "PATCH",
        &format!("/v1/review/comments/{draft}"),
        json!({"body": "Keep the order."}),
    );
    assert_eq!(
        app.req(
            "POST",
            "/v1/review/comments",
            json!({"taskId": task, "path": "a.txt", "line": 9, "body": "x"})
        )
        .0,
        400
    );
    let gone = app.ok(
        "POST",
        "/v1/review/comments",
        json!({"taskId": task, "path": "a.txt", "line": 1, "body": "never mind"}),
    );
    let gid = gone["id"].as_str().unwrap();
    app.ok("DELETE", &format!("/v1/review/comments/{gid}"), Value::Null);
    next_event(&ev, |e| {
        e["type"] == "review.thread_deleted" && e["data"]["id"] == gid
    });
    assert_eq!(
        app.ok(
            "GET",
            &format!("/v1/review/comments?task={task}&status=pending"),
            Value::Null
        )
        .as_array()
        .unwrap()
        .len(),
        2
    );

    ws.send(Message::binary(b"busy\r".to_vec())).unwrap();
    wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["line"]["id"] == line.as_str() && l["local"]["busy"] == true)
            .then_some(())
    });
    let sub = app.ok(
        "POST",
        "/v1/review/submit",
        json!({"taskId": task, "kind": "request_changes", "body": "Nearly there."}),
    );
    assert_eq!(sub["delivery"]["status"], "queued", "{sub}");
    assert_eq!(sub["delivery"]["reason"], "the agent is busy");
    assert_eq!(sub["delivery"]["target"], format!("term:{term}"));
    let text = sub["text"].as_str().unwrap();
    assert!(
        text.starts_with("Code review: changes requested on 2 files (3 comments)."),
        "{text}"
    );
    assert!(
        text.contains(&format!(
            "1. a.txt:2-3 (id {id1})\n   > two\n   > three\n   Spell it out.\n   Suggested change:"
        )),
        "{text}"
    );
    assert!(
        text.contains("   -two\n   -three\n   +TWO\n   +THREE\n   Keep the order."),
        "{text}"
    );
    assert!(text.ends_with("Overall: Nearly there."));
    assert!(sub["threads"]
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["status"] == "sent"
            && t["comments"]
                .as_array()
                .unwrap()
                .iter()
                .all(|c| c["pending"] == false)));
    assert_eq!(
        app.req("DELETE", &format!("/v1/review/comments/{id2}"), Value::Null)
            .0,
        409
    );

    ws.send(Message::binary(b"done\r".to_vec())).unwrap();
    let d = next_event(&ev, |e| {
        e["type"] == "review.delivery" && e["data"]["delivery"]["status"] == "delivered"
    });
    assert_eq!(d["data"]["worktree"], wt.as_str());
    read_until(&mut ws, "Code review: changes requested on 2 files");
    assert_eq!(app.thread(&task, &id1)["delivery"]["status"], "delivered");
    let again = app.ok(
        "POST",
        "/v1/review/submit",
        json!({"taskId": task, "kind": "approve"}),
    );
    assert_eq!(again["delivery"]["status"], "none");

    w("a.txt", "zero\none\ntwo\nthree\nfour\n");
    let ch = next_event(&ev, |e| {
        e["type"] == "review.changed" && e["data"]["worktree"] == wt.as_str()
    });
    assert_eq!(ch["data"]["files"], json!(["a.txt"]));
    wait(|| app.thread(&task, &id1)["line"] == 3);
    let t = app.thread(&task, &id1);
    assert_eq!(
        (t["endLine"].as_u64(), t["changed"].as_bool()),
        (Some(4), Some(false))
    );
    assert_eq!(t["originalLine"], 2);
    w("a.txt", "zero\none\nTwo!\nthree\nfour\n");
    wait(|| app.thread(&task, &id1)["changed"] == true);
    let t = app.thread(&task, &id1);
    assert_eq!(
        (t["line"].as_u64(), t["endLine"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(
        app.req(
            "POST",
            &format!("/v1/review/comments/{id1}/apply"),
            Value::Null
        )
        .0,
        409
    );
    w("a.txt", "zero\none\ntwo\nthree\nfour\n");
    wait(|| app.thread(&task, &id1)["changed"] == false);

    let out = mcp(
        &app,
        &[
            json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}}),
            tool(1, "list_review_comments", json!({"worktree": wt})),
            tool(
                2,
                "resolve_review_comment",
                json!({"id": id2, "note": "Now says hi."}),
            ),
            tool(
                3,
                "list_review_comments",
                json!({"worktree": wt, "status": "addressed"}),
            ),
        ],
    );
    let listed = &out[1]["result"]["structuredContent"]["threads"];
    assert_eq!(listed.as_array().unwrap().len(), 2, "{listed}");
    assert_eq!(out[2]["result"]["isError"], false, "{}", out[2]);
    assert_eq!(out[2]["result"]["structuredContent"]["status"], "addressed");
    assert_eq!(
        out[2]["result"]["structuredContent"]["comments"][1],
        json!({"author": "agent", "body": "Now says hi.", "pending": false,
               "id": out[2]["result"]["structuredContent"]["comments"][1]["id"],
               "createdAt": out[2]["result"]["structuredContent"]["comments"][1]["createdAt"],
               "updatedAt": out[2]["result"]["structuredContent"]["comments"][1]["updatedAt"]})
    );
    let addressed = &out[3]["result"]["structuredContent"]["threads"];
    assert_eq!(addressed[0]["id"], id2.as_str());

    let applied = app.ok(
        "POST",
        &format!("/v1/review/comments/{id1}/apply"),
        Value::Null,
    );
    assert_eq!(applied["status"], "resolved");
    assert_eq!(
        std::fs::read_to_string(Path::new(&wt).join("a.txt")).unwrap(),
        "zero\none\nTWO\nTHREE\nfour\n"
    );
    app.ok(
        "PATCH",
        &format!("/v1/review/comments/{id2}"),
        json!({"status": "resolved"}),
    );
    let open = app.ok(
        "PATCH",
        &format!("/v1/review/comments/{id2}"),
        json!({"status": "open"}),
    );
    assert_eq!(open["status"], "sent");
    assert_eq!(
        app.req(
            "PATCH",
            &format!("/v1/review/comments/{id2}"),
            json!({"status": "nope"})
        )
        .0,
        400
    );
}

#[test]
fn delivery_pastes_into_the_agents_tmux_pane() {
    let t = common::T::new();
    let wt = t.wt.to_string_lossy().into_owned();
    common::git(&t.wt, &["config", "user.name", "Dev"]);
    std::fs::write(t.wt.join("a.txt"), "one\n").unwrap();
    common::git(&t.wt, &["add", "-A"]);
    common::git(&t.wt, &["commit", "-qm", "init"]);
    std::fs::write(t.wt.join("a.txt"), "one\ntwo\n").unwrap();
    let pane = t.start_agent("a", common::SID, "claude --model opus");
    let sock =
        std::env::temp_dir().join(format!("sb-test-review-tmux-{}.sock", std::process::id()));
    std::fs::create_dir_all(t.base.join("state")).unwrap();
    let mut c = Command::new(bin());
    t.env(&mut c);
    c.env("OVERSEER_SOCKET", &sock)
        .env("OVERSEER_REVIEW_POLL", "1")
        .args(["daemon", "--interval", "1"]);
    let _d = Reap::spawn(c.stderr(Stdio::null()));
    wait(|| UnixStream::connect(&sock).is_ok());
    let ev = events(&sock);
    let req = |m: &str, p: &str, b: Value| {
        let s = UnixStream::connect(&sock).unwrap();
        let body = if b.is_null() {
            String::new()
        } else {
            b.to_string()
        };
        let (code, out) = common::exchange(s, m, p, &body, "unix");
        (
            code,
            serde_json::from_str::<Value>(&out).unwrap_or(Value::Null),
        )
    };
    let (code, th) = req(
        "POST",
        "/v1/review/comments",
        json!({"worktree": wt, "path": "a.txt", "line": 2, "body": "Two is fine, but say why."}),
    );
    assert_eq!(code, 201, "{th}");
    let (code, sub) = req(
        "POST",
        "/v1/review/submit",
        json!({"worktree": wt, "kind": "comment"}),
    );
    assert_eq!(code, 200, "{sub}");
    let did = sub["delivery"]["id"].as_str().unwrap().to_string();
    let id = th["id"].as_str().unwrap().to_string();
    wait_for(|| {
        let (_, v) = req(
            "GET",
            &format!("/v1/review/comments?worktree={wt}"),
            Value::Null,
        );
        (v[0]["delivery"]["status"] == "delivered").then_some(())
    });
    let (_, v) = req(
        "GET",
        &format!("/v1/review/comments?worktree={wt}"),
        Value::Null,
    );
    assert_eq!(v[0]["delivery"]["id"], did.as_str());
    assert_eq!(v[0]["delivery"]["target"], format!("tmux:{pane}"));
    t.wait(|| t.screen(&pane).contains("Code review: 1 file (1 comment)."));
    assert!(t.screen(&pane).contains(&format!("a.txt:2 (id {id})")));
    std::thread::sleep(Duration::from_millis(2600));
    std::fs::write(t.wt.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let ch = next_event(&ev, |e| e["type"] == "review.changed");
    assert_eq!(ch["data"]["files"], json!(["a.txt"]));
    let _ = std::fs::remove_file(&sock);
}
