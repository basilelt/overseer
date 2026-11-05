mod common;

use common::{bin, read_until, send, wait, wait_for, App, Reap, SID, T};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn events(app: &App) -> (UnixStream, String) {
    let mut s = UnixStream::connect(&app.fx.sock).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    s.write_all(b"GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n")
        .unwrap();
    let mut buf = String::new();
    expect(&mut s, &mut buf, &["retry:"]);
    (s, buf)
}

fn expect(s: &mut UnixStream, buf: &mut String, want: &[&str]) {
    let end = Instant::now() + Duration::from_secs(20);
    while !want.iter().all(|w| buf.contains(w)) {
        assert!(Instant::now() < end, "missing {want:?} in {buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
}

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_string()
}

fn find(list: &Value, k: &str, v: &str) -> Option<Value> {
    list.as_array().unwrap().iter().find(|x| x[k] == v).cloned()
}

fn line_of(app: &App, term: &str) -> Option<String> {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["local"]["termId"] == term)
        .map(|l| l["line"]["id"].as_str().unwrap().to_string())
}

fn result(v: &Value, target: &str) -> Value {
    v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["target"] == target)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn workspaces_folders_and_groups_crud_with_events() {
    let app = App::new("ws-crud", json!({"autoConcurrency": 0}));
    let (mut sse, mut buf) = events(&app);
    let wt = app.fx.wt.to_string_lossy().into_owned();
    app.ok("POST", "/v1/projects", json!({"path": wt}));
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    let auto = find(&list, "path", &wt).unwrap();
    assert_eq!(auto["auto"], true);
    assert_eq!(auto["name"], "repo");
    assert_eq!(auto["station"], "vm-local");
    assert_eq!(auto["folderId"], Value::Null);
    assert_eq!(auto["tabs"], json!([]));
    assert_eq!(auto["pinned"], false);
    assert_eq!(auto["notifyIdle"], false);
    let n = list.as_array().unwrap().len();
    assert_eq!(
        app.ok("GET", "/v1/workspaces", Value::Null)
            .as_array()
            .unwrap()
            .len(),
        n
    );
    for p in [wt.clone(), format!("{wt}/")] {
        let (code, v) = app.req("POST", "/v1/workspaces", json!({ "path": p }));
        assert_eq!((code, id(&v)), (200, id(&auto)));
    }
    let file = app.fx.wt.join("a.txt");
    for bad in [
        json!({}),
        json!({"path": "rel/x"}),
        json!({"path": "/nonexistent/sb"}),
        json!({ "path": file }),
        json!({"path": wt, "folderId": "nope"}),
        json!({"path": wt, "name": "  "}),
    ] {
        assert_eq!(
            app.req("POST", "/v1/workspaces", bad.clone()).0,
            400,
            "{bad}"
        );
    }
    std::fs::create_dir_all(app.fx.base.join("home/notes")).unwrap();
    let (code, notes) = app.req("POST", "/v1/workspaces", json!({"path": "~/notes"}));
    assert_eq!(code, 201, "{notes}");
    assert_eq!(notes["auto"], false);
    assert_eq!(notes["name"], "notes");
    assert_eq!(
        notes["path"],
        app.fx.base.join("home/notes").to_string_lossy().as_ref()
    );
    let nid = id(&notes);
    let p = app.ok(
        "PATCH",
        &format!("/v1/workspaces/{nid}"),
        json!({"name": "Notes", "pinned": true, "tabs": ["line:x", "browser:https://e.x"], "notifyIdle": true, "rank": 5}),
    );
    assert_eq!(p["name"], "Notes");
    assert_eq!(p["tabs"], json!(["line:x", "browser:https://e.x"]));
    assert_eq!(
        (p["pinned"].clone(), p["notifyIdle"].clone()),
        (json!(true), json!(true))
    );
    assert_eq!(
        app.ok("GET", "/v1/workspaces", Value::Null)[0]["id"],
        nid.as_str()
    );
    for bad in [
        json!({"name": ""}),
        json!({"tabs": [1]}),
        json!({"folderId": "nope"}),
        json!({"pinned": "yes"}),
        json!({"rank": "x"}),
    ] {
        assert_eq!(
            app.req("PATCH", &format!("/v1/workspaces/{nid}"), bad.clone())
                .0,
            400,
            "{bad}"
        );
    }
    assert_eq!(app.req("PATCH", "/v1/workspaces/nope", json!({})).0, 404);

    app.ok(
        "PATCH",
        &format!("/v1/workspaces/{nid}"),
        json!({"archived": true}),
    );
    let (code, back) = app.req("POST", "/v1/workspaces", json!({"path": "~/notes"}));
    assert_eq!((code, back["archived"].clone()), (200, json!(false)));

    let aid = id(&auto);
    assert_eq!(
        app.ok("DELETE", &format!("/v1/workspaces/{aid}"), Value::Null),
        json!({ "deleted": aid })
    );
    assert!(find(&app.ok("GET", "/v1/workspaces", Value::Null), "path", &wt).is_none());
    let (code, manual) = app.req("POST", "/v1/workspaces", json!({ "path": wt }));
    assert_eq!((code, manual["auto"].clone()), (201, json!(false)));
    app.ok(
        "DELETE",
        &format!("/v1/workspaces/{}", id(&manual)),
        Value::Null,
    );
    assert!(find(&app.ok("GET", "/v1/workspaces", Value::Null), "path", &wt).is_none());
    assert_eq!(app.req("DELETE", "/v1/workspaces/nope", Value::Null).0, 404);

    let a = app.ok("POST", "/v1/folders", json!({"name": "A"}));
    let b = app.ok(
        "POST",
        "/v1/folders",
        json!({"name": "B", "parentId": id(&a)}),
    );
    assert_eq!(b["parentId"], id(&a).as_str());
    assert_eq!(b["collapsed"], false);
    let other = app.ok("POST", "/v1/folders", json!({"name": "Other"}));
    for bad in [
        json!({"name": "C", "parentId": id(&b)}),
        json!({"name": "C", "parentId": "nope"}),
        json!({"name": " "}),
        json!({ "name": "x".repeat(81) }),
    ] {
        assert_eq!(app.req("POST", "/v1/folders", bad.clone()).0, 400, "{bad}");
    }
    assert_eq!(
        app.req(
            "PATCH",
            &format!("/v1/folders/{}", id(&a)),
            json!({"parentId": id(&other)})
        )
        .0,
        400
    );
    assert_eq!(
        app.req(
            "PATCH",
            &format!("/v1/folders/{}", id(&b)),
            json!({"parentId": id(&b)})
        )
        .0,
        400
    );
    let b2 = app.ok(
        "PATCH",
        &format!("/v1/folders/{}", id(&b)),
        json!({"name": "B2", "collapsed": true, "rank": 1}),
    );
    assert_eq!(
        (b2["name"].clone(), b2["collapsed"].clone()),
        (json!("B2"), json!(true))
    );
    let moved = app.ok(
        "PATCH",
        &format!("/v1/workspaces/{nid}"),
        json!({"folderId": id(&b)}),
    );
    assert_eq!(moved["folderId"], id(&b).as_str());
    app.ok("DELETE", &format!("/v1/folders/{}", id(&b)), Value::Null);
    let ws = |wid: &str| find(&app.ok("GET", "/v1/workspaces", Value::Null), "id", wid).unwrap();
    assert_eq!(ws(&nid)["folderId"], id(&a).as_str());
    let c = app.ok(
        "POST",
        "/v1/folders",
        json!({"name": "C", "parentId": id(&a)}),
    );
    app.ok("DELETE", &format!("/v1/folders/{}", id(&a)), Value::Null);
    assert_eq!(ws(&nid)["folderId"], Value::Null);
    let folders = app.ok("GET", "/v1/folders", Value::Null);
    assert_eq!(
        find(&folders, "id", &id(&c)).unwrap()["parentId"],
        Value::Null
    );
    assert!(find(&folders, "id", &id(&a)).is_none());
    assert_eq!(
        app.req("DELETE", &format!("/v1/folders/{}", id(&a)), Value::Null)
            .0,
        404
    );

    let g = app.ok(
        "POST",
        "/v1/groups",
        json!({"name": " G ", "members": ["line:x", "term:y", "line:x"]}),
    );
    assert_eq!(g["name"], "G");
    assert_eq!(g["members"], json!(["line:x", "term:y"]));
    for bad in [
        json!({"name": "G", "members": ["foo:1"]}),
        json!({"name": "G", "members": ["line:"]}),
        json!({"name": "G", "members": "line:x"}),
        json!({"members": []}),
    ] {
        assert_eq!(app.req("POST", "/v1/groups", bad.clone()).0, 400, "{bad}");
    }
    let gid = id(&g);
    let g = app.ok(
        "PATCH",
        &format!("/v1/groups/{gid}"),
        json!({"add": ["line:z", "line:x"], "remove": ["term:y"], "notifyIdle": true}),
    );
    assert_eq!(g["members"], json!(["line:x", "line:z"]));
    assert_eq!(g["notifyIdle"], true);
    let g = app.ok(
        "PATCH",
        &format!("/v1/groups/{gid}"),
        json!({"members": ["term:q"], "rank": 3}),
    );
    assert_eq!(g["members"], json!(["term:q"]));
    assert_eq!(
        app.req("PATCH", &format!("/v1/groups/{gid}"), json!({"add": ["x"]}))
            .0,
        400
    );
    assert_eq!(
        app.ok("GET", "/v1/groups", Value::Null)[0]["id"],
        gid.as_str()
    );
    app.ok("DELETE", &format!("/v1/groups/{gid}"), Value::Null);
    assert_eq!(
        app.req("DELETE", &format!("/v1/groups/{gid}"), Value::Null)
            .0,
        404
    );

    expect(
        &mut sse,
        &mut buf,
        &[
            "event: workspace.created",
            "event: workspace.updated",
            "event: workspace.deleted",
            "event: folder.created",
            "event: folder.updated",
            "event: folder.deleted",
            "event: group.created",
            "event: group.updated",
            "event: group.deleted",
        ],
    );
}

#[test]
fn broadcast_types_into_idle_daemon_terms_and_queues_for_busy_ones() {
    let app = App::new("bcast", json!({"autoConcurrency": 0}));
    let (mut sse, mut buf) = events(&app);
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let t = app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "claude"}));
    let term = id(&t);
    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");
    let line = wait_for(|| line_of(&app, &term));
    let lk = format!("line:{line}");
    let bc = |body: Value| app.ok("POST", "/v1/broadcast", body);

    let r = wait_for(|| {
        let r = bc(json!({"text": "  hello one  ", "lineIds": [line]}));
        (result(&r, &lk)["result"] == "sent").then_some(r)
    });
    assert_eq!(r["results"].as_array().unwrap().len(), 1);
    assert!(r["id"].is_string());
    read_until(&mut ws, "got: hello one");

    send(&mut ws, "busy\r");
    wait(|| app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null)["busy"] == true);
    let r = bc(json!({"text": "nope", "lineIds": [line], "termIds": [term]}));
    assert_eq!(
        result(&r, &lk),
        json!({"target": lk, "result": "skipped", "reason": "busy"})
    );
    assert_eq!(result(&r, &format!("term:{term}"))["result"], "skipped");
    let r = bc(json!({"text": "forced", "lineIds": [line], "onlyIdle": false}));
    assert_eq!(result(&r, &lk)["result"], "sent");
    read_until(&mut ws, "got: forced");
    let r = bc(json!({"text": "later", "lineIds": [line], "queue": true}));
    assert_eq!(result(&r, &lk)["result"], "queued");
    let qid = id(&r);
    send(&mut ws, "done\r");
    read_until(&mut ws, "got: later");
    expect(
        &mut sse,
        &mut buf,
        &[
            "event: broadcast.delivered",
            &format!(r#""id":"{qid}""#),
            r#""result":"sent""#,
        ],
    );
    wait(|| app.ok("GET", &format!("/v1/lines/{line}/activity"), Value::Null)["busy"] == false);

    let sh = id(&app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]})));
    let mut shws = app.ws(&sh);
    let g = app.ok(
        "POST",
        "/v1/groups",
        json!({"name": "both", "members": [lk, format!("term:{sh}")]}),
    );
    std::thread::sleep(Duration::from_millis(2000));
    let r = bc(json!({"text": "echo grp-$((6*7))", "groupId": id(&g)}));
    assert_eq!(result(&r, &lk)["result"], "sent", "{r}");
    assert_eq!(result(&r, &format!("term:{sh}"))["result"], "sent", "{r}");
    read_until(&mut shws, "grp-42");
    read_until(&mut ws, "got: echo grp-$((6*7))");

    let w = find(&app.ok("GET", "/v1/workspaces", Value::Null), "path", &wt).unwrap();
    std::thread::sleep(Duration::from_millis(2000));
    let r = bc(json!({"text": "echo ws-$((5*5))", "workspaceId": id(&w)}));
    assert_eq!(r["results"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(result(&r, &lk)["result"], "sent", "{r}");
    assert_eq!(result(&r, &format!("term:{sh}"))["result"], "sent", "{r}");
    read_until(&mut shws, "ws-25");

    for (body, code) in [
        (json!({"text": "x"}), 400),
        (
            json!({"text": "x", "lineIds": [line], "groupId": id(&g)}),
            400,
        ),
        (json!({"text": "   ", "lineIds": [line]}), 400),
        (
            json!({"text": "x".repeat(65 << 10), "lineIds": [line]}),
            400,
        ),
        (json!({"text": "x", "lineIds": [line], "queue": "yes"}), 400),
        (json!({"text": "x", "workspaceId": "nope"}), 404),
        (json!({"text": "x", "groupId": "nope"}), 404),
    ] {
        assert_eq!(
            app.req("POST", "/v1/broadcast", body.clone()).0,
            code,
            "{body}"
        );
    }
    let r = bc(json!({"text": "x", "lineIds": ["nope"], "termIds": ["nope"]}));
    assert_eq!(result(&r, "line:nope")["result"], "refused");
    assert_eq!(result(&r, "term:nope")["result"], "refused");

    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["exited"] == true);
    let r = bc(json!({"text": "gone", "termIds": [term]}));
    assert_eq!(
        result(&r, &format!("term:{term}")),
        json!({"target": format!("term:{term}"), "result": "refused", "reason": "exited"})
    );
    wait_for(|| {
        let r = bc(json!({"text": "gone", "lineIds": [line]}));
        (result(&r, &lk)["reason"] == "not running").then_some(())
    });

    let log = std::fs::read_to_string(app.fx.state.join("actions.log")).unwrap();
    let recs: Vec<Value> = log
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["action"] == "broadcast")
        .collect();
    assert!(recs.len() >= 8, "{log}");
    assert!(recs.iter().any(|v| v["chars"] == 9 && v["id"].is_string()));
    for secret in ["hello one", "forced", "later", "grp-", "ws-"] {
        assert!(!log.contains(secret), "{secret} leaked in {log}");
    }
    app.ok(
        "POST",
        &format!("/v1/terms/{sh}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_home_workspace_does_not_capture_a_repo_worktree() {
    let app = App::new("ws-home", json!({"autoConcurrency": 0}));
    let home = app.fx.base.join("home");
    let tree = home.join("trees/feature");
    common::git(
        &app.fx.wt,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feat/x",
            &tree.to_string_lossy(),
        ],
    );
    let tree = tree.canonicalize().unwrap();
    let h = app.ok("POST", "/v1/workspaces", json!({"path": "~"}));
    assert_eq!(h["path"], home.to_string_lossy().as_ref());
    let t = app.ok("POST", "/v1/terms", json!({"cwd": tree, "agent": "claude"}));
    let term = id(&t);
    let mut ws = app.ws(&term);
    read_until(&mut ws, "❯");
    let line = wait_for(|| line_of(&app, &term));
    let lk = format!("line:{line}");
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let repo = wait_for(|| find(&app.ok("GET", "/v1/workspaces", Value::Null), "path", &wt));
    let r = wait_for(|| {
        let r = app.ok(
            "POST",
            "/v1/broadcast",
            json!({"text": "in repo", "workspaceId": id(&repo)}),
        );
        (result(&r, &lk)["result"] == "sent").then_some(r)
    });
    assert_eq!(r["results"].as_array().unwrap().len(), 1, "{r}");
    read_until(&mut ws, "got: in repo");
    let r = app.ok(
        "POST",
        "/v1/broadcast",
        json!({"text": "in home", "workspaceId": id(&h)}),
    );
    assert_eq!(r["results"], json!([]), "{r}");
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn broadcast_pastes_into_a_tmux_pane() {
    let t = T::new();
    let pane = t.start_agent("a", SID, "claude");
    let sock = std::env::temp_dir().join(format!("sb-test-ws-tmux-{}.sock", std::process::id()));
    let mut c = Command::new(bin());
    t.env(&mut c);
    c.env("OVERSEER_SOCKET", &sock)
        .args(["daemon", "--interval", "1"]);
    let _d = Reap::spawn(c.stderr(Stdio::null()));
    wait(|| UnixStream::connect(&sock).is_ok());
    let line = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    let lk = format!("line:{line}");
    let body = json!({"text": "hello pane", "lineIds": [line]}).to_string();
    wait_for(|| {
        let (code, out) = common::exchange(
            UnixStream::connect(&sock).unwrap(),
            "POST",
            "/v1/broadcast",
            &body,
            "unix",
        );
        assert_eq!(code, 200, "{out}");
        let v: Value = serde_json::from_str(&out).unwrap();
        (result(&v, &lk)["result"] == "sent").then_some(())
    });
    t.wait(|| t.screen(&pane).contains("got: hello pane"));
    let _ = std::fs::remove_file(&sock);
}

#[test]
fn discovery_adds_repos_and_missing_folders_hide_auto_workspaces() {
    let app = App::new(
        "ws-missing",
        json!({"autoConcurrency": 0, "discoverRoots": ["~/code"]}),
    );
    let code = app.fx.base.join("home/code");
    for r in ["alpha", "beta", "node_modules/dep"] {
        std::fs::create_dir_all(code.join(r)).unwrap();
        common::git(&code.join(r), &["init", "-q"]);
    }
    let canon = |r: &str| {
        code.join(r)
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let (alpha, beta, dep) = (canon("alpha"), canon("beta"), canon("node_modules/dep"));
    let v = app.ok("POST", "/v1/projects/discover", Value::Null);
    assert!(v["added"].is_u64(), "{v}");
    assert_eq!(
        find(&v["projects"], "path", &alpha).unwrap()["missing"],
        false
    );
    assert!(find(&v["projects"], "path", &beta).is_some(), "{v}");
    assert!(find(&v["projects"], "path", &dep).is_none(), "{v}");
    let again = app.ok("POST", "/v1/projects/discover", Value::Null);
    assert_eq!(again["added"], 0, "{again}");
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    let w = find(&list, "path", &beta).unwrap();
    assert_eq!(w["auto"], true);
    assert_eq!(w["missing"], false);

    let scratch = app.fx.base.join("home/scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let mine = app.ok("POST", "/v1/workspaces", json!({"path": scratch}));
    assert_eq!(mine["auto"], false);

    std::fs::rename(code.join("beta"), code.join("beta.away")).unwrap();
    std::fs::remove_dir_all(&scratch).unwrap();
    let projects = app.ok("GET", "/v1/projects", Value::Null);
    let gone = find(&projects, "path", &beta).unwrap();
    assert_eq!(gone["missing"], true, "{projects}");
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert!(find(&list, "path", &beta).is_none(), "{list}");
    let kept = find(&list, "id", &id(&mine)).unwrap();
    assert_eq!(kept["missing"], true);
    assert_eq!(find(&list, "path", &alpha).unwrap()["missing"], false);

    std::fs::rename(code.join("beta.away"), code.join("beta")).unwrap();
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert_eq!(find(&list, "path", &beta).unwrap()["id"], w["id"]);
}

#[test]
fn sessions_move_to_an_explicitly_assigned_workspace() {
    // Given an agent line and a shell term in the repo, and a second workspace elsewhere
    // When both are assigned to that workspace, then the workspace is deleted
    // Then broadcast membership follows the assignment and the deletion clears it
    let app = App::new("ws-assign", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let other = app.fx.base.join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let w = id(&app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": other.to_string_lossy()}),
    ));
    let term = id(&app.ok("POST", "/v1/terms", json!({"cwd": wt, "agent": "claude"})));
    let mut tws = app.ws(&term);
    read_until(&mut tws, "❯");
    let line = wait_for(|| line_of(&app, &term));
    let lk = format!("line:{line}");
    let sh = id(&app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]})));
    let sk = format!("term:{sh}");
    let repo = wait_for(|| find(&app.ok("GET", "/v1/workspaces", Value::Null), "path", &wt));
    let targets = |wid: &str| -> Vec<String> {
        app.ok(
            "POST",
            "/v1/broadcast",
            json!({"text": "noop", "workspaceId": wid}),
        )["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["target"].as_str().unwrap().to_string())
            .collect()
    };
    let local = || {
        app.ok("GET", "/v1/lines?all=true", Value::Null)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == line.as_str())
            .unwrap()["local"]
            .clone()
    };
    assert_eq!(targets(&w), Vec::<String>::new());

    let r = app.ok(
        "PUT",
        &format!("/v1/lines/{line}/workspace"),
        json!({"workspaceId": w}),
    );
    assert_eq!(r, json!({"lineId": line, "workspaceId": w}));
    let t = app.ok(
        "PUT",
        &format!("/v1/terms/{sh}/workspace"),
        json!({"workspaceId": w}),
    );
    assert_eq!(t["workspaceId"], w.as_str());
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{sh}"), Value::Null)["workspaceId"],
        w.as_str()
    );
    // The line list is refreshed by the rescan the write asks for, not inside the request.
    wait(|| local()["workspaceId"] == w.as_str());
    let mut got = targets(&w);
    got.sort();
    let mut want = vec![lk.clone(), sk.clone()];
    want.sort();
    assert_eq!(got, want);
    assert!(!targets(&id(&repo)).contains(&lk));

    for (path, body, code) in [
        (
            format!("/v1/lines/{line}/workspace"),
            json!({"workspaceId": "nope"}),
            404,
        ),
        (
            format!("/v1/lines/{line}/workspace"),
            json!({"workspaceId": 3}),
            400,
        ),
        (format!("/v1/lines/{line}/workspace"), json!({}), 400),
        (
            "/v1/lines/nope/workspace".into(),
            json!({"workspaceId": w}),
            404,
        ),
        (
            format!("/v1/terms/{sh}/workspace"),
            json!({"workspaceId": "nope"}),
            404,
        ),
        (
            "/v1/terms/nope/workspace".into(),
            json!({"workspaceId": w}),
            404,
        ),
    ] {
        assert_eq!(app.req("PUT", &path, body.clone()).0, code, "{path} {body}");
    }

    let t = app.ok(
        "PUT",
        &format!("/v1/terms/{sh}/workspace"),
        json!({"workspaceId": null}),
    );
    assert!(t.get("workspaceId").is_none(), "{t}");
    app.ok("DELETE", &format!("/v1/workspaces/{w}"), Value::Null);
    wait(|| local().get("workspaceId").is_none());
    assert!(targets(&id(&repo)).contains(&lk));
    for t in [&term, &sh] {
        app.ok(
            "POST",
            &format!("/v1/terms/{t}/kill"),
            json!({"confirm": true}),
        );
    }
}
