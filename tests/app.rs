mod common;

use common::{bin, output, read_until, send, wait, wait_for, App, Fx};
use serde_json::{json, Value};
use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tungstenite::Message;

#[cfg(target_os = "linux")]
fn tasks(pid: u64) -> Vec<String> {
    std::fs::read_dir(format!("/proc/{pid}/task"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn switches(pid: u64) -> u64 {
    tasks(pid)
        .iter()
        .filter_map(|t| std::fs::read_to_string(format!("/proc/{pid}/task/{t}/status")).ok())
        .flat_map(|s| {
            s.lines()
                .filter_map(|l| l.strip_prefix("voluntary_ctxt_switches:"))
                .filter_map(|v| v.trim().parse::<u64>().ok())
                .collect::<Vec<_>>()
        })
        .sum()
}

#[cfg(target_os = "linux")]
#[test]
fn idle_term_sockets_sleep_and_release_their_threads() {
    let app = App::new("wsidle", json!({}));
    let pid = app.ok("GET", "/v1/version", Value::Null)["pid"]
        .as_u64()
        .unwrap();
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let ids: Vec<String> = (0..5)
        .map(|_| {
            app.ok(
                "POST",
                "/v1/terms",
                json!({"cwd": wt, "cmd": ["/bin/cat"], "cols": 80, "rows": 24}),
            )["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    std::thread::sleep(Duration::from_millis(500));
    assert!(app.fx.state.join("overseer.db-wal").exists());
    let before = tasks(pid).len();
    let mut socks: Vec<_> = ids.iter().map(|id| app.ws(id)).collect();
    for (i, ws) in socks.iter_mut().enumerate() {
        send(ws, &format!("hello-{i}\n"));
        read_until(ws, &format!("hello-{i}"));
    }
    let a = switches(pid);
    std::thread::sleep(Duration::from_secs(2));
    let woke = switches(pid) - a;
    assert!(woke < 120, "{woke} wakeups in 2 s with 5 idle sockets");

    let begin = Instant::now();
    send(&mut socks[0], "fast\n");
    read_until(&mut socks[0], "fast");
    assert!(begin.elapsed() < Duration::from_secs(2));

    app.ok(
        "POST",
        &format!("/v1/terms/{}/kill", ids[4]),
        json!({"confirm": true}),
    );
    let mut last = socks.pop().unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < end, "socket of a killed term stayed open");
        match last.read() {
            Ok(Message::Close(_)) => break,
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => break,
            Ok(_) => {}
        }
    }
    drop(last);
    drop(socks);
    wait(|| tasks(pid).len() <= before);
}

#[test]
fn terms_roundtrip_replay_resize_and_lifecycle() {
    let app = App::new("terms", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh"], "cols": 80, "rows": 24, "title": "sh"}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    assert!(t["pid"].as_u64().unwrap() > 0);
    assert_eq!(t["exited"], false);
    assert_eq!(t["cmd"], json!(["/bin/sh"]));
    let (code, _) = app.req("POST", "/v1/terms", json!({"cwd": "/nonexistent"}));
    assert_eq!(code, 400);

    let mut a = app.ws(&id);
    let mut b = app.ws(&id);
    send(&mut a, "echo hi-$((40+2))\n");
    read_until(&mut a, "hi-42");
    read_until(&mut b, "hi-42");
    a.send(Message::text(r#"{"resize":{"cols":100,"rows":30}}"#))
        .unwrap();
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["cols"] == 100);
    send(&mut a, "stty size\n");
    read_until(&mut a, "30 100");
    drop(a);
    drop(b);

    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"],
        false
    );
    let mut c = app.ws(&id);
    read_until(&mut c, "hi-42");
    send(&mut c, "echo again-$((1+1))\n");
    read_until(&mut c, "again-2");

    let r = app.ok(
        "POST",
        &format!("/v1/terms/{id}/resize"),
        json!({"cols": 90, "rows": 20}),
    );
    assert_eq!(
        (r["cols"].as_u64(), r["rows"].as_u64()),
        (Some(90), Some(20))
    );
    assert_eq!(app.ok("GET", "/v1/terms", Value::Null)[0]["id"], id);

    assert_eq!(
        app.req("DELETE", &format!("/v1/terms/{id}"), Value::Null).0,
        409
    );
    assert_eq!(
        app.req("POST", &format!("/v1/terms/{id}/kill"), json!({}))
            .0,
        400
    );
    assert_eq!(
        app.req(
            "POST",
            &format!("/v1/terms/{id}/kill"),
            json!({"confirm": true})
        )
        .0,
        202
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"] == true);
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        match c.read() {
            Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => break,
            Err(tungstenite::Error::Io(e)) if e.kind() != ErrorKind::WouldBlock => break,
            _ => assert!(Instant::now() < end, "no close frame"),
        }
    }
    let mut late = app.ws(&id);
    read_until(&mut late, "again-2");
    assert_eq!(
        app.req(
            "POST",
            &format!("/v1/terms/{id}/kill"),
            json!({"confirm": true})
        )
        .0,
        409
    );
    app.ok("DELETE", &format!("/v1/terms/{id}"), Value::Null);
    assert_eq!(
        app.req("GET", &format!("/v1/terms/{id}"), Value::Null).0,
        404
    );

    let ring = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh", "-c", "head -c 2400000 /dev/zero | tr '\\0' x; echo; echo END; sleep 30"]}),
    );
    let rid = ring["id"].as_str().unwrap().to_string();
    let mut w = app.ws(&rid);
    read_until(&mut w, "END");
    drop(w);
    let mut w = app.ws(&rid);
    let s = read_until(&mut w, "END");
    assert!(
        s.len() <= 2 << 20 && s.len() > (2 << 20) - 4096,
        "{}",
        s.len()
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{rid}/kill"),
        json!({"confirm": true}),
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn ports_kill_stops_a_background_server_started_in_a_term() {
    let app = App::new("portkill", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let script = r#"$s=IO::Socket::INET->new(Listen=>1,LocalAddr=>"127.0.0.1",LocalPort=>0) or die; print "PORT=",$s->sockport,"=\n"; sleep 120"#;
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh", "-c", format!("perl -MIO::Socket::INET -e '{script}' & wait")]}),
    );
    let term = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&term);
    let out = read_until(&mut ws, "=\r\n");
    let port: u64 = out
        .split("PORT=")
        .nth(1)
        .and_then(|s| s.split('=').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no port in {out:?}"));
    let row = wait_for(|| {
        app.ok("GET", "/v1/ports", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["port"] == port)
            .cloned()
    });
    assert_eq!(row["termId"], term);
    let pid = row["pid"].as_u64().unwrap();
    let path = format!("/v1/ports/{port}/kill");

    assert_eq!(app.req("POST", &path, json!({"pid": pid})).0, 400);
    assert_eq!(
        app.req("POST", &path, json!({"pid": pid + 1, "confirm": true}))
            .0,
        404
    );
    assert_eq!(
        app.req(
            "POST",
            "/v1/ports/1/kill",
            json!({"pid": pid, "confirm": true})
        )
        .0,
        404
    );
    let (code, v) = app.req("POST", &path, json!({"pid": pid, "confirm": true}));
    assert_eq!((code, v["pid"].as_u64()), (202, Some(pid)), "{v}");
    wait(|| {
        !app.ok("GET", "/v1/ports", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["port"] == port)
    });
    wait(|| app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["exited"] == true);
    assert_eq!(
        app.req("POST", &path, json!({"pid": pid, "confirm": true}))
            .0,
        404
    );
}

fn stub_log(app: &App) -> String {
    std::fs::read_to_string(app.fx.base.join("stub.log")).unwrap_or_default()
}

#[test]
fn tasks_crud_numbers_and_projects() {
    let app = App::new("tasks", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let sub = app.fx.wt.join("src");
    std::fs::create_dir_all(&sub).unwrap();
    let a = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "First", "project": sub, "ticketKey": "as-12"}),
    );
    assert_eq!(a["number"], 1);
    assert_eq!(a["project"], wt);
    assert_eq!(a["status"], "pending");
    assert_eq!(a["agent"], "claude");
    assert_eq!(a["effort"], "high");
    assert_eq!(a["model"], "");
    assert_eq!(a["station"], "vm-local");
    assert_eq!(a["ticketKey"], "AS-12");
    let id = a["id"].as_str().unwrap().to_string();
    let b = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Second", "project": wt, "status": "auto", "agent": "codex", "effort": "max", "model": "o3"}),
    );
    assert_eq!(b["number"], 2);
    for bad in [
        json!({"project": wt}),
        json!({"title": "x", "project": "/"}),
        json!({"title": "x", "project": wt, "status": "in_progress"}),
        json!({"title": "x", "project": wt, "effort": "huge"}),
    ] {
        assert_eq!(app.req("POST", "/v1/tasks", bad).0, 400);
    }
    let p = app.ok(
        "PATCH",
        &format!("/v1/tasks/{id}"),
        json!({"title": "First!", "progress": {"done": 1, "total": 3, "note": "n"}, "ticketKey": null, "status": "in_testing"}),
    );
    assert_eq!(p["title"], "First!");
    assert_eq!(p["progress"], json!({"done": 1, "total": 3, "note": "n"}));
    assert!(p.get("ticketKey").is_none());
    assert_eq!(p["status"], "in_testing");
    assert_eq!(
        app.req(
            "PATCH",
            &format!("/v1/tasks/{id}"),
            json!({"status": "in_progress"})
        )
        .0,
        400
    );
    let l = app.ok("GET", "/v1/tasks?status=in_testing,completed", Value::Null);
    assert_eq!(l.as_array().unwrap().len(), 1);
    let l = app.ok(
        "GET",
        &format!("/v1/tasks?project={}", wt.replace('/', "%2F")),
        Value::Null,
    );
    assert_eq!(l.as_array().unwrap().len(), 2);
    app.ok("DELETE", &format!("/v1/tasks/{id}"), Value::Null);
    assert_eq!(
        app.req("GET", &format!("/v1/tasks/{id}"), Value::Null).0,
        404
    );
    let c = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Third", "project": wt}),
    );
    assert_eq!(c["number"], 3);
    let projects = app.ok("GET", "/v1/projects", Value::Null);
    assert_eq!(
        projects,
        json!([{"path": wt, "name": "repo", "repo": "github.com/Acme/board", "missing": false, "defaultBranch": "feature/AS-42-board"}])
    );
    let agy = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Antigravity", "project": wt, "agent": "agy", "effort": "high"}),
    );
    let agy_id = agy["id"].as_str().unwrap();
    let agy = app.ok(
        "PATCH",
        &format!("/v1/tasks/{agy_id}"),
        json!({"effort": "xhigh"}),
    );
    assert_eq!(agy["effort"], "xhigh");
}

#[test]
fn a_websocket_head_that_arrives_late_still_attaches() {
    let app = App::new("latehead", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]}));
    let id = t["id"].as_str().unwrap().to_string();
    let s = UnixStream::connect(&app.fx.sock).unwrap();
    std::thread::sleep(Duration::from_secs(6));
    s.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let (mut ws, res) = tungstenite::client(format!("ws://localhost/v1/terms/{id}/io"), s).unwrap();
    assert_eq!(res.status(), 101);
    send(&mut ws, "echo late-$((40+2))\n");
    read_until(&mut ws, "late-42");
}

#[test]
fn task_start_runs_the_agent_in_a_term_and_goes_in_testing() {
    let app = App::new("start", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "busy", "description": "", "project": wt, "model": "opus"}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    assert_eq!(s["status"], "in_progress");
    assert_eq!(s["branch"], "task/1-busy");
    let wtp = s["worktreePath"].as_str().unwrap().to_string();
    assert!(
        wtp.contains("/worktrees/repo-") && wtp.ends_with("/task-1-busy"),
        "{wtp}"
    );
    assert!(Path::new(&wtp).join("a.txt").exists());
    let term = s["termId"].as_str().unwrap().to_string();
    let line = s["lineId"].as_str().unwrap().to_string();
    assert_eq!(
        app.req("POST", &format!("/v1/tasks/{id}/start"), json!({}))
            .0,
        409
    );

    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");
    wait(|| stub_log(&app).contains("--effort high"));
    let log = stub_log(&app);
    assert!(log.contains("--session-id"), "{log}");
    assert!(log.contains("--model opus"), "{log}");
    let lines = wait_for(|| {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        v.as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == line.as_str() && l["local"]["termId"] == term.as_str())
            .cloned()
    });
    assert_eq!(lines["line"]["branch"], "task/1-busy");
    wait(|| app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["lineId"] == line.as_str());

    wait_for(|| {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        v.as_array()
            .unwrap()
            .iter()
            .any(|l| l["line"]["id"] == line.as_str() && l["local"]["busy"] == true)
            .then_some(())
    });
    assert_eq!(app.task(&id)["status"], "in_progress");
    send(&mut ws, "done\r");
    wait_for(|| (app.task(&id)["status"] == "in_testing").then_some(()));

    let act = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
    assert_eq!(act["lineId"], line);
    assert!(act["workLog"].is_array() && act["todos"].is_array());

    let (code, _) = app.req(
        "POST",
        &format!("/v1/tasks/{id}/review"),
        json!({"action": "commit"}),
    );
    assert_eq!(code, 409);
    std::fs::write(Path::new(&wtp).join("b.txt"), "two\n").unwrap();
    let r = app.ok(
        "POST",
        &format!("/v1/tasks/{id}/review"),
        json!({"action": "commit"}),
    );
    assert_eq!(r["task"]["status"], "completed");
    assert_eq!(r["commit"].as_str().unwrap().len(), 40);
    let msg = common::git(Path::new(&wtp), &["log", "-1", "--format=%s"]);
    assert_eq!(msg.trim(), "task #1: busy");
    let r = app.ok(
        "POST",
        &format!("/v1/tasks/{id}/review"),
        json!({"action": "approve", "merge": true, "approvePr": true}),
    );
    assert_eq!(r["task"]["status"], "completed");
    read_until(&mut ws, "then merge it (`gh pr merge`)");
    app.ok(
        "POST",
        &format!("/v1/tasks/{id}/review"),
        json!({"action": "reject", "message": "busy"}),
    );
    assert_eq!(app.task(&id)["status"], "in_progress");
    assert_eq!(
        app.req("DELETE", &format!("/v1/tasks/{id}"), Value::Null).0,
        409
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait_for(|| (app.task(&id)["status"] == "in_testing").then_some(()));
    assert!(app.task(&id).get("error").is_none());
}

fn mcp_status(app: &App, own: &str, status: &str) -> Value {
    let msgs = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "set_task_status", "arguments": {"status": status}}}),
    ];
    let input: String = msgs.iter().map(|m| format!("{m}\n")).collect();
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    c.env("OVERSEER_TASK_ID", own).arg("mcp");
    let out = output(&mut c, Some(&input));
    let last = String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()
        .unwrap()
        .to_string();
    serde_json::from_str::<Value>(&last).unwrap()["result"].clone()
}

#[test]
fn a_landed_task_moves_to_merged_and_its_agent_can_resume_it() {
    let app = App::new("merged", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "land", "description": "", "project": wt}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    let wtp = s["worktreePath"].as_str().unwrap().to_string();
    let term = s["termId"].as_str().unwrap().to_string();
    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");

    let r = app.req(
        "PATCH",
        &format!("/v1/tasks/{id}"),
        json!({"status": "in_progress"}),
    );
    assert_eq!(r.0, 200, "a running task stays in progress: {r:?}");

    std::fs::write(Path::new(&wtp).join("land.txt"), "landed\n").unwrap();
    common::git(Path::new(&wtp), &["add", "-A"]);
    common::git(Path::new(&wtp), &["commit", "-q", "-m", "land"]);
    common::git(&app.fx.wt, &["merge", "-q", "--ff-only", "task/1-land"]);
    send(&mut ws, "done\r");
    wait_for(|| (app.task(&id)["status"] == "merged").then_some(()));
    let l = app.ok("GET", "/v1/tasks?status=merged", Value::Null);
    assert_eq!(l.as_array().unwrap().len(), 1, "{l}");

    let r = mcp_status(&app, &id, "in_progress");
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(app.task(&id)["status"], "in_progress");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(
        app.task(&id)["status"],
        "in_progress",
        "the last turn's Stop must not bounce a resumed task back to review"
    );

    let r = mcp_status(&app, &id, "in_testing");
    assert_eq!(r["isError"], false, "{r}");
    let r = mcp_status(&app, &id, "merged");
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(app.task(&id)["status"], "merged");
    let r = mcp_status(&app, &id, "completed");
    assert_eq!(r["isError"], true, "{r}");

    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["exited"] == true);
    let (code, body) = app.req(
        "PATCH",
        &format!("/v1/tasks/{id}"),
        json!({"status": "in_progress"}),
    );
    assert_eq!(code, 400, "{body}");
    assert!(
        body.to_string().contains("use POST /v1/tasks/{id}/start"),
        "{body}"
    );
    let p = app.ok(
        "PATCH",
        &format!("/v1/tasks/{id}"),
        json!({"status": "completed"}),
    );
    assert_eq!(p["status"], "completed");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert_eq!(
        app.task(&id)["status"],
        "completed",
        "a task closed by hand after it merged stays closed"
    );
}

#[test]
fn auto_runner_respects_concurrency_in_fifo_order() {
    let app = App::new("auto", json!({"autoConcurrency": 1}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let ids: Vec<String> = (0..3)
        .map(|i| {
            app.ok(
                "POST",
                "/v1/tasks",
                json!({"title": format!("job {i}"), "project": wt, "status": "auto"}),
            )["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let statuses = || -> Vec<String> {
        ids.iter()
            .map(|i| app.task(i)["status"].as_str().unwrap().to_string())
            .collect()
    };
    wait(|| statuses() == ["in_progress", "auto", "auto"]);
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(statuses(), ["in_progress", "auto", "auto"]);
    let term = app.task(&ids[0])["termId"].as_str().unwrap().to_string();
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| statuses() == ["in_testing", "in_progress", "auto"]);
    let term = app.task(&ids[1])["termId"].as_str().unwrap().to_string();
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| statuses() == ["in_testing", "in_testing", "in_progress"]);
    let term = app.task(&ids[2])["termId"].as_str().unwrap().to_string();
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| statuses() == ["in_testing", "in_testing", "in_testing"]);
}

#[test]
fn auto_runner_waits_for_dependencies_and_starts_from_their_branch() {
    let app = App::new("deps", json!({"autoConcurrency": 2}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let new = |title: &str, extra: Value| -> String {
        let mut b = json!({"title": title, "project": wt});
        for (k, v) in extra.as_object().unwrap() {
            b[k] = v.clone();
        }
        app.ok("POST", "/v1/tasks", b)["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let a = new("first", json!({}));
    let b = new("second", json!({"dependsOn": [a], "status": "auto"}));
    assert_eq!(app.task(&b)["dependsOn"], json!([a]));
    let url = |id: &str| format!("/v1/tasks/{id}");
    assert_eq!(app.req("PATCH", &url(&a), json!({"dependsOn": [b]})).0, 400);
    assert_eq!(app.req("PATCH", &url(&b), json!({"dependsOn": [b]})).0, 400);
    assert_eq!(
        app.req("PATCH", &url(&b), json!({"dependsOn": ["nope"]})).0,
        400
    );
    app.ok("PATCH", &url(&a), json!({"status": "auto"}));
    wait(|| app.task(&a)["status"] == "in_progress");
    let term = app.task(&a)["termId"].as_str().unwrap().to_string();
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.task(&a)["status"] == "in_testing");
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(app.task(&b)["status"], "auto");
    let (code, body) = app.req(
        "POST",
        &format!("/v1/tasks/{a}/review"),
        json!({"action": "approve", "merge": true}),
    );
    assert_eq!(code, 409, "{body}");
    assert_eq!(
        body["error"],
        "the agent is not running; start the task again"
    );
    assert_eq!(app.task(&a)["status"], "in_testing");
    let awt = app.task(&a)["worktreePath"].as_str().unwrap().to_string();
    std::fs::write(Path::new(&awt).join("dep.txt"), "from a\n").unwrap();
    app.ok(
        "POST",
        &format!("/v1/tasks/{a}/review"),
        json!({"action": "commit"}),
    );
    wait(|| app.task(&b)["status"] == "in_progress");
    let bwt = app.task(&b)["worktreePath"].as_str().unwrap().to_string();
    assert!(Path::new(&bwt).join("dep.txt").exists());

    let c = new("third", json!({}));
    let d = new("fourth", json!({"dependsOn": [c], "inheritBranch": false}));
    assert_eq!(app.task(&d)["inheritBranch"], false);
    app.ok("DELETE", &url(&c), Value::Null);
    assert_eq!(app.task(&d)["dependsOn"], json!([]));

    let e = new("fifth", json!({"dependsOn": ["#4", 1]}));
    assert_eq!(app.task(&e)["dependsOn"], json!([d, a]));
    assert_eq!(
        app.req("PATCH", &url(&e), json!({"dependsOn": [99]})).0,
        400
    );
}

#[test]
fn term_title_override_is_set_and_cleared() {
    // Given a running plain terminal
    // When a client renames it, then clears the custom title
    // Then the term reports the custom title next to its auto title, and none once cleared
    let app = App::new("termtitle", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]}));
    let id = t["id"].as_str().unwrap();
    assert_eq!(t["titleOverride"], Value::Null);
    let path = format!("/v1/terms/{id}/title");
    assert_eq!(app.req("PUT", &path, json!({})).0, 400);
    assert_eq!(
        app.req("PUT", "/v1/terms/nope/title", json!({"title": "x"}))
            .0,
        404
    );
    let v = app.ok("PUT", &path, json!({"title": " build watch "}));
    assert_eq!(
        (v["titleOverride"].as_str(), v["title"].as_str()),
        (Some("build watch"), Some("sh"))
    );
    let v = app.ok("GET", &format!("/v1/terms/{id}"), Value::Null);
    assert_eq!(v["titleOverride"], "build watch");
    let v = app.ok("PUT", &path, json!({"title": ""}));
    assert_eq!(v["titleOverride"], Value::Null);
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn events_cover_terms_and_tasks() {
    let app = App::new("events", json!({}));
    let mut s = UnixStream::connect(&app.fx.sock).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    use std::io::{Read, Write};
    s.write_all(b"GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    while !buf.contains("retry:") {
        let mut b = [0u8; 256];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]}));
    let id = t["id"].as_str().unwrap();
    let task = app.ok("POST", "/v1/tasks", json!({"title": "x", "project": wt}));
    let tid = task["id"].as_str().unwrap();
    app.ok("PATCH", &format!("/v1/tasks/{tid}"), json!({"title": "y"}));
    app.ok("DELETE", &format!("/v1/tasks/{tid}"), Value::Null);
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
    let end = Instant::now() + Duration::from_secs(10);
    let want = [
        "event: term.created",
        "event: task.created",
        "event: task.updated",
        "event: task.deleted",
        "event: term.exited",
    ];
    while !want.iter().all(|w| buf.contains(w)) {
        assert!(Instant::now() < end, "{buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
}

#[test]
fn resume_target_term_starts_the_resume_argv() {
    let fx_sid = "aaaaaaaa-0000-4000-8000-000000000000";
    let app = App::new("resume", json!({}));
    let ts = overseer::util::format_rfc3339(overseer::procs::now_ms() - 60_000);
    std::fs::write(
        app.fx
            .base
            .join(format!("home/.claude/projects/p/{fx_sid}.jsonl")),
        format!(
            "{}\n",
            json!({"type": "user", "sessionId": fx_sid, "cwd": app.fx.wt, "timestamp": ts})
        ),
    )
    .unwrap();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == fx_sid)
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let plan = app.ok(
        "POST",
        &format!("/v1/lines/{line}/resume?dryRun=true"),
        json!({"target": "term"}),
    );
    assert_eq!(plan["where"], "term");
    let before = app
        .ok("GET", "/v1/terms", Value::Null)
        .as_array()
        .unwrap()
        .len();
    let (code, e) = app.req(
        "POST",
        &format!("/v1/lines/{line}/resume"),
        json!({"target": "term", "attachOnly": true}),
    );
    assert_eq!(code, 409, "{e}");
    assert_eq!(
        app.ok("GET", "/v1/terms", Value::Null)
            .as_array()
            .unwrap()
            .len(),
        before,
        "an attach must never start the agent"
    );
    let (code, t) = app.req(
        "POST",
        &format!("/v1/lines/{line}/resume"),
        json!({"target": "term", "cols": 100, "rows": 30}),
    );
    assert_eq!(code, 201, "{t}");
    assert_eq!(t["lineId"], line);
    assert_eq!(t["cmd"], json!(["claude", "--resume", fx_sid]));
    let mut ws = app.ws(t["id"].as_str().unwrap());
    read_until(&mut ws, "❯");
    let tid = t["id"].as_str().unwrap();
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn hold_and_resume_work_on_every_cycle_of_the_same_line() {
    let sid = "bbbbbbbb-0000-4000-8000-000000000000";
    let app = App::new("holdcycles", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["claude", "--session-id", sid], "cols": 100, "rows": 30}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid && l["local"]["pid"].is_u64())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let state = || {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == line)
            .map(|l| l["line"]["state"].as_str().unwrap().to_string())
            .unwrap()
    };
    let resumes = || {
        stub_log(&app)
            .lines()
            .filter(|l| l.contains(&format!("--resume {sid}")))
            .count()
    };
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    for cycle in 1..=3 {
        let held = app.ok(
            "POST",
            &format!("/v1/lines/{line}/hold?dryRun=false"),
            Value::Null,
        );
        assert_eq!(held["action"], "held", "cycle {cycle}: {held}");
        wait(|| state() == "held");
        wait(|| line_json(&app, &line)["local"]["heldAt"].is_string());
        let mut ws = app.ws(&tid);
        read_until(&mut ws, "press Enter to resume");
        let (code, r) = app.req(
            "POST",
            &format!("/v1/lines/{line}/resume?dryRun=false"),
            json!({"target": "term"}),
        );
        assert_eq!(code, 200, "cycle {cycle}: {r}");
        assert_eq!(r["id"], tid.as_str());
        wait(|| resumes() == cycle);
        wait(|| state() != "held");
        wait(|| line_json(&app, &line)["local"]["heldAt"].is_null());
        let mut ws = app.ws(&tid);
        read_until(&mut ws, "❯");
    }
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

fn claude_term(app: &App, cmd: Value, sid: &str) -> (String, String) {
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": cmd, "cols": 100, "rows": 30}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid && l["local"]["pid"].is_u64())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    (tid, line)
}

fn line_json(app: &App, line: &str) -> Value {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["id"] == line)
        .cloned()
        .unwrap()
}

#[test]
fn holding_a_busy_line_after_the_turn_waits_for_the_turn_to_end() {
    // Given a claude session that is mid-turn
    // When it is held with afterTurn=true, then its turn ends
    // Then the hold stays pending (still alive) and applies on the busy to idle transition
    let sid = "dddddddd-0000-4000-8000-000000000001";
    let app = App::new("holdafter", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    let (code, v) = app.req(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true"),
        Value::Null,
    );
    assert_eq!(
        (code, v["action"].as_str()),
        (200, Some("hold-pending")),
        "{v}"
    );
    wait(|| line_json(&app, &line)["local"]["holdPending"] == true);
    assert_ne!(line_json(&app, &line)["line"]["state"], "held");
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    let l = line_json(&app, &line);
    assert!(l["local"]["holdPending"].is_null(), "{l}");
    assert_eq!(l["local"]["heldReason"], "user");
    let at = l["local"]["heldAt"].as_str().unwrap();
    assert!(at.ends_with('Z') && at.len() >= 20, "{at}");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_pending_hold_can_be_cancelled_and_never_applies() {
    // Given a busy claude session with a pending hold
    // When the pending hold is cancelled with DELETE and the turn ends
    // Then the line is never held
    let sid = "dddddddd-0000-4000-8000-000000000002";
    let app = App::new("holdcancel", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    assert_eq!(app.ok("POST", &hold, Value::Null)["action"], "hold-pending");
    wait(|| line_json(&app, &line)["local"]["holdPending"] == true);
    let (code, v) = app.req("DELETE", &format!("/v1/lines/{line}/hold"), Value::Null);
    assert_eq!(
        (code, v["holdPending"].as_bool()),
        (200, Some(false)),
        "{v}"
    );
    wait(|| line_json(&app, &line)["local"]["holdPending"].is_null());
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == false);
    std::thread::sleep(Duration::from_secs(3));
    let l = line_json(&app, &line);
    assert_ne!(l["line"]["state"], "held", "{l}");
    assert!(l["local"]["heldAt"].is_null(), "{l}");
    let (code, v) = app.req("DELETE", "/v1/lines/nope-nope/hold", Value::Null);
    assert_eq!(code, 404, "{v}");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn after_turn_holds_an_idle_line_now_and_still_refuses_a_pinned_one() {
    // Given an idle claude session
    // When it is held with afterTurn=true, once pinned and once not
    // Then the pinned line is refused (nothing pending) and the other is held at once
    let sid = "dddddddd-0000-4000-8000-000000000004";
    let app = App::new("holdidle", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    app.ok(
        "PUT",
        &format!("/v1/lines/{line}/pin"),
        json!({"pinned": true}),
    );
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    let (code, v) = app.req("POST", &hold, Value::Null);
    assert_eq!(code, 409, "{v}");
    assert_eq!(v["predicates"]["notPinned"]["ok"], false);
    app.ok(
        "PUT",
        &format!("/v1/lines/{line}/pin"),
        json!({"pinned": false}),
    );
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == false);
    let (code, v) = app.req("POST", &hold, Value::Null);
    assert_eq!((code, v["action"].as_str()), (200, Some("held")), "{v}");
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

fn child_pid(app: &App, name: &str) -> i32 {
    let f = app.fx.base.join(format!("stub.log.{name}"));
    wait_for(|| std::fs::read_to_string(&f).ok()?.trim().parse().ok())
}

fn gone(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) != 0 }
}

#[test]
fn holding_a_line_stops_the_processes_its_agent_left_behind() {
    // Given an idle agent whose MCP-like children outlive it, one of them ignoring SIGTERM
    // When the line is held
    // Then both children are gone, and the term still runs the held stub
    let sid = "dddddddd-0000-4000-8000-000000000005";
    let app = App::with_env(
        "holdtree",
        json!({}),
        &[("OVERSEER_HOLD_TIMEOUT_MS", "1500")],
    );
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "mcp\r");
    let polite = child_pid(&app, "mcp");
    send(&mut ws, "mcpstubborn\r");
    let stubborn = child_pid(&app, "mcpstubborn");
    assert!(!gone(polite) && !gone(stubborn));
    let held = app.ok(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false"),
        Value::Null,
    );
    assert_eq!(held["action"], "held", "{held}");
    assert!(held["steps"].to_string().contains("got SIGKILL"), "{held}");
    wait(|| gone(polite) && gone(stubborn));
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "press Enter to resume");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_pending_hold_blocked_for_good_is_dropped_and_the_refusal_is_recorded() {
    // Given a busy session with a pending hold
    // When the line is pinned meanwhile and its turn ends
    // Then the pending hold is dropped (not held, nothing queued) and the refusal is logged
    let sid = "dddddddd-0000-4000-8000-000000000006";
    let app = App::new("holdblocked", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    assert_eq!(app.ok("POST", &hold, Value::Null)["action"], "hold-pending");
    wait(|| line_json(&app, &line)["local"]["holdPending"] == true);
    app.ok(
        "PUT",
        &format!("/v1/lines/{line}/pin"),
        json!({"pinned": true}),
    );
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &line)["local"]["holdPending"].is_null());
    let l = line_json(&app, &line);
    assert_ne!(l["line"]["state"], "held", "{l}");
    let log = std::fs::read_to_string(app.fx.state.join("actions.log")).unwrap();
    assert!(
        log.contains("hold-pending-refused") && log.contains("notPinned"),
        "{log}"
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_pending_hold_is_refused_when_the_subagents_never_finish() {
    // Given a claude session whose turn ended while a subagent still runs, with a pending hold
    // When the subagent never stops and the bounded subagent wait (2 s) passes
    // Then the pending hold is dropped and the refusal is logged
    let sid = "dddddddd-0000-4000-8000-000000000007";
    let app = App::with_env(
        "holdsublost",
        json!({}),
        &[("OVERSEER_HOLD_SUBAGENT_MS", "2000")],
    );
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    send(&mut ws, "sub\r");
    wait(|| line_json(&app, &line)["local"]["subagents"] == 1);
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    assert_eq!(app.ok("POST", &hold, Value::Null)["action"], "hold-pending");
    send(&mut ws, "done\r");
    let logged = || std::fs::read_to_string(app.fx.state.join("actions.log")).unwrap_or_default();
    wait(|| logged().contains("hold-pending-refused"));
    wait(|| line_json(&app, &line)["local"]["holdPending"].is_null());
    let l = line_json(&app, &line);
    assert_ne!(l["line"]["state"], "held", "{l}");
    let log = logged();
    assert!(
        log.contains("hold-pending-refused") && log.contains("subagents"),
        "{log}"
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_pending_hold_waits_for_the_subagents_after_the_turn_ended() {
    // Given a claude session whose turn ended while one subagent still runs, with a pending hold
    // When twice the grace period (2 s) passes, then the subagent stops
    // Then local.subagents shows 1, the hold stays pending meanwhile and applies after the stop
    let sid = "dddddddd-0000-4000-8000-000000000006";
    let app = App::with_env("holdsub", json!({}), &[("OVERSEER_HOLD_GRACE_MS", "2000")]);
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    assert_eq!(line_json(&app, &line)["local"]["subagents"], 0);
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    send(&mut ws, "sub\r");
    wait(|| line_json(&app, &line)["local"]["subagents"] == 1);
    let hold = format!("/v1/lines/{line}/hold?dryRun=false&afterTurn=true");
    assert_eq!(app.ok("POST", &hold, Value::Null)["action"], "hold-pending");
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == false);
    std::thread::sleep(Duration::from_secs(4));
    let l = line_json(&app, &line);
    assert_eq!(l["local"]["subagents"], 1, "{l}");
    assert_eq!(l["local"]["holdPending"], true, "{l}");
    assert_ne!(l["line"]["state"], "held", "{l}");
    send(&mut ws, "unsub\r");
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    let l = line_json(&app, &line);
    assert_eq!(l["local"]["subagents"], 0, "{l}");
    assert!(l["local"]["holdPending"].is_null(), "{l}");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn holding_without_after_turn_still_refuses_a_busy_line() {
    // Given a busy claude session
    // When it is held without afterTurn
    // Then the hold is refused as before and nothing is pending
    let sid = "dddddddd-0000-4000-8000-000000000003";
    let app = App::new("holdbusy", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    send(&mut ws, "busy\r");
    wait(|| line_json(&app, &line)["local"]["busy"] == true);
    let (code, v) = app.req(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false"),
        Value::Null,
    );
    assert_eq!(code, 409, "{v}");
    assert_eq!(v["predicates"]["atPrompt"]["ok"], false);
    assert!(line_json(&app, &line)["local"]["holdPending"].is_null());
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn closing_an_agent_session_with_shell_leaves_a_shell_in_its_term() {
    // Given a claude session that is the root process of its term
    // When the session is closed as the panel does, with shell=true
    // Then the term keeps running a shell that shows how to resume the session,
    // detached from the line, and killing the term still ends it
    let sid = "cccccccc-0000-4000-8000-000000000000";
    let app = App::new("closeshell", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    let r = app.ok(
        "POST",
        &format!("/v1/lines/{line}/close?dryRun=false&shell=true"),
        Value::Null,
    );
    assert_eq!(r["action"], "closed", "{r}");
    let mut ws = app.ws(&tid);
    read_until(
        &mut ws,
        &format!("claude closed. Session {sid}, resume with:"),
    );
    let term = app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null);
    assert_eq!(
        (&term["exited"], &term["agent"], &term["lineId"]),
        (&json!(false), &Value::Null, &Value::Null)
    );
    send(&mut ws, "echo SHELL-$((40+3))\r");
    read_until(&mut ws, "SHELL-43");
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null)["exited"] == true);
}

#[test]
fn closing_a_resumed_session_with_shell_prints_the_resume_note() {
    // Given a claude session that was held and resumed, so it runs under the stub wrapper
    // When it is closed as the panel does, with shell=true
    // Then the term shows the closed note and a shell, like a never-held session
    let sid = "cccccccc-0000-4000-8000-000000000001";
    let app = App::new("closeresumed", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false"),
        Value::Null,
    );
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "press Enter to resume");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/resume?dryRun=false"),
        json!({"target": "term"}),
    );
    wait(|| line_json(&app, &line)["line"]["state"] != "held");
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/close?dryRun=false&shell=true"),
        Value::Null,
    );
    let mut ws = app.ws(&tid);
    read_until(
        &mut ws,
        &format!("claude closed. Session {sid}, resume with:"),
    );
    let term = app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null);
    assert_eq!(
        (&term["exited"], &term["agent"], &term["lineId"]),
        (&json!(false), &Value::Null, &Value::Null)
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn closing_an_agent_session_without_shell_still_ends_its_term() {
    // Given a claude session that is the root process of its term
    // When the session is closed without shell=true, as closing its tab does
    // Then the term exits
    let sid = "dddddddd-0000-4000-8000-000000000000";
    let app = App::new("closenoshell", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    let r = app.ok(
        "POST",
        &format!("/v1/lines/{line}/close?dryRun=false"),
        Value::Null,
    );
    assert_eq!(r["action"], "closed", "{r}");
    wait(|| app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null)["exited"] == true);
}

#[test]
fn closing_an_agent_session_started_from_a_shell_keeps_that_shell() {
    // Given a claude session started from the shell of a term
    // When the session is closed with shell=true
    // Then the shell keeps running in the term, detached from the line
    let sid = "eeeeeeee-0000-4000-8000-000000000000";
    let app = App::new("closenested", json!({}));
    let (tid, line) = claude_term(
        &app,
        json!([
            "/bin/sh",
            "-c",
            format!("claude --session-id {sid}; exec /bin/sh")
        ]),
        sid,
    );
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    let r = app.ok(
        "POST",
        &format!("/v1/lines/{line}/close?dryRun=false&shell=true"),
        Value::Null,
    );
    assert_eq!(r["action"], "closed", "{r}");
    wait(|| app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null)["lineId"].is_null());
    send(&mut ws, "echo SHELL-$((40+4))\r");
    read_until(&mut ws, "SHELL-44");
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{tid}"), Value::Null)["exited"],
        false
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn mcp_server_talks_to_the_daemon() {
    let app = App::new("mcp", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let own = app.ok("POST", "/v1/tasks", json!({"title": "own", "project": wt}));
    let own_id = own["id"].as_str().unwrap();
    let msgs = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "create_task", "arguments": {"title": "child"}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "update_task_progress", "arguments": {"done": 2, "total": 4, "note": "half"}}}),
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "set_task_status", "arguments": {"status": "completed"}}}),
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "set_task_status", "arguments": {"status": "in_testing"}}}),
    ];
    let input: String = msgs.iter().map(|m| format!("{m}\n")).collect();
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    c.env("OVERSEER_TASK_ID", own_id).arg("mcp");
    let out = output(&mut c, Some(&input));
    let replies: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(replies.len(), 6, "{replies:?}");
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 19);
    let child = &replies[2]["result"]["structuredContent"];
    assert_eq!(child["status"], "pending");
    assert_eq!(child["origin"], "agent");
    assert_eq!(child["parentId"], own_id);
    assert_eq!(child["project"], wt);
    assert_eq!(replies[3]["result"]["isError"], false);
    assert_eq!(replies[4]["result"]["isError"], true);
    assert_eq!(replies[5]["result"]["isError"], false);
    let t = app.task(own_id);
    assert_eq!(t["status"], "in_testing");
    assert_eq!(
        t["progress"],
        json!({"done": 2, "total": 4, "note": "half"})
    );
}

#[test]
fn version_socket_path_and_detach_is_idempotent() {
    let fx = Fx::new("detach", 0);
    let run = |args: &[&str]| {
        let mut c = Command::new(bin());
        fx.env(&mut c);
        output(c.args(args), None)
    };
    let v: Value = serde_json::from_slice(&run(&["version", "--json"]).stdout).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["protocol"], 1);
    assert_eq!(v["os"], std::env::consts::OS);
    assert!(!v["commit"].as_str().unwrap().is_empty());
    let sp = String::from_utf8_lossy(&run(&["socket-path"]).stdout).into_owned();
    assert_eq!(sp.trim(), fx.sock.to_string_lossy());

    let pidfile = fx.state.join("daemon.pid");
    let mut w = Command::new("sh");
    w.args([
        "-c",
        &format!(
            "while kill -0 {} 2>/dev/null; do sleep 1; done; kill $(cat '{}') 2>/dev/null",
            std::process::id(),
            pidfile.display()
        ),
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    std::thread::spawn(move || w.spawn().and_then(|mut c| c.wait()));
    struct Kill(std::path::PathBuf);
    impl Drop for Kill {
        fn drop(&mut self) {
            if let Ok(p) = std::fs::read_to_string(&self.0) {
                let _ = Command::new("kill").arg(p.trim()).status();
            }
        }
    }
    let _kill = Kill(pidfile.clone());
    let first = run(&["daemon", "--detach", "--interval", "1"]);
    assert!(first.status.success(), "{}", common::err(&first));
    assert_eq!(String::from_utf8_lossy(&first.stdout).trim(), sp.trim());
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    let start = Instant::now();
    let second = run(&["daemon", "--detach"]);
    assert!(second.status.success());
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(String::from_utf8_lossy(&second.stdout).trim(), sp.trim());
    assert_eq!(std::fs::read_to_string(&pidfile).unwrap(), pid);
    let (code, body) = fx.req("GET", "/v1/version", "");
    assert_eq!(code, 200);
    let mut live = serde_json::from_str::<Value>(&body).unwrap();
    assert_eq!(live["epoch"], 0);
    assert_eq!(live["pid"].to_string(), pid.trim());
    let o = live.as_object_mut().unwrap();
    o.remove("epoch");
    o.remove("pid");
    assert_eq!(live, v);
}

#[test]
fn chat_images_are_served_from_the_transcript() {
    let sid = "cccccccc-0000-4000-8000-000000000038";
    let app = App::new("chat-image", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["claude", "--session-id", sid]}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid && l["local"]["termId"] == tid.as_str())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let proj: String = wt
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let dir = app.fx.base.join(format!("home/.claude/projects/{proj}"));
    std::fs::create_dir_all(&dir).unwrap();
    let ts = overseer::util::format_rfc3339(overseer::procs::now_ms());
    let first = json!({"type": "user", "uuid": "u0", "sessionId": sid, "timestamp": ts,
        "message": {"content": "hello"}})
    .to_string()
        + "\n";
    let img = json!({"type": "user", "uuid": "u1", "sessionId": sid, "timestamp": ts,
        "message": {"content": [{"type": "text", "text": "see"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGVsbG8="}},
            {"type": "image", "source": {"type": "base64", "media_type": "image/svg+xml", "data": "PHN2Zy8+"}}]}});
    std::fs::write(dir.join(format!("{sid}.jsonl")), format!("{first}{img}\n")).unwrap();
    let entry = wait_for(|| {
        let (code, c) = app.req("GET", &format!("/v1/lines/{line}/chat"), Value::Null);
        (code == 200)
            .then(|| {
                c["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"] == "u1")
                    .cloned()
            })
            .flatten()
    });
    assert_eq!(entry["images"], 2);
    let at = entry["imagesAt"].as_u64().unwrap();
    assert_eq!(at, first.len() as u64);
    let url = |at: &str, i: &str| format!("/v1/lines/{line}/chat/image?at={at}&i={i}");
    assert_eq!(
        app.fx.req("GET", &url(&at.to_string(), "0"), ""),
        (200, "hello".into())
    );
    assert_eq!(app.fx.req("GET", &url(&at.to_string(), "1"), "").0, 415);
    assert_eq!(app.fx.req("GET", &url(&at.to_string(), "2"), "").0, 404);
    assert_eq!(app.fx.req("GET", &url("3", "0"), "").0, 404);
    assert_eq!(app.fx.req("GET", &url("x", "0"), "").0, 400);
    assert_eq!(
        app.fx
            .req("GET", "/v1/lines/nope/chat/image?at=0&i=0", "")
            .0,
        404
    );
}

#[test]
fn transcript_growth_emits_line_activity() {
    use std::io::{Read, Write};
    let sid = "cccccccc-0000-4000-8000-000000000001";
    let app = App::new("activity", json!({}));
    let mut s = UnixStream::connect(&app.fx.sock).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    s.write_all(b"GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n")
        .unwrap();
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["claude", "--session-id", sid]}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid && l["local"]["termId"] == tid.as_str())
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    std::thread::sleep(Duration::from_millis(1500));
    let proj: String = wt
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let path = app
        .fx
        .base
        .join(format!("home/.claude/projects/{proj}/{sid}.jsonl"));
    let chat0 = app.ok("GET", &format!("/v1/lines/{line}/chat"), Value::Null);
    assert_eq!(chat0["lineId"], line);
    let cursor0 = chat0["cursor"].as_u64().unwrap();
    let ts = overseer::util::format_rfc3339(overseer::procs::now_ms());
    let entry = json!({"type": "assistant", "sessionId": sid, "timestamp": ts, "message": {"content": [
        {"type": "tool_use", "id": "toolu_1", "name": "Edit", "input": {"file_path": format!("{wt}/a.txt")}}]}});
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{entry}").unwrap();
    let want = format!("\"lineId\":\"{line}\"");
    let mut buf = String::new();
    let end = Instant::now() + Duration::from_secs(10);
    while !buf
        .split("\n\n")
        .any(|e| e.contains("event: line.activity") && e.contains(&want))
        || !buf
            .split("\n\n")
            .any(|e| e.contains("event: line.chat") && e.contains(&want))
    {
        assert!(Instant::now() < end, "{buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
    let chat = app.ok(
        "GET",
        &format!("/v1/lines/{line}/chat?after={cursor0}"),
        Value::Null,
    );
    assert_eq!(chat["reset"], false);
    assert!(chat["cursor"].as_u64().unwrap() > cursor0);
    let added = chat["entries"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(added["id"], "toolu_1");
    for (uuid, c) in [
        ("sh1", "<bash-input> git status</bash-input>"),
        (
            "sh2",
            "<bash-stdout>On branch main</bash-stdout><bash-stderr></bash-stderr>",
        ),
    ] {
        let e = json!({"type": "user", "uuid": uuid, "sessionId": sid, "timestamp": ts, "message": {"content": c}});
        writeln!(f, "{e}").unwrap();
    }
    let shell = wait_for(|| {
        app.ok("GET", &format!("/v1/lines/{line}/chat"), Value::Null)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "sh1" && e["tool"]["state"] == "done")
            .cloned()
    });
    assert_eq!(shell["tool"]["label"], "! git status");
    assert_eq!(shell["tool"]["output"], "On branch main");
    assert_eq!(added["tool"]["kind"], "edit");
    assert_eq!(added["tool"]["state"], "running");
    let (code, _) = app.req(
        "GET",
        &format!("/v1/lines/{line}/chat?after=x"),
        Value::Null,
    );
    assert_eq!(code, 400);
    let subs = path.with_extension("").join("subagents");
    std::fs::create_dir_all(&subs).unwrap();
    let sub = subs.join("agent-a1b2.jsonl");
    let side = |uuid: &str, content: Value| json!({"isSidechain": true, "type": "assistant", "uuid": uuid, "timestamp": ts, "message": {"content": content}});
    std::fs::write(
        &sub,
        format!(
            "{}\n{}\n",
            json!({"isSidechain": true, "type": "user", "uuid": "s1", "timestamp": ts, "message": {"content": "Map the parser"}}),
            side(
                "s2",
                json!([{"type": "tool_use", "id": "t1", "name": "Grep", "input": {"pattern": "fn page"}}])
            )
        ),
    )
    .unwrap();
    writeln!(
        f,
        "{}\n{}",
        json!({"type": "assistant", "sessionId": sid, "timestamp": ts, "message": {"content": [
            {"type": "tool_use", "id": "toolu_ag", "name": "Agent", "input": {"description": "Map it", "subagent_type": "Explore", "prompt": "Map the parser", "run_in_background": true}}]}}),
        json!({"type": "user", "sessionId": sid, "timestamp": ts, "toolUseResult": {"isAsync": true, "status": "async_launched", "agentId": "a1b2"},
            "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_ag", "content": "Async agent launched"}]}})
    )
    .unwrap();
    let agent = wait_for(|| {
        let c = app.ok("GET", &format!("/v1/lines/{line}/chat"), Value::Null);
        c["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == "toolu_ag")
            .map(|e| e["tool"]["agent"].clone())
    });
    assert_eq!(agent["agentId"], "a1b2");
    assert_eq!(agent["state"], "running");
    assert_eq!(agent["lastActivity"], "fn page");
    let page = app.ok(
        "GET",
        &format!("/v1/lines/{line}/chat/agents/a1b2"),
        Value::Null,
    );
    assert_eq!(page["agentId"], "a1b2");
    assert_eq!(page["entries"][0]["text"], "Map the parser");
    assert_eq!(page["entries"][1]["tool"]["label"], "fn page");
    for bad in ["nope", "..%2Fx"] {
        let (code, _) = app.req(
            "GET",
            &format!("/v1/lines/{line}/chat/agents/{bad}"),
            Value::Null,
        );
        assert_eq!(code, 404);
    }
    buf.clear();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&sub)
        .unwrap()
        .write_all(
            format!(
                "{}\n",
                side(
                    "s3",
                    json!([{"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": "/w/lib.rs"}}])
                )
            )
            .as_bytes(),
        )
        .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    while !buf
        .split("\n\n")
        .any(|e| e.contains("event: line.chat") && e.contains(&want))
    {
        assert!(Instant::now() < end, "{buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
    let c = app.ok("GET", &format!("/v1/lines/{line}/chat"), Value::Null);
    let e = c["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "toolu_ag")
        .unwrap();
    assert_eq!(e["tool"]["agent"]["lastActivity"], "Read lib.rs");
    let cmds = app.ok("GET", &format!("/v1/lines/{line}/commands"), Value::Null);
    assert!(cmds["commands"].is_array());
    let a = app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null);
    assert_eq!(a["workLog"][0]["kind"], "edit");
    assert_eq!(a["workLog"][0]["state"], "running");
    assert_eq!(a["editedFiles"], json!([format!("{wt}/a.txt")]));
    let later = app.ok(
        "GET",
        &format!("/v1/lines/{line}/activity?since=2999-01-01T00:00:00Z"),
        Value::Null,
    );
    assert_eq!(later["workLog"], json!([]));
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

fn line_of(app: &App, term: &str) -> Option<Value> {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["local"]["termId"] == term)
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
fn agents_are_detected_and_launched_from_the_table() {
    let app = App::new("agents", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let list = app.ok("GET", "/v1/agents", Value::Null);
    let ids: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            "claude", "codex", "gemini", "agy", "opencode", "amp", "copilot", "cursor", "vibe",
            "shell"
        ]
    );
    let get = |id: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(get("claude")["installed"], true);
    assert_eq!(get("claude")["version"], "0.0.0 (stub)");
    assert_eq!(
        get("claude")["models"],
        json!(["opus", "sonnet", "haiku", "fable"])
    );
    assert_eq!(get("claude")["turns"], true);
    assert_eq!(get("gemini")["version"], "gemini 0.0.0 (stub)");
    assert_eq!(get("gemini")["efforts"], json!([]));
    assert_eq!(get("gemini")["sessions"], true);
    assert_eq!(
        get("agy")["efforts"],
        json!(["low", "medium", "high", "xhigh", "max"])
    );
    assert_eq!(get("amp")["installed"], false);
    assert!(get("amp").get("version").is_none());
    assert_eq!(get("amp")["models"], json!([]));
    assert_eq!(get("cursor")["verified"], false);
    assert!(get("cursor")["install"]
        .as_str()
        .unwrap()
        .contains("cursor"));
    assert_eq!(get("shell")["installed"], true);
    assert_eq!(get("shell")["resume"], false);

    for body in [
        json!({"cwd": wt, "agent": "amp"}),
        json!({"cwd": wt, "agent": "nope"}),
        json!({"cwd": wt, "agent": "claude", "cmd": ["/bin/sh"]}),
        json!({"cwd": wt, "agent": "codex", "effort": "huge"}),
    ] {
        assert_eq!(app.req("POST", "/v1/terms", body).0, 400);
    }

    let g = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "gemini", "model": "gemini-2.5-pro", "effort": "max"}),
    );
    assert_eq!(g["agent"], "gemini");
    assert_eq!(g["title"], "Gemini CLI");
    let cmd = strs(&g["cmd"]);
    assert_eq!(cmd[..3], ["gemini", "--approval-mode=yolo", "--session-id"]);
    assert_eq!(cmd[4..], ["-m", "gemini-2.5-pro"]);

    let agy = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "agy", "effort": "xhigh"}),
    );
    assert_eq!(
        strs(&agy["cmd"]),
        ["agy", "--mode=auto", "--effort", "xhigh"]
    );

    let c = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "copilot", "model": "gpt-5.2", "effort": "max"}),
    );
    let ctid = c["id"].as_str().unwrap().to_string();
    let cmd = strs(&c["cmd"]);
    assert_eq!(cmd[1], "--allow-all");
    let sid = cmd[2].strip_prefix("--session-id=").unwrap().to_string();
    assert_eq!(cmd[3..], ["--model", "gpt-5.2", "--effort", "xhigh"]);
    wait(|| {
        stub_log(&app).contains(&format!(
            "copilot --allow-all --session-id={sid} --model gpt-5.2 --effort xhigh"
        ))
    });

    let l = wait_for(|| line_of(&app, &ctid));
    assert_eq!(l["line"]["agent"], "copilot");
    assert_eq!(l["line"]["agentSessionId"], sid.as_str());
    assert_eq!(l["local"]["resumeVerified"], true);
    let lid = l["line"]["id"].as_str().unwrap().to_string();
    let (code, act) = app.req("GET", &format!("/v1/lines/{lid}/activity"), Value::Null);
    assert_eq!(code, 200);
    assert_eq!(act["transcript"], false);
    assert_eq!(act["lineId"], lid.as_str());
    assert!(act["workLog"].as_array().unwrap().is_empty());
    app.ok(
        "POST",
        &format!("/v1/terms/{ctid}/kill"),
        json!({"confirm": true}),
    );
    wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["line"]["id"] == lid.as_str() && x["line"]["state"] == "held")
            .cloned()
    });
    let (code, r) = app.req(
        "POST",
        &format!("/v1/lines/{lid}/resume"),
        json!({"target": "term"}),
    );
    assert_eq!(code, 201, "{r}");
    assert_eq!(
        strs(&r["cmd"]),
        [
            "copilot".to_string(),
            "--model".into(),
            "gpt-5.2".into(),
            "--effort".into(),
            "xhigh".into(),
            "--allow-all".into(),
            format!("--resume={sid}")
        ]
    );
    let rid = r["id"].as_str().unwrap().to_string();
    let again = wait_for(|| line_of(&app, &rid));
    assert_eq!(again["line"]["id"], lid.as_str());

    let a = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "agy"}));
    let atid = a["id"].as_str().unwrap().to_string();
    let al = wait_for(|| line_of(&app, &atid));
    assert_eq!(al["line"]["agent"], "agy");
    assert_eq!(al["line"]["agentSessionId"], atid.as_str());
    assert!(al["local"]["resumeCommand"]
        .as_str()
        .unwrap()
        .ends_with("agy --continue"));

    let chats = app.fx.base.join("home/.gemini/tmp/repo/chats");
    std::fs::create_dir_all(&chats).unwrap();
    std::fs::write(app.fx.base.join("home/.gemini/tmp/repo/.project_root"), &wt).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/activity/gemini.jsonl"),
        chats.join("session-2026-10-29T10-00-9b2c4e1a.jsonl"),
    )
    .unwrap();
    let gl = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agent"] == "gemini")
            .cloned()
    });
    assert_eq!(
        gl["line"]["agentSessionId"],
        "9b2c4e1a-0000-4000-8000-000000000001"
    );
    assert_eq!(gl["line"]["branch"], "feature/AS-42-board");
    let gid = gl["line"]["id"].as_str().unwrap();
    let act = app.ok("GET", &format!("/v1/lines/{gid}/activity"), Value::Null);
    assert_eq!(act["goal"], "Fix the login bug in src/auth.rs");

    let sh = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "shell"}));
    assert_eq!(sh["cmd"], json!(["/bin/sh"]));
    for t in [
        g["id"].as_str().unwrap(),
        &rid,
        &atid,
        sh["id"].as_str().unwrap(),
    ] {
        app.ok(
            "POST",
            &format!("/v1/terms/{t}/kill"),
            json!({"confirm": true}),
        );
    }
}

#[test]
fn agent_auto_approval_settings_persist_and_control_new_term_argv() {
    let mut app = App::new("agent-auto-approval", json!({"autoConcurrency": 0}));
    let default = app.ok("GET", "/v1/agents/settings", Value::Null);
    assert_eq!(default["autoApprove"]["claude"], true);
    assert_eq!(default["autoApprove"]["codex"], true);
    assert_eq!(default["autoApprove"].as_object().unwrap().len(), 8);
    assert_eq!(default["autoApprove"]["vibe"], true);

    let disabled = app.ok(
        "PUT",
        "/v1/agents/settings",
        json!({"agent": "codex", "autoApprove": false}),
    );
    assert_eq!(disabled["autoApprove"]["codex"], false);
    assert_eq!(disabled["autoApprove"]["claude"], true);
    app.crash_and_restart();
    assert_eq!(
        app.ok("GET", "/v1/agents/settings", Value::Null)["autoApprove"]["codex"],
        false
    );

    let cwd = app.fx.wt.to_string_lossy().into_owned();
    let codex = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": cwd, "agent": "codex", "cols": 120, "rows": 32}),
    );
    let disabled_id = codex["id"].as_str().unwrap().to_string();
    let disabled_cmd = strs(&codex["cmd"]);
    assert!(!disabled_cmd.contains(&"--sandbox".to_string()));
    assert!(!disabled_cmd.contains(&"--ask-for-approval".to_string()));

    app.ok(
        "PUT",
        "/v1/agents/settings",
        json!({"agent": "codex", "autoApprove": true}),
    );
    let codex = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": cwd, "agent": "codex", "cols": 120, "rows": 32}),
    );
    let cmd = strs(&codex["cmd"]);
    assert!(cmd.windows(2).any(|w| w == ["--ask-for-approval", "never"]));
    let enabled_id = codex["id"].as_str().unwrap().to_string();

    let task = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Auto approve", "project": cwd, "agent": "codex"}),
    );
    let task = app.ok(
        "POST",
        &format!("/v1/tasks/{}/start", task["id"].as_str().unwrap()),
        json!({}),
    );
    let task_term = app.ok(
        "GET",
        &format!("/v1/terms/{}", task["termId"].as_str().unwrap()),
        Value::Null,
    );
    assert!(strs(&task_term["cmd"])
        .windows(2)
        .any(|w| w == ["--ask-for-approval", "never"]));

    assert_eq!(
        app.req("PUT", "/v1/agents/settings", json!({"agent": "claude"}))
            .0,
        400
    );
    assert_eq!(
        app.req(
            "PUT",
            "/v1/agents/settings",
            json!({"agent": "amp", "autoApprove": false})
        )
        .0,
        400
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{disabled_id}/kill"),
        json!({"confirm": true}),
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{enabled_id}/kill"),
        json!({"confirm": true}),
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{}/kill", task["termId"].as_str().unwrap()),
        json!({"confirm": true}),
    );
}

#[test]
fn explicit_start_args_win_over_auto_approve_in_task_argv() {
    let app = App::new(
        "explicit-approval",
        json!({"autoConcurrency": 0, "startArgs": ["--permission-mode", "acceptEdits"]}),
    );
    let cwd = app.fx.wt.to_string_lossy().into_owned();
    let task = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Explicit", "project": cwd, "agent": "claude"}),
    );
    let task = app.ok(
        "POST",
        &format!("/v1/tasks/{}/start", task["id"].as_str().unwrap()),
        json!({}),
    );
    let term = app.ok(
        "GET",
        &format!("/v1/terms/{}", task["termId"].as_str().unwrap()),
        Value::Null,
    );
    let cmd = strs(&term["cmd"]);
    assert_eq!(
        cmd.iter().filter(|a| *a == "--permission-mode").count(),
        1,
        "{cmd:?}"
    );
    assert!(cmd
        .windows(2)
        .any(|w| w == ["--permission-mode", "acceptEdits"]));
    assert!(!cmd.contains(&"auto".to_string()));
    app.ok(
        "POST",
        &format!("/v1/terms/{}/kill", task["termId"].as_str().unwrap()),
        json!({"confirm": true}),
    );
}

#[test]
fn tasks_take_any_agent_and_deliver_the_prompt_by_flag() {
    let app = App::new("taskagents", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    assert_eq!(
        app.req(
            "POST",
            "/v1/tasks",
            json!({"title": "x", "project": wt, "agent": "shell"})
        )
        .0,
        400
    );
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Fix it", "description": "now", "project": wt, "agent": "gemini", "model": "m1"}),
    );
    assert_eq!(t["agent"], "gemini");
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    assert!(s["lineId"].is_string(), "{s}");
    let term = app.ok(
        "GET",
        &format!("/v1/terms/{}", s["termId"].as_str().unwrap()),
        Value::Null,
    );
    let cmd = strs(&term["cmd"]);
    assert_eq!(cmd[cmd.len() - 2..], ["-i", "<prompt>"]);
    assert!(!term.to_string().contains("now"), "{term}");
    wait(|| stub_log(&app).contains("-m m1 -i Fix it"));

    let p = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Paste", "project": wt, "agent": "amp"}),
    );
    let pid = p["id"].as_str().unwrap();
    assert_eq!(
        app.req("POST", &format!("/v1/tasks/{pid}/start"), json!({}))
            .0,
        409
    );
    let o = app.ok(
        "PATCH",
        &format!("/v1/tasks/{pid}"),
        json!({"agent": "codex", "effort": "low"}),
    );
    assert_eq!(o["agent"], "codex");
    let o = app.ok("POST", &format!("/v1/tasks/{pid}/start"), json!({}));
    assert!(o.get("lineId").is_none());
    let tid = o["termId"].as_str().unwrap().to_string();
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "got: Paste");
    let log = stub_log(&app);
    assert!(log.contains("codex --no-daemon -c "), "{log}");
    assert!(
        log.contains("-c mcp_servers.overseer.args=[\"mcp\"]"),
        "{log}"
    );
    assert!(
        log.contains(&format!(
            "-c mcp_servers.overseer.env.OVERSEER_TASK_ID=\"{pid}\""
        )),
        "{log}"
    );
    assert!(log.contains("--ask-for-approval never"), "{log}");
    assert!(
        log.contains("-c mcp_servers.overseer.default_tools_approval_mode=\"approve\""),
        "{log}"
    );
    assert!(log.contains("-c model_reasoning_effort=low"), "{log}");
    for t in [s["termId"].as_str().unwrap(), &tid] {
        app.ok(
            "POST",
            &format!("/v1/terms/{t}/kill"),
            json!({"confirm": true}),
        );
    }
}

#[test]
fn claude_tasks_get_the_overseer_mcp_server_and_cli() {
    let app = App::new("claudemcp", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Mcp", "project": wt, "agent": "claude"}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    wait(|| stub_log(&app).contains("--mcp-config"));
    let log = stub_log(&app);
    let line = log.lines().find(|l| l.contains("--mcp-config")).unwrap();
    assert!(line.contains("--allowedTools mcp__overseer"), "{line}");
    let json_start = line.find("{\"mcpServers\"").expect(line);
    let json_end = line.rfind("}}}").unwrap() + 3;
    let cfg: Value = serde_json::from_str(&line[json_start..json_end]).unwrap();
    let srv = &cfg["mcpServers"]["overseer"];
    assert_eq!(srv["args"], json!(["mcp"]));
    assert_eq!(srv["env"]["OVERSEER_TASK_ID"], id.as_str());
    assert_eq!(
        srv["env"]["OVERSEER_STATE_DIR"],
        app.fx.base.join("state").to_string_lossy().as_ref()
    );
    wait(|| app.fx.base.join("stub.log.path").exists());
    let path = std::fs::read_to_string(app.fx.base.join("stub.log.path")).unwrap();
    let shim_dir = app
        .fx
        .base
        .join("home/.overseer/bin")
        .to_string_lossy()
        .into_owned();
    assert!(path.trim().starts_with(&format!("{shim_dir}:")), "{path}");
    app.ok(
        "POST",
        &format!("/v1/terms/{}/kill", s["termId"].as_str().unwrap()),
        json!({"confirm": true}),
    );
}

#[test]
fn task_rank_and_added_projects() {
    let app = App::new("rank", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let a = app.ok("POST", "/v1/tasks", json!({"title": "a", "project": wt}));
    let b = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "b", "project": wt, "rank": -3.5}),
    );
    assert!(a["rank"].as_f64().unwrap() > 1e12);
    assert_eq!(b["rank"], -3.5);
    let id = a["id"].as_str().unwrap();
    let p = app.ok(
        "PATCH",
        &format!("/v1/tasks/{id}"),
        json!({"rank": 1.25, "status": "auto"}),
    );
    assert_eq!(p["rank"], 1.25);
    assert_eq!(app.task(id)["rank"], 1.25);
    assert_eq!(
        app.req("PATCH", &format!("/v1/tasks/{id}"), json!({"rank": "x"}))
            .0,
        400
    );

    let other = app.fx.base.join("other");
    std::fs::create_dir_all(other.join("sub")).unwrap();
    common::git(&other, &["init", "-q"]);
    let o = other.canonicalize().unwrap().to_string_lossy().into_owned();
    let (code, v) = app.req("POST", "/v1/projects", json!({"path": other.join("sub")}));
    assert_eq!(code, 201, "{v}");
    let mut v = v;
    assert!(
        v["defaultBranch"].as_str().is_some_and(|b| !b.is_empty()),
        "{v}"
    );
    v.as_object_mut().unwrap().remove("defaultBranch");
    assert_eq!(
        v,
        json!({"path": o, "name": "other", "repo": format!("local:{o}"), "missing": false})
    );
    assert_eq!(app.req("POST", "/v1/projects", json!({"path": o})).0, 200);
    let plain = std::env::temp_dir().join(format!("sb-plain-{}", std::process::id()));
    std::fs::create_dir_all(&plain).unwrap();
    for bad in [
        json!({"path": plain}),
        json!({"path": "rel/x"}),
        json!({}),
        json!({"path": "/nonexistent-sb"}),
    ] {
        assert_eq!(app.req("POST", "/v1/projects", bad).0, 400);
    }
    let _ = std::fs::remove_dir_all(&plain);
    let all = app.ok("GET", "/v1/projects", Value::Null);
    let paths: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    assert!(
        paths.contains(&o.as_str()) && paths.contains(&wt.as_str()),
        "{all}"
    );
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn read_events(s: &mut UnixStream, buf: &mut String, want: &[&str]) {
    use std::io::Read;
    let end = Instant::now() + Duration::from_secs(15);
    while !want.iter().all(|w| buf.contains(w)) {
        assert!(Instant::now() < end, "missing {want:?} in {buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
}

#[test]
fn worktrees_graph_and_events_through_the_daemon() {
    use std::io::Write;
    let app = App::new("worktrees", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let p = enc(&wt);
    let mut s = UnixStream::connect(&app.fx.sock).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    s.write_all(b"GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    read_events(&mut s, &mut buf, &["retry:"]);

    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh"], "worktree": true}),
    );
    let term = t["id"].as_str().unwrap().to_string();
    let cwd = t["cwd"].as_str().unwrap().to_string();
    let store = app.fx.base.join("worktrees").canonicalize().unwrap();
    assert!(cwd.starts_with(store.to_str().unwrap()), "{cwd}");
    let branch = common::git(Path::new(&cwd), &["symbolic-ref", "--short", "HEAD"]);
    assert!(branch.trim().starts_with("sb/shell/"), "{branch}");
    let (code, _) = app.req(
        "POST",
        "/v1/terms",
        json!({"cwd": "/", "cmd": ["/bin/sh"], "worktree": true}),
    );
    assert_eq!(code, 400);

    let l = app.ok("GET", &format!("/v1/projects/{p}/worktrees"), Value::Null);
    assert_eq!(l["project"], wt.as_str());
    assert_eq!(l["worktrees"][0]["kind"], "main");
    let mine = l["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["path"] == cwd.as_str())
        .unwrap()
        .clone();
    assert_eq!(mine["state"], "in_use");
    assert_eq!(mine["termIds"], json!([term]));
    let plan = app.ok(
        "POST",
        &format!("/v1/worktrees/{}/cleanup", enc(&cwd)),
        json!({}),
    );
    assert_eq!(plan["dryRun"], true);
    assert_eq!(plan["refused"], "in use by a running terminal");
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["exited"] == true);
    let done = app.ok(
        "POST",
        &format!("/v1/worktrees/{}/cleanup", enc(&cwd)),
        json!({"dryRun": false, "project": wt}),
    );
    assert_eq!(done["remove"][0]["kind"], "worktree");
    assert!(!Path::new(&cwd).exists());

    let (code, w) = app.req(
        "POST",
        "/v1/worktrees",
        json!({"project": wt, "agent": "claude", "slug": "Fix login"}),
    );
    assert_eq!(code, 201, "{w}");
    assert_eq!(w["branch"], "sb/claude/fix-login");
    let wpath = w["path"].as_str().unwrap().to_string();
    assert_eq!(
        app.req("POST", "/v1/worktrees", json!({"project": "/"})).0,
        400
    );

    let g = app.ok(
        "GET",
        &format!("/v1/projects/{p}/graph?limit=50"),
        Value::Null,
    );
    assert_eq!(g["commits"][0]["subject"], "init");
    assert_eq!(g["head"]["branch"], "feature/AS-42-board");
    assert_eq!(g["overlays"][0]["branch"], "sb/claude/fix-login");
    assert_eq!(g["overlays"][0]["worktree"], wpath.as_str());
    let sha = g["commits"][0]["sha"].as_str().unwrap();
    let c = app.ok(
        "GET",
        &format!("/v1/projects/{p}/commits/{sha}"),
        Value::Null,
    );
    assert_eq!(c["subject"], "init");
    assert_eq!(
        app.req("GET", &format!("/v1/projects/{p}/commits/zz"), Value::Null)
            .0,
        400
    );
    let (code, _) = app.req(
        "POST",
        &format!("/v1/projects/{p}/branches"),
        json!({"name": "from-graph", "sha": sha}),
    );
    assert_eq!(code, 201);

    std::fs::write(Path::new(&wpath).join("x.txt"), "x\n").unwrap();
    common::git(Path::new(&wpath), &["add", "-A"]);
    common::git(Path::new(&wpath), &["commit", "-q", "-m", "agent work"]);
    read_events(
        &mut s,
        &mut buf,
        &[
            "event: worktree.created",
            "event: worktree.removed",
            "event: git.changed",
        ],
    );

    let task = app.ok("POST", "/v1/tasks", json!({"title": "here", "project": wt}));
    let id = task["id"].as_str().unwrap().to_string();
    let st = app.ok(
        "POST",
        &format!("/v1/tasks/{id}/start"),
        json!({"inPlace": true}),
    );
    assert_eq!(st["worktreePath"], wt.as_str());
    assert_eq!(st["branch"], "feature/AS-42-board");
    let tt = st["termId"].as_str().unwrap().to_string();
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{tt}"), Value::Null)["cwd"],
        wt.as_str()
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{tt}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{tt}"), Value::Null)["exited"] == true);
}

#[test]
fn task_attachments_move_out_of_the_upload_sweep_and_go_with_the_task() {
    let app = App::new("task-attach", json!({}));
    let (code, out) = app
        .fx
        .req("POST", "/v1/uploads?name=Screen%20Shot.png", "png");
    assert_eq!(code, 201, "{out}");
    let up: Value = serde_json::from_str(&out).unwrap();
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "Shot", "description": "See the image", "project": wt, "agent": "claude", "model": "", "attachments": [{"path": up["path"], "name": up["name"]}]}),
    );
    let id = t["id"].as_str().unwrap();
    let kept = app
        .fx
        .base
        .join(format!("home/.overseer/tasks/{id}/Screen_Shot.png"));
    assert_eq!(std::fs::read(&kept).unwrap(), b"png");
    assert!(!Path::new(up["path"].as_str().unwrap()).exists());
    assert_eq!(
        t["description"],
        format!("See the image\n\nAttachments:\n- {}", kept.display())
    );
    let secret = app.fx.base.join("secret.txt");
    std::fs::write(&secret, "x").unwrap();
    let (code, v) = app.req(
        "POST",
        "/v1/tasks",
        json!({"title": "Steal", "project": wt, "attachments": [{"path": secret}]}),
    );
    assert_eq!(code, 400, "{v}");
    assert!(secret.exists());
    app.ok("DELETE", &format!("/v1/tasks/{id}"), Value::Null);
    assert!(!kept.parent().unwrap().exists());
}

#[test]
fn uploads_stream_to_a_short_home_path_and_report_env() {
    use std::io::{Read, Write};
    let app = App::new("uploads", json!({}));
    let (code, out) = app
        .fx
        .req("POST", "/v1/uploads?name=my%20notes.txt", "hello");
    assert_eq!(code, 201, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["name"], "my_notes.txt");
    assert_eq!(v["size"], 5);
    let path = Path::new(v["path"].as_str().unwrap()).to_path_buf();
    assert!(path.is_absolute());
    assert_eq!(
        path.parent().unwrap(),
        app.fx.base.join("home/.overseer/u"),
        "{}",
        path.display()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    let file = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(file.len(), "a1b2c3.txt".len(), "{file}");
    assert!(file.ends_with(".txt"));
    assert_eq!(v["short"], format!("~/.overseer/u/{file}"));

    let big = "x".repeat(3 << 20);
    let (code, out) = app.fx.req("POST", "/v1/uploads?name=big.bin", &big);
    assert_eq!(code, 201, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["size"], 3 << 20);
    assert_eq!(
        std::fs::metadata(v["path"].as_str().unwrap())
            .unwrap()
            .len(),
        3 << 20
    );

    let (code, out) = app.fx.req("POST", "/v1/uploads?name=..", "x");
    assert_eq!(code, 400, "{out}");

    let raw = |head: &str| {
        let mut s = UnixStream::connect(&app.fx.sock).unwrap();
        s.write_all(head.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    };
    let too_big =
        raw("POST /v1/uploads?name=a HTTP/1.1\r\nHost: unix\r\nContent-Length: 52428801\r\n\r\n");
    assert!(too_big.starts_with("HTTP/1.1 413 "), "{too_big}");
    let chunked = raw("POST /v1/uploads?name=a HTTP/1.1\r\nHost: unix\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n");
    assert!(chunked.starts_with("HTTP/1.1 411 "), "{chunked}");

    let env = app.ok("GET", "/v1/env", Value::Null);
    assert_eq!(env["source"], "inherited");
    assert!(env["path"].as_str().unwrap().contains("/usr/bin"), "{env}");
    let agents = app.ok("POST", "/v1/agents/rescan", Value::Null);
    assert_eq!(agents, app.ok("GET", "/v1/agents", Value::Null));
}

#[test]
fn agent_sessions_become_tasks_that_follow_them() {
    let app = App::new("session-tasks", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let sid = "eeeeeeee-0000-4000-8000-000000000001";
    let prompt = "Fix the flaky login test in the checkout flow please";
    let ts = overseer::util::format_rfc3339(overseer::procs::now_ms() - 60_000);
    std::fs::write(
        app.fx
            .base
            .join(format!("home/.claude/projects/p/{sid}.jsonl")),
        format!(
            "{}\n",
            json!({"type": "user", "sessionId": sid, "cwd": wt, "timestamp": ts,
                   "message": {"role": "user", "content": prompt}})
        ),
    )
    .unwrap();
    let sessions = || -> Vec<Value> {
        app.ok("GET", "/v1/tasks", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["origin"] == "session")
            .cloned()
            .collect()
    };
    let t = wait_for(|| sessions().into_iter().next());
    let id = t["id"].as_str().unwrap().to_string();
    assert_eq!(t["title"], prompt);
    assert_eq!(t["description"], prompt);
    assert_eq!(t["project"], wt.as_str());
    assert_eq!(t["agent"], "claude");
    assert_eq!(t["status"], "completed");
    assert!(t["lineId"].as_str().is_some_and(|l| !l.is_empty()));
    let url = format!("/v1/tasks/{id}");
    assert_eq!(app.req("PATCH", &url, json!({"status": "auto"})).0, 400);
    let p = app.ok("PATCH", &url, json!({"status": "in_progress"}));
    assert_eq!(p["status"], "in_progress");
    assert_eq!(p["manualStatus"], "in_progress");
    std::thread::sleep(Duration::from_millis(2500));
    let all = sessions();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0]["status"], "in_progress");
    assert_eq!(app.req("DELETE", &url, Value::Null).0, 409);
    assert_eq!(app.req("POST", &format!("{url}/start"), json!({})).0, 409);
    let r = app.ok(
        "POST",
        &format!("{url}/review"),
        json!({"action": "approve"}),
    );
    assert_eq!(r["task"]["status"], "completed");
    let user = app.ok("POST", "/v1/tasks", json!({"title": "mine", "project": wt}));
    assert_eq!(user["origin"], "user");
}

#[test]
fn profiles_apply_at_launch_with_workspace_and_task_precedence() {
    let app = App::new("profiles", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let p = app.ok("GET", "/v1/profiles", Value::Null);
    assert_eq!(p["default"], "");
    assert_eq!(p["support"]["agy"]["effort"], "flag");
    assert_eq!(p["support"]["gemini"]["subagents"], "file");
    assert_eq!(p["support"]["agy"]["subagents"], "file");
    let ids: Vec<&str> = p["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["economy", "balanced", "max"]);
    assert_eq!(p["support"]["claude"]["subagents"], "file");
    assert_eq!(p["support"]["codex"]["subagentModel"], "flag");

    let plain = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "claude"}));
    let plain_cmd = strs(&plain["cmd"]);
    assert_eq!(plain_cmd.len(), 5);
    assert!(plain_cmd
        .windows(2)
        .any(|w| w == ["--permission-mode", "auto"]));
    assert!(plain.get("profile").is_none());

    for bad in [
        json!({"default": "ghost"}),
        json!({"scopes": {"rel": "max"}}),
        json!({"profiles": [{"id": "max", "name": "Mine"}]}),
        json!({"profiles": [{"id": "x", "name": "X", "agents": {"gemini": {"effort": "low"}}}]}),
    ] {
        assert_eq!(app.req("PUT", "/v1/profiles", bad).0, 400);
    }
    app.ok(
        "PUT",
        "/v1/profiles",
        json!({
            "default": "balanced",
            "scopes": {wt.clone(): "economy"},
            "profiles": [
                {"id": "lean", "name": "Lean", "agents": {"claude": {"model": "haiku", "effort": "low"}}},
                {"id": "codex-overrides", "name": "Codex overrides", "agents": {"codex": {"subagentModel": "gpt-5.4-mini", "subagentEffort": "medium"}}}
            ],
        }),
    );

    let scoped = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "claude"}));
    let cmd = strs(&scoped["cmd"]);
    assert!(cmd.windows(2).any(|w| w == ["--model", "sonnet"]));
    assert!(cmd.windows(2).any(|w| w == ["--effort", "low"]));
    assert_eq!(scoped["profile"]["name"], "Economy");
    assert_eq!(scoped["profile"]["effort"], "low");

    let codex = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "profile": "codex-overrides"}),
    );
    assert_eq!(
        strs(&codex["cmd"]),
        [
            "codex",
            "--no-daemon",
            "-c",
            "agents.default_subagent_model=\"gpt-5.4-mini\"",
            "-c",
            "agents.default_subagent_reasoning_effort=\"medium\"",
            "--ask-for-approval",
            "never",
        ]
    );

    let home = app.ok("POST", "/v1/terms", json!({"cwd": "/", "agent": "claude"}));
    let home_cmd = strs(&home["cmd"]);
    assert!(home_cmd.windows(2).any(|w| w == ["--model", "sonnet"]));
    assert!(home_cmd.windows(2).any(|w| w == ["--effort", "medium"]));

    let explicit = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "claude", "profile": "lean", "effort": "high"}),
    );
    let explicit_cmd = strs(&explicit["cmd"]);
    assert!(explicit_cmd.windows(2).any(|w| w == ["--model", "haiku"]));
    assert!(explicit_cmd.windows(2).any(|w| w == ["--effort", "high"]));
    let none = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "claude", "profile": "none"}),
    );
    let none_cmd = strs(&none["cmd"]);
    assert_eq!(none_cmd.len(), 5);
    assert!(none_cmd
        .windows(2)
        .any(|w| w == ["--permission-mode", "auto"]));
    let inherit = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "claude", "profile": ""}),
    );
    assert_eq!(inherit["profile"]["id"], "economy");
    assert_eq!(
        app.req(
            "POST",
            "/v1/terms",
            json!({"cwd": wt, "agent": "claude", "profile": "ghost"})
        )
        .0,
        400
    );

    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "lean", "project": wt, "profile": "max"}),
    );
    assert_eq!(t["profile"], "max");
    assert_eq!(t["effort"], "");
    let u = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "plain", "project": wt, "agent": "gemini"}),
    );
    assert_eq!(u["effort"], "high");
    assert!(u.get("profile").is_none());
    assert_eq!(
        app.req(
            "POST",
            "/v1/tasks",
            json!({"title": "x", "project": wt, "profile": "ghost"})
        )
        .0,
        400
    );
    let id = t["id"].as_str().unwrap().to_string();
    app.ok("PUT", "/v1/profiles", json!({"profiles": []}));
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    let term = app.ok(
        "GET",
        &format!("/v1/terms/{}", s["termId"].as_str().unwrap()),
        Value::Null,
    );
    let cmd = strs(&term["cmd"]);
    assert_eq!(cmd[cmd.len() - 4..], ["--model", "opus", "--effort", "max"]);
    assert_eq!(term["profile"]["id"], "max");

    let subs = app.ok(
        "GET",
        &format!("/v1/profiles/subagents?cwd={}", wt.replace('/', "%2F")),
        Value::Null,
    );
    assert!(subs["subagents"].is_array());
    let dry = app.ok(
        "POST",
        "/v1/profiles/apply-files",
        json!({"profile": "economy", "cwd": wt}),
    );
    assert_eq!(dry["dryRun"], true);
    assert_eq!(
        app.req(
            "POST",
            "/v1/profiles/apply-files",
            json!({"profile": "economy", "dryRun": false})
        )
        .0,
        400
    );
}

#[test]
fn task_type_picks_the_profile_between_explicit_and_workspace() {
    let app = App::new("profiles-types", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let p = app.ok("GET", "/v1/profiles", Value::Null);
    assert_eq!(
        p["taskTypes"],
        json!(["fix", "feature", "engineering", "refactor", "triage"])
    );
    assert_eq!(p["types"], json!({}));
    let bad = app.req(
        "PUT",
        "/v1/profiles",
        json!({"types": {"migration": "max"}}),
    );
    assert_eq!(bad.0, 400);
    assert!(bad.1["error"]
        .as_str()
        .unwrap()
        .starts_with("unknown task type \"migration\""));
    assert_eq!(
        app.req("PUT", "/v1/profiles", json!({"types": {"fix": "ghost"}}))
            .0,
        400
    );
    let saved = app.ok(
        "PUT",
        "/v1/profiles",
        json!({
            "scopes": {wt.clone(): "economy"},
            "types": {"refactor": "max", "triage": "none"},
        }),
    );
    assert_eq!(saved["types"]["refactor"], "max");

    let refactor = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "migrate", "project": wt, "taskType": "refactor"}),
    );
    assert_eq!(refactor["taskType"], "refactor");
    assert_eq!(refactor["effort"], "");
    let triage = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "logs", "project": wt, "taskType": "triage", "agent": "gemini"}),
    );
    assert_eq!(triage["effort"], "high");
    let r = app.req(
        "POST",
        "/v1/tasks",
        json!({"title": "x", "project": wt, "taskType": "chore"}),
    );
    assert_eq!(r.0, 400);
    assert_eq!(
        r.1["error"],
        "taskType must be one of fix, feature, engineering, refactor, triage"
    );

    let start = |id: &str| {
        let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
        app.ok(
            "GET",
            &format!("/v1/terms/{}", s["termId"].as_str().unwrap()),
            Value::Null,
        )
    };
    let term = start(refactor["id"].as_str().unwrap());
    let cmd = strs(&term["cmd"]);
    assert_eq!(cmd[cmd.len() - 4..], ["--model", "opus", "--effort", "max"]);
    assert_eq!(term["profile"]["id"], "max");

    let feature = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "small", "project": wt}),
    );
    let fid = feature["id"].as_str().unwrap();
    let patched = app.ok(
        "PATCH",
        &format!("/v1/tasks/{fid}"),
        json!({"taskType": "feature", "effort": ""}),
    );
    assert_eq!(patched["taskType"], "feature");
    let term = start(fid);
    assert_eq!(term["profile"]["id"], "economy");

    let explicit = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "pinned", "project": wt, "taskType": "refactor", "profile": "balanced"}),
    );
    let term = start(explicit["id"].as_str().unwrap());
    assert_eq!(term["profile"]["id"], "balanced");

    let cleared = app.ok(
        "PATCH",
        &format!("/v1/tasks/{}", triage["id"].as_str().unwrap()),
        json!({"taskType": null}),
    );
    assert!(cleared.get("taskType").is_none());
}

#[test]
fn profiles_list_and_apply_gemini_and_antigravity_subagent_models_over_http() {
    let app = App::new("gsubs", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let home = app.fx.base.join("home/.gemini");
    let gemini_project = app.fx.wt.join(".gemini/agents/researcher.md");
    let gemini_user = home.join("agents/researcher.md");
    let agy_project = app.fx.wt.join(".agents/agents/security/agent.md");
    let agy_user = home.join("config/agents/security/agent.md");
    for path in [&gemini_project, &gemini_user, &agy_project, &agy_user] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    std::fs::write(
        &gemini_project,
        "---\nname: researcher\nmodel: gemini-2.5-flash\n---\nproject agent\n",
    )
    .unwrap();
    std::fs::write(
        &gemini_user,
        "---\nname: researcher\nmodel: gemini-2.5-pro\n---\nuser agent\n",
    )
    .unwrap();
    std::fs::write(
        &agy_project,
        "---\nname: security\nmodel: flash\n---\nproject agent\n",
    )
    .unwrap();
    std::fs::write(
        &agy_user,
        "---\nname: security\nmodel: pro\n---\nuser agent\n",
    )
    .unwrap();

    let query = format!("/v1/profiles/subagents?cwd={}", wt.replace('/', "%2F"));
    let listed = app.ok("GET", &query, Value::Null);
    let defs = listed["subagents"].as_array().unwrap();
    let gemini = defs
        .iter()
        .find(|d| d["agent"] == "gemini" && d["name"] == "researcher")
        .unwrap();
    let agy = defs
        .iter()
        .find(|d| d["agent"] == "agy" && d["name"] == "security")
        .unwrap();
    assert_eq!(gemini["scope"], "project");
    assert_eq!(gemini["model"], "gemini-2.5-flash");
    assert_eq!(agy["scope"], "project");
    assert_eq!(agy["model"], "flash");

    app.ok(
        "PUT",
        "/v1/profiles",
        json!({"profiles": [{"id": "google-agents", "name": "Google agents", "agents": {
            "gemini": {"subagents": {"researcher": {"model": "gemini-3.1-pro"}}},
            "agy": {"subagents": {"security": {"model": "pro"}}}
        }}]}),
    );
    let dry = app.ok(
        "POST",
        "/v1/profiles/apply-files",
        json!({"profile": "google-agents", "cwd": wt, "dryRun": true}),
    );
    let files: Vec<String> = dry["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        files,
        [
            agy_project.to_string_lossy().into_owned(),
            gemini_project.to_string_lossy().into_owned(),
        ]
    );
    let applied = app.ok(
        "POST",
        "/v1/profiles/apply-files",
        json!({"profile": "google-agents", "cwd": wt, "dryRun": false, "files": files}),
    );
    assert!(applied["files"]
        .as_array()
        .unwrap()
        .iter()
        .all(|f| f["written"] == true));
    for file in applied["files"].as_array().unwrap() {
        let backup = file["backup"].as_str().unwrap();
        let previous = std::fs::read_to_string(backup).unwrap();
        if file["path"] == gemini_project.to_string_lossy().as_ref() {
            assert!(previous.contains("model: gemini-2.5-flash"));
        } else {
            assert_eq!(file["path"], agy_project.to_string_lossy().as_ref());
            assert!(previous.contains("model: flash"));
        }
    }
    assert!(std::fs::read_to_string(gemini_project)
        .unwrap()
        .contains("model: gemini-3.1-pro"));
    assert!(std::fs::read_to_string(agy_project)
        .unwrap()
        .contains("model: pro"));
    assert!(std::fs::read_to_string(gemini_user)
        .unwrap()
        .contains("model: gemini-2.5-pro"));
    assert!(std::fs::read_to_string(agy_user)
        .unwrap()
        .contains("model: pro"));
}

#[test]
fn lifecycle_reports_autostart_and_stores_the_after_reboot_choice() {
    let app = App::new("lifecycle", json!({}));
    let v = app.ok("GET", "/v1/lifecycle", Value::Null);
    assert_eq!(v["afterReboot"], "agents");
    assert!(v["autostart"]["supported"].is_boolean(), "{v}");
    assert_eq!(v["autostart"]["installed"], false);
    let v = app.ok("PATCH", "/v1/lifecycle", json!({"afterReboot": "shells"}));
    assert_eq!(v["afterReboot"], "shells");
    assert_eq!(
        app.ok("GET", "/v1/lifecycle", Value::Null)["afterReboot"],
        "shells"
    );
    let (code, e) = app.req("PATCH", "/v1/lifecycle", json!({"afterReboot": "all"}));
    assert_eq!(code, 400, "{e}");
}

#[test]
fn a_daemon_killed_like_an_oom_kill_restores_its_sessions_when_it_restarts() {
    let fx = Fx::new("crash-restore", 0);
    let call = |m: &str, t: &str, b: Value| {
        let body = if b.is_null() {
            String::new()
        } else {
            b.to_string()
        };
        let (code, out) = fx.req(m, t, &body);
        (
            code,
            serde_json::from_str::<Value>(&out).unwrap_or(Value::Null),
        )
    };
    let live = |id: &str| {
        call("GET", "/v1/terms", Value::Null)
            .1
            .as_array()
            .is_some_and(|a| a.iter().any(|t| t["id"] == id && t["exited"] != true))
    };
    let cwd = fx.wt.to_string_lossy().into_owned();
    let d = fx.daemon(false);
    let (code, t) = call("POST", "/v1/terms", json!({"cwd": cwd, "cmd": ["/bin/sh"]}));
    assert_eq!(code, 201, "{t}");
    let id = t["id"].as_str().unwrap().to_string();
    let (code, w) = call("POST", "/v1/workspaces", json!({"path": cwd}));
    assert!(code == 200 || code == 201, "{w}");
    let wid = w["id"].as_str().unwrap().to_string();
    let (code, v) = call(
        "PUT",
        &format!("/v1/terms/{id}/workspace"),
        json!({"workspaceId": wid}),
    );
    assert_eq!(code, 200, "{v}");
    let (code, v) = call(
        "PUT",
        &format!("/v1/terms/{id}/title"),
        json!({"title": "build"}),
    );
    assert_eq!(code, 200, "{v}");
    std::thread::sleep(Duration::from_secs(3));
    drop(d);
    let d = fx.daemon(false);
    wait(|| live(&id));
    let t = call("GET", &format!("/v1/terms/{id}"), Value::Null).1;
    assert_eq!(t["workspaceId"], wid.as_str(), "{t}");
    assert_eq!(t["titleOverride"], "build", "{t}");
    let (code, v) = call("PATCH", "/v1/lifecycle", json!({"afterReboot": "nothing"}));
    assert_eq!(code, 200, "{v}");
    std::thread::sleep(Duration::from_secs(3));
    drop(d);
    let _d = fx.daemon(false);
    std::thread::sleep(Duration::from_secs(2));
    assert!(!live(&id), "the after-restart setting says nothing");
}

#[test]
fn a_pin_or_title_is_visible_in_the_very_next_lines_read() {
    // Given a codex session in a term
    // When it is pinned and renamed, each time reading /v1/lines right after the PUT returns
    // Then that read already shows the new value, every time
    let app = App::new("pinvisible", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "cols": 120, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    send(&mut ws, "busy\r");
    let lid = wait_for(|| line_of(&app, &id))["line"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let local = |key: &str| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == lid.as_str())
            .map(|l| l["local"][key].clone())
            .unwrap()
    };
    for i in 0..20 {
        let on = i % 2 == 0;
        app.ok(
            "PUT",
            &format!("/v1/lines/{lid}/pin"),
            json!({"pinned": on}),
        );
        assert_eq!(local("pinned"), json!(on), "round {i}");
        let title = format!("name {i}");
        app.ok(
            "PUT",
            &format!("/v1/lines/{lid}/title"),
            json!({"title": title}),
        );
        assert_eq!(local("titleOverride"), json!(title), "round {i}");
    }
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_term_rename_survives_a_daemon_kill_right_after_it() {
    // Given a shell term
    // When it is renamed and the daemon dies immediately, without waiting for any scan
    // Then the restarted daemon restores the shell with its new title
    let fx = Fx::new("rename-crash", 0);
    let call = |m: &str, t: &str, b: Value| {
        let body = if b.is_null() {
            String::new()
        } else {
            b.to_string()
        };
        let (code, out) = fx.req(m, t, &body);
        (
            code,
            serde_json::from_str::<Value>(&out).unwrap_or(Value::Null),
        )
    };
    let cwd = fx.wt.to_string_lossy().into_owned();
    let d = fx.daemon(false);
    let (code, t) = call("POST", "/v1/terms", json!({"cwd": cwd, "cmd": ["/bin/sh"]}));
    assert_eq!(code, 201, "{t}");
    let id = t["id"].as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_secs(3));
    let (code, v) = call(
        "PUT",
        &format!("/v1/terms/{id}/title"),
        json!({"title": "build"}),
    );
    assert_eq!(code, 200, "{v}");
    drop(d);
    let _d = fx.daemon(false);
    wait(|| call("GET", &format!("/v1/terms/{id}"), Value::Null).0 == 200);
    let t = call("GET", &format!("/v1/terms/{id}"), Value::Null).1;
    assert_eq!(t["titleOverride"], "build", "{t}");
}

#[test]
fn file_route_resolves_relative_paths_against_the_line_cwd() {
    use std::io::{Read, Write};
    let sid = "cccccccc-0000-4000-8000-000000000057";
    let app = App::new("file-line", json!({}));
    let get = |target: &str| {
        let mut s = UnixStream::connect(&app.fx.sock).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        let at = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&out[..at]).into_owned();
        let code: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, head, out[at + 4..].to_vec())
    };
    let dir = app.fx.wt.join("J3 docs");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("J3-modifications.pdf"), b"%PDF-1.7\n").unwrap();
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["claude", "--session-id", sid]}),
    );
    let tid = t["id"].as_str().unwrap().to_string();
    let line = wait_for(|| {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid)
            .map(|l| l["line"]["id"].as_str().unwrap().to_string())
    });
    let rel = "J3%20docs%2FJ3-modifications.pdf";

    let (code, _, body) = get(&format!("/v1/file?path={rel}"));
    assert_eq!(code, 400, "{}", String::from_utf8_lossy(&body));
    let (code, head, body) = get(&format!("/v1/file?path={rel}&line={line}"));
    assert_eq!(code, 200, "{head} {}", String::from_utf8_lossy(&body));
    assert!(head.contains("Content-Type: application/pdf"), "{head}");
    assert_eq!(body, b"%PDF-1.7\n");
    let (code, _, _) = get(&format!("/v1/file?path={rel}&line=nope"));
    assert_eq!(code, 400);
    let (code, _, _) = get(&format!("/v1/file?path={rel}&term=gone&line={line}"));
    assert_eq!(code, 200);

    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn file_route_serves_raw_bytes_resolved_against_the_term_cwd() {
    use std::io::{Read, Write};
    let app = App::new("file-route", json!({}));
    let get = |target: &str| {
        let mut s = UnixStream::connect(&app.fx.sock).unwrap();
        write!(s, "GET {target} HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
        let mut out = Vec::new();
        s.read_to_end(&mut out).unwrap();
        let at = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8_lossy(&out[..at]).into_owned();
        let code: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (code, head, out[at + 4..].to_vec())
    };
    let dir = app.fx.wt.join("media dir");
    std::fs::create_dir_all(&dir).unwrap();
    let png = [
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0xff, 0x00, 0x10,
    ];
    std::fs::write(dir.join("shot.png"), png).unwrap();

    let abs = dir.join("shot.png").to_string_lossy().replace(' ', "%20");
    let (code, head, body) = get(&format!("/v1/file?path={abs}"));
    assert_eq!(code, 200, "{head}");
    assert!(head.contains("Content-Type: image/png"), "{head}");
    assert_eq!(body, png);

    let (code, _, body) = get(&format!("/v1/file?path={}", abs.replace(".png", ".pn%67")));
    assert_eq!(code, 200);
    assert_eq!(body, png);
    let (code, _, body) = get("/v1/file?path=media%20dir/shot.png");
    assert_eq!(code, 400, "{}", String::from_utf8_lossy(&body));
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].is_string(), "{v}");

    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": app.fx.wt, "cmd": ["/bin/sh"], "cols": 80, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let (code, head, body) = get(&format!("/v1/file?path=media%20dir/shot.png&term={id}"));
    assert_eq!(code, 200, "{head}");
    assert_eq!(body, png);
    let (code, _, _) = get("/v1/file?path=media%20dir/shot.png&term=nope");
    assert_eq!(code, 400);

    let (code, _, body) = get(&format!("/v1/file?path=media%20dir&term={id}"));
    assert_eq!(code, 400, "{}", String::from_utf8_lossy(&body));
    let (code, _, _) = get(&format!("/v1/file?path=missing.png&term={id}"));
    assert_eq!(code, 404);

    let big = dir.join("big.mp4");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(overseer::files::MAX + 1)
        .unwrap();
    let (code, _, body) = get(&format!("/v1/file?path=media%20dir/big.mp4&term={id}"));
    assert_eq!(code, 413);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].as_str().unwrap().contains("52428801"), "{v}");
    let mut ws = app.ws(&id);
    send(&mut ws, "cd 'media dir'\n");
    wait(|| get(&format!("/v1/file?path=shot.png&term={id}")).0 == 200);
    drop(ws);
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn organize_routes_build_record_and_apply_proposals() {
    let app = App::new("organize", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let p = app.ok("POST", "/v1/organize/deterministic", json!({}));
    assert_eq!(p["source"], "deterministic");
    assert_eq!(p["status"], "pending_review");
    assert_eq!(p["version"], 1);
    let (code, v) = app.req(
        "POST",
        "/v1/organize/proposals",
        json!({"plan": {"actions": [{"id": "a", "kind": "rm"}]}}),
    );
    assert_eq!(code, 400);
    assert!(v["problems"][0].as_str().unwrap().contains("unknown kind"));
    let plan = json!({"summary": "s", "actions": [
        {"id": "a1", "kind": "create_folder", "name": "Clients", "ref": "f1"},
        {"id": "a2", "kind": "create_workspace", "path": wt, "folder": "f1"}]});
    let rec = app.ok("POST", "/v1/organize/proposals", json!({ "plan": plan }));
    let id = rec["id"].as_str().unwrap();
    assert_eq!(rec["source"], "agent");
    assert_eq!(rec["actions"][1]["folderName"], "Clients");
    let apply = format!("/v1/organize/proposals/{id}/apply");
    let r = app.ok("POST", &apply, json!({"actionIds": ["a1", "a2"]}));
    assert_eq!(r["results"][0]["status"], "done", "{r}");
    assert_eq!(r["results"][1]["status"], "done", "{r}");
    let again = app.ok("POST", &apply, json!({"actionIds": ["a1", "a2"]}));
    assert!(
        again["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["status"] == "skipped"),
        "{again}"
    );
    let folder = &app.ok("GET", "/v1/folders", Value::Null)[0];
    let ws = app.ok("GET", "/v1/workspaces", Value::Null);
    let mine = ws
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["path"] == wt.as_str())
        .unwrap();
    assert_eq!(mine["folderId"], folder["id"]);
    assert_eq!(
        app.ok("GET", &format!("/v1/organize/proposals/{id}"), Value::Null)["status"],
        "applied"
    );
    assert_eq!(
        app.ok("GET", "/v1/organize/proposals", Value::Null)
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let own = app.ok("POST", "/v1/tasks", json!({"title": "own", "project": wt}));
    let (code, _) = app.req(
        "POST",
        "/v1/organize/proposals",
        json!({"plan": {}, "taskId": own["id"]}),
    );
    assert_eq!(code, 403);
    assert_eq!(
        app.ok(
            "DELETE",
            &format!("/v1/organize/proposals/{id}"),
            Value::Null
        )["status"],
        "dismissed"
    );
    assert_eq!(app.req("POST", &apply, json!({"actionIds": []})).0, 409);
    assert!(app.ok("GET", "/v1/organize/sessions?limit=5", Value::Null)["sessions"].is_array());
    assert!(app.ok("GET", "/v1/organize/layout", Value::Null)["folders"].is_array());
    assert!(app
        .ok("GET", "/v1/organize/projects", Value::Null)
        .is_array());
}

#[test]
fn ctrl_c_interrupts_the_job_and_exit_reports_the_sigint_status() {
    let app = App::new("ctrlc", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["/bin/sh"], "cols": 80, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut a = app.ws(&id);
    // dash drops the rest of the line after a SIGINT and bash 3.2 runs it, so the status is read on its own line.
    send(&mut a, "echo ARMED-$((1+1)); sleep 3719\n");
    read_until(&mut a, "ARMED-2");
    std::thread::sleep(Duration::from_millis(300));
    send(&mut a, "\x03");
    std::thread::sleep(Duration::from_millis(300));
    send(&mut a, "s=$?; echo AFTER-$s; exit $s\n");
    read_until(&mut a, "AFTER-130");
    let pgrep = std::process::Command::new("pgrep")
        .args(["-f", "sleep 3719"])
        .output()
        .unwrap();
    assert_eq!(
        pgrep.status.code(),
        Some(1),
        "the interrupted job is still running or pgrep failed"
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"] == true);
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exitCode"],
        130
    );
    app.ok("DELETE", &format!("/v1/terms/{id}"), Value::Null);
    assert_eq!(
        app.req("GET", &format!("/v1/terms/{id}"), Value::Null).0,
        404
    );
}

#[test]
fn an_agent_that_quits_leaves_a_shell_with_its_session_in_the_tab() {
    // Given a codex session started in a term, linked to its line
    // When codex quits on its own
    // Then the term keeps running a shell that shows the session and how to resume it,
    // and killing the term still ends it
    let app = App::new("agentquit", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "cols": 120, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    send(&mut ws, "busy\r");
    let line = wait_for(|| line_of(&app, &id));
    let sid = line["line"]["agentSessionId"].as_str().unwrap().to_string();
    send(&mut ws, "exit\r");
    read_until(
        &mut ws,
        &format!("codex exited. Session {sid}, resume with:"),
    );
    let term = app.ok("GET", &format!("/v1/terms/{id}"), Value::Null);
    assert_eq!(
        (&term["exited"], &term["agent"], &term["lineId"]),
        (&json!(false), &Value::Null, &Value::Null)
    );
    send(&mut ws, "echo SHELL-$((40+2))\r");
    read_until(&mut ws, "SHELL-42");
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"] == true);
}

#[test]
fn an_agent_that_quits_right_after_starting_a_turn_still_leaves_its_session() {
    // Given a codex session whose first turn just started, before any scan linked it to the term
    // When codex quits at once
    // Then the shell note still carries the session and how to resume it
    let app = App::new("agentquitfast", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "cols": 120, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    send(&mut ws, "busy\r");
    send(&mut ws, "exit\r");
    let shown = read_until(&mut ws, ", resume with:");
    assert!(shown.contains("codex exited. Session "), "{shown:?}");
}

#[test]
fn closing_an_idle_codex_session_accepts_its_dim_composer_prompt() {
    // Given a Codex session at its idle composer, whose prompt marker is dim gray
    // When the app checks and closes the selected agent through the real daemon
    // Then the prompt passes the safety check and the line is closed
    let app = App::new("closecodex", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "cols": 120, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    send(&mut ws, "busy\r");
    let line = wait_for(|| line_of(&app, &id));
    let lid = line["line"]["id"].as_str().unwrap().to_string();
    wait(|| line_json(&app, &lid)["local"]["busy"] == true);
    send(&mut ws, "done\r");
    wait(|| line_json(&app, &lid)["local"]["busy"] == false);

    let (code, plan) = app.req("POST", &format!("/v1/lines/{lid}/close"), Value::Null);
    assert_eq!(code, 200, "{plan}");
    assert_eq!(plan["action"], "would-close", "{plan}");
    assert_eq!(plan["predicates"]["inputEmpty"]["ok"], true, "{plan}");

    let closed = app.ok(
        "POST",
        &format!("/v1/lines/{lid}/close?dryRun=false"),
        Value::Null,
    );
    assert_eq!(closed["action"], "closed", "{closed}");
    wait(|| line_json(&app, &lid)["line"]["state"] == "hung-up");
}

#[test]
fn ending_a_busy_agent_session_needs_force_and_then_ends_its_term() {
    // Given a codex session in a term that is busy with a turn
    // When the session is closed as ending its tab does, first without and then with force=true
    // Then the plain close is refused and keeps the term running, and the forced one ends the term
    let app = App::new("endbusy", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "agent": "codex", "cols": 120, "rows": 24}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    send(&mut ws, "busy\r");
    let line = wait_for(|| line_of(&app, &id));
    let lid = line["line"]["id"].as_str().unwrap().to_string();
    wait(|| line_of(&app, &id).is_some_and(|l| l["local"]["busy"] == true));
    let (code, r) = app.req(
        "POST",
        &format!("/v1/lines/{lid}/close?dryRun=false"),
        Value::Null,
    );
    assert_eq!(code, 409, "{r}");
    assert_eq!(r["predicates"]["atPrompt"]["ok"], false, "{r}");
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"],
        false
    );
    let (code, r) = app.req(
        "POST",
        &format!("/v1/lines/{lid}/close?dryRun=false&force=true"),
        Value::Null,
    );
    assert_eq!(code, 200, "{r}");
    assert_eq!(r["action"], "closed", "{r}");
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"] == true);
}

#[test]
fn killing_an_agent_term_closes_it_without_a_shell() {
    // Given a codex session running in a term
    // When the term is killed, as closing its tab does
    // Then the term exits and no shell is started in it
    let app = App::new("agentkill", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "codex"}));
    let id = t["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&id);
    read_until(&mut ws, "codex ready");
    app.ok(
        "POST",
        &format!("/v1/terms/{id}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{id}"), Value::Null)["exited"] == true);
    let term = app.ok("GET", &format!("/v1/terms/{id}"), Value::Null);
    assert_eq!(term["cmd"][0], "codex");
}

#[test]
fn codex_line_never_takes_the_pid_of_another_terminal_of_its_repo() {
    let app = App::new("codexpid", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let start = || {
        let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "codex"}));
        let id = t["id"].as_str().unwrap().to_string();
        let mut ws = app.ws(&id);
        read_until(&mut ws, "codex ready");
        (id, t["pid"].as_u64().unwrap(), ws)
    };
    let (_a, pid_a, mut ws_a) = start();
    let (_b, pid_b, mut ws_b) = start();
    let codex_lines = || -> Vec<(u64, Option<u64>)> {
        let v = app.ok("GET", "/v1/lines?all=true", Value::Null);
        let mut out: Vec<(u64, Option<u64>)> = v
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["line"]["agent"] == "codex")
            .map(|l| {
                (
                    l["line"]["activityMs"].as_u64().unwrap_or(0),
                    l["local"]["pid"].as_u64(),
                )
            })
            .collect();
        out.sort();
        out
    };
    send(&mut ws_a, "busy\r");
    wait_for(|| (codex_lines().len() == 1).then_some(()));
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(500));
        let l = codex_lines();
        assert_eq!(l.len(), 1, "{l:?}");
        assert_eq!(l[0].1, Some(pid_a), "{l:?} a={pid_a} b={pid_b}");
    }
    send(&mut ws_b, "busy\r");
    wait_for(|| (codex_lines().len() == 2).then_some(()));
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(500));
        let mut pids: Vec<Option<u64>> = codex_lines().iter().map(|l| l.1).collect();
        pids.sort();
        assert_eq!(pids, [Some(pid_a), Some(pid_b)]);
    }
    send(&mut ws_a, "busy\r");
    std::thread::sleep(Duration::from_secs(2));
    let mut pids: Vec<Option<u64>> = codex_lines().iter().map(|l| l.1).collect();
    pids.sort();
    assert_eq!(pids, [Some(pid_a), Some(pid_b)]);
}

#[test]
fn a_restored_feedback_session_resumes_without_its_initial_prompt() {
    let fx = Fx::new("resume-no-prompt", 0);
    let log = fx.base.join("stub.log");
    let spawn = || {
        let mut c = Command::new(bin());
        fx.env(&mut c);
        c.env("SB_BIN", bin())
            .env("SB_STUB_LOG", &log)
            .env("SHELL", "/bin/sh")
            .args(["daemon", "--interval", "1"]);
        let d = common::Reap::spawn(c.stderr(Stdio::null()));
        wait(|| UnixStream::connect(&fx.sock).is_ok());
        d
    };
    let stub = || std::fs::read_to_string(&log).unwrap_or_default();
    let cwd = fx.wt.to_string_lossy().into_owned();
    let d = spawn();
    let body = json!({
        "agent": "claude", "effort": "high", "cwd": cwd,
        "body": "the app froze",
        "prompt": "File this issue from @REPORT@ and do not ask questions",
    });
    let (code, out) = fx.req("POST", "/v1/feedback/start", &body.to_string());
    assert_eq!(code, 201, "{out}");
    let term = serde_json::from_str::<Value>(&out).unwrap()["term"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    wait(|| stub().contains("File this issue from"));
    assert!(stub().contains("--permission-mode auto"), "{}", stub());
    assert_eq!(stub().lines().count(), 1, "{}", stub());
    wait(|| {
        let (_, v) = fx.req("GET", "/v1/lines?all=true", "");
        v.contains(&term) && v.contains("--effort")
    });
    std::thread::sleep(Duration::from_secs(4));
    drop(d);
    let _d = spawn();
    wait(|| stub().lines().count() >= 2);
    let log = stub();
    let relaunch = log.lines().nth(1).unwrap();
    assert!(relaunch.contains("--resume"), "{log}");
    assert!(relaunch.contains("--effort high"), "{log}");
    assert!(!relaunch.contains("File this issue"), "{log}");
    assert!(!relaunch.contains("do not ask"), "{log}");
}

fn upload(fx: &Fx, name: &str, data: &str) -> Value {
    let (code, out) = fx.req("POST", &format!("/v1/uploads?name={name}"), data);
    assert_eq!(code, 201, "{out}");
    serde_json::from_str(&out).unwrap()
}

fn attach_body() -> &'static str {
    "It broke\n\n### Attachments\n@ATTACHMENTS@\n\n_Sent from Overseer._"
}

fn direct(fx: &Fx, repo: &str, files: &[&Value]) -> (u16, Value) {
    let list: Vec<Value> = files
        .iter()
        .map(|f| json!({"path": f["path"], "name": f["name"]}))
        .collect();
    let body = json!({"repo": repo, "title": "Bug: x", "labels": ["bug"], "body": attach_body(), "attachments": list});
    let (code, out) = fx.req("POST", "/v1/feedback/direct", &body.to_string());
    (
        code,
        serde_json::from_str(&out).unwrap_or(Value::String(out)),
    )
}

fn gh_log(fx: &Fx) -> String {
    std::fs::read_to_string(fx.base.join("gh.log")).unwrap_or_default()
}

#[test]
fn direct_feedback_attaches_files_with_gh() {
    let fx = Fx::new("feedback-attach", 0);
    let _d = fx.daemon(false);
    let shot = upload(&fx, "shot.png", "PNGDATA");
    let clip = upload(&fx, "clip.mov", "MOVDATA");
    let (code, out) = direct(&fx, "o/r", &[&shot, &clip]);
    assert_eq!(code, 201, "{out}");
    assert!(out.get("attachWarning").is_none(), "{out}");
    let log = gh_log(&fx);
    for f in [&shot, &clip] {
        assert!(
            log.contains(&format!("--attach={}", f["path"].as_str().unwrap())),
            "{log}"
        );
    }
    let issue = std::fs::read_to_string(fx.base.join("gh.log.body")).unwrap();
    assert!(
        !issue.contains("@ATTACHMENTS@") && issue.contains("Attached below."),
        "{issue}"
    );
}

#[test]
fn direct_feedback_attaches_a_screen_recording_and_keeps_its_action_log() {
    let fx = Fx::new("feedback-recording", 0);
    let _d = fx.daemon(false);
    let rec = upload(&fx, "recording-20261109-162639.webm", "WEBMDATA");
    let report = "It broke\n\n### Attachments\n@ATTACHMENTS@\n\n<details><summary>Recorded actions (0:05)</summary>\n\n```text\n0:00  start  Recording started with screen video\n0:03  click  button \"Settings\"\n```\n\n</details>\n\n_Sent from Overseer._";
    let body = json!({"repo": "o/r", "title": "Bug: x", "labels": ["bug"], "body": report, "attachments": [{"path": rec["path"], "name": rec["name"]}]});
    let (code, out) = fx.req("POST", "/v1/feedback/direct", &body.to_string());
    assert_eq!(code, 201, "{out}");
    assert!(!out.contains("attachWarning"), "{out}");
    let log = gh_log(&fx);
    assert!(
        log.contains(&format!("--attach={}", rec["path"].as_str().unwrap())),
        "{log}"
    );
    let issue = std::fs::read_to_string(fx.base.join("gh.log.body")).unwrap();
    assert!(issue.contains("Recorded actions (0:05)"), "{issue}");
    assert!(
        issue.contains("0:03  click  button \"Settings\""),
        "{issue}"
    );
}

#[test]
fn direct_feedback_reports_files_gh_refused_but_keeps_the_issue() {
    let fx = Fx::new("feedback-attach-partial", 0);
    let _d = fx.daemon(false);
    let clip = upload(&fx, "clip.mov", "MOVDATA");
    let (code, out) = direct(&fx, "partial/r", &[&clip]);
    assert_eq!(code, 201, "{out}");
    assert_eq!(out["url"], "https://github.com/partial/r/issues/7");
    assert!(
        out["attachWarning"].as_str().unwrap().contains("clip.mov"),
        "{out}"
    );
}

#[test]
fn direct_feedback_lists_files_when_gh_is_too_old_to_attach() {
    let fx = Fx::new("feedback-attach-old", 0);
    std::fs::create_dir_all(fx.base.join("gh")).unwrap();
    std::fs::write(fx.base.join("gh/old"), "").unwrap();
    let _d = fx.daemon(false);
    let clip = upload(&fx, "clip.mov", "MOVDATA");
    let (code, out) = direct(&fx, "o/r", &[&clip]);
    assert_eq!(code, 201, "{out}");
    assert!(!gh_log(&fx).contains("--attach"), "{}", gh_log(&fx));
    let issue = std::fs::read_to_string(fx.base.join("gh.log.body")).unwrap();
    assert!(
        issue.contains("(not uploaded: gh 2.99+ is needed to attach files)"),
        "{issue}"
    );
    assert!(issue.contains("~/.overseer/u/"), "{issue}");
}

#[test]
fn feedback_rejects_attachments_github_would_not_accept() {
    let fx = Fx::new("feedback-attach-bad", 0);
    let _d = fx.daemon(false);
    let notes = upload(&fx, "notes.txt", "hello");
    let (code, out) = direct(&fx, "o/r", &[&notes]);
    assert_eq!(code, 400, "{out}");
    assert!(
        out["error"].as_str().unwrap().contains("notes.txt"),
        "{out}"
    );
    let big = upload(&fx, "big.png", &"x".repeat(10 * 1024 * 1024 + 1));
    let (code, out) = direct(&fx, "o/r", &[&big]);
    assert_eq!(code, 400, "{out}");
    assert!(out["error"].as_str().unwrap().contains("10 MB"), "{out}");
    let outside = json!({"path": "/etc/passwd", "name": "passwd.png"});
    assert_eq!(direct(&fx, "o/r", &[&outside]).0, 400);
    assert!(gh_log(&fx).is_empty());
}

fn install_amp(app: &App) -> (String, WebSocketLike) {
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"install": "amp", "cwd": wt, "cols": 120, "rows": 32}),
    );
    assert_eq!(
        t["cmd"],
        json!(["npm install -g --allow-scripts=@ampcode/cli @sourcegraph/amp"])
    );
    assert_eq!(t["title"], "Install Amp");
    assert!(t["agent"].is_null());
    let id = t["id"].as_str().unwrap().to_string();
    let ws = app.ws(&id);
    (id, ws)
}

type WebSocketLike = tungstenite::WebSocket<UnixStream>;

fn exit_code(app: &App, id: &str) -> Value {
    wait_for(|| {
        let t = app.ok("GET", &format!("/v1/terms/{id}"), Value::Null);
        (t["exited"] == true).then(|| t["exitCode"].clone())
    })
}

#[test]
fn install_term_runs_the_agent_install_command_and_exits_with_its_status() {
    let app = App::new("install", json!({}));
    let (id, mut ws) = install_amp(&app);
    read_until(&mut ws, "added 1 package (stub)");
    assert_eq!(exit_code(&app, &id), 0);
    let log = std::fs::read_to_string(app.fx.base.join("stub.log")).unwrap();
    assert!(
        log.contains("npm install -g --allow-scripts=@ampcode/cli @sourcegraph/amp"),
        "{log}"
    );
}

#[test]
fn install_term_reports_a_failed_install_and_keeps_its_exit_code() {
    let app = App::with_env("installfail", json!({}), &[("SB_NPM_FAIL", "1")]);
    let (id, mut ws) = install_amp(&app);
    read_until(&mut ws, "Install failed with exit code 1");
    assert_eq!(exit_code(&app, &id), 1);
}

#[test]
fn install_term_rejects_agents_without_an_install_command() {
    let app = App::new("installbad", json!({}));
    let (code, v) = app.req("POST", "/v1/terms", json!({"install": "agy"}));
    assert_eq!(
        (code, v["error"].as_str()),
        (400, Some("Antigravity has no install command"))
    );
    let (code, v) = app.req("POST", "/v1/terms", json!({"install": "nope"}));
    assert_eq!(
        (code, v["error"].as_str()),
        (400, Some("unknown agent nope"))
    );
    let (code, v) = app.req(
        "POST",
        "/v1/terms",
        json!({"install": "amp", "agent": "amp"}),
    );
    assert_eq!(
        (code, v["error"].as_str()),
        (400, Some("give either install, agent or cmd"))
    );
}

fn resumes_of(app: &App, sid: &str) -> usize {
    stub_log(app)
        .lines()
        .filter(|l| l.contains(&format!("--resume {sid}")))
        .count()
}

fn resume_line(app: &App, line: &str) -> Value {
    let (code, r) = app.req(
        "POST",
        &format!("/v1/lines/{line}/resume?dryRun=false"),
        json!({"target": "term"}),
    );
    assert!(code == 200 || code == 201, "{code}: {r}");
    r
}

#[test]
fn resuming_again_while_the_agent_waits_on_its_trust_prompt_reuses_its_term() {
    // Given a stopped claude session whose resume stops at the folder trust prompt, before any hook
    // When Resume is pressed again while that prompt is shown
    // Then the same term comes back, no second agent starts, and trusting it brings the session back
    let sid = "cccccccc-0000-4000-8000-000000000071";
    let app = App::new("resumetrust", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
    wait(|| line_json(&app, &line)["local"]["pid"].is_null());
    std::fs::write(app.fx.base.join("stub.log.trust"), "").unwrap();
    let first = resume_line(&app, &line)["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&first);
    read_until(&mut ws, "Yes, I trust this folder");
    let again = resume_line(&app, &line);
    assert_eq!(again["id"], first.as_str());
    assert_eq!(resumes_of(&app, sid), 1);
    assert_eq!(line_json(&app, &line)["local"]["pid"], Value::Null);
    send(&mut ws, "trust\r");
    wait(|| line_json(&app, &line)["local"]["pid"].is_u64());
    assert_eq!(resumes_of(&app, sid), 1);
    app.ok(
        "POST",
        &format!("/v1/terms/{first}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn resuming_a_paused_session_twice_does_not_press_enter_on_its_trust_prompt() {
    // Given a paused claude session resumed from its held stub, now at the folder trust prompt
    // When Resume is pressed a second time
    // Then no Enter reaches the prompt (it would answer "No, exit"), and the agent still waits on it
    let sid = "cccccccc-0000-4000-8000-000000000072";
    let app = App::new("resumeheldtrust", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false"),
        Value::Null,
    );
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "press Enter to resume");
    std::fs::write(app.fx.base.join("stub.log.trust"), "").unwrap();
    assert_eq!(resume_line(&app, &line)["id"], tid.as_str());
    read_until(&mut ws, "Yes, I trust this folder");
    wait(|| line_json(&app, &line)["local"]["heldReason"] != "user");
    assert_eq!(resume_line(&app, &line)["id"], tid.as_str());
    send(&mut ws, "trust\r");
    wait(|| line_json(&app, &line)["local"]["pid"].is_u64());
    assert_eq!(resumes_of(&app, sid), 1);
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn closing_a_session_still_at_its_trust_prompt_ends_the_starting_agent() {
    // Given a stopped claude session resumed into a term that waits on the folder trust prompt
    // When the session is closed
    // Then that term ends instead of being left behind with an agent nobody sees
    let sid = "cccccccc-0000-4000-8000-000000000073";
    let app = App::new("closetrust", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
    wait(|| line_json(&app, &line)["local"]["pid"].is_null());
    std::fs::write(app.fx.base.join("stub.log.trust"), "").unwrap();
    let starting = resume_line(&app, &line)["id"].as_str().unwrap().to_string();
    let mut ws = app.ws(&starting);
    read_until(&mut ws, "Yes, I trust this folder");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/close?dryRun=false"),
        Value::Null,
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{starting}"), Value::Null)["exited"] == true);
}

#[test]
fn two_quick_resumes_of_a_paused_session_press_enter_on_its_stub_once() {
    // Given a paused claude session whose agent will stop at the folder trust prompt
    // When Resume is pressed twice in a row, before the stub has started the agent
    // Then only the first press reaches the stub, so the prompt still waits for an answer
    let sid = "cccccccc-0000-4000-8000-000000000074";
    let app = App::new("resumeheldtwice", json!({}));
    let (tid, line) = claude_term(&app, json!(["claude", "--session-id", sid]), sid);
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "❯");
    app.ok(
        "POST",
        &format!("/v1/lines/{line}/hold?dryRun=false"),
        Value::Null,
    );
    wait(|| line_json(&app, &line)["line"]["state"] == "held");
    let mut ws = app.ws(&tid);
    read_until(&mut ws, "press Enter to resume");
    std::fs::write(app.fx.base.join("stub.log.trust"), "").unwrap();
    assert_eq!(resume_line(&app, &line)["id"], tid.as_str());
    assert_eq!(resume_line(&app, &line)["id"], tid.as_str());
    read_until(&mut ws, "Yes, I trust this folder");
    std::thread::sleep(std::time::Duration::from_millis(500));
    send(&mut ws, "trust\r");
    wait(|| line_json(&app, &line)["local"]["pid"].is_u64());
    assert_eq!(resumes_of(&app, sid), 1);
    app.ok(
        "POST",
        &format!("/v1/terms/{tid}/kill"),
        json!({"confirm": true}),
    );
}

// Given a daemon started before an installer created ~/.local/bin
// When the agent lands there and the app asks for a rescan
// Then the agent is detected, /v1/env shows the dir and new terms find the agent
#[test]
fn rescan_finds_an_agent_installed_into_a_dir_created_after_start() {
    let app = App::new("rescan-new-dir", json!({}));
    let cursor = |agents: &Value| {
        agents
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "cursor")
            .unwrap()["installed"]
            .clone()
    };
    assert_eq!(
        cursor(&app.ok("POST", "/v1/agents/rescan", Value::Null)),
        false
    );
    let dir = app.fx.base.join("home/.local/bin");
    assert!(!dir.exists());
    std::fs::create_dir_all(&dir).unwrap();
    let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stubs/agent");
    std::fs::copy(stub, dir.join("cursor-agent")).unwrap();

    assert_eq!(
        cursor(&app.ok("POST", "/v1/agents/rescan", Value::Null)),
        true
    );
    assert_eq!(cursor(&app.ok("GET", "/v1/agents", Value::Null)), true);
    let dir = dir.to_string_lossy().into_owned();
    let env = app.ok("GET", "/v1/env", Value::Null);
    assert!(
        env["path"].as_str().unwrap().split(':').any(|d| d == dir),
        "{env}"
    );
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": wt, "cmd": ["cursor-agent"], "cols": 80, "rows": 24}),
    );
    let mut ws = app.ws(t["id"].as_str().unwrap());
    read_until(&mut ws, "cursor-agent ready");
    send(&mut ws, "exit\n");

    let t = app.ok(
        "POST",
        "/v1/tasks",
        json!({"title": "cur", "description": "", "project": wt, "agent": "cursor"}),
    );
    let id = t["id"].as_str().unwrap().to_string();
    let s = app.ok("POST", &format!("/v1/tasks/{id}/start"), json!({}));
    let mut ws = app.ws(s["termId"].as_str().unwrap());
    read_until(&mut ws, "cursor-agent ready");
}
