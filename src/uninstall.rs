use crate::{autostart, install, statusline, worktrees, Ctx};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const APP_ID: &str = "io.github.basilelt.overseer";

static PENDING: Mutex<Option<PathBuf>> = Mutex::new(None);

#[derive(Default)]
pub struct Report {
    pub dry_run: bool,
    pub removed: Vec<(String, String)>,
    pub kept: Vec<(String, String, String)>,
    pub errors: Vec<(String, String, String)>,
}

impl Report {
    fn removed(&mut self, kind: &str, path: impl AsRef<Path>) {
        self.removed
            .push((kind.into(), path.as_ref().to_string_lossy().into_owned()));
    }

    fn kept(&mut self, kind: &str, path: impl AsRef<Path>, reason: impl Into<String>) {
        self.kept.push((
            kind.into(),
            path.as_ref().to_string_lossy().into_owned(),
            reason.into(),
        ));
    }

    fn error(&mut self, kind: &str, path: impl AsRef<Path>, e: impl std::fmt::Display) {
        self.errors.push((
            kind.into(),
            path.as_ref().to_string_lossy().into_owned(),
            e.to_string(),
        ));
    }

    fn gone(&self) -> HashSet<PathBuf> {
        self.removed.iter().map(|(_, p)| PathBuf::from(p)).collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "dryRun": self.dry_run,
            "removed": self.removed.iter().map(|(k, p)| json!({"kind": k, "path": p})).collect::<Vec<_>>(),
            "kept": self.kept.iter().map(|(k, p, r)| json!({"kind": k, "path": p, "reason": r})).collect::<Vec<_>>(),
            "errors": self.errors.iter().map(|(k, p, e)| json!({"kind": k, "path": p, "error": e})).collect::<Vec<_>>(),
        })
    }
}

pub fn lines(v: &Value) -> Vec<String> {
    let mut out = vec![];
    let col = |label: &str, row: &Value, extra: &str| {
        let kind = row["kind"].as_str().unwrap_or_default();
        let path = row["path"].as_str().unwrap_or_default();
        let sep = if extra.is_empty() { "" } else { "  " };
        format!("{label:<8} {kind:<10} {path}{sep}{extra}")
    };
    let verb = if v["dryRun"] == true {
        "would remove"
    } else {
        "removed"
    };
    for r in v["removed"].as_array().into_iter().flatten() {
        out.push(col(verb, r, ""));
    }
    for r in v["kept"].as_array().into_iter().flatten() {
        let why = format!("({})", r["reason"].as_str().unwrap_or_default());
        out.push(col("kept", r, &why));
    }
    for r in v["errors"].as_array().into_iter().flatten() {
        let why = format!("({})", r["error"].as_str().unwrap_or_default());
        out.push(col("error", r, &why));
    }
    out
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn remove_tree(rep: &mut Report, kind: &str, p: &Path, apply: bool) {
    if !exists(p) {
        return;
    }
    if apply {
        let r = if p.is_dir() && !p.is_symlink() {
            std::fs::remove_dir_all(p)
        } else {
            std::fs::remove_file(p)
        };
        if let Err(e) = r {
            return rep.error(kind, p, e);
        }
    }
    rep.removed(kind, p);
}

fn sweep(
    rep: &mut Report,
    dir: &Path,
    gone: &HashSet<PathBuf>,
    apply: bool,
    recurse: bool,
) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return false;
    };
    let mut empty = true;
    for e in rd.flatten() {
        let p = e.path();
        if !apply && gone.contains(&p) {
            continue;
        }
        let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
        if recurse && is_dir && !p.join(".git").exists() && sweep(rep, &p, gone, apply, true) {
            continue;
        }
        empty = false;
    }
    if empty {
        if apply {
            if let Err(e) = std::fs::remove_dir(dir) {
                rep.error("dir", dir, e);
                return false;
            }
        }
        rep.removed("dir", dir);
    }
    empty
}

fn is_backup(name: &str, stem_ok: impl Fn(&str) -> bool) -> bool {
    let Some((stem, stamp)) = name.rsplit_once(".bak-") else {
        return false;
    };
    stamp.len() == 12 && stamp.bytes().all(|b| b.is_ascii_digit()) && stem_ok(stem)
}

fn backups(rep: &mut Report, dir: &Path, stem_ok: impl Fn(&str) -> bool, apply: bool) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut found: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| is_backup(&e.file_name().to_string_lossy(), &stem_ok))
        .map(|e| e.path())
        .collect();
    found.sort();
    for p in found {
        remove_tree(rep, "backup", &p, apply);
    }
}

fn service(rep: &mut Report, state_dir: &Path, apply: bool) {
    let env = autostart::Env::current(state_dir);
    if let Some(p) = env.path().filter(|p| exists(p)) {
        if apply {
            if let Err(e) = autostart::disable(&env, &autostart::run_cmd) {
                return rep.error("service", &p, e);
            }
        }
        rep.removed("service", p);
    }
    if autostart::linger_flag(state_dir).exists() {
        let label = "loginctl disable-linger";
        if apply {
            match autostart::disable_linger_if_ours(&env, &autostart::run_cmd, state_dir) {
                Ok(_) => {}
                Err(e) => return rep.error("linger", label, e),
            }
        }
        rep.removed("linger", label);
    }
}

fn keep_copy(rep: &mut Report, kind: &str, path: &Path) -> bool {
    let bak = PathBuf::from(format!("{}.overseer-uninstall.bak", path.display()));
    if rep.kept.iter().any(|(_, p, _)| Path::new(p) == bak) {
        return true;
    }
    match std::fs::copy(path, &bak) {
        Ok(_) => {
            rep.kept(
                kind,
                &bak,
                "copy of the file before Overseer's entries were removed",
            );
            true
        }
        Err(e) => {
            rep.error(kind, &bak, e);
            false
        }
    }
}

fn hollow(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|m| m.iter().all(|(k, v)| k == "hooks" && v == &json!({})))
}

fn hooks(rep: &mut Report, state_dir: &Path, apply: bool) {
    let bin = install::hook_bin();
    for t in install::targets() {
        let Ok(text) = std::fs::read_to_string(&t.path) else {
            continue;
        };
        if !text.contains(install::MARKER) {
            continue;
        }
        let planned = match install::plan(&t, true, &bin, state_dir) {
            Ok(p) => p,
            Err(e) => {
                rep.error("hook", &t.path, e);
                continue;
            }
        };
        if !planned.changed() {
            continue;
        }
        if apply {
            if !planned.owned && !keep_copy(rep, "hook", &t.path) {
                continue;
            }
            let emptied = !planned.owned && hollow(&planned.after);
            let done = if emptied {
                std::fs::remove_file(&t.path).map_err(|e| format!("{}: {e}", t.path.display()))
            } else {
                install::write(&t, &planned, state_dir, "").map(|_| ())
            };
            if let Err(e) = done {
                rep.error("hook", &t.path, e);
                continue;
            }
        }
        rep.removed("hook", &t.path);
    }
}

fn statusline_hook(rep: &mut Report, state_dir: &Path, apply: bool) {
    let settings = statusline::settings_path();
    let Ok(text) = std::fs::read_to_string(&settings) else {
        return;
    };
    if !text.contains(statusline::MARKER) {
        return;
    }
    let bin = install::hook_bin();
    match statusline::plan(&text, false, &bin, state_dir) {
        Err(e) => rep.error("statusline", &settings, e),
        Ok(p) if !p.changed => {}
        Ok(p) => {
            if apply {
                if !keep_copy(rep, "statusline", &settings) {
                    return;
                }
                if let Err(e) = install::atomic_write(&settings, &p.after_text) {
                    return rep.error("statusline", &settings, e);
                }
            }
            rep.removed("statusline", &settings);
        }
    }
}

fn config_backups(ctx: &Ctx, rep: &mut Report, apply: bool) {
    let mut seen: Vec<(PathBuf, String)> = vec![];
    let mut add = |p: &Path| {
        if let (Some(dir), Some(name)) = (p.parent(), p.file_name()) {
            let k = (dir.to_path_buf(), name.to_string_lossy().into_owned());
            if !seen.contains(&k) {
                seen.push(k);
            }
        }
    };
    for t in install::targets() {
        add(&t.path);
    }
    add(&statusline::settings_path());
    for (dir, name) in seen {
        backups(rep, &dir, |s| s == name, apply);
    }
    backups(
        rep,
        &ctx.claude_dir.join("agents"),
        |s| s.ends_with(".md"),
        apply,
    );
}

fn owned_dirs(rep: &mut Report, apply: bool) {
    let gone = rep.gone();
    for t in install::targets() {
        if t.format != install::Format::File {
            continue;
        }
        if let Some(dir) = t.path.parent() {
            sweep(rep, dir, &gone, apply, false);
        }
    }
}

fn stores(ctx: &Ctx) -> Vec<PathBuf> {
    let default = crate::paths::default_worktrees_dir();
    let mut v = vec![ctx.config.worktree_dir.clone()];
    let same = |a: &Path, b: &Path| {
        a.canonicalize().unwrap_or(a.into()) == b.canonicalize().unwrap_or(b.into())
    };
    if !same(&default, &v[0]) {
        v.push(default);
    }
    v
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

fn owned_branch(b: &str) -> bool {
    crate::layout::OWNED_BRANCH_PREFIXES
        .iter()
        .any(|p| b.starts_with(p))
}

fn worktree_dirs(ctx: &Ctx, rep: &mut Report, apply: bool) -> Vec<PathBuf> {
    let mut kept_roots = vec![];
    for store in stores(ctx) {
        for project in subdirs(&store) {
            for wt in subdirs(&project) {
                if !wt.join(".git").exists() {
                    continue;
                }
                let id = wt.to_string_lossy().into_owned();
                match worktrees::cleanup_where(ctx, &id, !apply, Some(&id), &owned_branch) {
                    Err((_, why)) => {
                        rep.kept("worktree", &wt, why);
                        if let Ok((root, _)) = worktrees::project(&id) {
                            kept_roots.push(PathBuf::from(root));
                        }
                    }
                    Ok(p) => {
                        if let Some(why) = p["refused"].as_str() {
                            rep.kept("worktree", &wt, why);
                            if let Ok((root, _)) = worktrees::project(&id) {
                                kept_roots.push(PathBuf::from(root));
                            }
                            continue;
                        }
                        for r in p["remove"].as_array().into_iter().flatten() {
                            match r["kind"].as_str() {
                                Some("branch") => {
                                    rep.removed("branch", r["name"].as_str().unwrap_or_default())
                                }
                                _ => rep.removed("worktree", &wt),
                            }
                        }
                        for k in p["keep"].as_array().into_iter().flatten() {
                            rep.kept(
                                "branch",
                                k["name"].as_str().unwrap_or_default(),
                                k["reason"].as_str().unwrap_or_default(),
                            );
                        }
                    }
                }
            }
        }
    }
    kept_roots
}

fn home_dirs(rep: &mut Report, kept_roots: &[PathBuf], apply: bool) {
    let base = crate::paths::overseer_home();
    for d in crate::paths::HOME_DIRS {
        let p = base.join(d);
        if d == crate::paths::REVIEWS_DIR && kept_roots.iter().any(|r| r.starts_with(&p)) {
            rep.kept("dir", &p, "holds the repository of a kept worktree");
            continue;
        }
        remove_tree(rep, "dir", &p, apply);
    }
}

fn empty_stores(ctx: &Ctx, rep: &mut Report, apply: bool) {
    let default = crate::paths::default_worktrees_dir();
    for store in stores(ctx) {
        let gone = rep.gone();
        if store == default || store.canonicalize().ok() == default.canonicalize().ok() {
            sweep(rep, &store, &gone, apply, true);
        } else {
            for project in subdirs(&store) {
                sweep(rep, &project, &gone, apply, true);
            }
        }
    }
    let base = crate::paths::overseer_home();
    let gone = rep.gone();
    if exists(&base) && !sweep(rep, &base, &gone, apply, false) {
        rep.kept("dir", &base, "holds files Overseer did not create");
    }
}

pub fn app_data_paths() -> Vec<PathBuf> {
    let home = crate::home();
    let xdg = |var: &str, default: &str| crate::env_dir(var, default);
    let env = |var: &str| {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        let lib = home.join("Library");
        return vec![
            lib.join("Application Support").join(APP_ID),
            lib.join("Caches").join(APP_ID),
            lib.join("Logs").join(APP_ID),
            lib.join("WebKit").join(APP_ID),
            lib.join("Preferences").join(format!("{APP_ID}.plist")),
            lib.join("Saved Application State")
                .join(format!("{APP_ID}.savedState")),
        ];
    }
    if cfg!(windows) {
        return [env("LOCALAPPDATA"), env("APPDATA")]
            .into_iter()
            .flatten()
            .map(|d| d.join(APP_ID))
            .collect();
    }
    vec![
        xdg("XDG_DATA_HOME", ".local/share").join(APP_ID),
        xdg("XDG_CACHE_HOME", ".cache").join(APP_ID),
        xdg("XDG_CONFIG_HOME", ".config").join(APP_ID),
    ]
}

fn app_data(rep: &mut Report, apply: bool) {
    for p in app_data_paths() {
        remove_tree(rep, "appData", &p, apply);
    }
}

fn state_dir_safe(state_dir: &Path) -> Result<(), String> {
    let home = crate::home();
    if state_dir.parent().is_none() || state_dir == home || home.starts_with(state_dir) {
        return Err(format!(
            "{} is not a dedicated state directory",
            state_dir.display()
        ));
    }
    if !looks_like_state_dir(state_dir) {
        return Err(format!(
            "{} does not look like an Overseer state directory",
            state_dir.display()
        ));
    }
    Ok(())
}

fn looks_like_state_dir(dir: &Path) -> bool {
    let Ok(mut entries) = std::fs::read_dir(dir) else {
        return true;
    };
    let Some(first) = entries.next() else {
        return true;
    };
    first.is_ok() && crate::paths::STATE_DBS.iter().any(|n| dir.join(n).exists())
}

pub fn run(ctx: &Ctx, apply: bool) -> Report {
    let mut rep = Report {
        dry_run: !apply,
        ..Default::default()
    };
    let state_dir = ctx.state_dir.as_path();
    service(&mut rep, state_dir, apply);
    hooks(&mut rep, state_dir, apply);
    statusline_hook(&mut rep, state_dir, apply);
    config_backups(ctx, &mut rep, apply);
    owned_dirs(&mut rep, apply);
    let kept_roots = worktree_dirs(ctx, &mut rep, apply);
    home_dirs(&mut rep, &kept_roots, apply);
    empty_stores(ctx, &mut rep, apply);
    app_data(&mut rep, apply);
    if exists(state_dir) {
        match state_dir_safe(state_dir) {
            Ok(()) => rep.removed("stateDir", state_dir),
            Err(e) => rep.error("stateDir", state_dir, e),
        }
    }
    rep
}

// The daemon's workers keep running until exit and can write a file back after the removal.
fn remove_until_gone(dir: &Path) -> Result<(), String> {
    let mut last = Ok(());
    for _ in 0..10 {
        last = match std::fs::remove_dir_all(dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", dir.display())),
        };
        std::thread::sleep(std::time::Duration::from_millis(100));
        if last.is_ok() && !exists(dir) {
            break;
        }
    }
    last
}

pub fn finish(state_dir: &Path) -> Result<(), String> {
    let mut last = Ok(());
    if exists(state_dir) {
        state_dir_safe(state_dir)?;
        last = remove_until_gone(state_dir);
    }
    let env = autostart::Env::current(state_dir);
    let unloaded = autostart::unload(&env, &autostart::run_cmd);
    last.and(unloaded)
}

pub fn take_pending() -> Option<PathBuf> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).take()
}

pub fn routes(ctx: &Ctx, method: &str, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    if (method, segs) != ("POST", &["v1", "uninstall"][..]) {
        return None;
    }
    let apply = body["dryRun"].as_bool() == Some(false);
    let rep = run(ctx, apply);
    if apply && !rep.errors.iter().any(|(k, _, _)| k == "stateDir") {
        *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx.state_dir.clone());
    }
    Some((200, rep.to_json()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_dir_written_back_during_removal_is_removed_again() {
        let d = std::env::temp_dir().join(format!("sb-un-race-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("overseer.db"), "").unwrap();
        let w = d.clone();
        let writer = std::thread::spawn(move || {
            while w.exists() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::fs::create_dir_all(&w).unwrap();
            std::fs::write(w.join("late.log"), "x").unwrap();
        });
        assert_eq!(remove_until_gone(&d), Ok(()));
        writer.join().unwrap();
        assert!(!d.exists());
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-uninstall-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_twelve_digit_stamps_are_overseer_backups() {
        let any = |_: &str| true;
        assert!(is_backup("settings.json.bak-202601010000", any));
        assert!(!is_backup("settings.json.bak", any));
        assert!(!is_backup("settings.json.bak-foo", any));
        assert!(!is_backup("settings.json.bak-20260101000", any));
        assert!(!is_backup("settings.json.bak-2026010100001", any));
        assert!(!is_backup("settings.json.bak-20260101000a", any));
        assert!(!is_backup("settings.json.bak-202601010000", |s| s == "hooks.json"));
    }

    #[test]
    fn backups_removes_only_matching_files_and_dry_run_keeps_them() {
        let d = tmp("backups");
        for n in [
            "settings.json.bak-202601010000",
            "settings.json.bak-202601010001",
            "settings.json.bak",
            "other.json.bak-202601010000",
        ] {
            std::fs::write(d.join(n), "x").unwrap();
        }
        let mut rep = Report::default();
        backups(&mut rep, &d, |s| s == "settings.json", false);
        assert_eq!(rep.removed.len(), 2);
        assert!(d.join("settings.json.bak-202601010000").exists());
        let mut rep = Report::default();
        backups(&mut rep, &d, |s| s == "settings.json", true);
        assert_eq!(rep.removed.len(), 2);
        assert!(!d.join("settings.json.bak-202601010000").exists());
        assert!(d.join("settings.json.bak").exists());
        assert!(d.join("other.json.bak-202601010000").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sweep_removes_only_directories_that_end_up_empty() {
        let d = tmp("sweep");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::create_dir_all(d.join("keep/wt")).unwrap();
        std::fs::write(d.join("keep/wt/.git"), "gitdir: x").unwrap();
        std::fs::create_dir_all(d.join("keep/wt/emptysub")).unwrap();
        let mut rep = Report::default();
        assert!(!sweep(&mut rep, &d, &HashSet::new(), false, true));
        let names: Vec<_> = rep.removed.iter().map(|(_, p)| p.clone()).collect();
        assert!(names.iter().any(|p| p.ends_with("a/b")));
        assert!(names.iter().any(|p| p.ends_with("/a")));
        assert!(!names.iter().any(|p| p.contains("keep")));
        assert!(d.join("a/b").exists());
        let mut rep = Report::default();
        sweep(&mut rep, &d, &HashSet::new(), true, true);
        assert!(!d.join("a").exists());
        assert!(d.join("keep/wt/emptysub").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sweep_treats_paths_already_slated_for_removal_as_gone_in_a_dry_run() {
        let d = tmp("sweep-gone");
        std::fs::write(d.join("overseer.js"), "x").unwrap();
        let gone: HashSet<PathBuf> = [d.join("overseer.js")].into();
        let mut rep = Report::default();
        assert!(sweep(&mut rep, &d, &gone, false, false));
        assert!(d.join("overseer.js").exists());
        let mut rep = Report::default();
        assert!(!sweep(&mut rep, &d, &HashSet::new(), false, false));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn remove_tree_handles_missing_dirs_files_and_symlinks() {
        let d = tmp("tree");
        std::fs::create_dir_all(d.join("dir/in")).unwrap();
        std::fs::write(d.join("file"), "x").unwrap();
        let mut rep = Report::default();
        remove_tree(&mut rep, "dir", &d.join("nope"), true);
        assert!(rep.removed.is_empty());
        remove_tree(&mut rep, "dir", &d.join("dir"), false);
        assert!(d.join("dir/in").exists());
        remove_tree(&mut rep, "dir", &d.join("dir"), true);
        remove_tree(&mut rep, "dir", &d.join("file"), true);
        assert_eq!(rep.removed.len(), 3);
        assert!(!d.join("dir").exists() && !d.join("file").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_state_dir_guard_refuses_root_and_the_home_directory() {
        assert!(state_dir_safe(Path::new("/")).is_err());
        assert!(state_dir_safe(&crate::home()).is_err());
        assert!(state_dir_safe(Path::new("/")).is_err());
        assert!(state_dir_safe(&crate::home().join(".local/state/overseer")).is_ok());
    }

    #[test]
    fn the_state_dir_guard_refuses_a_directory_that_is_not_overseers() {
        let d = tmp("foreign-state");
        std::fs::write(d.join("notes.txt"), "mine").unwrap();
        assert!(state_dir_safe(&d).is_err());
        assert!(finish(&d).is_err());
        assert!(d.join("notes.txt").exists());
        std::fs::write(d.join("overseer.db"), "").unwrap();
        assert!(state_dir_safe(&d).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_state_dir_guard_accepts_an_empty_directory() {
        let d = tmp("empty-state");
        assert!(state_dir_safe(&d).is_ok());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hollow_hook_files_hold_nothing_of_the_users() {
        assert!(hollow(&json!({})));
        assert!(hollow(&json!({"hooks": {}})));
        assert!(!hollow(&json!({"hooks": {"Stop": []}})));
        assert!(!hollow(&json!({"hooks": {}, "model": "x"})));
        assert!(!hollow(&json!([])));
    }

    #[test]
    fn report_serializes_to_the_documented_shape_and_prints_one_line_per_row() {
        let mut rep = Report {
            dry_run: true,
            ..Default::default()
        };
        rep.removed("hook", "/h/settings.json");
        rep.kept("worktree", "/w/a", "1 uncommitted change(s)");
        rep.error("appData", "/x", "denied");
        let v = rep.to_json();
        assert_eq!(v["dryRun"], true);
        assert_eq!(
            v["removed"],
            json!([{"kind": "hook", "path": "/h/settings.json"}])
        );
        assert_eq!(
            v["kept"],
            json!([{"kind": "worktree", "path": "/w/a", "reason": "1 uncommitted change(s)"}])
        );
        assert_eq!(
            v["errors"],
            json!([{"kind": "appData", "path": "/x", "error": "denied"}])
        );
        let l = lines(&v);
        assert_eq!(l.len(), 3);
        assert!(l[0].starts_with("would remove") && l[0].contains("/h/settings.json"));
        assert!(l[1].starts_with("kept") && l[1].ends_with("(1 uncommitted change(s))"));
        assert!(l[2].starts_with("error") && l[2].ends_with("(denied)"));
    }

    #[test]
    fn the_branch_filter_only_deletes_branches_overseer_creates() {
        assert!(owned_branch("sb/claude/0101"));
        assert!(owned_branch("review/pr-12"));
        assert!(owned_branch("task/7-fix-login"));
        assert!(!owned_branch("feature/x"));
        assert!(!owned_branch("main"));
    }

    #[test]
    fn app_data_covers_the_tauri_identifier() {
        let conf = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("app/src-tauri/tauri.conf.json"),
        )
        .unwrap();
        let conf: serde_json::Value = serde_json::from_str(&conf).unwrap();
        let id = conf["identifier"].as_str().unwrap();
        assert_eq!(APP_ID, id);
        let p = app_data_paths();
        assert!(!p.is_empty());
        assert!(p.iter().all(|p| p.to_string_lossy().contains(id)));
    }
}
