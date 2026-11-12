mod common;

use common::{git, Fx};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SETTINGS: &str = r#"{
  "model": "opus",
  "statusLine": {"type": "command", "command": "echo hi"},
  "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "mine"}]}]}
}"#;

fn put(p: &Path, text: &str) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, String> {
    fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let t = e.file_type().unwrap();
            if t.is_dir() {
                out.insert(p.clone(), "dir".into());
                walk(&p, out);
            } else if t.is_symlink() {
                out.insert(p.clone(), format!("link {:?}", std::fs::read_link(&p)));
            } else if t.is_file() {
                out.insert(p, std::fs::read_to_string(e.path()).unwrap_or_default());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, &mut out);
    out
}

fn home(fx: &Fx) -> PathBuf {
    fx.base.join("home")
}

fn store(fx: &Fx) -> PathBuf {
    let root = fx.wt.to_string_lossy().into_owned();
    home(fx).join(format!(
        ".overseer/worktrees/repo-{}",
        overseer::layout::store_hash(&root)
    ))
}

fn worktree(fx: &Fx, name: &str) -> PathBuf {
    let dir = store(fx).join(name);
    std::fs::create_dir_all(store(fx)).unwrap();
    git(
        &fx.wt,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            &format!("sb/{name}"),
            dir.to_str().unwrap(),
        ],
    );
    git(&dir, &["config", "user.email", "dev@example.com"]);
    git(&dir, &["config", "user.name", "Dev"]);
    dir
}

fn branches(fx: &Fx) -> String {
    git(&fx.wt, &["branch", "--list", "sb/*"])
}

fn populate(fx: &Fx) {
    let h = home(fx);
    put(&h.join(".claude/settings.json"), SETTINGS);
    std::fs::create_dir_all(h.join(".codex")).unwrap();
    std::fs::create_dir_all(h.join(".config/opencode")).unwrap();
    assert!(fx.sb(&["hooks", "install", "--apply"]).status.success());
    assert!(fx
        .sb(&["statusline-tap", "install", "--apply"])
        .status
        .success());
    let settings = std::fs::read_to_string(h.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("#overseer-hook") && settings.contains("#overseer-statusline"));
    assert!(h.join(".config/opencode/plugins/overseer.js").exists());
    assert!(h.join(".codex/hooks.json").exists());
    assert!(h.join(".overseer/bin/overseer").exists());
    for n in [
        ".claude/settings.json.bak-202601010000",
        ".claude/settings.json.bak",
        ".claude/settings.json.bak-foo",
        ".claude/settings.json.bak-1234567890123",
        ".claude/agents/reviewer.md.bak-202601010000",
        ".claude/agents/reviewer.md",
        ".claude/keep.txt.bak-202601010000",
        ".codex/hooks.json.bak-202601010000",
    ] {
        put(&h.join(n), "x");
    }
    put(&h.join(".overseer/u/shot.png"), "png");
    put(&h.join(".overseer/tasks/t/a.txt"), "a");
    put(&h.join(".overseer/server/1.0.0/overseer"), "bin");
    put(&h.join(".local/share/io.github.basilelt.overseer/db"), "d");
    put(&h.join(".cache/io.github.basilelt.overseer/c"), "c");
    put(&h.join(".config/io.github.basilelt.overseer/s"), "s");
}

fn kinds(v: &Value, list: &str) -> Vec<(String, String)> {
    v[list]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["kind"].as_str().unwrap().to_string(),
                r["path"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn has(rows: &[(String, String)], kind: &str, ends: &str) -> bool {
    rows.iter().any(|(k, p)| k == kind && p.ends_with(ends))
}

#[test]
fn dry_run_lists_everything_and_touches_nothing() {
    let fx = Fx::new("un-dry", 0);
    populate(&fx);
    let clean = worktree(&fx, "clean");
    let before = (snapshot(&home(&fx)), snapshot(&fx.state));
    let out = fx.sb(&["uninstall"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{text}{}", common::err(&out));
    assert!(text.contains("would remove"), "{text}");
    assert!(text.contains("dry run: nothing was changed"), "{text}");
    for needle in [
        ".claude/settings.json",
        "settings.json.bak-202601010000",
        ".config/opencode/plugins/overseer.js",
        ".overseer/bin",
        "io.github.basilelt.overseer",
        "stateDir",
        "worktree ",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    let after = (snapshot(&home(&fx)), snapshot(&fx.state));
    for (b, a) in [(&before.0, &after.0), (&before.1, &after.1)] {
        let changed: Vec<_> = a
            .iter()
            .filter(|(k, v)| b.get(*k) != Some(*v) && !k.ends_with("overseer.db"))
            .chain(b.iter().filter(|(k, _)| !a.contains_key(*k)))
            .map(|(k, _)| k)
            .collect();
        assert!(changed.is_empty(), "{changed:?}");
    }
    assert!(clean.exists() && branches(&fx).contains("sb/clean"));
}

#[test]
fn apply_removes_what_overseer_added_and_keeps_the_users_work() {
    let fx = Fx::new("un-apply", 0);
    populate(&fx);
    let h = home(&fx);
    let clean = worktree(&fx, "clean");
    let dirty = worktree(&fx, "dirty");
    put(&dirty.join("wip.txt"), "unsaved");
    let ahead = worktree(&fx, "ahead");
    put(&ahead.join("new.txt"), "n");
    git(&ahead, &["add", "-A"]);
    git(&ahead, &["commit", "-q", "-m", "work"]);

    let out = fx.sb(&["uninstall", "--apply"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{text}{}", common::err(&out));

    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(h.join(".claude/settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["model"], "opus");
    assert_eq!(settings["statusLine"]["command"], "echo hi");
    assert_eq!(settings["hooks"]["Stop"][0]["hooks"][0]["command"], "mine");
    assert_eq!(settings["hooks"].as_object().unwrap().len(), 1);
    assert!(!settings.to_string().contains("#overseer"));
    assert!(!h.join(".codex/hooks.json").exists());
    assert!(!h.join(".config/opencode/plugins").exists());
    assert!(h.join(".config/opencode").exists());

    for gone in [
        ".claude/settings.json.bak-202601010000",
        ".claude/agents/reviewer.md.bak-202601010000",
        ".codex/hooks.json.bak-202601010000",
        ".overseer/bin",
        ".overseer/u",
        ".overseer/tasks",
        ".overseer/server",
        ".local/share/io.github.basilelt.overseer",
        ".cache/io.github.basilelt.overseer",
        ".config/io.github.basilelt.overseer",
    ] {
        assert!(!h.join(gone).exists(), "{gone} survived");
    }
    for kept in [
        ".claude/settings.json.bak",
        ".claude/settings.json.bak-foo",
        ".claude/settings.json.bak-1234567890123",
        ".claude/agents/reviewer.md",
        ".claude/keep.txt.bak-202601010000",
    ] {
        assert!(h.join(kept).exists(), "{kept} was deleted");
    }
    assert!(!fx.state.exists());
    let bak = std::fs::read_to_string(h.join(".claude/settings.json.overseer-uninstall.bak"))
        .expect("uninstall keeps a copy of the config it edited");
    assert!(bak.contains("#overseer-hook") && bak.contains("#overseer-statusline"));
    assert!(text.contains("overseer-uninstall.bak"), "{text}");

    assert!(!clean.exists());
    assert!(dirty.join("wip.txt").exists() && ahead.join("new.txt").exists());
    let left = branches(&fx);
    assert!(!left.contains("sb/clean"), "{left}");
    assert!(
        left.contains("sb/dirty") && left.contains("sb/ahead"),
        "{left}"
    );
    assert!(
        text.contains("kept") && text.contains("uncommitted"),
        "{text}"
    );
    assert!(h.join(".overseer/worktrees").exists());
    let again = common::out(&fx.sb(&["uninstall", "--apply"]));
    assert!(!again.contains("stateDir"), "{again}");
}

#[test]
fn apply_with_nothing_kept_leaves_no_overseer_directory() {
    let fx = Fx::new("un-all", 0);
    populate(&fx);
    let clean = worktree(&fx, "clean");
    let out = fx.sb(&["uninstall", "--apply"]);
    assert!(out.status.success(), "{}", common::err(&out));
    assert!(!clean.exists());
    assert!(!home(&fx).join(".overseer").exists());
    assert!(!fx.state.exists());
    let again = fx.sb(&["uninstall", "--apply"]);
    assert!(again.status.success(), "{}", common::err(&again));
    assert!(
        !common::out(&again).contains("stateDir"),
        "{}",
        common::out(&again)
    );
}

#[test]
fn the_daemon_route_reports_then_removes_and_exits() {
    let fx = Fx::new("un-route", 0);
    populate(&fx);
    let clean = worktree(&fx, "clean");
    let mut daemon = fx.daemon(false);

    let (code, body) = fx.req("POST", "/v1/uninstall", r#"{"dryRun":true}"#);
    assert_eq!(code, 200, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["dryRun"], true);
    for list in ["removed", "kept", "errors"] {
        assert!(v[list].is_array(), "{list}");
    }
    let removed = kinds(&v, "removed");
    for (kind, ends) in [
        ("hook", ".claude/settings.json"),
        ("statusline", ".claude/settings.json"),
        ("backup", "settings.json.bak-202601010000"),
        ("dir", ".overseer/bin"),
        ("worktree", "clean"),
        ("branch", "sb/clean"),
        ("appData", "io.github.basilelt.overseer"),
        ("stateDir", "state"),
    ] {
        assert!(has(&removed, kind, ends), "{kind} {ends}: {removed:?}");
    }
    assert!(fx.state.exists() && clean.exists());
    assert!(fx.req("GET", "/v1/version", "").0 == 200);

    let (code, body) = fx.req("POST", "/v1/uninstall", r#"{"dryRun":false}"#);
    assert_eq!(code, 200, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["dryRun"], false);
    assert_eq!(v["errors"], json!([]), "{body}");
    assert!(has(&kinds(&v, "removed"), "hook", ".claude/settings.json"));

    common::wait(|| daemon.0.try_wait().unwrap().is_some());
    assert!(daemon.0.try_wait().unwrap().unwrap().success());
    assert!(!fx.state.exists());
    assert!(!clean.exists());
    let settings = std::fs::read_to_string(home(&fx).join(".claude/settings.json")).unwrap();
    assert!(!settings.contains("#overseer"));
}

#[test]
fn a_missing_body_is_a_dry_run() {
    let fx = Fx::new("un-nobody", 0);
    let _daemon = fx.daemon(false);
    let (code, body) = fx.req("POST", "/v1/uninstall", "{}");
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["dryRun"], true);
    assert!(fx.state.exists());
}

#[test]
fn the_cli_goes_through_a_running_daemon() {
    let fx = Fx::new("un-cli", 0);
    populate(&fx);
    let mut daemon = fx.daemon(false);
    let out = fx.sb(&["uninstall", "--apply"]);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{text}{}", common::err(&out));
    assert!(text.contains("removed"), "{text}");
    common::wait(|| daemon.0.try_wait().unwrap().is_some());
    assert!(daemon.0.try_wait().unwrap().unwrap().success());
    assert!(!fx.state.exists());
}

#[test]
fn a_foreign_state_dir_is_refused_and_left_alone() {
    let fx = Fx::new("un-foreign", 0);
    let foreign = fx.base.join("Documents");
    put(&foreign.join("thesis.txt"), "mine");
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_overseer"));
    fx.env(&mut c);
    let out = c
        .args(["uninstall", "--apply"])
        .env("OVERSEER_STATE_DIR", &foreign)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(foreign.join("thesis.txt").exists());
}
