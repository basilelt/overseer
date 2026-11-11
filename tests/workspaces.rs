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

fn targets_of(app: &App, wid: &str) -> Vec<String> {
    let mut t: Vec<String> = app.ok(
        "POST",
        "/v1/broadcast",
        json!({"text": "noop", "workspaceId": wid}),
    )["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["target"].as_str().unwrap().to_string())
        .collect();
    t.sort();
    t
}

fn local_of(app: &App, line: &str) -> Value {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["id"] == line)
        .map(|l| l["local"].clone())
        .unwrap_or(Value::Null)
}

#[test]
fn subjects_are_created_edited_listed_and_deleted_without_a_directory() {
    // Given two directories, one reachable as ~/shared
    // When two subjects share a cwd, one is patched, bad bodies are sent, then a subject is deleted
    // Then list shows kind and cwd with a null path, bad input is a 400 and the other subject stays
    let app = App::new("ws-subject-crud", json!({"autoConcurrency": 0}));
    let shared = app.fx.base.join("home").join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let canon = shared
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let a = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Deal A", "cwd": "~/shared", "pinned": true}),
    );
    assert_eq!(a["kind"], "subject");
    assert_eq!(a["cwd"], canon.as_str());
    assert!(a["path"].is_null(), "{a}");
    assert_eq!(a["pinned"], true);
    let b = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Deal B", "cwd": canon}),
    );
    assert_ne!(a["id"], b["id"]);

    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert_eq!(
        find(&list, "name", "Deal B").unwrap()["cwd"],
        canon.as_str()
    );
    let count = list.as_array().unwrap().len();
    let again = app.ok("GET", "/v1/workspaces", Value::Null);
    assert_eq!(again.as_array().unwrap().len(), count);

    let other = app.fx.base.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let other_canon = other.canonicalize().unwrap().to_string_lossy().into_owned();
    let p = app.ok(
        "PATCH",
        &format!("/v1/workspaces/{}", id(&a)),
        json!({"name": "Deal A2", "cwd": other.to_string_lossy()}),
    );
    assert_eq!(p["name"], "Deal A2");
    assert_eq!(p["cwd"], other_canon.as_str());
    assert!(p["path"].is_null());

    let dir_ws = app.ok("POST", "/v1/workspaces", json!({"path": other_canon}));
    assert_eq!(dir_ws["kind"], "dir");
    assert!(dir_ws["cwd"].is_null());
    let ws = "/v1/workspaces".to_string();
    for (m, path, body) in [
        ("POST", ws.clone(), json!({"kind": "subject", "cwd": canon})),
        ("POST", ws.clone(), json!({"kind": "subject", "name": "x"})),
        (
            "POST",
            ws.clone(),
            json!({"kind": "subject", "name": "x", "cwd": "relative"}),
        ),
        (
            "POST",
            ws.clone(),
            json!({"kind": "subject", "name": "x", "cwd": "/nonexistent-dir"}),
        ),
        (
            "POST",
            ws.clone(),
            json!({"kind": "subject", "name": "x", "cwd": canon, "path": canon}),
        ),
        ("POST", ws.clone(), json!({"kind": "folder", "path": canon})),
        ("POST", ws.clone(), json!({"path": canon, "cwd": canon})),
        (
            "PATCH",
            format!("/v1/workspaces/{}", id(&a)),
            json!({"cwd": "/nonexistent-dir"}),
        ),
        (
            "PATCH",
            format!("/v1/workspaces/{}", id(&dir_ws)),
            json!({"cwd": canon}),
        ),
    ] {
        assert_eq!(app.req(m, &path, body.clone()).0, 400, "{m} {path} {body}");
    }

    app.ok("DELETE", &format!("/v1/workspaces/{}", id(&a)), Value::Null);
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert!(find(&list, "name", "Deal A2").is_none());
    assert!(find(&list, "name", "Deal B").is_some());
}

#[test]
fn a_session_in_a_dir_workspace_assigned_to_a_subject_shows_only_in_the_subject() {
    // Given a shell term whose cwd is inside a dir workspace and a subject
    // When the term is assigned to the subject, then the subject is deleted
    // Then it is a member of the subject only, and falls back to the dir workspace without being killed
    let app = App::new("ws-subject-member", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let subject = id(&app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Support", "cwd": wt}),
    ));
    let sh = id(&app.ok("POST", "/v1/terms", json!({"cwd": wt, "cmd": ["/bin/sh"]})));
    let sk = format!("term:{sh}");
    let repo = id(&app.ok("POST", "/v1/workspaces", json!({"path": wt})));
    assert_eq!(targets_of(&app, &repo), vec![sk.clone()]);
    assert!(
        targets_of(&app, &subject).is_empty(),
        "a subject has no cwd membership"
    );

    app.ok(
        "PUT",
        &format!("/v1/terms/{sh}/workspace"),
        json!({"workspaceId": subject}),
    );
    assert_eq!(targets_of(&app, &subject), vec![sk.clone()]);
    assert!(targets_of(&app, &repo).is_empty());

    app.ok("DELETE", &format!("/v1/workspaces/{subject}"), Value::Null);
    assert_eq!(targets_of(&app, &repo), vec![sk]);
    let t = app.ok("GET", &format!("/v1/terms/{sh}"), Value::Null);
    assert_eq!(t["exited"], false);
    assert!(t.get("workspaceId").is_none());
    app.ok(
        "POST",
        &format!("/v1/terms/{sh}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn a_term_started_for_a_subject_starts_in_its_cwd_and_its_line_inherits_the_assignment() {
    // Given a subject whose cwd is the repo
    // When a shell and an agent are started with its workspaceId and no cwd
    // Then both run in the subject cwd and are assigned, the agent line inherits it, and an unknown id is a 404
    let app = App::new("ws-subject-start", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let subject = id(&app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Infra", "cwd": wt}),
    ));
    let (code, _) = app.req(
        "POST",
        "/v1/terms",
        json!({"workspaceId": "nope", "cmd": ["/bin/sh"]}),
    );
    assert_eq!(code, 404);
    let (code, _) = app.req("POST", "/v1/terms", json!({"workspaceId": 3}));
    assert_eq!(code, 400);

    let sh = app.ok(
        "POST",
        "/v1/terms",
        json!({"workspaceId": subject, "cmd": ["/bin/sh"]}),
    );
    assert_eq!(sh["workspaceId"], subject.as_str());
    assert_eq!(
        sh["cwd"],
        app.fx.wt.canonicalize().unwrap().to_string_lossy().as_ref()
    );
    let term = id(&app.ok(
        "POST",
        "/v1/terms",
        json!({"workspaceId": subject, "agent": "claude"}),
    ));
    let mut tws = app.ws(&term);
    read_until(&mut tws, "❯");
    let line = wait_for(|| line_of(&app, &term));
    wait(|| local_of(&app, &line)["workspaceId"] == subject.as_str());
    let mut want = vec![format!("line:{line}"), format!("term:{}", id(&sh))];
    want.sort();
    assert_eq!(targets_of(&app, &subject), want);

    // Clearing the line's assignment sticks: the term does not re-assign it on the next scan.
    app.ok(
        "PUT",
        &format!("/v1/lines/{line}/workspace"),
        json!({"workspaceId": null}),
    );
    wait(|| local_of(&app, &line).get("workspaceId").is_none());
    app.ok("GET", "/v1/lines?all=true", Value::Null);
    assert!(local_of(&app, &line).get("workspaceId").is_none());
    for t in [id(&sh), term] {
        app.ok(
            "POST",
            &format!("/v1/terms/{t}/kill"),
            json!({"confirm": true}),
        );
    }
}

#[test]
fn a_term_started_for_a_dir_workspace_without_a_cwd_starts_in_its_path() {
    // Given a dir workspace on a directory other than HOME
    // When a shell is started with its workspaceId and no cwd
    // Then it runs in the workspace path and is assigned to it
    let app = App::new("ws-dir-start", json!({"autoConcurrency": 0}));
    let dir = app.fx.base.join("elsewhere");
    std::fs::create_dir_all(&dir).unwrap();
    let w = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": dir.to_string_lossy()}),
    );
    let t = app.ok(
        "POST",
        "/v1/terms",
        json!({"workspaceId": w["id"], "cmd": ["/bin/sh"]}),
    );
    assert_eq!(t["cwd"], w["path"]);
    assert_eq!(t["workspaceId"], w["id"]);
    app.ok(
        "POST",
        &format!("/v1/terms/{}/kill", id(&t)),
        json!({"confirm": true}),
    );
}

#[test]
fn patching_a_subject_cwd_to_null_is_refused_and_keeps_the_cwd() {
    // Given a subject with a cwd
    // When its cwd is patched to null
    // Then the daemon answers 400 and the cwd is unchanged
    let app = App::new("ws-subject-null-cwd", json!({"autoConcurrency": 0}));
    let dir = app.fx.base.join("home");
    let s = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "S", "cwd": dir.to_string_lossy()}),
    );
    let (code, _) = app.req(
        "PATCH",
        &format!("/v1/workspaces/{}", id(&s)),
        json!({"cwd": null}),
    );
    assert_eq!(code, 400);
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert_eq!(find(&list, "id", &id(&s)).unwrap()["cwd"], s["cwd"]);
}

#[test]
fn deleting_a_subject_unassigns_its_lines_and_keeps_them_running() {
    // Given an agent line assigned to a subject
    // When the subject is deleted
    // Then the line has no assignment any more and its term is still alive
    let app = App::new("ws-subject-del-line", json!({"autoConcurrency": 0}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let subject = id(&app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Gone", "cwd": wt}),
    ));
    let term = id(&app.ok(
        "POST",
        "/v1/terms",
        json!({"workspaceId": subject, "agent": "claude"}),
    ));
    let mut tws = app.ws(&term);
    read_until(&mut tws, "❯");
    let line = wait_for(|| line_of(&app, &term));
    wait(|| local_of(&app, &line)["workspaceId"] == subject.as_str());
    app.ok("DELETE", &format!("/v1/workspaces/{subject}"), Value::Null);
    wait(|| local_of(&app, &line).get("workspaceId").is_none());
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{term}"), Value::Null)["exited"],
        false
    );
    app.ok(
        "POST",
        &format!("/v1/terms/{term}/kill"),
        json!({"confirm": true}),
    );
}

#[test]
fn adding_a_project_announces_it_and_its_workspace() {
    let app = App::new("ws-project-event", json!({"autoConcurrency": 0}));
    let (mut sse, mut buf) = events(&app);
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let (code, p) = app.req("POST", "/v1/projects", json!({"path": wt}));
    assert_eq!(code, 201);
    expect(
        &mut sse,
        &mut buf,
        &["event: project.added", "event: workspace.created", &wt],
    );
    assert_eq!(p["path"], wt.as_str());
    let before = buf.len();
    assert_eq!(app.req("POST", "/v1/projects", json!({"path": wt})).0, 200);
    std::thread::sleep(Duration::from_millis(300));
    let mut b = [0u8; 4096];
    if let Ok(n) = sse.read(&mut b) {
        buf.push_str(&String::from_utf8_lossy(&b[..n]));
    }
    assert_eq!(buf[before..].matches("project.added").count(), 0);
}

fn canon_of(p: &std::path::Path) -> String {
    p.canonicalize().unwrap().to_string_lossy().into_owned()
}

fn meta_repo(root: &std::path::Path, subs: &[&str], checked_out: &[&str]) {
    std::fs::create_dir_all(root).unwrap();
    common::git(root, &["init", "-q"]);
    let mut text = String::new();
    for s in subs {
        text.push_str(&format!(
            "[submodule \"{s}\"]\n\tpath = {s}\n\turl = git@example.com:o/{s}.git\n\tignore = all\n"
        ));
    }
    std::fs::write(root.join(".gitmodules"), text).unwrap();
    for s in checked_out {
        std::fs::create_dir_all(root.join(s)).unwrap();
        std::fs::write(
            root.join(s).join(".git"),
            format!("gitdir: ../.git/modules/{s}\n"),
        )
        .unwrap();
    }
}

#[test]
fn checked_out_submodules_become_nested_workspaces_and_a_deleted_one_stays_gone() {
    // Given a repo whose .gitmodules lists two checked-out submodules (one two levels deep) and one not checked out
    // When the repo is added as a workspace, a submodule is deleted, folders are rescanned, then the parent is deleted
    // Then each checked-out submodule is nested under it, the deleted one does not come back, and the parent takes its children along
    let app = App::new("ws-submodules", json!({"autoConcurrency": 0}));
    let meta = app.fx.base.join("home/endurance");
    meta_repo(
        &meta,
        &["40_code/os", "src/backend/docs", "absent"],
        &["40_code/os", "src/backend/docs"],
    );
    let parent = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": meta.to_string_lossy()}),
    );
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    let os = find(&list, "path", &canon_of(&meta.join("40_code/os"))).expect("os nested");
    assert_eq!(os["parentId"], parent["id"]);
    assert_eq!(os["name"], "os");
    assert_eq!(os["auto"], true);
    assert_eq!(os["kind"], "dir");
    assert!(os["cwd"].is_null(), "{os}");
    let docs = find(&list, "path", &canon_of(&meta.join("src/backend/docs"))).expect("docs nested");
    assert_eq!(docs["parentId"], parent["id"]);
    assert_eq!(docs["name"], "docs");
    assert!(!list.to_string().contains("absent"), "{list}");
    assert!(parent["parentId"].is_null());

    app.ok(
        "DELETE",
        &format!("/v1/workspaces/{}", id(&os)),
        Value::Null,
    );
    app.ok("POST", "/v1/projects/discover", Value::Null);
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert!(find(&list, "id", &id(&os)).is_none(), "{list}");
    assert!(
        find(&list, "path", os["path"].as_str().unwrap()).is_none(),
        "{list}"
    );
    assert!(find(&list, "id", &id(&docs)).is_some());

    let gone = app.ok(
        "DELETE",
        &format!("/v1/workspaces/{}", id(&parent)),
        Value::Null,
    );
    assert_eq!(gone["children"], json!([id(&docs)]));
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    assert!(find(&list, "id", &id(&docs)).is_none(), "{list}");
}

#[test]
fn a_top_level_workspace_on_a_submodule_is_adopted_with_its_id_and_tabs() {
    // Given a top-level workspace already on a submodule checkout, with tabs
    // When its enclosing repo is added
    // Then the same workspace is nested under the repo, keeps its id and tabs, and loses its folder and pin
    let app = App::new("ws-submodule-adopt", json!({"autoConcurrency": 0}));
    let meta = app.fx.base.join("home/atk");
    meta_repo(&meta, &["src/webplatform"], &["src/webplatform"]);
    let f = app.ok("POST", "/v1/folders", json!({"name": "Clients"}));
    let sub = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": meta.join("src/webplatform").to_string_lossy(), "folderId": f["id"], "pinned": true}),
    );
    app.ok(
        "PATCH",
        &format!("/v1/workspaces/{}", id(&sub)),
        json!({"tabs": ["line:a"]}),
    );
    let parent = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": meta.to_string_lossy()}),
    );
    let list = app.ok("GET", "/v1/workspaces", Value::Null);
    let w = find(&list, "id", &id(&sub)).unwrap();
    assert_eq!(w["parentId"], parent["id"]);
    assert_eq!(w["tabs"], json!(["line:a"]));
    assert!(w["folderId"].is_null());
    assert_eq!(w["pinned"], false);
}

#[test]
fn nested_workspaces_validate_their_parent_and_start_in_their_cwd() {
    // Given a top-level workspace with a sub-folder
    // When a nested workspace starting at the parent root is created, a shell starts for it and another runs in the sub-folder, and bad bodies are sent
    // Then the first shell starts at the root, the second belongs to the nested workspace by path, and each bad body is a 400 with its message
    let app = App::new("ws-nested", json!({"autoConcurrency": 0}));
    let root = app.fx.base.join("home/backbone");
    let sub = root.join("20_processus/onboarding");
    std::fs::create_dir_all(sub.join("kyc")).unwrap();
    let parent = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": root.to_string_lossy()}),
    );
    let rootp = canon_of(&root);
    let n = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"path": canon_of(&sub), "name": "Onboarding PME", "parentId": parent["id"], "cwd": rootp}),
    );
    assert_eq!(n["parentId"], parent["id"]);
    assert_eq!(n["name"], "Onboarding PME");
    assert_eq!(n["cwd"], rootp.as_str());
    assert_eq!(n["auto"], false);

    let started = app.ok(
        "POST",
        "/v1/terms",
        json!({"workspaceId": n["id"], "cmd": ["/bin/sh"]}),
    );
    assert_eq!(started["cwd"], rootp.as_str());
    let inside = app.ok(
        "POST",
        "/v1/terms",
        json!({"cwd": canon_of(&sub.join("kyc")), "cmd": ["/bin/sh"]}),
    );
    let want = [id(&started), id(&inside)];
    let got: Vec<String> = targets_of(&app, &id(&n))
        .into_iter()
        .filter(|t| want.iter().any(|w| t.contains(w.as_str())))
        .collect();
    assert_eq!(got.len(), 2, "{got:?}");

    let subject = app.ok(
        "POST",
        "/v1/workspaces",
        json!({"kind": "subject", "name": "Archi V3", "parentId": parent["id"]}),
    );
    assert_eq!(subject["cwd"], rootp.as_str());
    assert_eq!(subject["parentId"], parent["id"]);

    let p = app.ok(
        "PATCH",
        &format!("/v1/workspaces/{}", id(&n)),
        json!({"cwd": null}),
    );
    assert!(p["cwd"].is_null(), "{p}");
    let other = app.fx.base.join("elsewhere");
    std::fs::create_dir_all(&other).unwrap();
    let f = app.ok("POST", "/v1/folders", json!({"name": "F"}));
    let ws = "/v1/workspaces".to_string();
    for (m, path, body, msg) in [
        (
            "POST",
            ws.clone(),
            json!({"path": canon_of(&other), "parentId": parent["id"]}),
            "is not inside",
        ),
        (
            "POST",
            ws.clone(),
            json!({"path": rootp, "parentId": parent["id"]}),
            "is not inside",
        ),
        (
            "POST",
            ws.clone(),
            json!({"path": canon_of(&sub.join("kyc")), "parentId": n["id"]}),
            "the parent must be a top-level folder workspace",
        ),
        (
            "POST",
            ws.clone(),
            json!({"path": canon_of(&sub.join("kyc")), "parentId": "nope"}),
            "no workspace nope",
        ),
        (
            "POST",
            ws.clone(),
            json!({"path": canon_of(&sub.join("kyc")), "parentId": parent["id"], "pinned": true}),
            "cannot be pinned",
        ),
        (
            "POST",
            ws.clone(),
            json!({"path": canon_of(&other), "cwd": rootp}),
            "cwd is only for subjects and nested workspaces",
        ),
        (
            "PATCH",
            format!("{ws}/{}", id(&n)),
            json!({"folderId": f["id"]}),
            "cannot be pinned",
        ),
        (
            "PATCH",
            format!("{ws}/{}", id(&n)),
            json!({"pinned": true}),
            "cannot be pinned",
        ),
        (
            "PATCH",
            format!("{ws}/{}", id(&parent)),
            json!({"cwd": rootp}),
            "cwd is only for subjects and nested workspaces",
        ),
    ] {
        let (code, v) = app.req(m, &path, body.clone());
        assert_eq!(code, 400, "{m} {path} {body} {v}");
        assert!(
            v["error"].as_str().unwrap().contains(msg),
            "{m} {body}: {v}"
        );
    }
    for t in [&started, &inside] {
        app.ok(
            "POST",
            &format!("/v1/terms/{}/kill", id(t)),
            json!({"confirm": true}),
        );
    }
}
