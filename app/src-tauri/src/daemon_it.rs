use crate::http::json_body;
use crate::transport::{DaemonConn, PROTOCOL};
use crate::ws::{self, TermInput};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc;

struct Daemon {
    root: PathBuf,
    bin: PathBuf,
}

impl Daemon {
    fn env(&self, cmd: &mut Command) {
        let home = self.root.join("home");
        cmd.env("HOME", &home)
            .env("OVERSEER_STATE_DIR", self.root.join("state"))
            .env("OVERSEER_SOCKET", self.root.join("d.sock"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("TMUX_TMPDIR", self.root.join("tmux"))
            .env("SHELL", "/bin/sh")
            .env("OVERSEER_LOGIN_ENV", "0")
            .env_remove("TMUX");
    }

    fn run(&self, args: &[&str]) -> String {
        let mut cmd = Command::new(&self.bin);
        cmd.args(args);
        self.env(&mut cmd);
        let out = cmd.output().expect("run overseer");
        assert!(
            out.status.success(),
            "overseer {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Ok(pid) = std::fs::read_to_string(self.root.join("state/daemon.pid")) {
            let _ = Command::new("kill").arg(pid.trim()).status();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn daemon_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OVERSEER_BIN") {
        return Some(PathBuf::from(p));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["target/debug/overseer", "target/release/overseer"]
        .iter()
        .map(|p| repo.join(p))
        .find(|p| p.is_file())
}

async fn call(conn: &DaemonConn, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let raw = body.map(|b| b.to_string().into_bytes());
    let res = conn
        .request(method, path, raw.as_deref())
        .await
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
    (res.head.status, json_body(&res.body))
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=it",
            "-c",
            "user.email=it@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

#[tokio::test]
async fn app_talks_to_a_real_daemon() {
    let _env = crate::TEST_ENV.lock().await;
    let Some(bin) = daemon_binary() else {
        eprintln!("skipped: build the daemon (cargo build at the repo root) or set OVERSEER_BIN");
        return;
    };
    let root = std::env::temp_dir().join(format!("sbit-{}", std::process::id()));
    let repo = root.join("home/src/demo");
    std::fs::create_dir_all(&repo).unwrap();
    let d = Daemon { root, bin };

    let socket = d.run(&["daemon", "--detach"]);
    assert_eq!(
        PathBuf::from(socket.lines().last().unwrap()),
        d.root.join("d.sock")
    );
    let conn = DaemonConn::Local(d.root.join("d.sock"));

    let (status, version) = call(&conn, "GET", "/v1/version", None).await;
    assert_eq!(status, 200);
    assert_eq!(version["protocol"].as_u64(), Some(PROTOCOL));
    assert_eq!(
        conn.version().await.unwrap(),
        version["version"].as_str().unwrap()
    );

    std::fs::write(repo.join("README.md"), "demo\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    let project = repo.to_str().unwrap();
    let (status, task) = call(
        &conn,
        "POST",
        "/v1/tasks",
        Some(json!({"title": "Write docs", "project": project})),
    )
    .await;
    assert_eq!(status, 201, "{task}");
    assert_eq!(
        (task["number"].as_u64(), task["status"].as_str()),
        (Some(1), Some("pending"))
    );
    let (_, projects) = call(&conn, "GET", "/v1/projects", None).await;
    assert!(
        projects
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["path"] == project),
        "{projects}"
    );
    let id = task["id"].as_str().unwrap();
    let (status, patched) = call(
        &conn,
        "PATCH",
        &format!("/v1/tasks/{id}"),
        Some(json!({"status": "in_testing"})),
    )
    .await;
    assert_eq!(
        (status, patched["status"].as_str()),
        (200, Some("in_testing"))
    );
    let (status, _) = call(
        &conn,
        "PATCH",
        &format!("/v1/tasks/{id}"),
        Some(json!({"status": "in_progress"})),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(
        call(&conn, "DELETE", &format!("/v1/tasks/{id}"), None)
            .await
            .0,
        200
    );

    let cmd = json!(["sh", "-c", "echo sb-ready; read x; echo got:$x"]);
    let (status, term) = call(
        &conn,
        "POST",
        "/v1/terms",
        Some(json!({"cwd": project, "cmd": cmd, "cols": 80, "rows": 24})),
    )
    .await;
    assert_eq!(status, 201, "{term}");
    let term_id = term["id"].as_str().unwrap().to_string();

    let io = conn.open().await.unwrap();
    let socket = ws::connect(io, &format!("/v1/terms/{term_id}/io"))
        .await
        .unwrap();
    let (tx, rx) = mpsc::channel(8);
    let (out_tx, mut out_rx) = mpsc::unbounded_channel();
    let pump = tokio::spawn(ws::pump(socket, rx, move |b| {
        let _ = out_tx.send(b);
    }));
    let mut seen = String::new();
    let mut wait_for = async |needle: &str| {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !seen.contains(needle) {
                seen.push_str(&String::from_utf8_lossy(
                    &out_rx.recv().await.expect("terminal closed early"),
                ));
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {needle:?} in {seen:?}"));
    };
    wait_for("sb-ready").await;
    tx.send(TermInput::Resize(100, 30)).await.unwrap();
    tx.send(TermInput::Data("hello\r".into())).await.unwrap();
    wait_for("got:hello").await;
    tokio::time::timeout(Duration::from_secs(10), pump)
        .await
        .expect("socket closes after exit")
        .unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let after = loop {
        let (_, t) = call(&conn, "GET", &format!("/v1/terms/{term_id}"), None).await;
        if t["exited"].as_bool() == Some(true) || tokio::time::Instant::now() >= deadline {
            break t;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(
        (after["exited"].as_bool(), after["cols"].as_u64()),
        (Some(true), Some(100)),
        "{after}"
    );
}

#[tokio::test]
async fn one_window_reaches_two_daemons_through_its_slots() {
    let _env = crate::TEST_ENV.lock().await;
    let Some(bin) = daemon_binary() else {
        eprintln!("skipped: build the daemon (cargo build at the repo root) or set OVERSEER_BIN");
        return;
    };
    let start = |name: &str| {
        let root = std::env::temp_dir().join(format!("sbit-{name}-{}", std::process::id()));
        std::fs::create_dir_all(root.join("home")).unwrap();
        let d = Daemon {
            root,
            bin: bin.clone(),
        };
        d.run(&["daemon", "--detach"]);
        d
    };
    let (a, b) = (start("slot-a"), start("slot-b"));
    let state = crate::commands::AppState::default();
    let main = state.window(&crate::commands::conn_key("main", None));
    let extra = state.window(&crate::commands::conn_key("main", Some("h1")));
    main.conn.use_conn(DaemonConn::Local(a.root.join("d.sock")));
    extra
        .conn
        .use_conn(DaemonConn::Local(b.root.join("d.sock")));

    let wait = Duration::from_secs(5);
    let cwd = a.root.join("home");
    let body = json!({"cwd": cwd, "cmd": ["sh", "-c", "sleep 30"], "cols": 80, "rows": 24})
        .to_string()
        .into_bytes();
    let made = main
        .conn
        .request("POST", "/v1/terms", Some(&body), wait)
        .await
        .unwrap();
    assert_eq!(
        made.head.status,
        201,
        "{}",
        String::from_utf8_lossy(&made.body)
    );
    let id = json_body(&made.body)["id"].as_str().unwrap().to_string();
    let ids = |raw: &[u8]| -> Vec<String> {
        json_body(raw)
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap_or("").to_string())
            .collect()
    };
    let on_a = main
        .conn
        .request("GET", "/v1/terms", None, wait)
        .await
        .unwrap();
    let on_b = extra
        .conn
        .request("GET", "/v1/terms", None, wait)
        .await
        .unwrap();
    assert!(ids(&on_a.body).contains(&id));
    assert!(!ids(&on_b.body).contains(&id));

    state.drop_slot("main", "h1");
    assert!(extra.conn.current().is_none());
    assert!(main.conn.current().is_some());
    let again = state.window(&crate::commands::conn_key("main", Some("h1")));
    again
        .conn
        .use_conn(DaemonConn::Local(b.root.join("d.sock")));
    state.close("main");
    assert!(main.conn.current().is_none() && again.conn.current().is_none());
}
