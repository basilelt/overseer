use overseer::github;
use overseer::procs::{Probe, ProcInfo};
use overseer::{Config, Ctx};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

static INIT: Once = Once::new();
static N: AtomicUsize = AtomicUsize::new(0);

fn tmp() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("gh-{}", std::process::id()))
}

fn setup() -> PathBuf {
    INIT.call_once(|| {
        let _ = std::fs::remove_dir_all(tmp());
        std::fs::create_dir_all(tmp().join("home")).unwrap();
        std::env::set_var("HOME", tmp().join("home"));
        std::env::set_var("GIT_CONFIG_GLOBAL", "/dev/null");
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CEILING_DIRECTORIES", tmp().canonicalize().unwrap());
    });
    let d = tmp().join(format!("t{}", N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
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

fn ctx<'a>(base: &Path, probe: &'a Nobody) -> Ctx<'a> {
    Ctx {
        claude_dir: base.join("claude"),
        codex_dir: base.join("codex"),
        gemini_dir: base.join("gemini"),
        state_dir: base.join("state"),
        snapshot: base.join("snapshot.json"),
        now_ms: overseer::procs::now_ms(),
        probe,
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
            worktree_dir: base.join("store"),
            start_args: vec![],
            memory_policy: None,
            auto_concurrency: 0,
            auto_organize: false,
            worktree_fetch: false,
            worktree_stale_days: 14,
            session_tasks_days: 7,
            discover_roots: vec![],
        },
    }
}

fn script(dir: &Path, body: &str) -> String {
    let p = dir.join("gh");
    let src = dir.join("gh.src");
    std::fs::write(&src, format!("#!/bin/sh\n{body}\n")).unwrap();
    // Written by a child process: a write fd held here leaks into git children forked by
    // parallel tests, and exec of the script then fails with ETXTBSY ("Text file busy").
    assert!(Command::new("cp")
        .arg(&src)
        .arg(&p)
        .status()
        .unwrap()
        .success());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
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
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn origin(base: &Path) -> (PathBuf, String) {
    let src = base.join("src");
    std::fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "-q"]);
    git(&src, &["config", "user.email", "t@example.com"]);
    git(&src, &["config", "user.name", "t"]);
    std::fs::write(src.join("a.txt"), "one\n").unwrap();
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-q", "-m", "first"]);
    git(&src, &["switch", "-q", "-c", "feature"]);
    std::fs::write(src.join("b.txt"), "two\n").unwrap();
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-q", "-m", "feature"]);
    let head = git(&src, &["rev-parse", "HEAD"]);
    git(&src, &["switch", "-q", "main"]);
    let bare = base.join("origin.git");
    git(
        base,
        &[
            "clone",
            "-q",
            "--bare",
            src.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    git(&bare, &["update-ref", "refs/pull/7/head", &head]);
    git(&bare, &["branch", "-D", "feature"]);
    (bare, head)
}

const SEARCH: &str = r#"{"data":{"viewer":{"login":"me"},"search":{"nodes":[
 {"number":7,"title":"Fix login","url":"https://github.com/acme/api/pull/7","isDraft":false,"updatedAt":"2026-10-29T10:00:00Z","headRefName":"fix/login","baseRefName":"main","additions":12,"deletions":3,"changedFiles":2,"reviewDecision":"REVIEW_REQUIRED","author":{"login":"bob"},"repository":{"nameWithOwner":"acme/api"},"reviews":{"nodes":[{"author":{"login":"me"},"state":"COMMENTED"},{"author":{"login":"me"},"state":"PENDING"}]}},
 {"number":9,"title":"WIP","url":"https://github.com/acme/web/pull/9","isDraft":true,"updatedAt":"2026-10-28T10:00:00Z","headRefName":"wip","baseRefName":"main","additions":1,"deletions":0,"changedFiles":1,"reviewDecision":null,"author":{"login":"eve"},"repository":{"nameWithOwner":"acme/web"},"reviews":{"nodes":[{"author":{"login":"bob"},"state":"APPROVED"}]}},
 {}
]}}}"#;

#[test]
fn lists_review_requests_from_gh_graphql() {
    let d = setup();
    std::fs::write(d.join("out.json"), SEARCH).unwrap();
    let gh = script(
        &d,
        &format!(
            "[ \"$1 $2\" = \"api graphql\" ] || exit 9\necho \"$@\" > {log}\ncat {out}",
            log = d.join("args").display(),
            out = d.join("out.json").display()
        ),
    );
    let v = github::fetch(&gh).unwrap();
    let args = std::fs::read_to_string(d.join("args")).unwrap();
    assert!(args.contains("review-requested:@me"), "{args}");
    assert_eq!(v["login"], "me");
    let prs = v["prs"].as_array().unwrap();
    assert_eq!(prs.len(), 2);
    assert_eq!(prs[0]["repo"], "acme/api");
    assert_eq!(prs[0]["author"], "bob");
    assert_eq!(prs[0]["reviewed"], true);
    assert_eq!(prs[0]["myState"], "COMMENTED");
    assert_eq!(prs[0]["additions"], 12);
    assert_eq!(prs[1]["isDraft"], true);
    assert_eq!(prs[1]["reviewed"], false);
    assert_eq!(prs[1]["reviewDecision"], json!(null));
}

#[test]
fn reports_missing_and_logged_out_gh() {
    let d = setup();
    let e = github::fetch(d.join("nope").to_str().unwrap()).unwrap_err();
    assert_eq!((e.0, e.2), (424, "missing"));
    let gh = script(
        &d,
        "echo 'To get started with GitHub CLI, please run:  gh auth login' >&2\nexit 4",
    );
    let e = github::fetch(&gh).unwrap_err();
    assert_eq!((e.0, e.2), (424, "auth"), "{}", e.1);
    let gh = script(
        &d,
        "[ \"$1\" = auth ] && exit 0\necho 'HTTP 502: Bad gateway' >&2\nexit 1",
    );
    let e = github::fetch(&gh).unwrap_err();
    assert_eq!((e.0, e.2), (502, "failed"));
    assert!(e.1.contains("Bad gateway"), "{}", e.1);
}

#[test]
fn offline_with_a_stored_token_is_a_network_error_not_a_logout() {
    let d = setup();
    let gh = script(
        &d,
        "case \"$1 $2\" in\n\"auth status\") echo 'The token in ~/.config/gh/hosts.yml is invalid' >&2; exit 1;;\n\"auth token\") echo gho_x; exit 0;;\nesac\necho 'Post \"https://api.github.com/graphql\": proxyconnect tcp: connection refused' >&2\nexit 1",
    );
    let e = github::fetch(&gh).unwrap_err();
    assert_eq!((e.0, e.2), (502, "failed"), "{}", e.1);
    assert!(e.1.starts_with("GitHub could not be reached"), "{}", e.1);
    assert!(e.1.contains("connection refused"), "{}", e.1);
    assert!(!e.1.contains("not logged in"), "{}", e.1);
}

#[test]
fn logged_out_gh_is_still_reported_as_auth() {
    let d = setup();
    let gh = script(&d, "echo 'no oauth token found' >&2\nexit 1");
    let e = github::fetch(&gh).unwrap_err();
    assert_eq!((e.0, e.2), (424, "auth"), "{}", e.1);
    assert_eq!(e.1, "gh is not logged in on this host");
}

#[test]
fn clones_and_checks_out_the_pr_head_in_a_worktree() {
    let d = setup();
    let (bare, head) = origin(&d);
    let gh = script(
        &d,
        &format!(
            "[ \"$1 $2\" = \"repo clone\" ] || exit 9\nexec git clone -q {} \"$4\"",
            bare.display()
        ),
    );
    let probe = Nobody;
    let c = ctx(&d, &probe);
    let clones = d.join("reviews");
    let p = github::prepare(&c, &gh, &clones, None, "acme/api", 7).unwrap();
    assert_eq!(
        Path::new(&p.root),
        clones.join("acme/api").canonicalize().unwrap()
    );
    assert_eq!(p.branch, "review/pr-7");
    assert!(p.worktree.starts_with(d.join("store").to_str().unwrap()));
    assert_eq!(git(Path::new(&p.worktree), &["rev-parse", "HEAD"]), head);
    assert!(Path::new(&p.worktree).join("b.txt").exists());

    let again = github::prepare(&c, &gh, &clones, None, "acme/api", 7).unwrap();
    assert_eq!(again.worktree, p.worktree);

    git(
        Path::new(&p.root),
        &["worktree", "remove", "--force", &p.worktree],
    );
    git(Path::new(&p.root), &["branch", "-f", "review/pr-7", "main"]);
    let third = github::prepare(&c, &gh, &clones, None, "acme/api", 7).unwrap();
    assert_eq!(
        git(Path::new(&third.worktree), &["rev-parse", "HEAD"]),
        head
    );
}

#[test]
fn reuses_a_known_local_clone() {
    let d = setup();
    let (bare, head) = origin(&d);
    let local = d.join("local");
    git(
        &d,
        &[
            "clone",
            "-q",
            bare.to_str().unwrap(),
            local.to_str().unwrap(),
        ],
    );
    let gh = script(&d, "exit 9");
    let probe = Nobody;
    let c = ctx(&d, &probe);
    let p = github::prepare(
        &c,
        &gh,
        &d.join("reviews"),
        Some(local.to_string_lossy().into_owned()),
        "acme/api",
        7,
    )
    .unwrap();
    assert_eq!(Path::new(&p.root), local);
    assert!(!d.join("reviews").exists());
    assert_eq!(git(Path::new(&p.worktree), &["rev-parse", "HEAD"]), head);
}

#[test]
fn a_failed_clone_leaves_nothing_that_looks_like_a_repo() {
    let d = setup();
    let (bare, head) = origin(&d);
    let bad = script(&d, "mkdir -p \"$4/.git\"\nexit 1");
    let probe = Nobody;
    let c = ctx(&d, &probe);
    let clones = d.join("reviews");
    assert!(github::prepare(&c, &bad, &clones, None, "acme/api", 7).is_err());
    assert!(!clones.join("acme/api").exists());
    assert!(!clones.join("acme/api.partial").exists());
    let gh = script(
        &d,
        &format!(
            "[ \"$1 $2\" = \"repo clone\" ] || exit 9\nexec git clone -q {} \"$4\"",
            bare.display()
        ),
    );
    let p = github::prepare(&c, &gh, &clones, None, "acme/api", 7).unwrap();
    assert_eq!(git(Path::new(&p.worktree), &["rev-parse", "HEAD"]), head);
    assert!(!clones.join("acme/api.partial").exists());
}
