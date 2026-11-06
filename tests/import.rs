mod common;

use common::{bin, wait, Fx, Reap};
use overseer::registry::Registry;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

fn old_state(fx: &Fx) -> std::path::PathBuf {
    let old = fx.base.join("old-state");
    std::fs::create_dir_all(&old).unwrap();
    let r = Registry::open(&old).unwrap();
    r.conn
        .execute_batch(
            "INSERT INTO lines (id, agent, session_id, owner, station_id, repo, branch, worktree_path, cwd, state, last_activity_at, first_seen_at, updated_at, held_by_user)
               VALUES ('l1', 'claude', 'sess-old', 'o', 's', 'r', 'b', '/w', '/w', 'idle', 't', 't', 't', 31337);
             INSERT INTO tasks (id, number, title, description, project, status, agent, model, effort, created_ms, updated_ms)
               VALUES ('t1', 1, 'old task', '', '/p', 'backlog', 'claude', '', '', 1, 1);
             INSERT INTO workspaces (id, name, path, rank, created_ms, updated_ms)
               VALUES ('w1', 'old ws', '/old/ws', 1.0, 1, 1);
             INSERT INTO task_numbers VALUES ('/p', 1);
             PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .unwrap();
    drop(r);
    std::fs::rename(old.join("overseer.db"), old.join("switchboard.db")).unwrap();
    for ext in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(old.join(format!("overseer.db{ext}")));
    }
    std::fs::write(
        old.join("config.json"),
        json!({"stationId": "old", "remoteCommand": "switchboard", "idleAfterSeconds": 9})
            .to_string(),
    )
    .unwrap();
    std::fs::write(old.join("profiles.json"), "{}").unwrap();
    old
}

fn daemon(fx: &Fx, old: &Path) -> Reap {
    let mut c = Command::new(bin());
    fx.env(&mut c);
    c.env("SWITCHBOARD_STATE_DIR", old)
        .args(["daemon", "--interval", "1"]);
    let r = Reap::spawn(c.stderr(Stdio::null()));
    wait(|| UnixStream::connect(&fx.sock).is_ok());
    r
}

fn get(fx: &Fx, path: &str) -> Value {
    let (code, body) = fx.req("GET", path, "");
    assert_eq!(code, 200, "{path}: {body}");
    serde_json::from_str(&body).unwrap()
}

fn localstorage(fx: &Fx) {
    let home = fx.base.join("home");
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/WebKit/fr.armtek.switchboard/WebKit/ls")
    } else {
        home.join(".local/share/fr.armtek.switchboard/WebKit/ls")
    };
    std::fs::create_dir_all(&dir).unwrap();
    let c = Connection::open(dir.join("localstorage.sqlite3")).unwrap();
    c.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB NOT NULL)")
        .unwrap();
    let v: Vec<u8> = "dark"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    c.execute(
        "INSERT INTO ItemTable VALUES ('switchboard.theme', ?1)",
        [v],
    )
    .unwrap();
}

#[test]
fn import_brings_everything_over_and_marks_it_done() {
    let fx = Fx::new("import-full", 0);
    let old = old_state(&fx);
    localstorage(&fx);
    let claude = fx.base.join("home/.claude/settings.json");
    std::fs::write(
        &claude,
        json!({"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": "echo mine"},
            {"type": "command", "command": "sb hook claude Stop #switchboard-hook"}
        ]}]}})
        .to_string(),
    )
    .unwrap();
    let _d = daemon(&fx, &old);

    let s = get(&fx, "/v1/import/switchboard");
    assert_eq!(s["found"], true);
    assert_eq!(s["dir"], old.to_string_lossy().as_ref());
    assert_eq!(
        (
            s["sessions"].clone(),
            s["tasks"].clone(),
            s["workspaces"].clone()
        ),
        (json!(1), json!(1), json!(1))
    );
    assert_eq!(
        (
            s["imported"].clone(),
            s["running"].clone(),
            s["hooks"].clone()
        ),
        (json!(false), json!(false), json!(true))
    );
    assert_eq!(s["settings"], json!({"overseer.theme": "dark"}));

    let (code, body) = fx.req("POST", "/v1/import/switchboard", "{}");
    assert_eq!(code, 200, "{body}");
    let r: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        r,
        json!({"imported": true, "sessions": 1, "tasks": 1, "workspaces": 1, "stopped": false, "hooks": true})
    );

    let tasks = get(&fx, "/v1/tasks");
    assert!(
        tasks
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["title"] == "old task"),
        "{tasks}"
    );
    let ws = get(&fx, "/v1/workspaces");
    assert!(ws.to_string().contains("old ws"), "{ws}");
    let db = Connection::open(fx.state.join("overseer.db")).unwrap();
    let held: i64 = db
        .query_row("SELECT held_by_user FROM lines WHERE id = 'l1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(held, 0);

    let cfg: Value =
        serde_json::from_slice(&std::fs::read(fx.state.join("config.json")).unwrap()).unwrap();
    assert_eq!(cfg["idleAfterSeconds"], 9);
    assert!(cfg.get("remoteCommand").is_none());
    assert!(fx.state.join("profiles.json").exists());

    let text = std::fs::read_to_string(&claude).unwrap();
    assert!(
        !text.contains("#switchboard-hook")
            && text.contains("echo mine")
            && text.contains("#overseer-hook"),
        "{text}"
    );

    let s = get(&fx, "/v1/import/switchboard");
    assert_eq!(
        (s["imported"].clone(), s["hooks"].clone()),
        (json!(true), json!(false))
    );
}

#[test]
fn import_without_old_data_is_404() {
    let fx = Fx::new("import-none", 0);
    let old = fx.base.join("no-old-state");
    std::fs::create_dir_all(&old).unwrap();
    let _d = daemon(&fx, &old);
    let s = get(&fx, "/v1/import/switchboard");
    assert_eq!(
        (s["found"].clone(), s["dir"].clone(), s["settings"].clone()),
        (json!(false), Value::Null, json!({}))
    );
    let (code, body) = fx.req("POST", "/v1/import/switchboard", "{}");
    assert_eq!(code, 404);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"error": "no switchboard data found"})
    );
}

#[test]
fn import_stops_the_old_daemon_process() {
    let fx = Fx::new("import-stop", 0);
    let old = old_state(&fx);
    let mut fake = Command::new("bash")
        .args(["-c", "exec -a switchboard sleep 60"])
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::write(old.join("daemon.pid"), fake.id().to_string()).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let _d = daemon(&fx, &old);
    assert_eq!(get(&fx, "/v1/import/switchboard")["running"], true);
    // Reap it as it dies: macOS has no /proc to tell a zombie from a live process.
    let reaper = std::thread::spawn(move || fake.wait());
    let (code, body) = fx.req("POST", "/v1/import/switchboard", "{}");
    assert_eq!(code, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["stopped"],
        true
    );
    assert!(reaper.join().unwrap().is_ok());
    assert_eq!(get(&fx, "/v1/import/switchboard")["running"], false);
}

#[test]
fn import_leaves_an_unrelated_pid_alone() {
    let fx = Fx::new("import-pid", 0);
    let old = old_state(&fx);
    let mut other = Command::new("sleep").arg("60").spawn().unwrap();
    std::fs::write(old.join("daemon.pid"), other.id().to_string()).unwrap();
    let _d = daemon(&fx, &old);
    let (code, body) = fx.req("POST", "/v1/import/switchboard", "{}");
    assert_eq!(code, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["stopped"],
        false
    );
    assert!(other.try_wait().unwrap().is_none());
    other.kill().unwrap();
    other.wait().unwrap();
}
