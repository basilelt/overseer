use crate::procs::now_ms;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const KEEP_MS: i64 = 30 * 60_000;
const TICK: Duration = Duration::from_millis(250);
const RELIST_MS: i64 = 2000;
const POLL_MS: i64 = 500;
const SAFETY_MS: i64 = 5000;
const DEBOUNCE_MS: i64 = 500;
const MAX_WAIT_MS: i64 = 2000;
const MAX_WATCHED: usize = 32;

static TOUCHED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

pub fn touch(wt: &str) {
    TOUCHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(wt.to_string(), now_ms());
}

pub type Entries = BTreeMap<String, (String, i64, u64)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub head: Option<String>,
    pub files: Entries,
    pub subs: BTreeMap<String, (Option<String>, Entries)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sub {
    pub path: String,
    pub gitdir: Option<PathBuf>,
}

thread_local! {
    static STATUS_RUNS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn status_runs() -> usize {
    STATUS_RUNS.with(|c| c.get())
}

fn status(dir: &str, prefix: &str, extra: &[&str]) -> Option<Entries> {
    STATUS_RUNS.with(|c| c.set(c.get() + 1));
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--no-renames",
    ];
    args.extend_from_slice(extra);
    let st = crate::git::git(dir, &args)?;
    let mut files = BTreeMap::new();
    for e in st.split('\0').filter(|e| e.len() > 3) {
        let (code, path) = e.split_at(3);
        let meta = std::fs::metadata(Path::new(dir).join(path)).ok();
        let mtime = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);
        files.insert(
            format!("{prefix}{path}"),
            (
                code.trim().to_string(),
                mtime,
                meta.map(|m| m.len()).unwrap_or(0),
            ),
        );
    }
    Some(files)
}

fn head_of(dir: &str) -> Option<String> {
    crate::git::git(dir, &["rev-parse", "-q", "--verify", "HEAD"])
}

pub fn snapshot(wt: &str) -> Option<Snapshot> {
    Some(Snapshot {
        head: head_of(wt),
        files: status(wt, "", &["--ignore-submodules=dirty"])?,
        subs: BTreeMap::new(),
    })
}

pub fn refresh(wt: &str, prev: Option<&Snapshot>, touched: &BTreeSet<String>) -> Option<Snapshot> {
    let mut snap = snapshot(wt)?;
    snap.subs = prev.map(|p| p.subs.clone()).unwrap_or_default();
    for sub in touched {
        let dir = Path::new(wt).join(sub);
        let dir = dir.to_string_lossy();
        match status(&dir, &format!("{sub}/"), &[]) {
            Some(files) => {
                snap.subs.insert(sub.clone(), (head_of(&dir), files));
            }
            None => {
                snap.subs.remove(sub);
            }
        }
    }
    Some(snap)
}

pub fn submodules(wt: &str) -> Vec<Sub> {
    if !Path::new(wt).join(".gitmodules").is_file() {
        return vec![];
    }
    let Some(out) = crate::git::git(
        wt,
        &[
            "config",
            "-f",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    ) else {
        return vec![];
    };
    out.lines()
        .filter_map(|l| l.split_once(' ').map(|(_, p)| p.trim().to_string()))
        .filter(|p| !p.is_empty())
        .map(|path| {
            let dir = Path::new(wt).join(&path);
            let gitdir = dir
                .join(".git")
                .exists()
                .then(|| {
                    crate::git::git(&dir.to_string_lossy(), &["rev-parse", "--absolute-git-dir"])
                })
                .flatten()
                .map(PathBuf::from);
            Sub { path, gitdir }
        })
        .collect()
}

pub fn sub_of<'a>(wt: &str, path: &Path, subs: &'a [Sub]) -> Option<&'a Sub> {
    subs.iter()
        .filter(|s| {
            path.starts_with(Path::new(wt).join(&s.path))
                || s.gitdir
                    .as_ref()
                    .is_some_and(|g| path == g.join("HEAD") || path == g.join("logs").join("HEAD"))
        })
        .max_by_key(|s| s.path.len())
}

fn outside(wt: &str, subs: &[Sub]) -> Vec<PathBuf> {
    subs.iter()
        .filter_map(|s| s.gitdir.clone())
        .filter(|g| !g.starts_with(wt))
        .flat_map(|g| [g.join("logs"), g])
        .collect()
}

fn names(dir: &str, prefix: &str, x: &str, y: &str, out: &mut BTreeSet<String>) {
    if let Some(n) = crate::git::git(dir, &["diff", "--name-only", "-z", x, y]) {
        out.extend(
            n.split('\0')
                .filter(|s| !s.is_empty())
                .map(|s| format!("{prefix}{s}")),
        );
    }
}

fn diff_entries(a: &Entries, b: &Entries, out: &mut BTreeSet<String>) {
    for (p, v) in b {
        if a.get(p) != Some(v) {
            out.insert(p.clone());
        }
    }
    for p in a.keys() {
        if !b.contains_key(p) {
            out.insert(p.clone());
        }
    }
}

pub fn changed(wt: &str, a: &Snapshot, b: &Snapshot) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    diff_entries(&a.files, &b.files, &mut out);
    if let (Some(x), Some(y)) = (&a.head, &b.head) {
        if x != y {
            names(wt, "", x, y, &mut out);
        }
    }
    let empty = (None, Entries::new());
    for sub in a.subs.keys().chain(b.subs.keys()).collect::<BTreeSet<_>>() {
        let (ha, fa) = a.subs.get(sub).unwrap_or(&empty);
        let (hb, fb) = b.subs.get(sub).unwrap_or(&empty);
        diff_entries(fa, fb, &mut out);
        if let (Some(x), Some(y)) = (ha, hb) {
            if x != y {
                let dir = Path::new(wt).join(sub);
                names(&dir.to_string_lossy(), &format!("{sub}/"), x, y, &mut out);
            }
        }
    }
    out.into_iter().collect()
}

fn watched(state_dir: &Path) -> Vec<String> {
    let now = now_ms();
    let mut set: BTreeSet<String> = BTreeSet::new();
    {
        let mut g = TOUCHED.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        m.retain(|_, at| now - *at <= KEEP_MS);
        set.extend(m.keys().cloned());
    }
    let probe = crate::procs::System;
    let mut ctx = crate::Ctx::from_env(&probe);
    ctx.state_dir = state_dir.to_path_buf();
    set.extend(super::open_worktrees(&ctx));
    set.into_iter()
        .filter(|w| Path::new(w).is_dir())
        .take(MAX_WATCHED)
        .collect()
}

#[derive(Default)]
struct Seen {
    snap: Option<Snapshot>,
    dirty: Option<(i64, i64)>,
    checked: i64,
    native: bool,
    subs: Vec<Sub>,
    touched: BTreeSet<String>,
    extra: Vec<PathBuf>,
    subs_checked: i64,
}

impl Seen {
    fn mark(&mut self, now: i64) {
        self.dirty = Some(self.dirty.map_or((now, now), |(f, _)| (f, now)));
    }

    fn due(&self, now: i64) -> bool {
        match self.dirty {
            Some((first, last)) => now - last >= DEBOUNCE_MS || now - first >= MAX_WAIT_MS,
            None => now - self.checked >= if self.native { SAFETY_MS } else { POLL_MS },
        }
    }
}

pub fn owner<'a>(path: &Path, list: &'a [String]) -> Option<&'a String> {
    list.iter()
        .filter(|w| path.starts_with(w.as_str()))
        .max_by_key(|w| w.len())
        .filter(|w| {
            !path.strip_prefix(w.as_str()).is_ok_and(|r| {
                r.components()
                    .next()
                    .is_some_and(|c| c.as_os_str() == ".git")
            })
        })
}

pub fn route<'a>(
    path: &Path,
    list: &'a [String],
    subs: impl Fn(&str) -> &'a [Sub],
) -> Option<(&'a String, Option<&'a str>)> {
    for wt in list {
        let hit = subs(wt).iter().find(|s| {
            s.gitdir
                .as_ref()
                .is_some_and(|g| path == g.join("HEAD") || path == g.join("logs").join("HEAD"))
        });
        if let Some(s) = hit {
            return Some((wt, Some(s.path.as_str())));
        }
    }
    let wt = owner(path, list)?;
    Some((wt, sub_of(wt, path, subs(wt)).map(|s| s.path.as_str())))
}

fn rescan(native: Option<&mut Native>, wt: &str, s: &mut Seen) {
    use notify::{RecursiveMode, Watcher};
    s.subs = submodules(wt);
    let next = outside(wt, &s.subs);
    if let Some(w) = native {
        for p in s.extra.iter().filter(|p| !next.contains(p)) {
            let _ = w.unwatch(p);
        }
        for p in next.iter().filter(|p| !s.extra.contains(p)) {
            let _ = w.watch(p, RecursiveMode::NonRecursive);
        }
    }
    s.extra = next;
}

fn publish(state_dir: &Path, wt: &str, files: Vec<String>) {
    let probe = crate::procs::System;
    let mut ctx = crate::Ctx::from_env(&probe);
    ctx.state_dir = state_dir.to_path_buf();
    if let Ok(reg) = super::open(&ctx) {
        super::reanchor(&reg, wt);
    }
    crate::board::publish("review.changed", json!({"worktree": wt, "files": files}));
}

type Native = notify::RecommendedWatcher;

fn native_watcher(tx: std::sync::mpsc::Sender<notify::Result<notify::Event>>) -> Option<Native> {
    notify::recommended_watcher(tx).ok()
}

pub fn run(state_dir: PathBuf) {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let native_ok = std::env::var_os("OVERSEER_REVIEW_POLL").is_none_or(|v| v.is_empty());
    let mut native = native_ok.then(|| native_watcher(tx)).flatten();
    let mut seen: HashMap<String, Seen> = HashMap::new();
    let mut list: Vec<String> = vec![];
    let mut listed = 0i64;
    loop {
        let mut events = vec![];
        let wait = if list.is_empty() {
            Duration::from_millis(RELIST_MS as u64)
        } else {
            TICK
        };
        if let Ok(e) = rx.recv_timeout(wait) {
            events.push(e);
            events.extend(rx.try_iter());
        }
        let now = now_ms();
        if now - listed >= RELIST_MS {
            listed = now;
            let next = watched(&state_dir);
            for gone in list.iter().filter(|w| !next.contains(w)) {
                if let Some(s) = seen.remove(gone) {
                    if let Some(w) = native.as_mut() {
                        let _ = w.unwatch(Path::new(gone));
                        for p in &s.extra {
                            let _ = w.unwatch(p);
                        }
                    }
                }
            }
            let fresh: Vec<String> = next
                .iter()
                .filter(|w| !seen.contains_key(*w))
                .cloned()
                .collect();
            for wt in &fresh {
                let ok = native
                    .as_mut()
                    .is_some_and(|w| w.watch(Path::new(wt), RecursiveMode::Recursive).is_ok());
                let mut s = Seen {
                    native: ok,
                    ..Default::default()
                };
                rescan(native.as_mut().filter(|_| ok), wt, &mut s);
                seen.insert(wt.clone(), s);
            }
            list = next;
        }
        let none: &[Sub] = &[];
        let mut hits: Vec<(String, Option<String>, bool)> = vec![];
        for ev in events.into_iter().flatten() {
            for p in &ev.paths {
                let found = route(p, &list, |w| seen.get(w).map_or(none, |s| &s.subs[..]));
                if let Some((wt, sub)) = found {
                    let modules = p == &Path::new(wt).join(".gitmodules");
                    hits.push((wt.clone(), sub.map(str::to_string), modules));
                }
            }
        }
        for (wt, sub, modules) in hits {
            let Some(s) = seen.get_mut(&wt) else { continue };
            if modules {
                rescan(native.as_mut().filter(|_| s.native), &wt, s);
            }
            if let Some(sub) = sub {
                s.touched.insert(sub);
            }
            s.mark(now);
        }
        for wt in &list {
            let Some(s) = seen.get_mut(wt) else { continue };
            if !s.due(now) {
                continue;
            }
            if !s.native && s.dirty.is_none() && now - s.subs_checked >= SAFETY_MS {
                s.subs_checked = now;
                s.touched.extend(s.subs.iter().map(|x| x.path.clone()));
            }
            s.dirty = None;
            s.checked = now;
            let touched = std::mem::take(&mut s.touched);
            let Some(snap) = refresh(wt, s.snap.as_ref(), &touched) else {
                continue;
            };
            match &s.snap {
                None => s.snap = Some(snap),
                Some(prev) if *prev == snap => {}
                Some(prev) => {
                    let files = changed(wt, prev, &snap);
                    s.snap = Some(snap);
                    publish(&state_dir, wt, files);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_changed_paths() {
        let mut a = Snapshot {
            head: Some("h1".into()),
            ..Default::default()
        };
        a.files.insert("x".into(), ("M".into(), 1, 1));
        a.files.insert("gone".into(), ("M".into(), 1, 1));
        let mut b = a.clone();
        b.files.remove("gone");
        b.files.insert("x".into(), ("M".into(), 2, 1));
        b.files.insert("new".into(), ("??".into(), 1, 1));
        assert_eq!(changed("/nonexistent", &a, &b), ["gone", "new", "x"]);
    }

    fn git(d: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "protocol.file.allow=always",
            ])
            .args(args)
            .current_dir(d)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    fn fixture(tag: &str) -> (PathBuf, PathBuf) {
        let d = std::env::temp_dir().join(format!("sb-watch-sub-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let init = |name: &str| {
            let p = d.join(name);
            std::fs::create_dir_all(&p).unwrap();
            git(&p, &["init", "-q", "-b", "main"]);
            std::fs::write(p.join("a.txt"), "1\n").unwrap();
            git(&p, &["add", "-A"]);
            git(&p, &["commit", "-q", "-m", "init"]);
            p
        };
        let lib = init("lib");
        let other = init("other");
        let sup = init("sup");
        for (src, at) in [(&lib, "vendor/lib"), (&other, "vendor/other")] {
            git(&sup, &["submodule", "add", "-q", src.to_str().unwrap(), at]);
        }
        git(&sup, &["commit", "-q", "-m", "subs"]);
        (d, sup)
    }

    #[test]
    fn edits_inside_a_touched_submodule_are_detected_without_scanning_the_others() {
        let (d, sup) = fixture("edit");
        let wt = sup.to_string_lossy().into_owned();
        let subs = submodules(&wt);
        let paths: Vec<&str> = subs.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, ["vendor/lib", "vendor/other"]);
        assert!(subs
            .iter()
            .all(|s| s.gitdir.as_ref().is_some_and(|g| g.join("HEAD").is_file())));
        let all: BTreeSet<String> = paths.iter().map(|s| s.to_string()).collect();
        let base = refresh(&wt, None, &all).unwrap();
        std::fs::write(sup.join("vendor/lib/a.txt"), "dirty\n").unwrap();
        std::fs::write(sup.join("vendor/lib/new.txt"), "n\n").unwrap();
        let list = vec![wt.clone()];
        let p = sup.join("vendor/lib/a.txt");
        let (w, sub) = route(&p, &list, |_| &subs[..]).unwrap();
        assert_eq!((w.as_str(), sub), (wt.as_str(), Some("vendor/lib")));
        let before = status_runs();
        let touched = BTreeSet::from(["vendor/lib".to_string()]);
        let next = refresh(&wt, Some(&base), &touched).unwrap();
        assert_eq!(status_runs() - before, 2);
        assert_eq!(next.subs["vendor/other"], base.subs["vendor/other"]);
        assert_eq!(
            changed(&wt, &base, &next),
            ["vendor/lib/a.txt", "vendor/lib/new.txt"]
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_commit_inside_a_submodule_is_routed_and_detected() {
        let (d, sup) = fixture("commit");
        let wt = sup.to_string_lossy().into_owned();
        let subs = submodules(&wt);
        let touched = BTreeSet::from(["vendor/lib".to_string()]);
        let base = refresh(&wt, None, &touched).unwrap();
        let lib = sup.join("vendor/lib");
        std::fs::write(lib.join("b.txt"), "b\n").unwrap();
        git(&lib, &["add", "-A"]);
        git(&lib, &["commit", "-q", "-m", "work"]);
        let list = vec![wt.clone()];
        let g = subs[0].gitdir.clone().unwrap();
        for p in [g.join("logs").join("HEAD"), g.join("HEAD")] {
            assert_eq!(
                route(&p, &list, |_| &subs[..]).unwrap().1,
                Some("vendor/lib")
            );
        }
        assert_eq!(route(&g.join("index"), &list, |_| &subs[..]), None);
        let next = refresh(&wt, Some(&base), &touched).unwrap();
        assert_ne!(next.subs["vendor/lib"].0, base.subs["vendor/lib"].0);
        let c = changed(&wt, &base, &next);
        assert!(c.contains(&"vendor/lib/b.txt".to_string()), "{c:?}");
        assert!(c.contains(&"vendor/lib".to_string()), "{c:?}");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn submodule_status_work_scales_with_touched_submodules_not_total() {
        let (d, sup) = fixture("perf");
        let wt = sup.to_string_lossy().into_owned();
        let base = refresh(&wt, None, &BTreeSet::new()).unwrap();
        let runs = |touched: &[&str]| {
            let t: BTreeSet<String> = touched.iter().map(|s| s.to_string()).collect();
            let before = status_runs();
            refresh(&wt, Some(&base), &t).unwrap();
            status_runs() - before
        };
        assert_eq!(runs(&[]), 1);
        assert_eq!(runs(&["vendor/lib"]), 2);
        assert_eq!(runs(&["vendor/lib", "vendor/other"]), 3);
        assert!(submodules(&d.join("lib").to_string_lossy()).is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn events_map_to_the_innermost_worktree_outside_git() {
        let list = vec![
            "/w/a".to_string(),
            "/w/a/sub".to_string(),
            "/w/b".to_string(),
        ];
        let o = |p: &str| owner(Path::new(p), &list).cloned();
        assert_eq!(o("/w/a/src/x.rs").as_deref(), Some("/w/a"));
        assert_eq!(o("/w/a/sub/y").as_deref(), Some("/w/a/sub"));
        assert_eq!(o("/w/a/.git/index"), None);
        assert_eq!(o("/w/ab/z"), None);
        assert_eq!(o("/elsewhere"), None);
    }

    #[test]
    fn native_events_reach_the_watcher() {
        use notify::{RecursiveMode, Watcher};
        let d = std::env::temp_dir().join(format!("sb-watch-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut w = native_watcher(tx).unwrap();
        w.watch(&d, RecursiveMode::Recursive).unwrap();
        std::fs::write(d.join("f.txt"), "x").unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        let seen = std::iter::from_fn(|| {
            rx.recv_timeout(until.saturating_duration_since(std::time::Instant::now()))
                .ok()
        })
        .flatten()
        .any(|ev| ev.paths.iter().any(|p| p.ends_with("f.txt")));
        assert!(seen);
        drop(w);
        std::fs::remove_dir_all(d).unwrap();
    }
}
