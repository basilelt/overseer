use overseer::procs::{Probe, ProcInfo};
use overseer::util::parse_rfc3339;
use overseer::{scan, Config, Ctx, Line};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;
use std::time::{Duration, UNIX_EPOCH};

const S1: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const S2: &str = "aaaaaaaa-0000-4000-8000-000000000002";
const S3: &str = "aaaaaaaa-0000-4000-8000-000000000003";
const S4: &str = "aaaaaaaa-0000-4000-8000-000000000004";
const S5: &str = "aaaaaaaa-0000-4000-8000-000000000005";
const CX: &str = "bbbbbbbb-0000-7000-8000-000000000001";
const NOW: &str = "2026-10-28T09:05:00Z";

static INIT: Once = Once::new();
static N: AtomicUsize = AtomicUsize::new(0);

fn setup() {
    INIT.call_once(|| {
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    });
}

#[derive(Default)]
struct Fake(Vec<(u32, &'static str, Vec<String>, Option<String>)>);

impl Probe for Fake {
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        let p = self.0.iter().find(|p| p.0 == pid)?;
        Some(ProcInfo {
            start: Some(p.1.to_string()),
            argv: p.2.clone(),
            start_ms: None,
        })
    }
    fn list(&self) -> Vec<(u32, Vec<String>)> {
        self.0.iter().map(|p| (p.0, p.2.clone())).collect()
    }
    fn cwd(&self, pid: u32) -> Option<String> {
        self.0.iter().find(|p| p.0 == pid)?.3.clone()
    }
}

fn argv(s: &str) -> Vec<String> {
    s.split(' ').map(str::to_string).collect()
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

struct Fx {
    base: PathBuf,
    main: String,
    clone: String,
    wt_login: String,
    wt_lower: String,
    wt_chore: String,
}

impl Fx {
    fn new() -> Fx {
        setup();
        let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "fx-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("main/src")).unwrap();
        let base = base.canonicalize().unwrap();
        let main = base.join("main");
        std::fs::write(main.join("src/f.txt"), "x").unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["add", "."]);
        git(&main, &["commit", "-q", "-m", "init"]);
        git(
            &main,
            &["remote", "add", "origin", "git@github.com:Acme/widget.git"],
        );
        for (b, d) in [
            ("feature/AS-1234-login", "wt-login"),
            ("bugfix/AS-77-lower", "wt-lower"),
            ("chore/no-ticket", "wt-chore"),
        ] {
            git(
                &main,
                &["worktree", "add", "-q", "-b", b, &format!("../{d}")],
            );
        }
        git(&base, &["clone", "-q", "main", "clone"]);
        let clone = base.join("clone");
        git(
            &clone,
            &[
                "remote",
                "set-url",
                "origin",
                "https://user:tok@github.com/Acme/widget.git",
            ],
        );
        git(&clone, &["checkout", "-q", "-b", "feedback/AS-1234"]);
        let s = |p: PathBuf| p.to_string_lossy().into_owned();
        let fx = Fx {
            main: s(main),
            clone: s(clone),
            wt_login: s(base.join("wt-login")),
            wt_lower: s(base.join("wt-lower")),
            wt_chore: s(base.join("wt-chore")),
            base,
        };
        fx.copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
            &fx.base.join("home"),
        );
        for (sid, ts) in [
            (S1, "2026-10-28T09:01:00Z"),
            (S2, "2026-10-28T08:05:00Z"),
            (S3, "2026-10-26T08:00:00Z"),
            (S4, "2026-09-28T08:00:00Z"),
            (S5, "2026-10-28T07:00:00Z"),
        ] {
            touch(&fx.claude().join(format!("projects/p/{sid}.jsonl")), ts);
        }
        touch(
            &fx.codex().join("sessions/2026/10/27/rollout-2026-10-27T10-00-00-bbbbbbbb-0000-7000-8000-000000000001.jsonl"),
            "2026-10-28T09:04:00Z",
        );
        fx
    }

    fn copy(&self, from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap().flatten() {
            let dst = to.join(e.file_name());
            if e.path().is_dir() {
                self.copy(&e.path(), &dst);
            } else {
                let text = std::fs::read_to_string(e.path())
                    .unwrap()
                    .replace("{{MAIN}}", &self.main)
                    .replace("{{CLONE}}", &self.clone)
                    .replace("{{WT_LOGIN}}", &self.wt_login)
                    .replace("{{WT_LOWER}}", &self.wt_lower)
                    .replace("{{WT_CHORE}}", &self.wt_chore);
                std::fs::write(dst, text).unwrap();
            }
        }
    }

    fn claude(&self) -> PathBuf {
        self.base.join("home/claude")
    }
    fn codex(&self) -> PathBuf {
        self.base.join("home/codex")
    }
    fn state(&self) -> PathBuf {
        self.base.join("state")
    }

    fn probe(&self) -> Fake {
        Fake(vec![
            (
                4242,
                "100",
                argv(&format!("claude --model opus --resume {S1}")),
                None,
            ),
            (
                4343,
                "200",
                argv("/opt/claude/versions/2.1.289 -c --add-dir /x y"),
                None,
            ),
            (4545, "999", argv("claude"), None),
            (
                5000,
                "1",
                argv("codex --model o3"),
                Some(self.wt_chore.clone()),
            ),
        ])
    }

    fn scan(&self, probe: &dyn Probe) -> Vec<Line> {
        let ctx = Ctx {
            claude_dir: self.claude(),
            codex_dir: self.codex(),
            gemini_dir: self.base.join(".gemini"),
            vibe_dir: self.base.join(".vibe"),
            state_dir: self.state(),
            snapshot: self.base.join("snapshot.json"),
            tmux: None,
            now_ms: parse_rfc3339(NOW).unwrap(),
            probe,
            config: Config {
                station_id: "test-station".into(),
                owner: "tester@example.com".into(),
                roots: vec![self.base.clone()],
                idle_after_ms: 600_000,
                mcp_hints: vec![],
                accept_remote_commands: false,
                ssh: Default::default(),
                remote_command: "overseer".into(),
                repos: Default::default(),
                worktree_dir: self.base.join("worktrees"),
                start_args: vec![],
                memory_policy: None,
                auto_concurrency: 2,
                auto_organize: false,
                worktree_fetch: false,
                worktree_stale_days: 14,
                session_tasks_days: 7,
                discover_roots: vec![],
            },
        };
        scan(&ctx).unwrap()
    }
}

fn touch(p: &Path, ts: &str) {
    let t = UNIX_EPOCH + Duration::from_millis(parse_rfc3339(ts).unwrap() as u64);
    std::fs::File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

fn find<'a>(lines: &'a [Line], sid: &str) -> &'a Line {
    lines.iter().find(|l| l.session_id == sid).unwrap()
}

#[test]
fn discovers_lines_with_state_ticket_and_resume() {
    let fx = Fx::new();
    let lines = fx.scan(&fx.probe());
    assert_eq!(lines.len(), 6);
    assert!(lines.iter().all(|l| l.session_id != "agent-x"));

    let l1 = find(&lines, S1);
    assert_eq!(
        (l1.state, l1.ticket.as_deref(), l1.branch.as_str()),
        ("active", Some("AS-1234"), "feature/AS-1234-login")
    );
    assert_eq!(l1.repo, "github.com/Acme/widget");
    assert_eq!(l1.worktree_path, fx.wt_login);
    assert_eq!(l1.launch_args, ["--model", "opus"]);
    assert_eq!(
        l1.resume_command,
        format!("cd {} && claude --model opus --resume {S1}", fx.wt_login)
    );
    assert_eq!(l1.title.as_deref(), Some("Synthetic title"));
    assert!(l1.busy);

    let l2 = find(&lines, S2);
    assert_eq!(
        (l2.state, l2.ticket.as_deref(), l2.branch.as_str()),
        ("idle", Some("AS-77"), "bugfix/AS-77-lower")
    );
    assert_eq!(
        l2.resume_command,
        format!("cd {} && claude --add-dir /x y --resume {S2}", fx.wt_lower)
    );

    let l3 = find(&lines, S3);
    assert_eq!(
        (l3.state, l3.held_reason, l3.ticket.as_deref()),
        ("held", Some("lost"), None)
    );
    assert_eq!(l3.worktree_path, fx.main);
    assert_eq!(l3.cwd, format!("{}/src", fx.main));

    let l4 = find(&lines, S4);
    assert!(l4.cwd_missing);
    assert_eq!(
        (l4.repo.as_str(), l4.ticket.as_deref(), l4.state),
        ("unknown", Some("AS-9"), "held")
    );

    let l5 = find(&lines, S5);
    assert_eq!(
        (l5.state, l5.repo.as_str(), l5.ticket.as_deref()),
        ("held", "github.com/Acme/widget", Some("AS-1234"))
    );

    let cx = find(&lines, CX);
    assert_eq!(
        (cx.agent, cx.state, cx.branch.as_str(), cx.ticket.as_deref()),
        ("codex", "active", "chore/no-ticket", None)
    );
    assert_eq!(
        cx.resume_command,
        format!("cd {} && codex resume --model o3 {CX}", fx.wt_chore)
    );

    let db = rusqlite::Connection::open(fx.state().join("overseer.db")).unwrap();
    let wts: i64 = db
        .query_row("SELECT count(*) FROM worktrees", [], |r| r.get(0))
        .unwrap();
    assert_eq!(wts, 5);
    let ticket: String = db
        .query_row(
            "SELECT ticket_key FROM worktrees WHERE path = ?1",
            [&fx.wt_lower],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ticket, "AS-77");
}

#[test]
fn ids_are_stable_and_launch_args_survive_process_death() {
    let fx = Fx::new();
    let first = fx.scan(&fx.probe());
    let second = fx.scan(&Fake::default());
    for l in &first {
        assert_eq!(find(&second, &l.session_id).id, l.id);
        assert!(l.id.starts_with("test-station-") && l.id.len() == "test-station-".len() + 26);
    }
    let l1 = find(&second, S1);
    assert_eq!((l1.state, l1.held_reason), ("held", Some("lost")));
    assert_eq!(
        l1.resume_command,
        format!("cd {} && claude --model opus --resume {S1}", fx.wt_login)
    );
}

#[test]
fn an_empty_process_list_does_not_stamp_held_at_but_a_real_loss_does() {
    let fx = Fx::new();
    fx.scan(&fx.probe());
    let blind = fx.scan(&Fake::default());
    assert_eq!(find(&blind, S1).held_at, None);
    let other = Fake(vec![(999, "1", argv("sleep 1"), None)]);
    let lost = fx.scan(&other);
    assert!(find(&lost, S1).held_at.is_some());
}

#[test]
fn unchanged_lines_skip_the_write_but_a_new_database_gets_every_row() {
    let fx = Fx::new();
    let count = || {
        overseer::registry::Registry::open(&fx.state())
            .unwrap()
            .conn
            .query_row("SELECT count(*) FROM lines", [], |r| r.get::<_, i64>(0))
            .unwrap()
    };
    let first = fx.scan(&fx.probe());
    assert_eq!(count(), first.len() as i64);
    let again = fx.scan(&fx.probe());
    assert_eq!(again.len(), first.len());
    for l in &first {
        assert_eq!(find(&again, &l.session_id).id, l.id);
    }
    for f in ["overseer.db", "overseer.db-wal", "overseer.db-shm"] {
        let _ = std::fs::remove_file(fx.state().join(f));
    }
    let fresh = fx.scan(&fx.probe());
    assert_eq!(count(), fresh.len() as i64);
    let gone = fx.scan(&Fake::default());
    let l1 = find(&gone, S1);
    assert_eq!((l1.state, l1.held_reason), ("held", Some("lost")));
}

#[test]
fn idle_threshold_follows_clock() {
    use overseer::line_state;
    let now = parse_rfc3339(NOW).unwrap();
    assert_eq!(
        line_state(false, true, false, now - 599_000, now, 600_000).0,
        "active"
    );
    assert_eq!(
        line_state(false, true, false, now - 601_000, now, 600_000).0,
        "idle"
    );
    assert_eq!(
        line_state(false, true, true, now - 601_000, now, 600_000).0,
        "active"
    );
    assert_eq!(
        line_state(false, false, false, now, now, 600_000),
        ("held", Some("lost"))
    );
    assert_eq!(
        line_state(true, true, false, now, now, 600_000).0,
        "hung-up"
    );
}

#[test]
fn codex_state_db_is_read_from_a_copy() {
    let fx = Fx::new();
    let db_path = fx.codex().join("state_5.sqlite");
    let db = rusqlite::Connection::open(&db_path).unwrap();
    db.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
         CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
           title TEXT, archived INTEGER, git_branch TEXT, first_user_message TEXT);",
    )
    .unwrap();
    let t = parse_rfc3339("2026-10-28T09:00:00Z").unwrap() / 1000;
    db.execute(
        "INSERT INTO threads VALUES ('cx-archived', NULL, ?1, ?1, ?2, 'secret', 1, 'chore/no-ticket', 'secret'),
                                    ('cx-live', NULL, ?1, ?1, '/nonexistent', 'secret', 0, 'AS-5-x', 'secret')",
        rusqlite::params![t, fx.wt_chore],
    )
    .unwrap();
    assert!(fx.codex().join("state_5.sqlite-wal").exists());
    let probe = Fake(vec![(
        6000,
        "1",
        argv("/usr/local/bin/codex resume cx-live"),
        None,
    )]);
    let lines = fx.scan(&probe);
    let codex: Vec<&Line> = lines.iter().filter(|l| l.agent == "codex").collect();
    assert_eq!(codex.len(), 2);
    assert_eq!(find(&lines, "cx-archived").state, "hung-up");
    let live = find(&lines, "cx-live");
    assert_eq!(
        (live.state, live.ticket.as_deref(), live.cwd_missing),
        ("active", Some("AS-5"), true)
    );
    assert_eq!(
        std::fs::read_dir(fx.state().join("tmp")).unwrap().count(),
        0
    );
    drop(db);
}

#[test]
fn codex_desktop_and_server_processes_are_not_sessions() {
    let fx = Fx::new();
    let cwd = Some(fx.wt_chore.clone());
    let probe = Fake(vec![
        (
            7001,
            "1",
            argv("/u/.codex/packages/standalone/releases/0.166.0-aarch64-apple-darwin/bin/codex app"),
            cwd.clone(),
        ),
        (
            7002,
            "1",
            argv("/u/.codex/packages/standalone/releases/0.166.0-aarch64-apple-darwin/bin/codex app"),
            cwd.clone(),
        ),
        (
            7003,
            "1",
            argv("node /u/.nvm/versions/node/v22.14.0/bin/codex app-server"),
            cwd.clone(),
        ),
        (
            7004,
            "1",
            argv("/u/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/codex/codex app-server"),
            cwd.clone(),
        ),
        (
            7005,
            "1",
            argv("codex -c model=o3 mcp-server"),
            cwd,
        ),
    ]);
    let lines = fx.scan(&probe);
    let cx = find(&lines, CX);
    assert_eq!(
        (cx.pid, cx.state, cx.held_reason),
        (None, "held", Some("lost"))
    );
}

#[test]
fn codex_sandboxed_tool_call_does_not_wake_an_idle_thread() {
    let fx = Fx::new();
    let probe = Fake(vec![(
        7101,
        "1",
        argv("bwrap --new-session --die-with-parent --ro-bind / / --argv0 codex-linux-sandbox -- /u/.codex/packages/app-server-daemon/releases/0.166.0-x86_64-unknown-linux-musl/bin/codex --sandbox-policy-cwd /r --command-cwd /r -- /bin/zsh -c ls"),
        Some(fx.wt_chore.clone()),
    )]);
    let lines = fx.scan(&probe);
    let cx = find(&lines, CX);
    assert_eq!((cx.pid, cx.state), (None, "held"));
}

struct Timed(Fake, Vec<(u32, i64)>);

impl Probe for Timed {
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        let mut i = self.0.info(pid)?;
        i.start_ms = self.1.iter().find(|p| p.0 == pid).map(|p| p.1);
        Some(i)
    }
    fn list(&self) -> Vec<(u32, Vec<String>)> {
        self.0.list()
    }
    fn cwd(&self, pid: u32) -> Option<String> {
        self.0.cwd(pid)
    }
}

const T0: &str = "2026-10-28T09:00:00Z";

fn codex_threads(fx: &Fx, rows: &[(&str, i64, i64)]) {
    let db = rusqlite::Connection::open(fx.codex().join("state_5.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
           title TEXT, archived INTEGER, git_branch TEXT, first_user_message TEXT);",
    )
    .unwrap();
    let t0 = parse_rfc3339(T0).unwrap() / 1000;
    for (id, created, updated) in rows {
        db.execute(
            "INSERT INTO threads VALUES (?1, NULL, ?2, ?3, ?4, 't', 0, NULL, 'm')",
            rusqlite::params![id, t0 + created, t0 + updated, fx.wt_chore],
        )
        .unwrap();
    }
}

fn tuis(fx: &Fx, procs: &[(u32, &str, i64)]) -> Timed {
    let t0 = parse_rfc3339(T0).unwrap();
    Timed(
        Fake(
            procs
                .iter()
                .map(|(pid, cmd, _)| (*pid, "1", argv(cmd), Some(fx.wt_chore.clone())))
                .collect(),
        ),
        procs
            .iter()
            .map(|(pid, _, s)| (*pid, t0 + s * 1000))
            .collect(),
    )
}

fn pids(lines: &[Line], ids: &[&str]) -> Vec<Option<u32>> {
    ids.iter().map(|id| find(lines, id).pid).collect()
}

#[test]
fn codex_tuis_in_one_repo_keep_their_threads_whatever_was_used_last() {
    let fx = Fx::new();
    codex_threads(
        &fx,
        &[
            ("cx-old", -600, -500),
            ("cx-a", 20, 300),
            ("cx-b", 120, 200),
        ],
    );
    let lines = fx.scan(&tuis(&fx, &[(8002, "codex", 100), (8001, "codex", 0)]));
    assert_eq!(
        pids(&lines, &["cx-old", "cx-a", "cx-b"]),
        [None, Some(8001), Some(8002)]
    );
}

#[test]
fn two_fresh_codex_tuis_and_one_thread_leave_the_pid_unset() {
    let fx = Fx::new();
    codex_threads(&fx, &[("cx-a", 120, 130)]);
    let lines = fx.scan(&tuis(&fx, &[(8001, "codex", 0), (8002, "codex", 100)]));
    assert_eq!(pids(&lines, &["cx-a"]), [None]);
}

#[test]
fn codex_tui_that_quits_takes_only_its_own_thread_down() {
    let fx = Fx::new();
    codex_threads(&fx, &[("cx-a", 20, 300), ("cx-b", 120, 200)]);
    let lines = fx.scan(&tuis(&fx, &[(8002, "codex", 100)]));
    assert_eq!(pids(&lines, &["cx-a", "cx-b"]), [None, Some(8002)]);
}

#[test]
fn fresh_codex_tui_does_not_wake_an_idle_thread_of_its_repo() {
    let fx = Fx::new();
    codex_threads(&fx, &[("cx-old", -600, -500)]);
    let lines = fx.scan(&tuis(&fx, &[(8003, "codex", 0)]));
    assert_eq!(pids(&lines, &["cx-old"]), [None]);
}

#[test]
fn codex_resume_last_attaches_to_the_most_recent_thread_of_its_repo() {
    let fx = Fx::new();
    codex_threads(&fx, &[("cx-a", -900, -100), ("cx-b", -800, -400)]);
    let lines = fx.scan(&tuis(&fx, &[(8004, "codex resume --last", 0)]));
    assert_eq!(pids(&lines, &["cx-a", "cx-b"]), [Some(8004), None]);
}

#[test]
fn codex_tui_that_switches_to_an_older_thread_follows_it() {
    let fx = Fx::new();
    codex_threads(&fx, &[("cx-a", -900, 100)]);
    let lines = fx.scan(&tuis(&fx, &[(8005, "codex", 0)]));
    assert_eq!(pids(&lines, &["cx-a"]), [Some(8005)]);
}

struct Tree {
    procs: Fake,
    parents: Vec<(u32, u32)>,
    rss: Vec<(u32, u64)>,
}

impl Probe for Tree {
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        self.procs.info(pid)
    }
    fn list(&self) -> Vec<(u32, Vec<String>)> {
        self.procs.list()
    }
    fn cwd(&self, pid: u32) -> Option<String> {
        self.procs.cwd(pid)
    }
    fn ppid(&self, pid: u32) -> Option<u32> {
        self.parents.iter().find(|p| p.0 == pid).map(|p| p.1)
    }
    fn rss(&self, pid: u32) -> Option<u64> {
        self.rss.iter().find(|p| p.0 == pid).map(|p| p.1)
    }
}

#[test]
fn codex_npm_wrapper_and_native_child_are_one_session() {
    let fx = Fx::new();
    let db = rusqlite::Connection::open(fx.codex().join("state_5.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
           title TEXT, archived INTEGER, git_branch TEXT, first_user_message TEXT);",
    )
    .unwrap();
    let t = parse_rfc3339("2026-10-28T09:00:00Z").unwrap() / 1000;
    db.execute(
        "INSERT INTO threads VALUES ('cx-resumed', NULL, ?1, ?1, ?2, 't', 0, NULL, 'm'),
                                    ('cx-other', NULL, ?1 - 60, ?1 - 60, ?2, 't', 0, NULL, 'm')",
        rusqlite::params![t, fx.wt_chore],
    )
    .unwrap();
    drop(db);
    let cwd = Some(fx.wt_chore.clone());
    let probe = Tree {
        procs: Fake(vec![
            (
                14524,
                "1",
                argv("node /u/.nvm/versions/node/v22.14.0/bin/codex resume cx-resumed"),
                cwd.clone(),
            ),
            (
                14525,
                "1",
                argv("/u/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/codex/codex resume cx-resumed"),
                cwd,
            ),
        ]),
        parents: vec![(14524, 900), (14525, 14524)],
        rss: vec![(14524, 40 << 20), (14525, 300 << 20)],
    };
    let lines = fx.scan(&probe);
    let live = find(&lines, "cx-resumed");
    assert_eq!((live.pid, live.rss), (Some(14525), Some(300 << 20)));
    assert_eq!(find(&lines, "cx-other").pid, None);
}

#[test]
fn json_output_matches_line_contract() {
    let fx = Fx::new();
    let run = || {
        let out = Command::new(env!("CARGO_BIN_EXE_overseer"))
            .args(["ls", "--json", "--all"])
            .env(
                "OVERSEER_TMUX_SOCKET",
                format!("sb-test-none-{}", std::process::id()),
            )
            .env("OVERSEER_TMUX_SNAPSHOT", fx.base.join("none.json"))
            .env("OVERSEER_STATE_DIR", fx.state())
            .env("CLAUDE_CONFIG_DIR", fx.claude())
            .env("CODEX_HOME", fx.codex())
            .env("HOME", &fx.base)
            .env("USER", "tester")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<Vec<Value>>(&out.stdout).unwrap()
    };
    let items = run();
    assert_eq!(items.len(), 6);
    let schema: Value =
        serde_json::from_str(include_str!("../contract/schemas/line.schema.json")).unwrap();
    let props = schema["properties"].as_object().unwrap();
    for item in &items {
        let line = item["line"].as_object().unwrap();
        for req in schema["required"].as_array().unwrap() {
            let k = req.as_str().unwrap();
            let v = line.get(k).unwrap_or_else(|| panic!("missing {k}"));
            if props[k]["minLength"] == 1 {
                assert!(!v.as_str().unwrap().is_empty(), "{k} empty");
            }
        }
        for (k, v) in line {
            let p = props.get(k).unwrap_or_else(|| panic!("unexpected {k}"));
            assert!(v.is_string(), "{k} not a string");
            if let Some(e) = p["enum"].as_array() {
                assert!(e.contains(v), "{k}={v} not in enum");
            }
        }
        if let Some(t) = line.get("ticketKey").and_then(Value::as_str) {
            assert!(
                t.starts_with("AS-") && t[3..].chars().all(|c| c.is_ascii_digit()) && t.len() > 3
            );
        }
        assert!(parse_rfc3339(line["lastActivityAt"].as_str().unwrap()).is_some());
        assert!(item["local"]["resumeCommand"]
            .as_str()
            .unwrap()
            .contains(line["agentSessionId"].as_str().unwrap()));
    }
    let ids = |v: &[Value]| {
        v.iter()
            .map(|i| i["line"]["id"].as_str().unwrap().to_string())
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(ids(&items), ids(&run()));
    let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(fx.state()), 0o700);
    assert_eq!(mode(fx.state().join("overseer.db")), 0o600);
}

#[test]
fn large_transcript_reads_head_and_grows_tail() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("big-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sid = "cccccccc-0000-4000-8000-000000000001";
    let path = dir.join(format!("{sid}.jsonl"));
    let filler = format!(
        "{{\"type\":\"x\",\"sessionId\":\"{sid}\",\"pad\":\"{}\"}}\n",
        "z".repeat(1000)
    );
    let head = format!("{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"cwd\":\"/w\",\"gitBranch\":\"first\",\"timestamp\":\"2026-10-28T09:00:00Z\"}}\n");
    std::fs::write(&path, format!("{head}{}", filler.repeat(700))).unwrap();
    let t = overseer::claude::read_transcript(&path).unwrap();
    assert_eq!(
        (t.cwd.as_deref(), t.branch.as_deref()),
        (Some("/w"), Some("first"))
    );
    let last = format!(
        "{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"cwd\":\"/w\",\"gitBranch\":\"last\"}}\n"
    );
    std::fs::write(&path, format!("{head}{}{last}", filler.repeat(700))).unwrap();
    let t = overseer::claude::read_transcript(&path).unwrap();
    assert_eq!(t.branch.as_deref(), Some("last"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dead_session_flags_come_from_tmux_snapshot_and_owner_from_repo() {
    let fx = Fx::new();
    git(
        Path::new(&fx.main),
        &["config", "user.email", "dev@example.com"],
    );
    let snap = serde_json::json!({"boot_id": "b", "saved_at": "x", "panes": [
        {"session": "work-1", "pane": "0", "claude_session": S3, "claude_session_guessed": true, "claude_args": ["--model", "haiku"]},
        {"session": "work-2", "pane": "0", "claude_session": S3, "claude_args": ["--model", "sonnet", "-r", "old"]},
        {"session": "work-3", "pane": "0", "claude_session": S1, "claude_args": ["--model", "wrong"]},
        {"session": "work-4", "pane": "0", "cwd": "/x"}
    ]});
    std::fs::write(fx.base.join("snapshot.json"), snap.to_string()).unwrap();
    let lines = fx.scan(&fx.probe());
    let l3 = find(&lines, S3);
    assert_eq!(
        l3.resume_command,
        format!("cd {}/src && claude --model sonnet --resume {S3}", fx.main)
    );
    assert_eq!(find(&lines, S1).launch_args, ["--model", "opus"]);
    assert_eq!(l3.owner, "dev@example.com");
    assert_eq!(find(&lines, S4).owner, "tester@example.com");
}

#[test]
fn ls_defaults_to_last_seven_days() {
    let fx = Fx::new();
    let count = |all: bool| {
        let mut args = vec!["ls", "--json"];
        if all {
            args.push("--all");
        }
        let out = Command::new(env!("CARGO_BIN_EXE_overseer"))
            .args(args)
            .env("OVERSEER_STATE_DIR", fx.state())
            .env("OVERSEER_NOW", NOW)
            .env(
                "OVERSEER_TMUX_SOCKET",
                format!("sb-test-none-{}", std::process::id()),
            )
            .env("OVERSEER_TMUX_SNAPSHOT", fx.base.join("none.json"))
            .env("CLAUDE_CONFIG_DIR", fx.claude())
            .env("CODEX_HOME", fx.codex())
            .env("HOME", &fx.base)
            .output()
            .unwrap();
        serde_json::from_slice::<Vec<Value>>(&out.stdout)
            .unwrap()
            .len()
    };
    assert_eq!((count(false), count(true)), (5, 6));
}

fn ws(path: &str) -> overseer::workspaces::Workspace {
    overseer::workspaces::Workspace {
        id: path.into(),
        name: path.into(),
        path: path.into(),
        folder_id: None,
        pinned: false,
        archived: false,
        auto: false,
        rank: 0.0,
        tabs: vec![],
        notify_idle: false,
        created_ms: 0,
        updated_ms: 0,
        subject: false,
        cwd: None,
        parent_id: None,
    }
}

#[test]
fn live_cwd_beats_stale_transcript_cwd_from_a_renamed_project_dir() {
    const S6: &str = "aaaaaaaa-0000-4000-8000-000000000006";
    const S7: &str = "aaaaaaaa-0000-4000-8000-000000000007";
    let fx = Fx::new();
    let old = fx.base.join("renamed-away").to_string_lossy().into_owned();
    let dir = fx.claude().join("projects/-renamed-away");
    std::fs::create_dir_all(&dir).unwrap();
    for sid in [S6, S7] {
        std::fs::write(
            dir.join(format!("{sid}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"cwd\":\"{old}\",\"timestamp\":\"2026-10-28T09:00:00Z\"}}\n"
            ),
        )
        .unwrap();
    }
    for (pid, sid, start) in [(4646, S6, "600"), (4747, S7, "700")] {
        std::fs::write(
            fx.claude().join(format!("sessions/{pid}.json")),
            format!(
                "{{\"pid\":{pid},\"sessionId\":\"{sid}\",\"cwd\":\"{}\",\"procStart\":\"{start}\",\"status\":\"idle\",\"updatedAt\":0}}",
                fx.main
            ),
        )
        .unwrap();
    }
    let probe = Fake(vec![
        (
            4646,
            "600",
            argv("claude --session-id x"),
            Some(fx.wt_lower.clone()),
        ),
        (4747, "700", argv("claude"), None),
    ]);
    let lines = fx.scan(&probe);

    let by_proc = find(&lines, S6);
    assert_eq!(by_proc.cwd, fx.wt_lower);
    assert_eq!(by_proc.worktree_path, fx.wt_lower);
    assert!(!by_proc.cwd_missing);

    let by_session_file = find(&lines, S7);
    assert_eq!(by_session_file.cwd, fx.main);
    assert_eq!(by_session_file.worktree_path, fx.main);
    assert!(!by_session_file.cwd_missing);

    let ix = overseer::workspaces::Index {
        workspaces: vec![ws(&fx.main), ws(&fx.wt_lower)],
        tasks: vec![],
        projects: vec![],
    };
    let of = |l: &Line| ix.of_line(l).map(|w| w.path.clone());
    assert_eq!(of(by_proc).as_deref(), Some(fx.wt_lower.as_str()));
    assert_eq!(of(by_session_file).as_deref(), Some(fx.main.as_str()));

    let gone = fx.scan(&Fake::default());
    assert_eq!(find(&gone, S6).cwd, fx.main);
}

#[test]
fn two_live_processes_on_one_transcript_stay_two_lines_that_follow_their_own_session() {
    const OLD: &str = "aaaaaaaa-0000-4000-8000-000000000010";
    const NEW_A: &str = "aaaaaaaa-0000-4000-8000-00000000000a";
    const NEW_B: &str = "aaaaaaaa-0000-4000-8000-00000000000b";
    const LATER: &str = "aaaaaaaa-0000-4000-8000-000000000011";
    let fx = Fx::new();
    let dir = fx.claude().join("projects/-shared");
    std::fs::create_dir_all(&dir).unwrap();
    let transcript = |sid: &str, text: &str| {
        std::fs::write(
            dir.join(format!("{sid}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"cwd\":\"{}\",\"timestamp\":\"2026-10-28T09:00:00Z\",\"message\":{{\"role\":\"user\",\"content\":\"{text}\"}}}}\n",
                fx.main
            ),
        )
        .unwrap();
    };
    transcript(OLD, "the resumed conversation");
    transcript(NEW_B, "what B said before resuming");
    transcript(LATER, "a later conversation");
    let report = |pid: u32, sid: &str, start: &str| {
        std::fs::write(
            fx.claude().join(format!("sessions/{pid}.json")),
            format!(
                "{{\"pid\":{pid},\"sessionId\":\"{sid}\",\"cwd\":\"{}\",\"procStart\":\"{start}\",\"status\":\"idle\",\"updatedAt\":0}}",
                fx.main
            ),
        )
        .unwrap();
    };
    report(5001, OLD, "501");
    report(5002, OLD, "502");
    let probe = Fake(vec![
        (
            5001,
            "501",
            argv(&format!("claude --session-id {NEW_A}")),
            None,
        ),
        (
            5002,
            "502",
            argv(&format!("claude --session-id {NEW_B}")),
            None,
        ),
    ]);
    let lines = fx.scan(&probe);
    let on_old: Vec<&Line> = lines.iter().filter(|l| l.session_id == OLD).collect();
    assert_eq!(on_old.len(), 2, "one line per live process");
    assert!(on_old.iter().all(|l| l.alive()));
    let a = on_old.iter().find(|l| l.pid == Some(5001)).unwrap();
    let b = on_old.iter().find(|l| l.pid == Some(5002)).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(a.key, NEW_A);
    assert_eq!(b.key, NEW_B);
    assert!(
        !lines.iter().any(|l| l.session_id == NEW_B),
        "B's own old transcript is now B's live line, not a second entry"
    );
    let claude_dir = fx.claude();
    let read = |l: &Line| {
        let p = overseer::activity::find_transcript(
            &claude_dir,
            &claude_dir,
            &claude_dir,
            "claude",
            &l.session_id,
        )
        .unwrap();
        std::fs::read_to_string(p).unwrap()
    };
    assert!(read(a).contains("the resumed conversation"));
    assert!(read(b).contains("the resumed conversation"));
    assert!(a.resume_command.contains(OLD), "{}", a.resume_command);

    report(5001, LATER, "501");
    let lines = fx.scan(&probe);
    let a2 = lines.iter().find(|l| l.pid == Some(5001)).unwrap();
    assert_eq!(a2.id, a.id, "the line keeps its identity");
    assert_eq!(
        a2.session_id, LATER,
        "and follows the session the process now reports"
    );
    assert!(read(a2).contains("a later conversation"));
    let b2 = lines.iter().find(|l| l.pid == Some(5002)).unwrap();
    assert_eq!(b2.id, b.id);
    assert_eq!(b2.session_id, OLD);
    assert!(!b2.duplicate);

    let probe = Fake(vec![(
        5001,
        "501",
        argv(&format!("claude --session-id {NEW_A}")),
        None,
    )]);
    let lines = fx.scan(&probe);
    let b3 = lines
        .iter()
        .find(|l| l.id == b.id)
        .expect("B's line outlives its process");
    assert!(b3.pid.is_none());
    assert_eq!(
        b3.session_id, OLD,
        "it still shows the conversation it was in"
    );
    assert!(b3.resume_command.contains(OLD), "{}", b3.resume_command);
    assert!(read(b3).contains("the resumed conversation"));
    assert!(!lines
        .iter()
        .any(|l| l.id != b3.id && l.pid.is_none() && l.session_id == OLD));
    assert!(!lines.iter().any(|l| l.session_id == NEW_B));
}

#[test]
fn codex_resume_drops_the_resume_subcommand_of_the_process_it_came_from() {
    const OLD: &str = "01a181a9-d0fe-7810-b802-ae037ed37fed";
    let fx = Fx::new();
    let probe = Fake(vec![(
        5000,
        "1",
        argv(&format!("codex resume {OLD} --model o3")),
        Some(fx.wt_chore.clone()),
    )]);
    let lines = fx.scan(&probe);
    let cx = find(&lines, CX);
    assert_eq!(cx.pid, Some(5000));
    assert_eq!(cx.launch_args, ["--model", "o3"]);
    assert_eq!(
        cx.resume_command,
        format!("cd {} && codex resume --model o3 {CX}", fx.wt_chore)
    );
}

#[test]
fn codex_resume_repairs_flags_stored_with_a_resume_or_fork_subcommand() {
    let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    for (stored, want) in [
        (
            s(&["resume", "01a181a9-d0fe-7810-b802-ae037ed37fed"]),
            s(&["codex", "resume", CX]),
        ),
        (
            s(&["-m", "o3", "fork", "--last"]),
            s(&["codex", "resume", "-m", "o3", CX]),
        ),
        (
            s(&["resume", "--last", "-c", "model=o3"]),
            s(&["codex", "resume", "--no-daemon", "-c", "model=o3", CX]),
        ),
    ] {
        assert_eq!(
            overseer::resume_argv("codex", CX, &stored),
            want,
            "{stored:?}"
        );
    }
}

#[test]
fn resume_rejoins_a_json_flag_stored_split() {
    let flags: Vec<String> = [
        "--mcp-config={\"c\":\"/Application",
        "Support/x\"}",
        "--permission-mode",
        "auto",
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(
        overseer::resume_argv("claude", S1, &flags),
        [
            "claude",
            "--mcp-config={\"c\":\"/Application Support/x\"}",
            "--permission-mode",
            "auto",
            "--resume",
            S1
        ]
    );
}

#[test]
fn resuming_a_line_whose_session_changed_reattaches_the_process_to_that_line() {
    const KEY: &str = "aaaaaaaa-0000-4000-8000-000000000020";
    const NOW_SID: &str = "aaaaaaaa-0000-4000-8000-000000000021";
    let fx = Fx::new();
    let dir = fx.claude().join("projects/-moved");
    std::fs::create_dir_all(&dir).unwrap();
    for sid in [KEY, NOW_SID] {
        std::fs::write(
            dir.join(format!("{sid}.jsonl")),
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{sid}\",\"cwd\":\"{}\",\"timestamp\":\"2026-10-28T09:00:00Z\"}}\n",
                fx.main
            ),
        )
        .unwrap();
    }
    let report = |pid: u32, start: &str| {
        std::fs::write(
            fx.claude().join(format!("sessions/{pid}.json")),
            format!(
                "{{\"pid\":{pid},\"sessionId\":\"{NOW_SID}\",\"cwd\":\"{}\",\"procStart\":\"{start}\",\"status\":\"idle\",\"updatedAt\":0}}",
                fx.main
            ),
        )
        .unwrap();
    };
    let proc = |pid: u32, start: &'static str, cmd: String| (pid, start, argv(&cmd), None);
    report(6001, "601");
    let first = Fake(vec![proc(
        6001,
        "601",
        format!("claude --session-id {KEY}"),
    )]);
    let lines = fx.scan(&first);
    let line = lines.iter().find(|l| l.pid == Some(6001)).unwrap().clone();
    assert_eq!(
        (line.key.as_str(), line.session_id.as_str()),
        (KEY, NOW_SID)
    );
    let lines = fx.scan(&Fake::default());
    let held = lines.iter().find(|l| l.id == line.id).unwrap();
    assert!(held.resume_command.contains(&format!("--resume {NOW_SID}")));

    report(6002, "602");
    let resumed = Fake(vec![proc(
        6002,
        "602",
        format!("claude --resume {NOW_SID}"),
    )]);
    let lines = fx.scan(&resumed);
    let back = lines.iter().find(|l| l.pid == Some(6002)).unwrap();
    assert_eq!(back.id, line.id, "the resumed process is the original line");
    assert_eq!(back.key, KEY);
    assert_eq!(back.session_id, NOW_SID);
    assert!(!back.duplicate);
    assert_eq!(
        lines.iter().filter(|l| l.session_id == NOW_SID).count(),
        1,
        "no new line for the resumed session: {:?}",
        lines
            .iter()
            .map(|l| (&l.id, &l.key, l.pid))
            .collect::<Vec<_>>()
    );

    report(6003, "603");
    let twice = Fake(vec![
        proc(6002, "602", format!("claude --resume {NOW_SID}")),
        proc(6003, "603", format!("claude --resume {NOW_SID}")),
    ]);
    let lines = fx.scan(&twice);
    let back = lines.iter().find(|l| l.pid == Some(6002)).unwrap();
    assert_eq!(back.id, line.id);
    let extra = lines.iter().find(|l| l.pid == Some(6003)).unwrap();
    assert_ne!(extra.id, line.id);
    assert!(back.duplicate && extra.duplicate);
}

#[test]
fn a_vibe_session_lives_on_the_tui_above_its_app_server_lease() {
    let fx = Fx::new();
    let sid = "4a3b2c1d-0e9f-4a8b-b7c6-d5e4f3a2b1c0";
    let root = fx.base.join(".vibe/logs/session");
    let dir = root.join("unified").join(sid);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(root.join("active")).unwrap();
    std::fs::write(dir.join("CURRENT"), "{}").unwrap();
    std::fs::write(
        dir.join("meta.json"),
        serde_json::json!({"session_id": sid, "parent_session_id": null,
            "start_time": "2026-10-28T09:00:00+00:00",
            "environment": {"working_directory": fx.wt_chore}, "title": "Vibe work"})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join(format!("active/{sid}.lock.json")),
        serde_json::json!({"lease_version": 1, "session_id": sid, "process_id": 7101}).to_string(),
    )
    .unwrap();
    let cwd = Some(fx.wt_chore.clone());
    let probe = Tree {
        procs: Fake(vec![
            (
                7100,
                "1",
                argv("/u/.local/share/uv/tools/mistral-vibe/lib/python3.14/site-packages/vibe/_bin/vibe-rs --auto-approve -- fix the bug"),
                cwd.clone(),
            ),
            (
                7101,
                "1",
                argv("/u/.local/share/uv/tools/mistral-vibe/bin/python3 /u/.local/share/uv/tools/mistral-vibe/bin/vibe-app-server"),
                cwd,
            ),
        ]),
        parents: vec![(7101, 7100), (7100, 900)],
        rss: vec![],
    };
    let lines = fx.scan(&probe);
    let l = lines.iter().find(|l| l.session_id == sid).unwrap();
    assert_eq!(l.agent, "vibe");
    assert_eq!(l.pid, Some(7100));
    assert_eq!(l.launch_args, ["--auto-approve"]);
}
