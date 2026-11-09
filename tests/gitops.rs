mod common;

use common::{git, Fx, Reap};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn js(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

struct G {
    fx: Fx,
    _d: Reap,
    work: PathBuf,
    other: PathBuf,
}

fn ident(dir: &Path) {
    git(dir, &["config", "user.email", "dev@example.com"]);
    git(dir, &["config", "user.name", "Dev"]);
}

fn write(dir: &Path, f: &str, text: &str) {
    std::fs::write(dir.join(f), text).unwrap();
}

fn commit(dir: &Path, f: &str, text: &str, msg: &str) -> String {
    write(dir, f, text);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

fn setup(tag: &str) -> G {
    let fx = Fx::new(tag, 0);
    let origin = fx.base.join("origin.git");
    git(
        &fx.base,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    let work = fx.base.join("work");
    let other = fx.base.join("other");
    for d in [&work, &other] {
        git(
            &fx.base,
            &["clone", "-q", origin.to_str().unwrap(), d.to_str().unwrap()],
        );
        ident(d);
    }
    git(&work, &["checkout", "-q", "-b", "main"]);
    commit(&work, "a.txt", "one\n", "init");
    write(&work, "r.txt", "rename me\n");
    write(&work, "b.txt", "bee\n");
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "more"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    git(&other, &["fetch", "-q"]);
    git(&other, &["checkout", "-q", "-B", "main", "origin/main"]);
    let d = fx.daemon(false);
    G {
        fx,
        _d: d,
        work,
        other,
    }
}

impl G {
    fn wt(&self) -> String {
        self.work.to_str().unwrap().to_string()
    }

    fn status_of(&self, wt: &str) -> (u16, Value) {
        let (c, b) = self
            .fx
            .req("GET", &format!("/v1/git/status?worktree={}", enc(wt)), "");
        (c, js(&b))
    }

    fn status(&self) -> Value {
        let (c, v) = self.status_of(&self.wt());
        assert_eq!(c, 200, "{v}");
        assert!(v.get("error").is_none(), "{v}");
        v
    }

    fn post_to(&self, wt: &str, route: &str, extra: Value) -> (u16, Value) {
        let mut body = json!({ "worktree": wt });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        let (c, b) = self
            .fx
            .req("POST", &format!("/v1/git/{route}"), &body.to_string());
        (c, js(&b))
    }

    fn post(&self, route: &str, extra: Value) -> (u16, Value) {
        self.post_to(&self.wt(), route, extra)
    }

    fn ok(&self, route: &str, extra: Value) -> Value {
        let (c, v) = self.post(route, extra);
        assert_eq!(c, 200, "{route}: {v}");
        assert!(v.get("error").is_none(), "{route}: {v}");
        v
    }

    fn fails(&self, route: &str, extra: Value, code: u16, err: &str) {
        let (c, v) = self.post(route, extra);
        assert_eq!(
            (c, v["error"].as_str().unwrap_or_default()),
            (code, err),
            "{route}: {v}"
        );
    }
}

fn paths(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}{}",
                f["status"].as_str().unwrap(),
                f["path"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn status_lists_staged_changes_untracked_and_renames() {
    let g = setup("gitops-status");
    write(&g.work, "a.txt", "two\n");
    git(&g.work, &["add", "a.txt"]);
    write(&g.work, "b.txt", "changed\n");
    write(&g.work, "n.txt", "new\n");
    git(&g.work, &["mv", "r.txt", "r2.txt"]);
    let s = g.status();
    assert_eq!(s["worktree"], json!(g.wt()));
    assert_eq!(s["project"], json!(g.wt()));
    assert_eq!(s["branch"], "main");
    assert_eq!(s["upstream"], "origin/main");
    assert_eq!(s["base"], "main");
    assert_eq!(s["operation"], Value::Null);
    assert_eq!(s["stashes"], 0);
    assert_eq!(s["branches"], json!(["main"]));
    assert_eq!(paths(&s["staged"]), vec!["Ma.txt", "Rr2.txt"]);
    assert_eq!(s["staged"][1]["oldPath"], "r.txt");
    assert_eq!(paths(&s["changes"]), vec!["Mb.txt", "?n.txt"]);
    assert_eq!(s["conflicts"], json!([]));
    assert_eq!(
        (s["ahead"].as_u64(), s["behind"].as_u64()),
        (Some(0), Some(0))
    );
}

#[test]
fn stage_unstage_and_discard_files() {
    let g = setup("gitops-index");
    write(&g.work, "b.txt", "changed\n");
    write(&g.work, "n.txt", "new\n");
    let v = g.ok("stage", json!({"files": ["b.txt"]}));
    assert_eq!(paths(&v["status"]["staged"]), vec!["Mb.txt"]);
    assert_eq!(paths(&v["status"]["changes"]), vec!["?n.txt"]);
    let v = g.ok("unstage", json!({"files": ["b.txt"]}));
    assert_eq!(v["status"]["staged"], json!([]));
    let v = g.ok("stage", json!({"files": "all"}));
    assert_eq!(paths(&v["status"]["staged"]), vec!["Mb.txt", "An.txt"]);
    let v = g.ok("unstage", json!({"files": "all"}));
    assert_eq!(paths(&v["status"]["changes"]), vec!["Mb.txt", "?n.txt"]);
    let v = g.ok("discard", json!({"files": ["n.txt"]}));
    assert!(!g.work.join("n.txt").exists());
    assert_eq!(paths(&v["status"]["changes"]), vec!["Mb.txt"]);
    let v = g.ok("discard", json!({"files": ["b.txt"]}));
    assert_eq!(
        std::fs::read_to_string(g.work.join("b.txt")).unwrap(),
        "bee\n"
    );
    assert_eq!(v["status"]["changes"], json!([]));
    git(&g.work, &["mv", "r.txt", "r2.txt"]);
    let v = g.ok("discard", json!({"files": ["r2.txt"]}));
    assert!(g.work.join("r.txt").exists() && !g.work.join("r2.txt").exists());
    assert_eq!(v["status"]["staged"], json!([]));
    write(&g.work, "a.txt", "x\n");
    git(&g.work, &["add", "a.txt"]);
    write(&g.work, "b.txt", "y\n");
    write(&g.work, "z.txt", "z\n");
    let v = g.ok("discard", json!({"files": "all"}));
    assert_eq!(
        (&v["status"]["staged"], &v["status"]["changes"]),
        (&json!([]), &json!([]))
    );
    assert!(!g.work.join("z.txt").exists());
}

#[test]
fn bad_requests_are_rejected() {
    let g = setup("gitops-bad");
    g.fails(
        "stage",
        json!({"files": ["../x"]}),
        400,
        "invalid path ../x",
    );
    g.fails(
        "stage",
        json!({"files": ["/etc/passwd"]}),
        400,
        "invalid path /etc/passwd",
    );
    g.fails("stage", json!({}), 400, "files required");
    g.fails("discard", json!({"files": []}), 400, "files required");
    let (c, b) = g.fx.req(
        "POST",
        "/v1/git/stage",
        &json!({"files": "all"}).to_string(),
    );
    assert_eq!(
        (c, js(&b)["error"].clone()),
        (400, json!("worktree or lineId required"))
    );
    let (c, b) = g.fx.req("GET", "/v1/git/status", "");
    assert_eq!(
        (c, js(&b)["error"].clone()),
        (400, json!("worktree or lineId required"))
    );
    let (c, v) = g.status_of("relative/dir");
    assert_eq!(
        (c, v["error"].clone()),
        (400, json!("relative/dir is not an absolute path"))
    );
    let gone = g.fx.base.join("gone");
    let (c, _) = g.status_of(gone.to_str().unwrap());
    assert_eq!(c, 410);
    let (c, _) = g.fx.req("GET", "/v1/git/nope", "");
    assert_eq!(c, 404);
}

#[test]
fn commit_amend_and_push() {
    let g = setup("gitops-commit");
    g.fails(
        "commit",
        json!({"message": "  "}),
        400,
        "commit message required",
    );
    g.fails("commit", json!({"message": "x"}), 409, "nothing to commit");
    write(&g.work, "b.txt", "changed\n");
    write(&g.work, "n.txt", "new\n");
    let v = g.ok("commit", json!({"message": "feat: all", "all": true}));
    let sha = v["commit"].as_str().unwrap().to_string();
    assert_eq!(sha.len(), 40);
    let s = &v["status"];
    assert_eq!((&s["staged"], &s["changes"]), (&json!([]), &json!([])));
    assert_eq!(s["ahead"], 1);
    assert_eq!(
        s["outgoing"][0],
        json!({"sha": sha, "subject": "feat: all"})
    );
    let v = g.ok("commit", json!({"message": "feat: amended", "amend": true}));
    assert_ne!(v["commit"], json!(sha));
    assert_eq!(v["status"]["outgoing"][0]["subject"], "feat: amended");
    let v = g.ok("push", json!({}));
    assert_eq!(
        (
            v["status"]["ahead"].as_u64(),
            v["status"]["outgoing"].clone()
        ),
        (Some(0), json!([]))
    );
    let v = g.ok("switch", json!({"branch": "feat/x", "create": true}));
    assert_eq!(
        (
            v["status"]["branch"].clone(),
            v["status"]["upstream"].clone()
        ),
        (json!("feat/x"), Value::Null)
    );
    write(&g.work, "c.txt", "c\n");
    g.ok("stage", json!({"files": ["c.txt"]}));
    let v = g.ok("commit", json!({"message": "feat: c", "push": true}));
    assert_eq!(v["status"]["upstream"], "origin/feat/x");
    assert_eq!(v["status"]["ahead"], 0);
    let remote = git(&g.other, &["ls-remote", "origin", "feat/x"]);
    assert!(
        remote.starts_with(v["commit"].as_str().unwrap()),
        "{remote}"
    );
}

#[test]
fn push_and_fetch_without_remote_or_branch_fail() {
    let g = setup("gitops-noremote");
    let lone = g.fx.base.join("lone");
    std::fs::create_dir_all(&lone).unwrap();
    git(&lone, &["init", "-q", "-b", "main"]);
    ident(&lone);
    commit(&lone, "a", "a\n", "init");
    let l = lone.to_str().unwrap();
    let (c, v) = g.post_to(l, "push", json!({}));
    assert_eq!(
        (c, v["error"].clone()),
        (409, json!("no remote configured"))
    );
    let (c, v) = g.post_to(l, "fetch", json!({}));
    assert_eq!(
        (c, v["error"].clone()),
        (409, json!("no remote configured"))
    );
    let (c, v) = g.post_to(l, "pull", json!({}));
    assert_eq!(
        (c, v["error"].clone()),
        (409, json!("main has no upstream branch"))
    );
    write(&lone, "b", "b\n");
    let (c, v) = g.post_to(
        l,
        "commit",
        json!({"message": "b", "all": true, "push": true}),
    );
    assert_eq!(c, 409);
    let e = v["error"].as_str().unwrap();
    assert!(
        e.starts_with("committed ") && e.ends_with("but the push failed: no remote configured"),
        "{e}"
    );
    git(&lone, &["checkout", "-q", "--detach"]);
    let (c, v) = g.post_to(l, "push", json!({}));
    assert_eq!((c, v["error"].clone()), (409, json!("HEAD is detached")));
    let (c, v) = g.post_to(l, "pull", json!({}));
    assert_eq!((c, v["error"].clone()), (409, json!("HEAD is detached")));
    let (c, v) = g.status_of(l);
    assert_eq!((c, v["branch"].clone()), (200, Value::Null));
}

#[test]
fn fetch_and_pull_bring_incoming_commits_and_abort_conflicts() {
    let g = setup("gitops-pull");
    commit(&g.other, "o.txt", "o\n", "feat: other");
    git(&g.other, &["push", "-q", "origin", "main"]);
    let v = g.ok("fetch", json!({}));
    assert_eq!(v["status"]["behind"], 1);
    assert_eq!(v["status"]["incoming"][0]["subject"], "feat: other");
    let v = g.ok("pull", json!({}));
    assert_eq!(
        (
            v["status"]["behind"].as_u64(),
            v["status"]["incoming"].clone()
        ),
        (Some(0), json!([]))
    );
    assert!(g.work.join("o.txt").exists());
    commit(&g.other, "a.txt", "theirs\n", "theirs");
    git(&g.other, &["push", "-q", "origin", "main"]);
    commit(&g.work, "a.txt", "mine\n", "mine");
    g.fails(
        "pull",
        json!({}),
        409,
        "merge conflicts in a.txt; the pull was aborted",
    );
    let s = g.status();
    assert_eq!(
        (s["operation"].clone(), s["conflicts"].clone()),
        (Value::Null, json!([]))
    );
    assert_eq!(
        std::fs::read_to_string(g.work.join("a.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn switch_merge_stash_and_revert() {
    let g = setup("gitops-ops");
    g.fails(
        "switch",
        json!({"branch": "bad..name"}),
        400,
        "invalid branch name bad..name",
    );
    g.fails(
        "switch",
        json!({"branch": "-f"}),
        400,
        "invalid branch name -f",
    );
    g.ok("switch", json!({"branch": "feat/y", "create": true}));
    let f = commit(&g.work, "y.txt", "y\n", "feat: y");
    let v = g.ok("switch", json!({"branch": "main"}));
    assert_eq!(v["status"]["branch"], "main");
    assert_eq!(v["status"]["branches"], json!(["feat/y", "main"]));
    g.fails("merge", json!({"rev": "-x"}), 400, "invalid revision -x");
    g.fails(
        "merge",
        json!({"rev": "nope"}),
        404,
        "unknown revision nope",
    );
    write(&g.work, "a.txt", "dirty\n");
    g.fails(
        "merge",
        json!({"rev": "feat/y"}),
        409,
        "1 uncommitted change(s): commit or stash them first",
    );
    let v = g.ok("stash", json!({}));
    assert_eq!(
        (
            v["status"]["stashes"].as_u64(),
            v["status"]["changes"].clone()
        ),
        (Some(1), json!([]))
    );
    g.fails("stash", json!({}), 409, "nothing to stash");
    let v = g.ok("merge", json!({"rev": "feat/y"}));
    assert_eq!(v["status"]["head"], json!(f));
    let v = g.ok("stash", json!({"pop": true}));
    assert_eq!(
        (
            v["status"]["stashes"].as_u64(),
            paths(&v["status"]["changes"])
        ),
        (Some(0), vec!["Ma.txt".to_string()])
    );
    g.fails("stash", json!({"pop": true}), 409, "no stash to pop");
    g.ok("discard", json!({"files": "all"}));
    g.ok("switch", json!({"branch": "feat/y"}));
    commit(&g.work, "a.txt", "feat side\n", "feat a");
    g.ok("switch", json!({"branch": "main"}));
    commit(&g.work, "a.txt", "main side\n", "main a");
    g.fails(
        "merge",
        json!({"rev": "feat/y"}),
        409,
        "merge conflicts in a.txt; the merge was aborted",
    );
    assert_eq!(g.status()["operation"], Value::Null);
    g.fails(
        "revert",
        json!({"sha": "zz"}),
        400,
        "sha must be 4-64 hex digits",
    );
    g.fails(
        "revert",
        json!({"sha": "deadbeef"}),
        404,
        "unknown commit deadbeef",
    );
    let v = g.ok("revert", json!({"sha": &f[..8]}));
    assert_eq!(v["commit"], v["status"]["head"]);
    assert!(!g.work.join("y.txt").exists());
}

#[test]
fn status_reads_while_an_agent_holds_the_index_lock() {
    let g = setup("gitops-lock");
    write(&g.work, "a.txt", "changed\n");
    let lock = g.work.join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();
    let s = g.status();
    assert_eq!(paths(&s["changes"]), vec!["Ma.txt"]);
    assert!(lock.exists());
    std::fs::remove_file(lock).unwrap();
}

fn with_submodule(g: &G) -> PathBuf {
    let lib = g.fx.base.join("lib-src");
    git(
        &g.fx.base,
        &["init", "-q", "-b", "main", lib.to_str().unwrap()],
    );
    ident(&lib);
    commit(&lib, "f.txt", "one\n", "lib init");
    git(
        &g.work,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            lib.to_str().unwrap(),
            "lib",
        ],
    );
    git(&g.work, &["commit", "-q", "-m", "add lib"]);
    let inner = g.work.join("lib");
    ident(&inner);
    inner
}

#[test]
fn submodules_are_flagged_and_dirty_inside_cannot_be_staged() {
    let g = setup("gitops-sub");
    let inner = with_submodule(&g);
    let top = std::fs::canonicalize(&inner).unwrap();
    write(&inner, "f.txt", "dirty\n");
    let s = g.status();
    assert_eq!(paths(&s["changes"]), vec!["Mlib"]);
    assert_eq!(
        s["changes"][0]["submodule"],
        json!({"commit": false, "modified": true, "untracked": false})
    );
    assert_eq!(
        s["submodules"],
        json!([{"path": "lib", "worktree": top.to_str().unwrap()}])
    );
    g.fails(
        "stage",
        json!({"files": ["lib"]}),
        409,
        "lib is a submodule with uncommitted changes inside: commit them in the submodule first",
    );
    let (c, inside) = g.status_of(top.to_str().unwrap());
    assert_eq!(c, 200, "{inside}");
    assert!(inside.get("error").is_none(), "{inside}");
    assert_eq!(paths(&inside["changes"]), vec!["Mf.txt"]);

    git(&inner, &["commit", "-q", "-am", "inner"]);
    let s = g.status();
    assert_eq!(
        s["changes"][0]["submodule"],
        json!({"commit": true, "modified": false, "untracked": false})
    );
    let v = g.ok("stage", json!({"files": ["lib"]}));
    assert_eq!(paths(&v["status"]["staged"]), vec!["Mlib"]);
    assert!(v["status"]["staged"][0]["submodule"].is_object(), "{v}");
    let v = g.ok("commit", json!({"message": "bump lib"}));
    assert!(v["commit"].as_str().is_some_and(|c| c.len() == 40), "{v}");
    assert_eq!(v["status"]["changes"], json!([]));
    assert_eq!(v["status"]["staged"], json!([]));
}
