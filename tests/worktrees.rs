use overseer::daemon::Request;
use overseer::procs::{Probe, ProcInfo};
use overseer::registry::Registry;
use overseer::worktrees::{self, New};
use overseer::{gitgraph, Config, Ctx};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};

static INIT: Once = Once::new();
static N: AtomicUsize = AtomicUsize::new(0);

fn tmp() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("wt-{}", std::process::id()))
}

fn setup() {
    INIT.call_once(|| {
        let _ = std::fs::remove_dir_all(tmp());
        std::fs::create_dir_all(tmp().join("home")).unwrap();
        std::fs::create_dir_all(tmp().join("gh")).unwrap();
        let stubs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stubs");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{path}", stubs.display()));
        std::env::set_var("HOME", tmp().join("home"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CEILING_DIRECTORIES", tmp().canonicalize().unwrap());
        std::env::set_var("SB_GH_DIR", tmp().join("gh"));
    });
}

struct Nobody;

impl Probe for Nobody {
    fn info(&self, _: u32) -> Option<ProcInfo> {
        None
    }
    fn list(&self) -> Vec<(u32, Vec<String>)> {
        vec![]
    }
    fn cwd(&self, _: u32) -> Option<String> {
        None
    }
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, file: &str, text: &str, msg: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    run(dir, &["add", "-A"]);
    run(dir, &["commit", "-q", "-m", msg]);
    run(dir, &["rev-parse", "HEAD"])
}

struct Fx {
    base: PathBuf,
    root: PathBuf,
    probe: Nobody,
    fetch: bool,
}

impl Fx {
    fn new(tag: &str) -> Fx {
        setup();
        let base = tmp().join(format!("{tag}-{}", N.fetch_add(1, Ordering::SeqCst)));
        let root = base.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(base.join("state")).unwrap();
        let base = base.canonicalize().unwrap();
        let root = root.canonicalize().unwrap();
        run(&root, &["init", "-q"]);
        for (k, v) in [
            ("user.email", "t@example.com"),
            ("user.name", "t"),
            ("commit.gpgsign", "false"),
        ] {
            run(&root, &["config", k, v]);
        }
        commit(&root, "a.txt", "one\n", "first");
        Fx {
            base,
            root,
            probe: Nobody,
            fetch: false,
        }
    }

    fn r(&self) -> &str {
        self.root.to_str().unwrap()
    }

    fn ctx(&self) -> Ctx<'_> {
        Ctx {
            claude_dir: self.base.join("claude"),
            codex_dir: self.base.join("codex"),
            gemini_dir: self.base.join("gemini"),
            vibe_dir: self.base.join("vibe"),
            state_dir: self.base.join("state"),
            snapshot: self.base.join("snapshot.json"),
            now_ms: overseer::procs::now_ms(),
            probe: &self.probe,
            tmux: None,
            config: Config {
                station_id: "test".into(),
                owner: "t@example.com".into(),
                roots: vec![],
                idle_after_ms: 600_000,
                mcp_hints: vec![],
                accept_remote_commands: false,
                ssh: Default::default(),
                remote_command: "overseer".into(),
                repos: Default::default(),
                worktree_dir: self.base.join("store"),
                start_args: vec![],
                memory_policy: None,
                auto_concurrency: 0,
                auto_organize: false,
                worktree_fetch: self.fetch,
                worktree_stale_days: 14,
                session_tasks_days: 7,
                discover_roots: vec![],
            },
        }
    }

    fn create(&self, n: &New) -> (String, String) {
        let (_, p, b) = worktrees::create(&self.ctx(), self.r(), n).unwrap();
        (p, b)
    }

    fn list(&self) -> Value {
        worktrees::list(&self.ctx(), self.r()).unwrap()
    }

    fn wt(&self, path: &str) -> Value {
        let l = self.list();
        l["worktrees"]
            .as_array()
            .unwrap()
            .iter()
            .chain(l["orphans"].as_array().unwrap())
            .find(|w| w["path"] == path)
            .cloned()
            .unwrap_or_else(|| panic!("{path} not in {l:#}"))
    }

    fn cleanup(&self, path: &str, dry: bool) -> Result<Value, (u16, String)> {
        worktrees::cleanup(&self.ctx(), path, dry, Some(self.r()))
    }

    fn origin(&self) -> PathBuf {
        let remote = self.base.join("remote.git");
        run(
            &self.base,
            &["clone", "-q", "--bare", self.r(), remote.to_str().unwrap()],
        );
        run(
            &self.root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run(&self.root, &["fetch", "-q", "origin"]);
        remote
    }
}

#[test]
fn creates_worktrees_in_the_sibling_store_with_consistent_names() {
    let fx = Fx::new("create");
    let main = run(&fx.root, &["rev-parse", "HEAD"]);
    run(&fx.root, &["switch", "-q", "-c", "feature"]);
    commit(&fx.root, "b.txt", "b\n", "feature work");

    let (p, b) = fx.create(&New {
        agent: Some("claude"),
        slug: Some("Fix the Login!"),
        ..Default::default()
    });
    assert_eq!(b, "sb/claude/fix-the-login");
    let store = worktrees::store(&fx.ctx(), fx.r());
    assert_eq!(
        store.file_name().unwrap().to_str().unwrap(),
        format!("repo-{}", worktrees::fnv(fx.r()))
    );
    assert_eq!(Path::new(&p), store.join("sb-claude-fix-the-login"));
    assert!(
        !p.starts_with(fx.r()),
        "the store is outside the repository"
    );
    assert_eq!(
        run(Path::new(&p), &["rev-parse", "HEAD"]),
        main,
        "base = main, not the checkout"
    );
    assert_eq!(
        run(
            &fx.root,
            &[
                "for-each-ref",
                "--format=%(upstream)",
                &format!("refs/heads/{b}")
            ]
        ),
        "",
        "no upstream"
    );

    let (_, b2) = fx.create(&New {
        agent: Some("claude"),
        slug: Some("fix the login"),
        ..Default::default()
    });
    assert_eq!(b2, "sb/claude/fix-the-login-2");

    let (_, b3) = fx.create(&New {
        agent: Some("codex"),
        ..Default::default()
    });
    let re = b3.strip_prefix("sb/codex/").unwrap();
    assert!(
        re.len() == 9
            && re.as_bytes()[4] == b'-'
            && re.replace('-', "").bytes().all(|c| c.is_ascii_digit()),
        "{b3}"
    );

    let (p4, b4) = fx.create(&New {
        base: Some(&main[..10]),
        ..Default::default()
    });
    assert_eq!(
        b4,
        format!("sb/{}", run(&fx.root, &["rev-parse", "--short", &main]))
    );
    assert_eq!(run(Path::new(&p4), &["rev-parse", "HEAD"]), main);

    let (p5, b5) = fx.create(&New {
        branch: Some("task/7-fix-login"),
        ..Default::default()
    });
    assert_eq!(b5, "task/7-fix-login");
    assert!(p5.ends_with("/task-7-fix-login"));

    run(&fx.root, &["branch", "existing", &main]);
    let (p6, _) = fx.create(&New {
        branch: Some("existing"),
        ..Default::default()
    });
    assert_eq!(
        run(Path::new(&p6), &["symbolic-ref", "--short", "HEAD"]),
        "existing"
    );

    let e = worktrees::create(
        &fx.ctx(),
        fx.r(),
        &New {
            branch: Some("feature"),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(
        e.0, 409,
        "feature is checked out in the main worktree: {}",
        e.1
    );
    for bad in ["bad..name", "-x", "a b"] {
        let e = worktrees::create(
            &fx.ctx(),
            fx.r(),
            &New {
                branch: Some(bad),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(e.0, 400, "{bad}: {}", e.1);
    }
    let e = worktrees::create(
        &fx.ctx(),
        fx.r(),
        &New {
            base: Some("nope"),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.0, 400);
    assert_eq!(
        worktrees::create(&fx.ctx(), fx.base.to_str().unwrap(), &New::default())
            .unwrap_err()
            .0,
        400
    );
}

#[test]
fn base_follows_the_default_branch_and_the_remote() {
    let mut fx = Fx::new("base");
    fx.fetch = true;
    assert_eq!(worktrees::default_branch(fx.r()), "main");
    let other = Fx::new("trunk");
    run(&other.root, &["branch", "-m", "trunk"]);
    assert_eq!(worktrees::default_branch(other.r()), "trunk");

    let remote = fx.origin();
    run(&fx.root, &["remote", "set-head", "origin", "main"]);
    assert_eq!(worktrees::default_branch(fx.r()), "main");
    assert_eq!(worktrees::start_point(fx.r(), "main"), "main");

    let up = fx.base.join("up");
    run(
        &fx.base,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            up.to_str().unwrap(),
        ],
    );
    for (k, v) in [("user.email", "u@example.com"), ("user.name", "u")] {
        run(&up, &["config", k, v]);
    }
    let ahead = commit(&up, "r.txt", "r\n", "remote work");
    run(&up, &["push", "-q", "origin", "main"]);

    let (p, _) = fx.create(&New::default());
    assert_eq!(
        run(Path::new(&p), &["rev-parse", "HEAD"]),
        ahead,
        "the fetch brought origin/main and the worktree starts from it"
    );
    let branch = run(Path::new(&p), &["symbolic-ref", "--short", "HEAD"]);
    assert_eq!(
        run(
            Path::new(&p),
            &["config", &format!("branch.{branch}.overseer-base")]
        ),
        "origin/main"
    );
    assert_eq!(
        overseer::review::diff::created_from(&p, &branch).as_deref(),
        Some("origin/main")
    );

    commit(&fx.root, "l.txt", "l\n", "local only");
    run(&fx.root, &["merge", "-q", "--no-edit", "origin/main"]);
    assert_eq!(worktrees::start_point(fx.r(), "main"), "main");
}

fn set_task(fx: &Fx, path: &str, status: &str) {
    let ctx = fx.ctx();
    let t = overseer::tasks::create(&ctx, &json!({"title": "Busy", "project": fx.r()})).unwrap();
    let reg = Registry::open(&ctx.state_dir).unwrap();
    reg.conn
        .execute(
            "UPDATE tasks SET status = ?2, worktree_path = ?3, branch = 'x' WHERE id = ?1",
            rusqlite::params![t.id, status, path],
        )
        .unwrap();
}

#[test]
fn cleanup_never_loses_work() {
    let fx = Fx::new("cleanup");
    std::fs::write(fx.root.join(".gitignore"), "target/\n").unwrap();
    commit(&fx.root, ".gitignore", "target/\n", "ignore target");

    let main = fx.cleanup(fx.r(), true).unwrap();
    assert!(main["refused"].as_str().unwrap().contains("main checkout"));

    let (fresh, fb) = fx.create(&New {
        slug: Some("fresh"),
        ..Default::default()
    });
    let w = fx.wt(&fresh);
    assert_eq!(
        (
            w["kind"].as_str(),
            w["state"].as_str(),
            w["dirty"].as_bool(),
            w["merged"].as_bool()
        ),
        (Some("managed"), Some("idle"), Some(false), Some(false))
    );
    std::fs::create_dir_all(Path::new(&fresh).join("target")).unwrap();
    std::fs::write(Path::new(&fresh).join("target/big.o"), "x").unwrap();
    std::fs::write(Path::new(&fresh).join("notes.md"), "draft").unwrap();
    let w = fx.wt(&fresh);
    assert_eq!(w["changes"], 1, "ignored files do not count: {w}");
    let p = fx.cleanup(&fresh, true).unwrap();
    assert_eq!(p["refused"], "1 uncommitted change(s)");
    assert_eq!(fx.cleanup(&fresh, false).unwrap_err().0, 409);
    assert!(Path::new(&fresh).join("notes.md").exists());
    std::fs::remove_file(Path::new(&fresh).join("notes.md")).unwrap();
    let p = fx.cleanup(&fresh, true).unwrap();
    assert!(p["refused"].is_null(), "{p}");
    assert_eq!(
        p["remove"],
        json!([{"kind": "worktree", "path": fresh}, {"kind": "branch", "name": fb}])
    );
    let done = fx.cleanup(&fresh, false).unwrap();
    assert_eq!(done["dryRun"], false);
    assert!(
        !Path::new(&fresh).exists(),
        "ignored files go with the worktree"
    );
    assert!(run(&fx.root, &["branch", "--list", &fb]).is_empty());

    let (work, wb) = fx.create(&New {
        slug: Some("work"),
        ..Default::default()
    });
    commit(Path::new(&work), "w.txt", "w\n", "unpushed work");
    let w = fx.wt(&work);
    assert_eq!(
        (w["ahead"].as_u64(), w["unpushed"].as_u64()),
        (Some(1), Some(1))
    );
    let p = fx.cleanup(&work, true).unwrap();
    assert!(p["refused"]
        .as_str()
        .unwrap()
        .contains("1 commit(s) neither pushed nor merged into main"));
    assert_eq!(fx.cleanup(&work, false).unwrap_err().0, 409);

    run(&fx.root, &["merge", "-q", "--no-ff", "--no-edit", &wb]);
    let w = fx.wt(&work);
    assert_eq!(
        (
            w["merged"].as_bool(),
            w["unpushed"].as_u64(),
            w["state"].as_str()
        ),
        (Some(true), Some(0), Some("stale"))
    );
    fx.cleanup(&work, false).unwrap();
    assert!(!Path::new(&work).exists());
    assert!(run(&fx.root, &["branch", "--list", &wb]).is_empty());

    fx.origin();
    let (pushed, pb) = fx.create(&New {
        slug: Some("pushed"),
        ..Default::default()
    });
    commit(Path::new(&pushed), "p.txt", "p\n", "pushed work");
    run(Path::new(&pushed), &["push", "-q", "origin", &pb]);
    let w = fx.wt(&pushed);
    assert_eq!(
        (w["pushed"].as_bool(), w["unpushed"].as_u64()),
        (Some(true), Some(0))
    );
    let p = fx.cleanup(&pushed, false).unwrap();
    assert_eq!(p["keep"][0]["reason"], "not merged into main");
    assert!(!Path::new(&pushed).exists());
    assert_eq!(
        run(&fx.root, &["branch", "--list", &pb]).trim(),
        pb,
        "the branch is kept"
    );

    let (busy, _) = fx.create(&New {
        slug: Some("busy"),
        ..Default::default()
    });
    set_task(&fx, &busy, "in_progress");
    let w = fx.wt(&busy);
    assert_eq!(
        (w["state"].as_str(), w["task"]["number"].as_i64()),
        (Some("in_use"), Some(1))
    );
    assert!(fx.cleanup(&busy, true).unwrap()["refused"]
        .as_str()
        .unwrap()
        .contains("task in progress"));

    let (locked, _) = fx.create(&New {
        slug: Some("locked"),
        ..Default::default()
    });
    run(&fx.root, &["worktree", "lock", &locked]);
    assert!(fx.cleanup(&locked, true).unwrap()["refused"]
        .as_str()
        .unwrap()
        .contains("locked"));

    let (gone, _) = fx.create(&New {
        slug: Some("gone"),
        ..Default::default()
    });
    std::fs::remove_dir_all(&gone).unwrap();
    assert_eq!(fx.wt(&gone)["state"], "missing");
    let p = fx.cleanup(&gone, false).unwrap();
    assert_eq!(p["remove"][0]["kind"], "prune");
    assert!(!run(&fx.root, &["worktree", "list"]).contains(&gone));
}

#[test]
fn orphans_are_listed_and_never_touched() {
    let fx = Fx::new("orphan");
    let outside = fx.base.join("outside");
    run(
        &fx.root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "ext",
            outside.to_str().unwrap(),
        ],
    );
    let l = fx.list();
    let o = outside.to_str().unwrap();
    assert_eq!(l["orphans"][0]["path"], o);
    assert_eq!(l["orphans"][0]["kind"], "orphan");
    assert_eq!(l["worktrees"].as_array().unwrap().len(), 1);
    assert_eq!(l["worktrees"][0]["kind"], "main");
    let p = fx.cleanup(o, true).unwrap();
    assert!(p["refused"].as_str().unwrap().contains("outside Overseer"));
    assert_eq!(fx.cleanup(o, false).unwrap_err().0, 409);
    std::fs::remove_dir_all(&outside).unwrap();
    assert_eq!(
        fx.cleanup(o, false).unwrap_err().0,
        409,
        "even when missing"
    );
    assert!(run(&fx.root, &["worktree", "list", "--porcelain"]).contains(o));
    assert_eq!(
        worktrees::cleanup(&fx.ctx(), "/nonexistent/x", true, None)
            .unwrap_err()
            .0,
        404
    );
}

#[test]
fn squash_merged_pr_counts_as_merged() {
    let fx = Fx::new("squash");
    fx.origin();
    run(
        &fx.root,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:Acme/squash.git",
        ],
    );
    let remote = fx.base.join("remote.git");
    run(
        &fx.root,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            remote.to_str().unwrap(),
        ],
    );
    let (p, b) = fx.create(&New {
        slug: Some("squashed"),
        ..Default::default()
    });
    commit(Path::new(&p), "s.txt", "s\n", "to squash");
    run(
        Path::new(&p),
        &["push", "-q", "origin", &format!("{b}:{b}")],
    );
    run(
        &fx.root,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{b}"),
            &format!("refs/heads/{b}"),
        ],
    );
    let key: String = format!("github.com/Acme/squash {b}")
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    std::fs::write(
        tmp().join("gh").join(format!("{key}.json")),
        r#"[{"number":5,"url":"https://github.com/Acme/squash/pull/5","state":"MERGED","isDraft":false,"title":"x","statusCheckRollup":[]}]"#,
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(20);
    let w = loop {
        let w = fx.wt(&p);
        if w["pr"]["state"] == "MERGED" || Instant::now() > end {
            break w;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(
        (w["merged"].as_bool(), w["pr"]["number"].as_u64()),
        (Some(true), Some(5)),
        "{w}"
    );
    let done = fx.cleanup(&p, false).unwrap();
    assert_eq!(done["remove"][1]["name"], b.as_str());
    assert!(run(&fx.root, &["branch", "--list", &b]).is_empty());
}

#[test]
fn merge_base_and_cherry_pick_abort_on_conflicts() {
    let fx = Fx::new("merge");
    let (p, _) = fx.create(&New {
        slug: Some("m"),
        ..Default::default()
    });
    let pick = commit(&fx.root, "c.txt", "c\n", "on main");
    assert_eq!(fx.wt(&p)["behind"], 1);
    let w = worktrees::merge_base(&fx.ctx(), &p).unwrap();
    assert_eq!(w["behind"], 0);

    let (q, _) = fx.create(&New {
        base: Some("HEAD~1"),
        slug: Some("q"),
        ..Default::default()
    });
    let r = worktrees::cherry_pick(&fx.ctx(), &q, &pick).unwrap();
    let msg = run(Path::new(&q), &["log", "-1", "--format=%B"]);
    assert!(
        msg.contains(&format!("cherry picked from commit {pick}")),
        "{msg}"
    );
    assert_eq!(r["worktree"]["path"], q.as_str());
    assert_eq!(
        worktrees::cherry_pick(&fx.ctx(), &q, "zz").unwrap_err().0,
        400
    );

    commit(Path::new(&p), "a.txt", "mine\n", "wt side");
    commit(&fx.root, "a.txt", "theirs\n", "main side");
    let e = worktrees::merge_base(&fx.ctx(), &p).unwrap_err();
    assert_eq!(e.0, 409);
    assert!(e.1.contains("a.txt"), "{}", e.1);
    assert!(
        run(Path::new(&p), &["status", "--porcelain"]).is_empty(),
        "merge aborted"
    );
    let theirs = run(&fx.root, &["rev-parse", "HEAD"]);
    let e = worktrees::cherry_pick(&fx.ctx(), &p, &theirs).unwrap_err();
    assert_eq!(e.0, 409);
    assert!(
        run(Path::new(&p), &["status", "--porcelain"]).is_empty(),
        "cherry-pick aborted"
    );

    std::fs::write(Path::new(&p).join("dirty.txt"), "x").unwrap();
    let e = worktrees::merge_base(&fx.ctx(), &p).unwrap_err();
    assert!(e.1.contains("uncommitted"), "{}", e.1);
}

fn req(query: &[(&str, &str)]) -> Request {
    Request {
        method: "GET".into(),
        path: String::new(),
        query: query
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        headers: vec![],
        body: vec![],
    }
}

fn shas(v: &Value) -> Vec<String> {
    v["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["sha"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn graph_lists_histories_children_first_with_refs_and_pages() {
    let fx = Fx::new("graph");
    let d = &fx.root;
    let base = run(d, &["rev-parse", "HEAD"]);
    commit(d, "l.txt", "2\n", "linear two");
    run(d, &["switch", "-q", "-c", "feat/AS-12-x"]);
    commit(d, "f.txt", "f\n", "feature one");
    run(d, &["switch", "-q", "main"]);
    commit(d, "m.txt", "m\n", "main moves");
    run(d, &["merge", "-q", "--no-ff", "--no-edit", "feat/AS-12-x"]);
    for b in ["o1", "o2", "o3"] {
        run(d, &["switch", "-q", "-c", b, "main"]);
        commit(d, &format!("{b}.txt"), "o\n", b);
    }
    run(d, &["switch", "-q", "main"]);
    commit(d, "pre.txt", "p\n", "before the octopus");
    run(d, &["merge", "-q", "--no-edit", "o1", "o2", "o3"]);
    run(d, &["switch", "-q", "-c", "cross-a", &base]);
    commit(d, "ca.txt", "a\n", "cross a");
    run(d, &["switch", "-q", "-c", "cross-b", &base]);
    commit(d, "cb.txt", "b\n", "cross b");
    run(d, &["merge", "-q", "--no-edit", "cross-a"]);
    run(d, &["switch", "-q", "cross-a"]);
    run(d, &["merge", "-q", "--no-edit", "cross-b~1"]);
    run(d, &["switch", "-q", "main"]);
    run(d, &["tag", "-a", "v1", "-m", "v1"]);
    fx.origin();
    set_task(&fx, fx.r(), "pending");
    let reg = Registry::open(&fx.ctx().state_dir).unwrap();
    reg.conn
        .execute("UPDATE tasks SET branch = 'feat/AS-12-x', number = 9", [])
        .unwrap();

    let ctx = fx.ctx();
    let g = gitgraph::graph(&ctx, fx.r(), &req(&[("limit", "500")])).unwrap();
    let commits = g["commits"].as_array().unwrap();
    let order = shas(&g);
    for (i, c) in commits.iter().enumerate() {
        for p in c["parents"].as_array().unwrap() {
            let j = order.iter().position(|s| s == p.as_str().unwrap()).unwrap();
            assert!(j > i, "parent after child");
        }
    }
    let parents = |subject: &str| {
        commits
            .iter()
            .find(|c| c["subject"].as_str().unwrap().starts_with(subject))
            .unwrap()["parents"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!(parents("first"), 0);
    assert_eq!(parents("linear two"), 1);
    assert_eq!(parents("Merge branch 'feat/AS-12-x'"), 2);
    assert_eq!(parents("Merge branches 'o1', 'o2' and 'o3'"), 4);
    assert_eq!(
        commits.len().to_string(),
        run(d, &["rev-list", "--all", "--count"])
    );
    assert_eq!(g["head"]["branch"], "main");
    assert_eq!(g["base"], "main");
    let kinds = |k: &str| {
        g["refs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == k)
            .count()
    };
    assert_eq!((kinds("branch"), kinds("tag")), (7, 1));
    assert!(kinds("remote") >= 7);
    let tag = g["refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "tag")
        .unwrap();
    assert_eq!(tag["sha"], g["head"]["sha"], "annotated tags are peeled");
    let main = g["refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "main")
        .unwrap();
    assert_eq!(main["worktree"], fx.r());
    let ov = g["overlays"].as_array().unwrap();
    assert_eq!(ov.len(), 1, "{ov:?}");
    assert_eq!(
        (
            ov[0]["branch"].as_str(),
            ov[0]["taskNumber"].as_i64(),
            ov[0]["ticketKey"].as_str()
        ),
        (Some("feat/AS-12-x"), Some(9), Some("AS-12"))
    );

    let mut paged = vec![];
    let mut before: Option<String> = None;
    loop {
        let mut q = vec![("limit", "3")];
        if let Some(b) = &before {
            q.push(("before", b));
        }
        let p = gitgraph::graph(&ctx, fx.r(), &req(&q)).unwrap();
        paged.extend(shas(&p));
        match p["next"].as_str() {
            Some(n) => before = Some(n.to_string()),
            None => break,
        }
    }
    assert_eq!(paged, order, "pages concatenate to the full history");

    let cur = gitgraph::graph(&ctx, fx.r(), &req(&[("refs", "current")])).unwrap();
    assert!(!shas(&cur)
        .iter()
        .any(|s| *s == run(d, &["rev-parse", "cross-a"])));
    let some = gitgraph::graph(&ctx, fx.r(), &req(&[("refs", "cross-b,nope,--all")])).unwrap();
    assert_eq!(shas(&some)[0], run(d, &["rev-parse", "cross-b"]));
    assert_eq!(
        gitgraph::graph(&ctx, fx.r(), &req(&[("limit", "x")]))
            .unwrap_err()
            .0,
        400
    );
}

#[test]
fn commit_detail_files_diff_and_branches() {
    let fx = Fx::new("detail");
    let d = &fx.root;
    let root_sha = run(d, &["rev-list", "--max-parents=0", "HEAD"]);
    let c = gitgraph::commit(fx.r(), &root_sha).unwrap();
    assert_eq!(
        c["files"],
        json!([{"path": "a.txt", "status": "A", "added": 1, "removed": 0, "binary": false}])
    );

    std::fs::write(d.join("long.txt"), "line\n".repeat(50)).unwrap();
    commit(d, "a.txt", "one\ntwo\n", "add long");
    run(d, &["mv", "long.txt", "renamed.txt"]);
    std::fs::write(d.join("bin.dat"), [0u8, 1, 2, 0, 255]).unwrap();
    run(d, &["add", "-A"]);
    run(d, &["commit", "-q", "-m", "Rename\n\nWith a body."]);
    let sha = run(d, &["rev-parse", "HEAD"]);
    let c = gitgraph::commit(fx.r(), &sha[..8]).unwrap();
    assert_eq!(c["sha"], sha.as_str());
    assert_eq!(
        (c["subject"].as_str(), c["body"].as_str()),
        (Some("Rename"), Some("With a body."))
    );
    let files = c["files"].as_array().unwrap();
    let ren = files.iter().find(|f| f["status"] == "R").unwrap();
    assert_eq!(
        (ren["oldPath"].as_str(), ren["path"].as_str()),
        (Some("long.txt"), Some("renamed.txt"))
    );
    assert_eq!(
        files.iter().find(|f| f["path"] == "bin.dat").unwrap()["binary"],
        true
    );
    assert!(c["diff"]
        .as_str()
        .unwrap()
        .contains("rename to renamed.txt"));
    assert_eq!(c["refs"], json!(["main"]));

    std::fs::write(d.join("huge.txt"), "0123456789abcdef\n".repeat(70_000)).unwrap();
    run(d, &["add", "-A"]);
    run(d, &["commit", "-q", "-m", "huge"]);
    let c = gitgraph::commit(fx.r(), &run(d, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(c["truncated"], true);
    assert!(c["diff"].as_str().unwrap().len() <= 1 << 20);

    assert_eq!(gitgraph::commit(fx.r(), "xyz").unwrap_err().0, 400);
    assert_eq!(
        gitgraph::commit(fx.r(), "deadbeefdeadbeef").unwrap_err().0,
        404
    );

    let b = gitgraph::branch(fx.r(), &json!({"name": "from-root", "sha": root_sha})).unwrap();
    assert_eq!(b["sha"], root_sha.as_str());
    assert_eq!(run(d, &["rev-parse", "from-root"]), root_sha);
    let dup = gitgraph::branch(fx.r(), &json!({"name": "from-root", "sha": root_sha}));
    assert_eq!(dup.unwrap_err().0, 409);
    let bad = gitgraph::branch(fx.r(), &json!({"name": "a..b", "sha": root_sha}));
    assert_eq!(bad.unwrap_err().0, 400);
}

#[test]
fn ref_watcher_sees_commits_checkouts_and_new_branches() {
    let fx = Fx::new("watch");
    gitgraph::touch(fx.r());
    let mine = |v: Vec<String>| v.iter().any(|p| p == fx.r());
    assert!(!mine(gitgraph::poll()));
    std::thread::sleep(Duration::from_millis(20));
    commit(&fx.root, "w.txt", "w\n", "watch me");
    assert!(mine(gitgraph::poll()));
    assert!(!mine(gitgraph::poll()));
    let (p, _) = fx.create(&New {
        slug: Some("w"),
        ..Default::default()
    });
    assert!(mine(gitgraph::poll()));
    std::thread::sleep(Duration::from_millis(20));
    run(Path::new(&p), &["switch", "-q", "--detach"]);
    assert!(mine(gitgraph::poll()), "a worktree HEAD moved");
    run(&fx.root, &["pack-refs", "--all"]);
    assert!(mine(gitgraph::poll()));
}
