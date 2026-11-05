#![cfg(unix)]
mod common;

use common::{bin, read_until, send, wait, wait_for, App};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

fn version(sock: &Path) -> Option<Value> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    s.write_all(b"GET /v1/version HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    serde_json::from_str(out.split_once("\r\n\r\n")?.1).ok()
}

#[test]
fn hot_upgrade_keeps_terms_alive() {
    let app = App::new("upgrade", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let v0 = app.ok("GET", "/v1/version", Value::Null);
    assert_eq!(v0["epoch"], 0);
    assert!(v0.get("upgradedFrom").is_none());
    let pid = v0["pid"].as_u64().unwrap();

    let term = |cmd: &str| {
        let t = app.ok(
            "POST",
            "/v1/terms",
            json!({"cwd": wt, "cmd": [cmd], "cols": 80, "rows": 24}),
        );
        (
            t["id"].as_str().unwrap().to_string(),
            t["pid"].as_u64().unwrap(),
        )
    };
    let (cat, cat_pid) = term("/bin/cat");
    let (sh, sh_pid) = term("/bin/sh");
    let mut a = app.ws(&cat);
    send(&mut a, "before-cat\n");
    read_until(&mut a, "before-cat");
    let mut b = app.ws(&sh);
    send(&mut b, "echo before-$((6*7))\n");
    read_until(&mut b, "before-42");

    let (code, r) = app.req(
        "POST",
        "/v1/upgrade",
        json!({"path": "/nonexistent/overseer"}),
    );
    assert_eq!(code, 400, "{r}");
    let (code, r) = app.req("POST", "/v1/upgrade", json!({"path": bin()}));
    assert_eq!(code, 409, "same version without force: {r}");

    let (code, r) = app.req("POST", "/v1/upgrade", json!({"path": bin(), "force": true}));
    assert_eq!(code, 202, "{r}");
    assert_eq!(r["epoch"], 1);
    assert_eq!(r["from"], r["to"]);

    let v1 = wait_for(|| version(&app.fx.sock).filter(|v| v["epoch"] == 1));
    assert_eq!(v1["pid"].as_u64(), Some(pid));
    assert_eq!(v1["upgradedFrom"], v0["version"]);
    assert!(app
        .fx
        .state
        .read_dir()
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("takeover-")));

    let terms = app.ok("GET", "/v1/terms", Value::Null);
    let find = |id: &str| {
        terms
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        (find(&cat)["pid"].as_u64(), find(&cat)["exited"].clone()),
        (Some(cat_pid), json!(false))
    );
    assert_eq!(
        (find(&sh)["pid"].as_u64(), find(&sh)["exited"].clone()),
        (Some(sh_pid), json!(false))
    );

    let mut c = app.ws(&sh);
    read_until(&mut c, "before-42");
    send(&mut c, "echo after-$((1+1))\n");
    read_until(&mut c, "after-2");
    app.ok(
        "POST",
        &format!("/v1/terms/{sh}/resize"),
        json!({"cols": 100, "rows": 30}),
    );
    send(&mut c, "stty size\n");
    read_until(&mut c, "30 100");

    let mut d = app.ws(&cat);
    read_until(&mut d, "before-cat");
    send(&mut d, "after-cat\n");
    read_until(&mut d, "after-cat");

    app.ok(
        "POST",
        &format!("/v1/terms/{cat}/kill"),
        json!({"confirm": true}),
    );
    wait(|| app.ok("GET", &format!("/v1/terms/{cat}"), Value::Null)["exited"] == true);
    assert_eq!(
        app.ok("GET", &format!("/v1/terms/{sh}"), Value::Null)["exited"],
        false
    );
}

fn probe(sock: &Path, target: &str) -> Option<u16> {
    let mut s = UnixStream::connect(sock).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
    write!(
        s,
        "GET {target} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    out.split_whitespace().nth(1)?.parse().ok()
}

fn counter(text: &str) -> Vec<u64> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|w| w.strip_prefix('n')?.parse().ok())
        .collect()
}

#[test]
fn takeover_under_load_loses_nothing() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    let app = App::new("upgrade-load", json!({}));
    let wt = app.fx.wt.to_string_lossy().into_owned();
    let pid = app.ok("GET", "/v1/version", Value::Null)["pid"].as_u64();
    let script = "i=0; while :; do i=$((i+1)); echo n$i; sleep 0.003; done";
    let terms: Vec<(String, u64)> = (0..3)
        .map(|_| {
            let t = app.ok(
                "POST",
                "/v1/terms",
                json!({"cwd": wt, "cmd": ["/bin/sh", "-c", script], "cols": 80, "rows": 24}),
            );
            (
                t["id"].as_str().unwrap().to_string(),
                t["pid"].as_u64().unwrap(),
            )
        })
        .collect();
    let mut seen_before = vec![];
    for (id, _) in &terms {
        let mut ws = app.ws(id);
        let got = read_until(&mut ws, "n40\r\n");
        seen_before.push(counter(&got).into_iter().max().unwrap());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let ok_after = Arc::new(AtomicUsize::new(0));
    let upgraded = Arc::new(AtomicBool::new(false));
    let load: Vec<_> = ["/v1/lines", "/v1/projects", "/v1/terms", "/v1/workspaces"]
        .into_iter()
        .map(|target| {
            let (sock, stop, ok_after, upgraded) = (
                app.fx.sock.clone(),
                stop.clone(),
                ok_after.clone(),
                upgraded.clone(),
            );
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    let done = upgraded.load(Ordering::SeqCst);
                    if probe(&sock, target) == Some(200) && done {
                        ok_after.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        })
        .collect();

    let mut stalled = UnixStream::connect(&app.fx.sock).unwrap();
    stalled
        .write_all(b"GET /v1/lines HTTP/1.1\r\nHost: x\r\n")
        .unwrap();
    stalled
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();

    for epoch in 1..=2 {
        let (code, r) = app.req("POST", "/v1/upgrade", json!({"path": bin(), "force": true}));
        assert_eq!(code, 202, "{r}");
        let v = wait_for(|| version(&app.fx.sock).filter(|v| v["epoch"] == epoch));
        assert_eq!(v["pid"].as_u64(), pid);
    }
    upgraded.store(true, Ordering::SeqCst);

    let mut rest = vec![];
    let begin = std::time::Instant::now();
    let _ = stalled.read_to_end(&mut rest);
    assert!(
        begin.elapsed() < Duration::from_secs(15),
        "in-flight request hung"
    );

    for target in ["/v1/lines", "/v1/terms", "/v1/projects", "/v1/workspaces"] {
        let begin = std::time::Instant::now();
        assert_eq!(probe(&app.fx.sock, target), Some(200), "{target}");
        assert!(
            begin.elapsed() < Duration::from_secs(5),
            "{target} is stuck"
        );
    }

    let list = app.ok("GET", "/v1/terms", Value::Null);
    for ((id, tpid), before) in terms.iter().zip(&seen_before) {
        let t = list
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == id.as_str())
            .cloned()
            .unwrap();
        assert_eq!(
            (t["pid"].as_u64(), t["exited"].clone()),
            (Some(*tpid), json!(false))
        );
        let mut ws = app.ws(id);
        let want = format!("n{}\r\n", before + 400);
        let got = counter(&read_until(&mut ws, &want));
        assert!(
            got.windows(2).all(|w| w[1] == w[0] + 1),
            "term {id} output has gaps or repeats inside the replay: {got:?}"
        );
        send(&mut ws, "\u{3}");
    }

    wait(|| ok_after.load(Ordering::SeqCst) >= 8);
    stop.store(true, Ordering::SeqCst);
    for h in load {
        h.join().unwrap();
    }
    assert!(app
        .fx
        .state
        .read_dir()
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().starts_with("takeover-")));
    assert_eq!(
        app.ok("GET", "/v1/version", Value::Null)["pid"].as_u64(),
        pid
    );
}
