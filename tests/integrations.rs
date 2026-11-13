mod common;
mod fakes;

use common::{bin, config, err, exchange, git, out, wait_for, Api, Reap, ADMIN, SID, T};
use fakes::feedback_hub::FeedbackHub;
use overseer::sync::Transport;
use serde_json::{json, Value};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

struct Other {
    state: PathBuf,
    sock: PathBuf,
}

impl Other {
    fn new(t: &T, name: &str) -> Other {
        let state = t.base.join(name).join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(t.base.join(name).join("claude")).unwrap();
        let sock = std::env::temp_dir().join(format!(
            "sb-test-{name}-{}-{}.sock",
            std::process::id(),
            t.sock
        ));
        Other { state, sock }
    }

    fn cmd(&self, t: &T) -> Command {
        let mut c = Command::new(bin());
        t.env(&mut c);
        c.env("OVERSEER_STATE_DIR", &self.state)
            .env("OVERSEER_SOCKET", &self.sock)
            .env(
                "CLAUDE_CONFIG_DIR",
                self.state.parent().unwrap().join("claude"),
            )
            .env("OVERSEER_TMUX_SOCKET", format!("{}-none", t.sock));
        c
    }

    fn sb(&self, t: &T, args: &[&str], stdin: &str) -> Output {
        common::output(self.cmd(t).args(args), Some(stdin))
    }

    fn req(&self, method: &str, target: &str, body: &str) -> (u16, Value) {
        let (code, b) = exchange(
            UnixStream::connect(&self.sock).unwrap(),
            method,
            target,
            body,
            "unix",
        );
        (code, serde_json::from_str(&b).unwrap_or(Value::Null))
    }
}

impl Drop for Other {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.sock);
    }
}

fn sync(t: &T, api: &Api, station: &str, kind: &str, cfg: Value) -> Reap {
    config(t, cfg);
    let (code, v) = api.http(
        "POST",
        "/v1/stations",
        ADMIN,
        Some(json!({"stationId": station, "kind": kind})),
    );
    assert_eq!(code, 201, "{v}");
    let o = common::sb_stdin(
        t,
        &["enroll", "--api", &api.url()],
        v["token"].as_str().unwrap(),
    );
    assert!(o.status.success(), "{}", err(&o));
    let mut c = Command::new(bin());
    t.env(&mut c);
    let sync = Reap::spawn(c.arg("sync").stdout(Stdio::null()).stderr(Stdio::null()));
    let user = api.user("probe");
    wait_for(|| {
        let (_, v) = api.http("GET", "/v1/stations", &user, None);
        v.as_array()?
            .iter()
            .any(|s| s["stationId"] == station && s["online"] == true)
            .then_some(())
    });
    sync
}

fn commit(dir: &Path) {
    std::fs::write(dir.join("README"), "live\n").unwrap();
    git(dir, &["config", "user.name", "Dev"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

fn files_containing(dir: &Path, needle: &str) -> Vec<PathBuf> {
    let mut hits = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if std::fs::read(&p)
                .is_ok_and(|b| b.windows(needle.len()).any(|w| w == needle.as_bytes()))
            {
                hits.push(p);
            }
        }
    }
    hits
}

#[test]
fn a_relay_that_cannot_apply_snapshots_is_not_flooded_and_recovers() {
    let t = T::new();
    let api = Api::start(&t.base);
    let relay_db = || rusqlite::Connection::open(t.base.join("api.db")).unwrap();
    let _agent = t.start_agent("a", SID, "claude");
    let relay = relay_db();
    relay
        .execute_batch(
            "CREATE TRIGGER block_insert BEFORE INSERT ON lines BEGIN SELECT RAISE(ABORT, 'blocked'); END",
        )
        .unwrap();
    let _sync = sync(
        &t,
        &api,
        "st1",
        "vm",
        json!({"stationId": "st1", "owner": "dev@example.com"}),
    );
    std::thread::sleep(std::time::Duration::from_secs(3));
    let next_seq = rusqlite::Connection::open(t.base.join("state/sync.db"))
        .unwrap()
        .query_row("SELECT v FROM sync_meta WHERE k = 'next_seq'", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    assert!(next_seq <= 8, "station sent {next_seq} frames in 3 s");
    let lines = |api: &Api| {
        let (_, v) = api.http("GET", "/v1/lines?stationId=st1", ADMIN, None);
        v.as_array().map(Vec::len).unwrap_or(0)
    };
    assert_eq!(lines(&api), 0);
    relay.execute_batch("DROP TRIGGER block_insert").unwrap();
    wait_for(|| (lines(&api) == 1).then_some(()));
}

#[test]
fn board_actions_on_another_station_go_through_the_api() {
    let t = T::new();
    let api = Api::start(&t.base);
    let pane = t.start_agent("a", SID, "claude");
    let _b = sync(
        &t,
        &api,
        "st1",
        "vm",
        json!({"stationId": "st1", "owner": "dev@example.com", "acceptRemoteCommands": true}),
    );
    let id = wait_for(|| {
        let (_, v) = api.http("GET", "/v1/lines?stationId=st1", ADMIN, None);
        v.as_array()?
            .iter()
            .find(|r| r["line"]["agentSessionId"] == SID)
            .map(|r| r["line"]["id"].as_str().unwrap().to_string())
    });

    let a = Other::new(&t, "a");
    std::fs::write(
        a.state.join("config.json"),
        json!({"stationId": "st-a", "owner": "dev@example.com"}).to_string(),
    )
    .unwrap();
    let (_, v) = api.http(
        "POST",
        "/v1/stations",
        ADMIN,
        Some(json!({"stationId": "st-a", "kind": "mac"})),
    );
    let o = a.sb(
        &t,
        &["enroll", "--api", &api.url()],
        v["token"].as_str().unwrap(),
    );
    assert!(o.status.success(), "{}", err(&o));
    let o = a.sb(&t, &["sync", "--once"], "");
    assert!(o.status.success(), "{}", err(&o));
    let file: Value =
        serde_json::from_slice(&std::fs::read(a.state.join("stations/st1.json")).unwrap()).unwrap();
    assert!(file["lines"].to_string().contains(&id), "{file}");
    assert!(!a.state.join("stations/st-a.json").exists());

    let _d = Reap::spawn(
        a.cmd(&t)
            .args(["daemon", "--interval", "1"])
            .stderr(Stdio::null()),
    );
    wait_for(|| UnixStream::connect(&a.sock).ok());
    let entry = |a: &Other| {
        let (_, b) = a.req("GET", "/v1/board", "");
        b["lines"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["line"]["id"] == id.as_str())
            .cloned()
            .unwrap()
    };
    let e = entry(&a);
    assert_eq!(
        (e["actionable"].as_bool(), e["remote"].as_bool()),
        (Some(false), Some(true))
    );
    assert!(
        e["reason"].as_str().unwrap().contains("overseer login"),
        "{e}"
    );
    let (code, v) = a.req("POST", &format!("/v1/lines/{id}/hold"), "");
    assert_eq!(code, 409);
    assert!(
        v["error"].as_str().unwrap().contains("not logged in"),
        "{v}"
    );

    let o = a.sb(&t, &["login", "--api", &api.url()], &api.user("bob"));
    assert!(o.status.success(), "{}", err(&o));
    assert_eq!(entry(&a)["actionable"], true);

    t.keys(&pane, "half typed", false);
    let (code, v) = a.req("POST", &format!("/v1/lines/{id}/hold?dryRun=false"), "");
    assert_eq!(code, 409, "{v}");
    assert!(
        v["error"].as_str().unwrap().starts_with("inputEmpty"),
        "{v}"
    );
    t.tmux(&["send-keys", "-t", &pane, "C-u"]);

    let (code, v) = a.req("POST", &format!("/v1/lines/{id}/hold"), "");
    assert_eq!(
        (code, v["action"].as_str()),
        (200, Some("would-hold")),
        "{v}"
    );
    assert!(t.line(SID)["local"]["pid"].is_u64());
    let (code, v) = a.req("POST", &format!("/v1/lines/{id}/hold?dryRun=false"), "");
    assert_eq!((code, v["action"].as_str()), (200, Some("held")), "{v}");
    assert_eq!(
        t.tmux(&["show-options", "-p", "-v", "-t", &pane, "@sb_held"])
            .trim(),
        id
    );
    let (code, v) = a.req(
        "POST",
        &format!("/v1/lines/{id}/link"),
        r#"{"ticketKey":"AS-9"}"#,
    );
    assert_eq!((code, v["ticketKey"].as_str()), (200, Some("AS-9")), "{v}");
    assert_eq!(t.line(SID)["line"]["ticketKey"], "AS-9");
    let (_, audit) = api.http("GET", &format!("/v1/lines/{id}/commands"), ADMIN, None);
    let audit = audit.as_array().unwrap();
    assert!(!audit.is_empty(), "{audit:?}");
    assert!(audit.iter().all(|c| c["requestedBy"] == "bob"));

    let mut stale = file.clone();
    stale["lastSeenAt"] = json!("2020-01-01T00:00:00Z");
    std::fs::write(a.state.join("stations/st1.json"), stale.to_string()).unwrap();
    let (code, v) = a.req("POST", &format!("/v1/lines/{id}/close"), "");
    assert_eq!(code, 409);
    assert!(v["error"].as_str().unwrap().contains("offline"), "{v}");
}

#[test]
fn feedback_hub_starts_and_relaunches_a_line_on_a_runner() {
    let t = T::new();
    commit(&t.wt);
    let api = Api::start(&t.base);
    let wt_root = t.base.join("worktrees");
    let _sync = sync(
        &t,
        &api,
        "runner-1",
        "runner",
        json!({
            "stationId": "runner-1", "owner": "runner@example.com", "acceptRemoteCommands": true,
            "repos": {"github.com/acme/live": t.wt}, "worktreeDir": wt_root,
        }),
    );
    let hub = FeedbackHub {
        addr: api.addr.clone(),
        token: api.user("feedback-hub"),
    };
    let secret = "PROMPT-SENTINEL-42";
    let prompt = format!("{secret} the login button does nothing");

    let l = hub.open_line("fb-dry", "AS-500", &prompt, None);
    let c = hub.outcome(l["id"].as_str().unwrap(), "fb-dry");
    assert_eq!(c["status"], "succeeded", "{c}");
    assert!(
        c["reason"].as_str().unwrap().starts_with("would start"),
        "{c}"
    );
    assert!(!wt_root.exists());

    let l = hub.open_line("fb-1", "AS-500", &prompt, Some(false));
    let id = l["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("runner-1-"));
    let c = hub.outcome(&id, "fb-1");
    assert_eq!(c["status"], "succeeded", "{c}");
    let wt = wt_root.join("repo/feedback-AS-500");
    assert_eq!(
        git(&wt, &["symbolic-ref", "--short", "HEAD"]).trim(),
        "feedback/AS-500"
    );
    let session = format!("sb-{}", &id[id.len() - 6..]);
    t.wait(|| {
        t.tmux(&["capture-pane", "-p", "-t", &session])
            .contains(secret)
    });
    let row = wait_for(|| {
        let (_, v) = hub.http("GET", &format!("/v1/lines/{id}"), None);
        (v["line"]["agentSessionId"] != "pending").then_some(v)
    });
    let line = &row["line"];
    assert_eq!(
        (
            line["owner"].as_str(),
            line["origin"].as_str(),
            line["ticketKey"].as_str()
        ),
        (Some("feedback-bot"), Some("feedback"), Some("AS-500"))
    );
    assert_eq!(
        line["worktreePath"],
        wt.canonicalize().unwrap().to_str().unwrap()
    );
    fakes::schema::check("line.schema.json", line);

    let c = hub.relaunch(&id, "rl-dry", "MSG-SENTINEL-7", true);
    assert_eq!(c["status"], "succeeded", "{c}");
    assert!(
        c["reason"].as_str().unwrap().starts_with("would send"),
        "{c}"
    );
    let c = hub.relaunch(&id, "rl-1", "MSG-SENTINEL-7 it still fails", false);
    assert_eq!(c["status"], "succeeded", "{c}");
    t.wait(|| {
        t.tmux(&["capture-pane", "-p", "-t", &session])
            .contains("MSG-SENTINEL-7")
    });
    let c = hub.relaunch(&id, "rl-busy", "busy", false);
    assert_eq!(c["status"], "succeeded", "{c}");
    let c = wait_for(|| {
        let c = hub.relaunch(&id, &format!("rl-{}", overseer::util::now_ms()), "x", false);
        (c["status"] == "refused").then_some(c)
    });
    assert!(c["reason"].as_str().unwrap().contains("atPrompt"), "{c}");

    for needle in [secret, "MSG-SENTINEL-7"] {
        let hits: Vec<PathBuf> = files_containing(&t.base, needle)
            .into_iter()
            .filter(|p| !p.starts_with(t.base.join("home")))
            .collect();
        assert!(hits.is_empty(), "{needle} stored in {hits:?}");
    }
}

#[test]
fn patch_local_refuses_a_line_whose_branch_is_not_a_branch_name() {
    let t = T::new();
    commit(&t.wt);
    let origin = t.base.join("origin.git");
    git(&t.base, &["init", "-q", "--bare", origin.to_str().unwrap()]);
    git(
        &t.wt,
        &["remote", "set-url", "origin", origin.to_str().unwrap()],
    );
    let api = Api::start(&t.base);
    let wt_root = t.base.join("worktrees");
    config(
        &t,
        json!({
            "stationId": "st1", "owner": "dev@example.com", "ssh": {"vm2": "dev@vm2.internal"},
            "repos": {"github.com/acme/live": t.wt}, "worktreeDir": wt_root,
        }),
    );
    let (_, v) = api.http(
        "POST",
        "/v1/stations",
        ADMIN,
        Some(json!({"stationId": "vm2", "kind": "runner"})),
    );
    let mut ws =
        overseer::sync::Ws::connect(&api.url(), "vm2", v["token"].as_str().unwrap()).unwrap();
    ws.send(&json!({"kind": "hello", "stationId": "vm2", "daemonVersion": "t", "lastAckedSeq": 0}))
        .unwrap();
    common::recv(&mut ws, "ack");
    let marker = t.base.join("ran");
    let branch = format!("--upload-pack=touch {}", marker.display());
    ws.send(&json!({"kind": "snapshot", "seq": 1, "lines": [{
        "id": "vm2-01JPATCH000000000000000AC", "owner": "feedback-bot", "stationId": "vm2",
        "agent": "claude", "agentSessionId": "eeeeeeee-0000-4000-8000-000000000078",
        "repo": "github.com/acme/live", "branch": branch, "ticketKey": "AS-78",
        "worktreePath": "/w/live", "origin": "feedback", "state": "idle",
        "lastActivityAt": "2099-01-01T00:00:00Z",
    }]}))
    .unwrap();
    common::recv(&mut ws, "ack");
    let o = common::sb_stdin(&t, &["login", "--api", &api.url()], &api.user("bob"));
    assert!(o.status.success(), "{}", err(&o));

    for args in [
        &["patch", "AS-78", "--local"][..],
        &["patch", "AS-78", "--local", "--print"][..],
    ] {
        let o = t.sb(args);
        assert!(!o.status.success(), "{}", out(&o));
        assert!(
            err(&o).contains("is not a valid branch name"),
            "{}",
            err(&o)
        );
    }
    assert!(!marker.exists());
    assert!(!wt_root.exists());
}

#[test]
fn patch_attaches_by_default_or_opens_a_local_line_with_a_summary() {
    let t = T::new();
    commit(&t.wt);
    let origin = t.base.join("origin.git");
    git(&t.base, &["init", "-q", "--bare", origin.to_str().unwrap()]);
    git(
        &t.wt,
        &["remote", "set-url", "origin", origin.to_str().unwrap()],
    );
    git(&t.wt, &["branch", "feature/AS-77-remote"]);
    git(&t.wt, &["push", "-q", "origin", "feature/AS-77-remote"]);
    git(&t.wt, &["branch", "-D", "feature/AS-77-remote"]);
    let api = Api::start(&t.base);
    let wt_root = t.base.join("worktrees");
    config(
        &t,
        json!({
            "stationId": "st1", "owner": "dev@example.com", "ssh": {"vm2": "dev@vm2.internal"},
            "repos": {"github.com/acme/live": t.wt}, "worktreeDir": wt_root,
        }),
    );
    let (_, v) = api.http(
        "POST",
        "/v1/stations",
        ADMIN,
        Some(json!({"stationId": "vm2", "kind": "runner"})),
    );
    let mut ws =
        overseer::sync::Ws::connect(&api.url(), "vm2", v["token"].as_str().unwrap()).unwrap();
    ws.send(&json!({"kind": "hello", "stationId": "vm2", "daemonVersion": "t", "lastAckedSeq": 0}))
        .unwrap();
    common::recv(&mut ws, "ack");
    let rid = "vm2-01JPATCH000000000000000AB";
    ws.send(&json!({"kind": "snapshot", "seq": 1, "lines": [{
        "id": rid, "owner": "feedback-bot", "stationId": "vm2", "agent": "claude",
        "agentSessionId": "eeeeeeee-0000-4000-8000-000000000077", "repo": "github.com/acme/live",
        "branch": "feature/AS-77-remote", "ticketKey": "AS-77", "worktreePath": "/w/live",
        "origin": "feedback", "state": "idle", "lastActivityAt": "2099-01-01T00:00:00Z",
    }]}))
    .unwrap();
    common::recv(&mut ws, "ack");
    let o = common::sb_stdin(&t, &["login", "--api", &api.url()], &api.user("bob"));
    assert!(o.status.success(), "{}", err(&o));

    let o = t.sb(&["patch", "AS-77", "--print"]);
    assert_eq!(
        out(&o).trim(),
        format!("ssh -t dev@vm2.internal -- overseer resume {rid} --attach"),
        "{}",
        err(&o)
    );
    assert!(!t.sb(&["patch", "AS-404"]).status.success());
    let o = t.sb(&["patch", "AS-77"]);
    assert!(o.status.success(), "{}", err(&o));

    let o = t.sb(&["patch", "AS-77", "--local", "--print"]);
    assert!(out(&o).starts_with("would add worktree"), "{}", err(&o));
    let dir = wt_root.join("live/feature-AS-77-remote-patch");
    assert!(!dir.exists());
    let o = t.sb(&["patch", "AS-77", "--local"]);
    assert!(o.status.success(), "{}{}", out(&o), err(&o));
    assert_eq!(
        git(&dir, &["symbolic-ref", "--short", "HEAD"]).trim(),
        "feature/AS-77-remote"
    );
    let session = "sb-patch-feature-AS-77-remote";
    t.wait(|| {
        t.tmux(&["capture-pane", "-p", "-t", session])
            .contains(&format!("You take over line {rid}"))
    });
    let ssh = std::fs::read_to_string(t.base.join("ssh.log")).unwrap();
    assert_eq!(
        ssh.lines().collect::<Vec<_>>(),
        [
            format!("-t dev@vm2.internal -- overseer resume {rid} --attach"),
            format!("-T dev@vm2.internal -- overseer titles {rid}"),
        ]
    );
    let mine = wait_for(|| {
        t.json(&["ls", "--json", "--local", "--all"])
            .as_array()?
            .iter()
            .find(|r| r["line"]["worktreePath"] == dir.canonicalize().unwrap().to_str().unwrap())
            .cloned()
    });
    assert_eq!(
        (
            mine["line"]["ticketKey"].as_str(),
            mine["line"]["owner"].as_str()
        ),
        (Some("AS-77"), Some("dev@example.com"))
    );
    assert!(!std::fs::read_to_string(t.base.join("state/actions.log"))
        .unwrap()
        .contains("You take over"));
    assert!(files_containing(&t.base.join("state"), "You take over").is_empty());
    drop(ws);
}

#[test]
fn titles_memory_policy_and_doctor() {
    let t = T::new();
    config(
        &t,
        json!({"stationId": "st1", "owner": "dev@example.com", "idleAfterSeconds": 0,
               "memoryPolicy": {"enabled": true, "thresholdPercent": 0}}),
    );
    t.start_agent("a", SID, "claude");
    let id = t.line(SID)["line"]["id"].as_str().unwrap().to_string();
    let proj = std::fs::read_dir(t.base.join("home/.claude/projects"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(proj.join(format!("{SID}.jsonl")))
        .unwrap();
    use std::io::Write;
    for title in ["Fix login", "Fix login", "Add test"] {
        writeln!(
            f,
            "{}",
            json!({"type": "ai-title", "aiTitle": title, "sessionId": SID})
        )
        .unwrap();
    }
    assert_eq!(t.json(&["titles", &id]), json!(["Fix login", "Add test"]));

    let st = t.json(&["status", "--json"]);
    let p = &st["memoryPolicy"];
    assert_eq!(p["over"], true, "{st}");
    assert_eq!(p["wouldHold"][0]["lineId"], id.as_str());
    assert_eq!(p["wouldHold"][0]["holdable"], true, "{p}");
    let text = out(&t.sb(&["status"]));
    assert!(text.contains("report only") && text.contains(&id), "{text}");
    assert!(t.line(SID)["local"]["pid"].is_u64());
    assert_eq!(t.line(SID)["line"]["state"], "idle");

    let doctor = out(&t.sb(&["doctor"]));
    for name in [
        "memory policy",
        "api reachable",
        "station enrolment",
        "tmux",
    ] {
        assert!(doctor.contains(name), "{name}: {doctor}");
    }
    config(
        &t,
        json!({"stationId": "st1", "acceptRemoteCommands": true}),
    );
    let doctor = out(&t.sb(&["doctor"]));
    assert!(
        doctor.contains("runner") && doctor.contains("no repos or roots"),
        "{doctor}"
    );
    assert!(t.json(&["status", "--json"]).get("memoryPolicy").is_none());
}
