mod common;

use common::{exchange, git, wait, Fx};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn now_ms() -> i64 {
    overseer::procs::now_ms()
}

fn js(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

fn sse_client(sock: &Path, tx: mpsc::Sender<String>) {
    let mut s = UnixStream::connect(sock).unwrap();
    write!(s, "GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(s).lines() {
            let Ok(l) = l else { return };
            if let Some(ev) = l.strip_prefix("event: ") {
                if tx.send(ev.to_string()).is_err() {
                    return;
                }
            }
        }
    });
}

#[test]
fn board_end_to_end() {
    let fx = Fx::new("e2e", 3);
    fx.station("vm-b", 4, now_ms(), "idle");
    fx.station("mac-c", 2, now_ms() - 3_600_000, "held");
    let _d = fx.daemon(true);

    let (code, html) = fx.req("GET", "/board", "");
    assert_eq!(code, 200);
    assert!(html.contains("/board/board.js"));
    assert_eq!(fx.req("GET", "/board/board.js", "").0, 200);

    let (code, b) = fx.req("GET", "/v1/board", "");
    assert_eq!(code, 200, "{b}");
    let b = js(&b);
    assert_eq!(b["lines"].as_array().unwrap().len(), 9);
    assert_eq!(b["stations"].as_array().unwrap().len(), 3);
    assert_eq!(b["yardUrl"], "http://127.0.0.1:7070");
    let off = b["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["stationId"] == "mac-c")
        .unwrap();
    assert_eq!(
        (off["online"].as_bool(), off["actionable"].as_bool()),
        (Some(false), Some(false))
    );
    assert!(off["reason"].as_str().unwrap().contains("offline since"));
    let local = b["lines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["stationId"] == "vm-local")
        .unwrap();
    assert_eq!(local["actionable"], true);
    let id = local["line"]["id"].as_str().unwrap().to_string();
    assert_eq!(local["line"]["ticketKey"], "AS-42");

    let url = js(&std::fs::read_to_string(fx.state.join("board.json")).unwrap())["url"]
        .as_str()
        .unwrap()
        .to_string();
    let mode = std::os::unix::fs::PermissionsExt::mode(
        &std::fs::metadata(fx.state.join("board.json"))
            .unwrap()
            .permissions(),
    );
    assert_eq!(mode & 0o777, 0o600);
    let hostport = url
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let tok = url.split("token=").nth(1).unwrap().to_string();
    let tcp = |target: &str, host: &str| {
        exchange(
            std::net::TcpStream::connect(&hostport).unwrap(),
            "GET",
            target,
            "",
            host,
        )
        .0
    };
    assert_eq!(tcp("/board", &hostport), 403);
    assert_eq!(tcp("/v1/board?token=nope", &hostport), 403);
    assert_eq!(tcp(&format!("/board?token={tok}"), &hostport), 200);
    assert_eq!(tcp(&format!("/v1/board?token={tok}"), &hostport), 200);
    assert_eq!(tcp(&format!("/v1/board?token={tok}"), "evil.example"), 403);
    assert_eq!(tcp("/board/board.css", &hostport), 200);

    let out = fx.sb(&["board"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains(&url));

    std::fs::write(
        fx.base.join("gh/github_com_Acme_board_feature_AS_42_board.json"),
        r#"[{"number":7,"url":"https://github.com/Acme/board/pull/7","state":"OPEN","isDraft":true,"title":"t","statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"}]}]"#,
    )
    .unwrap();
    let pr = "/v1/pr?repo=github.com%2FAcme%2Fboard&branch=feature%2FAS-42-board";
    let (code, v) = fx.req("GET", pr, "");
    let v = js(&v);
    assert_eq!(
        (code, v["pr"]["number"].as_u64(), v["pr"]["ci"].as_str()),
        (200, Some(7), Some("failure"))
    );
    fx.req("GET", pr, "");
    assert_eq!(
        std::fs::read_to_string(fx.base.join("gh.log"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let (_, v) = fx.req(
        "GET",
        "/v1/pr?repo=github.com%2FAcme%2Fboard&branch=other",
        "",
    );
    assert_eq!(js(&v)["pr"], Value::Null);

    let (tx, rx) = mpsc::channel();
    for _ in 0..5 {
        sse_client(&fx.sock, tx.clone());
    }
    std::thread::sleep(Duration::from_millis(300));
    fx.station("vm-b", 4, now_ms(), "active");
    let start = Instant::now();
    let mut got = 0;
    while got < 5 * 4 {
        let ev = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("state change events");
        if ev == "line.state_changed" {
            got += 1;
        }
    }
    eprintln!("sse: 5 clients x 4 state changes in {:?}", start.elapsed());
    fx.station("vm-b", 4, now_ms() - 3_600_000, "active");
    wait(|| rx.try_iter().any(|e| e == "station.offline"));

    std::fs::write(fx.wt.join("a.txt"), "two\n").unwrap();
    let (code, v) = fx.req("GET", &format!("/v1/lines/{id}/diff"), "");
    let v = js(&v);
    assert_eq!(code, 200, "{v}");
    assert!(v["diff"].as_str().unwrap().contains("+two"));
    assert!(v["status"].as_str().unwrap().contains("a.txt"));
    assert_eq!(fx.req("GET", "/v1/lines/vm-b-01/diff", "").0, 404);
    assert_eq!(
        fx.req(
            "POST",
            &format!("/v1/lines/{id}/commit"),
            r#"{"message":" "}"#
        )
        .0,
        400
    );
    let (code, v) = fx.req(
        "POST",
        &format!("/v1/lines/{id}/commit"),
        r#"{"message":"wip"}"#,
    );
    let v = js(&v);
    assert_eq!((code, v["dryRun"].as_bool()), (200, Some(true)), "{v}");
    assert_eq!(git(&fx.wt, &["rev-list", "--count", "HEAD"]).trim(), "1");
    let (code, v) = fx.req(
        "POST",
        &format!("/v1/lines/{id}/commit?dryRun=false"),
        r#"{"message":"wip"}"#,
    );
    let v = js(&v);
    assert_eq!(code, 200, "{v}");
    assert_eq!(
        v["commit"].as_str().unwrap(),
        git(&fx.wt, &["rev-parse", "HEAD"]).trim()
    );
    assert_eq!(fx.req("POST", &format!("/v1/lines/{id}/stash"), "").0, 409);
    std::fs::write(fx.wt.join("b.txt"), "new\n").unwrap();
    let (code, v) = fx.req("POST", &format!("/v1/lines/{id}/stash?dryRun=false"), "");
    assert_eq!(code, 200, "{v}");
    assert!(js(&v)["stash"].as_str().unwrap().contains("overseer"));
    assert!(git(&fx.wt, &["status", "--porcelain"]).is_empty());
    let log = std::fs::read_to_string(fx.state.join("actions.log")).unwrap();
    assert!(log.contains("\"commit\"") && log.contains("\"stash\""));

    let (code, v) = fx.req(
        "POST",
        &format!("/v1/lines/{id}/link"),
        r#"{"ticketKey":"AS-77"}"#,
    );
    assert_eq!(code, 200, "{v}");
    let (code, _) = fx.req("PUT", &format!("/v1/lines/{id}/pin"), r#"{"pinned":true}"#);
    assert_eq!(code, 200);
    let (code, v) = fx.req("POST", &format!("/v1/lines/{id}/hold"), "");
    assert_eq!(code, 409, "{v}");
    let (code, v) = fx.req("POST", &format!("/v1/lines/{id}/close"), "");
    assert_eq!(
        (code, js(&v)["action"].as_str()),
        (200, Some("would-close")),
        "{v}"
    );
    let (code, v) = fx.req("POST", &format!("/v1/lines/{id}/resume"), "");
    assert_eq!(code, 200, "{v}");
    assert_eq!(js(&v)["cwd"].as_str().unwrap(), fx.wt.to_str().unwrap());
    wait(|| {
        js(&fx.req("GET", "/v1/board", "").1)["lines"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["line"]["ticketKey"] == "AS-77" && l["local"]["pinned"] == true)
    });
    let _ = std::fs::remove_file(&fx.sock);
}

#[test]
fn pinning_a_line_announces_the_pin_in_the_line_update() {
    // Given a daemon with one local session line and a client listening to events
    // When the line is pinned
    // Then the line list shows the pin at once and the announced update carries it
    let fx = Fx::new("pinevent", 1);
    let _d = fx.daemon(false);
    let mut s = UnixStream::connect(&fx.sock).unwrap();
    write!(s, "GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(s).lines() {
            let Ok(l) = l else { return };
            if let Some(d) = l.strip_prefix("data: ") {
                if tx.send(js(d)).is_err() {
                    return;
                }
            }
        }
    });
    let id = js(&fx.req("GET", "/v1/lines", "").1)[0]["line"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, v) = fx.req("PUT", &format!("/v1/lines/{id}/pin"), r#"{"pinned":true}"#);
    assert_eq!(code, 200, "{v}");
    assert_eq!(
        js(&fx.req("GET", "/v1/lines", "").1)[0]["local"]["pinned"],
        true
    );
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let ev = rx
            .recv_timeout(Duration::from_millis(200))
            .unwrap_or(Value::Null);
        if ev["type"] == "line.updated" && ev["data"]["local"]["pinned"] == true {
            break;
        }
        assert!(Instant::now() < end, "no line.updated carrying the pin");
    }
}

#[test]
fn line_updates_carry_the_same_local_as_the_line_list() {
    // Given a daemon with one held local session line and a client listening to events
    // When the line is pinned, then unpinned
    // Then each announced update carries the local the line list serves, held fields included
    let fx = Fx::new("evlocal", 1);
    let _d = fx.daemon(false);
    let mut s = UnixStream::connect(&fx.sock).unwrap();
    write!(s, "GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n").unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for l in BufReader::new(s).lines() {
            let Ok(l) = l else { return };
            if let Some(d) = l.strip_prefix("data: ") {
                if tx.send(js(d)).is_err() {
                    return;
                }
            }
        }
    });
    let id = js(&fx.req("GET", "/v1/lines", "").1)[0]["line"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    for pinned in [true, false] {
        fx.req(
            "PUT",
            &format!("/v1/lines/{id}/pin"),
            &format!(r#"{{"pinned":{pinned}}}"#),
        );
        let end = Instant::now() + Duration::from_secs(10);
        let ev = loop {
            let ev = rx
                .recv_timeout(Duration::from_millis(200))
                .unwrap_or(Value::Null);
            if ev["data"]["line"]["id"] == id.as_str() && ev["data"]["local"]["pinned"] == pinned {
                break ev["data"]["local"].clone();
            }
            assert!(Instant::now() < end, "no line event with pinned={pinned}");
        };
        let served = js(&fx.req("GET", "/v1/lines", "").1)[0]["local"].clone();
        assert_eq!(served["heldReason"], "lost", "{served}");
        for k in [
            "heldReason",
            "heldAt",
            "holdPending",
            "subagents",
            "tasks",
            "resumeCommand",
            "cwd",
        ] {
            assert_eq!(ev[k], served[k], "{k}: {ev} vs {served}");
        }
    }
}

#[test]
fn line_title_override_is_set_shared_and_cleared() {
    // Given a daemon with one local session line and a client listening to events
    // When a client renames the line, then clears the custom title
    // Then the trimmed title is served and announced, the auto title is kept, and clearing removes it
    let fx = Fx::new("title", 1);
    let _d = fx.daemon(false);
    let (tx, rx) = mpsc::channel();
    sse_client(&fx.sock, tx);
    let lines = js(&fx.req("GET", "/v1/lines", "").1);
    let id = lines[0]["line"]["id"].as_str().unwrap().to_string();
    let auto = lines[0]["local"]["title"].clone();
    let path = format!("/v1/lines/{id}/title");
    assert_eq!(fx.req("PUT", &path, "{}").0, 400);
    assert_eq!(
        fx.req("PUT", "/v1/lines/nope/title", r#"{"title":"x"}"#).0,
        404
    );
    let (code, v) = fx.req("PUT", &path, r#"{"title":"  Payments refactor  "}"#);
    assert_eq!(code, 200, "{v}");
    assert_eq!(js(&v)["titleOverride"], "Payments refactor");
    let end = Instant::now() + Duration::from_secs(10);
    while rx.recv_timeout(Duration::from_millis(200)).ok().as_deref() != Some("line.updated") {
        assert!(Instant::now() < end, "no line.updated after rename");
    }
    let local = |id: &str| {
        js(&fx.req("GET", "/v1/lines", "").1)
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["id"] == id)
            .unwrap()["local"]
            .clone()
    };
    assert_eq!(local(&id)["titleOverride"], "Payments refactor");
    assert_eq!(local(&id)["title"], auto);
    let (code, v) = fx.req("PUT", &path, r#"{"title":null}"#);
    assert_eq!((code, js(&v)["titleOverride"].clone()), (200, Value::Null));
    wait(|| local(&id)["titleOverride"] == Value::Null);
    fx.req("PUT", &path, r#"{"title":"x"}"#);
    wait(|| local(&id)["titleOverride"] == "x");
    let (code, _) = fx.req("PUT", &path, r#"{"title":"   "}"#);
    assert_eq!(code, 200);
    wait(|| local(&id)["titleOverride"] == Value::Null);
}

#[test]
fn http_refuses_non_loopback() {
    let fx = Fx::new("nolo", 0);
    let mut c = Command::new(env!("CARGO_BIN_EXE_overseer"));
    fx.env(&mut c);
    let out = common::output(c.args(["daemon", "--once", "--http", "0.0.0.0:0"]), None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("loopback"));
    let _ = std::fs::remove_file(&fx.sock);
}

fn browser() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    [
        "chromium",
        "chromium-browser",
        "google-chrome",
        "google-chrome-stable",
        "firefox",
    ]
    .iter()
    .flat_map(|b| std::env::split_paths(&path).map(move |d| d.join(b)))
    .find(|p| p.is_file())
}

#[test]
fn headless_browser_renders_board() {
    if std::env::var_os("OVERSEER_HEADLESS_TEST").is_none() {
        eprintln!("skipped: set OVERSEER_HEADLESS_TEST=1 to run the headless browser check");
        return;
    }
    let Some(b) = browser() else {
        eprintln!("skipped: no chromium/chrome/firefox on PATH for the headless check");
        return;
    };
    let fx = Fx::new("headless", 2);
    fx.station("mac-c", 3, now_ms() - 3_600_000, "held");
    let _d = fx.daemon(true);
    let url: Value =
        serde_json::from_str(&std::fs::read_to_string(fx.state.join("board.json")).unwrap())
            .unwrap();
    let profile = fx.base.join("browser-profile");
    std::fs::create_dir_all(&profile).unwrap();
    let firefox = b.ends_with("firefox");
    let mut c = Command::new(&b);
    c.env("HOME", &profile);
    if firefox {
        c.args(["--headless", "--no-remote", "--profile"])
            .arg(&profile)
            .arg("--screenshot")
            .arg(fx.base.join("shot.png"));
    } else {
        c.args([
            "--headless",
            "--disable-gpu",
            "--no-sandbox",
            "--virtual-time-budget=5000",
            "--dump-dom",
        ])
        .arg(format!("--user-data-dir={}", profile.display()));
    }
    let out = common::output(c.arg(url["url"].as_str().unwrap()), None);
    if firefox {
        assert!(fx.base.join("shot.png").exists());
    } else {
        let dom = String::from_utf8_lossy(&out.stdout);
        assert!(dom.contains("AS-42"), "{dom:.2000}");
        assert!(dom.contains("mac-c"));
    }
    let _ = std::fs::remove_file(&fx.sock);
}
