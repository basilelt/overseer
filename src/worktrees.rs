use crate::board::publish;
use crate::git::git_out;
use crate::git::{git, Git};
use crate::http::res;
use crate::util::percent_decode;
use crate::util::{format_rfc3339, ticket};
use crate::{Ctx, Line};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};

pub type Res<T> = Result<T, (u16, String)>;

static CREATE: Mutex<()> = Mutex::new(());
type Sizes = HashMap<String, (Instant, Option<u64>, bool)>;
static SIZES: Mutex<Option<Sizes>> = Mutex::new(None);
const SIZE_TTL: Duration = Duration::from_secs(600);
const DAY_MS: i64 = 86_400_000;

fn bad(m: impl Into<String>) -> (u16, String) {
    (400, m.into())
}

fn conflict(m: impl Into<String>) -> (u16, String) {
    (409, m.into())
}

pub fn project(path: &str) -> Res<(String, String)> {
    crate::tasks::main_root(&mut Git::default(), path)
        .ok_or_else(|| bad(format!("{path} is not in a git repository")))
}

pub fn store(ctx: &Ctx, root: &str) -> PathBuf {
    crate::layout::store_dir(&ctx.config.worktree_dir, root)
}

fn has_ref(dir: &str, r: &str) -> bool {
    git(dir, &["rev-parse", "--verify", "-q", r]).is_some()
}

fn is_ancestor(dir: &str, a: &str, b: &str) -> bool {
    git(dir, &["merge-base", "--is-ancestor", a, b]).is_some()
}

fn count(dir: &str, args: &[&str]) -> u64 {
    git(dir, args)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn run_timeout(dir: &str, args: &[&str], limit: Duration) -> bool {
    crate::git::run_in("git", dir, args, limit).is_ok_and(|o| o.status.success())
}

static OFFLINE: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
const OFFLINE_FOR: Duration = Duration::from_secs(600);

fn fetch_base(root: &str, def: &str) {
    let skip = OFFLINE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(root)
        .is_some_and(|at| at.elapsed() < OFFLINE_FOR);
    if skip || git(root, &["remote", "get-url", "origin"]).is_none() {
        return;
    }
    let ok = run_timeout(
        root,
        &["fetch", "--quiet", "--no-tags", "origin", def],
        Duration::from_secs(10),
    );
    let mut g = OFFLINE.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if ok {
        m.remove(root);
    } else {
        m.insert(root.to_string(), Instant::now());
    }
}

static DEFAULTS: Mutex<Option<HashMap<String, (String, Instant)>>> = Mutex::new(None);
const DEFAULT_FOR: Duration = Duration::from_secs(600);

pub fn default_branch_cached(root: &str) -> String {
    let hit = DEFAULTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(root)
        .filter(|(_, at)| at.elapsed() < DEFAULT_FOR)
        .map(|(b, _)| b.clone());
    if let Some(b) = hit {
        return b;
    }
    let b = default_branch(root);
    DEFAULTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(root.to_string(), (b.clone(), Instant::now()));
    b
}

pub fn default_branch(root: &str) -> String {
    if let Some(b) = git(
        root,
        &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"],
    )
    .as_deref()
    .and_then(|r| r.strip_prefix("origin/"))
    {
        return b.to_string();
    }
    for b in ["main", "master"] {
        if has_ref(root, &format!("refs/heads/{b}"))
            || has_ref(root, &format!("refs/remotes/origin/{b}"))
        {
            return b.into();
        }
    }
    git(root, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "HEAD".into())
}

type Landed = HashMap<(String, String), (Instant, bool)>;
static LANDED: Mutex<Option<Landed>> = Mutex::new(None);
static LANDING: Mutex<Vec<(String, String)>> = Mutex::new(vec![]);
static LANDING_ONE: Mutex<()> = Mutex::new(());
static FETCHED: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
const LANDED_FOR: Duration = Duration::from_secs(300);
const NOT_LANDED_FOR: Duration = Duration::from_secs(45);

fn landed_fresh(at: Instant, v: bool) -> bool {
    at.elapsed() < if v { LANDED_FOR } else { NOT_LANDED_FOR }
}

pub fn landed(root: &str, branch: &str, fetch: bool) -> bool {
    let key = (root.to_string(), branch.to_string());
    let hit = LANDED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .get(&key)
        .copied();
    if let Some((_, v)) = hit.filter(|&(at, v)| landed_fresh(at, v)) {
        return v;
    }
    {
        let mut f = LANDING.lock().unwrap_or_else(|e| e.into_inner());
        if f.contains(&key) {
            return hit.is_some_and(|h| h.1);
        }
        f.push(key.clone());
    }
    std::thread::spawn(move || {
        let (root, branch) = &key;
        let _one = LANDING_ONE.lock().unwrap_or_else(|e| e.into_inner());
        let due = fetch && fetch_due(root);
        if due {
            fetch_base(root, &default_branch_cached(root));
        }
        let v = branch_merged(root, branch) || {
            let pr = pr_of(root, branch, false);
            !pr.is_null() && landing(root, branch, &pr, due).0
        };
        let mut g = LANDED.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        m.retain(|_, (at, _)| at.elapsed() < LANDED_FOR * 12);
        m.insert(key.clone(), (Instant::now(), v));
        drop(g);
        LANDING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|k| *k != key);
    });
    hit.is_some_and(|h| h.1)
}

pub fn branch_landed(t: &crate::tasks::Task, fetch: bool) -> bool {
    t.branch
        .as_deref()
        .is_some_and(|b| landed(&t.project, b, fetch))
}

pub fn branch_landed_cached(t: &crate::tasks::Task) -> Option<bool> {
    t.branch
        .as_deref()
        .and_then(|b| landed_cached(&t.project, b))
}

pub fn landed_cached(root: &str, branch: &str) -> Option<bool> {
    LANDED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()?
        .get(&(root.to_string(), branch.to_string()))
        .map(|h| h.1)
}

pub fn forget_landed(branch: &str) {
    if let Some(m) = LANDED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.retain(|k, _| k.1 != branch);
    }
}

fn fetch_due(root: &str) -> bool {
    let mut g = FETCHED.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.get(root).is_some_and(|at| at.elapsed() < NOT_LANDED_FOR) {
        return false;
    }
    m.insert(root.to_string(), Instant::now());
    true
}

fn pr_of(root: &str, branch: &str, fresh: bool) -> Value {
    let Ok((_, repo)) = project(root) else {
        return Value::Null;
    };
    let v = if fresh {
        crate::gh::pr_fresh(&repo, branch)
    } else {
        crate::gh::pr_info(&repo, branch)
    };
    v["pr"].clone()
}

fn landing(root: &str, branch: &str, pr: &Value, fetch: bool) -> (bool, String) {
    let def = default_branch_cached(root);
    let base = pr["base"]
        .as_str()
        .filter(|b| !b.is_empty())
        .unwrap_or(&def)
        .to_string();
    if pr["state"] == "MERGED" {
        return (true, base);
    }
    if fetch && base != def {
        fetch_base(root, &base);
    }
    (branch_merged_into(root, branch, &base), base)
}

pub fn task_landed(ctx: &Ctx, id: &str) -> Res<Value> {
    let reg = crate::registry::Registry::open(&ctx.state_dir).map_err(|e| (500, e))?;
    let t = crate::tasks::get(&reg, id)?;
    let branch = t
        .branch
        .filter(|b| !b.is_empty())
        .ok_or_else(|| bad("the task has no branch: nothing to mark as merged"))?;
    let pr = pr_of(&t.project, &branch, true);
    let fetch = ctx.config.worktree_fetch;
    if fetch {
        fetch_base(&t.project, &default_branch_cached(&t.project));
    }
    let (landed, base) = landing(&t.project, &branch, &pr, fetch);
    let landed =
        landed || (base != default_branch_cached(&t.project) && branch_merged(&t.project, &branch));
    if landed {
        forget_landed(&branch);
    }
    let mut v = json!({"landed": landed, "branch": branch, "base": base});
    if !pr.is_null() {
        v["pr"] = json!({"number": pr["number"], "state": pr["state"], "base": pr["base"], "url": pr["url"]});
    }
    Ok(v)
}

pub fn branch_merged(root: &str, branch: &str) -> bool {
    branch_merged_into(root, branch, &default_branch_cached(root))
}

fn branch_merged_into(root: &str, branch: &str, name: &str) -> bool {
    if branch.is_empty() || branch == name || branch == "HEAD" {
        return false;
    }
    let Some((r, tip)) = [
        format!("refs/heads/{branch}"),
        format!("refs/remotes/origin/{branch}"),
    ]
    .into_iter()
    .find_map(|r| git(root, &["rev-parse", "-q", "--verify", &r]).map(|t| (r, t))) else {
        return false;
    };
    let base = start_point(root, name);
    if base == "HEAD" {
        return false;
    }
    if is_ancestor(root, &tip, &base) {
        let created = git(root, &["reflog", "show", "--format=%H", &r])
            .filter(|_| r.starts_with("refs/heads/"))
            .and_then(|s| s.lines().last().map(str::to_string));
        return match created {
            Some(c) => c != tip,
            None => git(root, &["rev-parse", &base]).is_some_and(|b| b != tip),
        };
    }
    git(root, &["cherry", &base, &tip])
        .is_some_and(|s| !s.is_empty() && s.lines().all(|l| l.starts_with('-')))
        || squashed(root, &base, &tip)
}

fn squashed(root: &str, base: &str, tip: &str) -> bool {
    let Some(mb) = git(root, &["merge-base", base, tip]) else {
        return false;
    };
    let Some(own) = patch_ids(root, &["diff-tree", "-p", "--no-color", &mb, tip])
        .into_iter()
        .next()
    else {
        return false;
    };
    let since = git(root, &["log", "-1", "--format=%ct", tip])
        .and_then(|t| t.trim().parse::<i64>().ok())
        .map_or(0, |t| t - 86_400);
    patch_ids(
        root,
        &[
            "log",
            "-p",
            "--no-merges",
            "--no-color",
            "--format=commit %H",
            "-n",
            "500",
            &format!("--since=@{since}"),
            &format!("{mb}..{base}"),
        ],
    )
    .contains(&own)
}

fn patch_ids(root: &str, args: &[&str]) -> Vec<String> {
    let Some(diff) = crate::git::run_in("git", root, args, crate::git::TIMEOUT)
        .ok()
        .filter(|o| o.status.success() && !o.stdout.is_empty())
    else {
        return vec![];
    };
    let Ok(o) = crate::exec::run_stdin(
        Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["patch-id", "--stable"]),
        diff.stdout,
        crate::git::TIMEOUT,
    ) else {
        return vec![];
    };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

pub fn start_point(root: &str, name: &str) -> String {
    let local = format!("refs/heads/{name}");
    let remote = format!("refs/remotes/origin/{name}");
    match (has_ref(root, &local), has_ref(root, &remote)) {
        (true, true) if !is_ancestor(root, &remote, &local) => format!("origin/{name}"),
        (true, _) => name.into(),
        (false, true) => format!("origin/{name}"),
        _ => "HEAD".into(),
    }
}

fn valid_branch(root: &str, b: &str) -> Res<()> {
    if b.starts_with('-') || git(root, &["check-ref-format", "--branch", b]).is_none() {
        return Err(bad(format!("'{b}' is not a valid branch name")));
    }
    Ok(())
}

fn short_time(ms: i64) -> String {
    let t = format_rfc3339(ms);
    format!("{}{}-{}{}", &t[5..7], &t[8..10], &t[11..13], &t[14..16])
}

#[derive(Default)]
pub struct New<'a> {
    pub branch: Option<&'a str>,
    pub base: Option<&'a str>,
    pub slug: Option<&'a str>,
    pub agent: Option<&'a str>,
}

pub fn create(ctx: &Ctx, path: &str, n: &New) -> Res<(String, String, String)> {
    let (root, _) = project(path)?;
    let def = default_branch(&root);
    if n.base.is_none() && ctx.config.worktree_fetch {
        fetch_base(&root, &def);
    }
    let _g = CREATE.lock().unwrap_or_else(|e| e.into_inner());
    let start = match n.base.map(str::trim) {
        Some(b) => {
            if b.is_empty() || b.starts_with('-') {
                return Err(bad("invalid base"));
            }
            git(
                &root,
                &["rev-parse", "--verify", "-q", &format!("{b}^{{commit}}")],
            )
            .ok_or_else(|| bad(format!("unknown base {b}")))?;
            b.to_string()
        }
        None => start_point(&root, &def),
    };
    let store = store(ctx, &root);
    let explicit = n.branch.map(str::trim).filter(|b| !b.is_empty());
    let branch = match explicit {
        Some(b) => {
            valid_branch(&root, b)?;
            b.to_string()
        }
        None => {
            let prefix = crate::layout::session_branch_prefix(n.agent);
            let s =
                n.slug
                    .map(crate::layout::task_slug)
                    .filter(|s| n.slug.is_some_and(|x| !x.trim().is_empty()) && !s.is_empty())
                    .unwrap_or_else(|| match n.agent {
                        Some(_) => short_time(crate::util::now_ms()),
                        None => git(&root, &["rev-parse", "--short", &start])
                            .unwrap_or_else(|| "wt".into()),
                    });
            let mut b = format!("{prefix}{s}");
            let mut i = 2;
            while has_ref(&root, &format!("refs/heads/{b}"))
                || store.join(crate::layout::branch_dir_name(&b)).exists()
            {
                b = format!("{prefix}{s}-{i}");
                i += 1;
                if i > 100 {
                    return Err(conflict("no free branch name"));
                }
            }
            valid_branch(&root, &b)?;
            b
        }
    };
    let dir = store.join(crate::layout::branch_dir_name(&branch));
    if dir.exists() {
        return Err(conflict(format!("{} already exists", dir.display())));
    }
    std::fs::create_dir_all(&store).map_err(|e| (500, format!("{}: {e}", store.display())))?;
    let d = dir.to_string_lossy();
    let r = if has_ref(&root, &format!("refs/heads/{branch}")) {
        git_out(&root, &["worktree", "add", &d, &branch])
    } else if explicit.is_some() && has_ref(&root, &format!("refs/remotes/origin/{branch}")) {
        git_out(
            &root,
            &[
                "worktree",
                "add",
                "--track",
                "-b",
                &branch,
                &d,
                &format!("origin/{branch}"),
            ],
        )
    } else {
        git_out(
            &root,
            &["worktree", "add", "--no-track", "-b", &branch, &d, &start],
        )
        .inspect(|_| {
            let key = format!("branch.{branch}.{}", crate::review::diff::BASE_KEY);
            let _ = git_out(&root, &["config", &key, &start]);
        })
    };
    r.map_err(conflict)?;
    let path = dir
        .canonicalize()
        .unwrap_or(dir)
        .to_string_lossy()
        .into_owned();
    crate::gitgraph::touch(&root);
    Ok((root, path, branch))
}

pub fn announce(root: String, path: String) {
    std::thread::spawn(move || {
        let probe = crate::procs::System;
        let ctx = Ctx::from_env(&probe);
        if let Ok(w) = describe(&ctx, &root, &path) {
            publish("worktree.created", json!({ "worktree": w }));
        }
    });
}

#[derive(Default, Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
    pub bare: bool,
}

pub fn entries(root: &str) -> Vec<Entry> {
    let Some(out) = git(root, &["worktree", "list", "--porcelain"]) else {
        return vec![];
    };
    let mut res: Vec<Entry> = vec![];
    for line in out.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            res.push(Entry {
                path: p.to_string(),
                ..Default::default()
            });
            continue;
        }
        let Some(w) = res.last_mut() else { continue };
        if let Some(h) = line.strip_prefix("HEAD ") {
            w.head = Some(h.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            w.branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
        } else if line == "locked" || line.starts_with("locked ") {
            w.locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            w.prunable = true;
        } else if line == "bare" {
            w.bare = true;
        }
    }
    res
}

use crate::layout::inside;

fn canon(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn mtime_ms(p: &Path) -> Option<i64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

pub(crate) fn admin_dir(path: &str) -> Option<PathBuf> {
    let dotgit = Path::new(path).join(".git");
    if dotgit.is_dir() {
        return Some(dotgit);
    }
    let s = std::fs::read_to_string(&dotgit).ok()?;
    let g = s.trim().strip_prefix("gitdir:")?.trim();
    let p = Path::new(g);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(path).join(p)
    })
}

fn dir_size(p: &Path, budget: &mut u64) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            n += dir_size(&e.path(), budget);
        } else if t.is_file() {
            n += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    n
}

fn size_of(project: &str, path: &str) -> Option<u64> {
    let mut g = SIZES.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    let (at, size, busy) = m
        .get(path)
        .cloned()
        .unwrap_or((Instant::now(), None, false));
    if !busy && (size.is_none() || at.elapsed() >= SIZE_TTL) {
        m.insert(path.to_string(), (at, size, true));
        let (project, path) = (project.to_string(), path.to_string());
        std::thread::spawn(move || {
            let mut budget = 2_000_000;
            let n = dir_size(Path::new(&path), &mut budget);
            SIZES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .insert(path, (Instant::now(), Some(n), false));
            publish("worktree.updated", json!({ "project": project }));
        });
    }
    size
}

pub struct Known {
    pub lines: Vec<Line>,
    pub terms: Vec<(String, String)>,
    pub tasks: Vec<crate::tasks::Task>,
}

impl Known {
    pub fn load(ctx: &Ctx, root: &str) -> Known {
        let tasks = crate::registry::Registry::open(&ctx.state_dir)
            .ok()
            .and_then(|r| crate::tasks::all(&r).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.project == root)
            .collect();
        let terms = crate::term::all()
            .iter()
            .filter(|t| !t.exited())
            .map(|t| {
                let j = t.to_json();
                (
                    j["cwd"].as_str().unwrap_or_default().to_string(),
                    j["id"].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        Known {
            lines: crate::daemon::lines(ctx),
            terms,
            tasks,
        }
    }
}

fn owner<'a>(paths: &'a [String], p: &str) -> Option<&'a str> {
    paths
        .iter()
        .filter(|w| inside(p, w))
        .max_by_key(|w| w.len())
        .map(String::as_str)
}

struct Facts {
    dirty: u64,
    ahead: u64,
    behind: u64,
    unpushed: u64,
    pushed: bool,
    merged_git: bool,
}

fn facts(root: &str, e: &Entry, base: &str, main: bool) -> Facts {
    let mut f = Facts {
        dirty: 0,
        ahead: 0,
        behind: 0,
        unpushed: 0,
        pushed: false,
        merged_git: false,
    };
    if Path::new(&e.path).is_dir() {
        f.dirty = git(
            &e.path,
            &["status", "--porcelain", "--untracked-files=normal"],
        )
        .map(|s| s.lines().filter(|l| !l.is_empty()).count() as u64)
        .unwrap_or(0);
    }
    let Some(tip) = e.head.as_deref() else {
        return f;
    };
    if let Some(lr) = git(
        root,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{base}...{tip}"),
        ],
    ) {
        let mut it = lr.split_whitespace().map(|x| x.parse().unwrap_or(0));
        f.behind = it.next().unwrap_or(0);
        f.ahead = it.next().unwrap_or(0);
    }
    f.unpushed = count(
        root,
        &["rev-list", "--count", tip, "--not", base, "--remotes"],
    );
    f.pushed = git(root, &["rev-list", "-n", "1", tip, "--not", "--remotes"])
        .is_some_and(|s| s.trim().is_empty());
    if !main {
        if let Some(b) = &e.branch {
            let created = git(
                root,
                &["reflog", "show", "--format=%H", &format!("refs/heads/{b}")],
            )
            .and_then(|s| s.lines().last().map(str::to_string));
            let moved = match created {
                Some(c) => c != tip,
                None => git(root, &["rev-parse", base]).is_some_and(|b| b != tip),
            };
            f.merged_git = moved && is_ancestor(root, tip, base);
        }
    }
    f
}

struct Shared<'a> {
    root: &'a str,
    repo: &'a str,
    base_name: &'a str,
    base: &'a str,
    store: String,
    managed_root: crate::layout::ManagedRoot,
    stale_ms: i64,
    now_ms: i64,
    known: &'a Known,
    paths: Vec<String>,
}

fn json_of(s: &Shared, e: &Entry) -> Value {
    let main = e.path == s.root;
    let canon_path = canon(Path::new(&e.path));
    let tasks: Vec<&crate::tasks::Task> = s
        .known
        .tasks
        .iter()
        .filter(|t| {
            !t.from_session()
                && t.worktree_path
                    .as_deref()
                    .is_some_and(|w| w == e.path || w == canon_path)
        })
        .collect();
    let kind = if main {
        "main"
    } else if !tasks.is_empty() || s.managed_root.holds_checkout(&e.path, &canon_path) {
        "managed"
    } else {
        "orphan"
    };
    let lines: Vec<&Line> = s
        .known
        .lines
        .iter()
        .filter(|l| {
            let p = if l.cwd.is_empty() {
                &l.worktree_path
            } else {
                &l.cwd
            };
            owner(&s.paths, p) == Some(e.path.as_str())
        })
        .collect();
    let terms: Vec<&String> = s
        .known
        .terms
        .iter()
        .filter(|(cwd, _)| owner(&s.paths, cwd) == Some(e.path.as_str()))
        .map(|(_, id)| id)
        .collect();
    let task = tasks
        .iter()
        .max_by_key(|t| t.updated_ms)
        .copied()
        .or_else(|| {
            s.known
                .tasks
                .iter()
                .filter(|t| main && !t.from_session() && t.worktree_path.as_deref() == Some(s.root))
                .max_by_key(|t| t.updated_ms)
        });
    let missing = e.prunable || !Path::new(&e.path).is_dir();
    let f = if missing {
        Facts {
            dirty: 0,
            ahead: 0,
            behind: 0,
            unpushed: 0,
            pushed: false,
            merged_git: false,
        }
    } else {
        facts(s.root, e, s.base, main)
    };
    let pr = match (&e.branch, main) {
        (Some(b), false) => {
            let project = s.root.to_string();
            crate::gh::cached_or_fetch(s.repo, b, move || {
                publish("worktree.updated", json!({ "project": project }))
            })["pr"]
                .clone()
        }
        _ => Value::Null,
    };
    let pr_merged = pr["state"] == "MERGED";
    let merged = f.merged_git || pr_merged;
    let in_use = lines.iter().any(|l| l.alive())
        || !terms.is_empty()
        || task.is_some_and(|t| t.status == "in_progress");
    let mut last = admin_dir(&e.path)
        .map(|a| {
            [a.join("index"), a.join("logs/HEAD"), a.join("HEAD")]
                .iter()
                .filter_map(|p| mtime_ms(p))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    for l in &lines {
        last = last.max(l.last_activity_ms);
    }
    let state = if missing {
        "missing"
    } else if in_use {
        "in_use"
    } else if !main && f.dirty == 0 && (merged || (last > 0 && s.now_ms - last > s.stale_ms)) {
        "stale"
    } else {
        "idle"
    };
    let tk = task
        .and_then(|t| t.ticket_key.clone())
        .or_else(|| lines.iter().find_map(|l| l.ticket.clone()))
        .or_else(|| e.branch.as_deref().and_then(ticket));
    let mut v = json!({
        "id": e.path, "path": e.path, "project": s.root, "branch": e.branch, "kind": kind,
        "base": s.base_name, "state": state, "dirty": f.dirty > 0, "changes": f.dirty,
        "ahead": f.ahead, "behind": f.behind, "merged": merged, "pushed": f.pushed,
        "unpushed": f.unpushed, "locked": e.locked,
        "lines": lines.iter().map(|l| json!({
            "id": l.id, "agent": l.agent, "state": l.state, "busy": l.busy, "needsInput": l.needs_input,
        })).collect::<Vec<_>>(),
        "termIds": terms,
        "sizeBytes": if missing { None } else { size_of(s.root, &e.path) },
    });
    if let Some(h) = &e.head {
        v["head"] = json!(h);
    }
    if let Some(t) = task {
        v["task"] = json!({"id": t.id, "number": t.number, "title": t.title, "status": t.status});
    }
    if let Some(k) = tk {
        v["ticketKey"] = json!(k);
    }
    if !pr.is_null() {
        v["pr"] = json!({"number": pr["number"], "state": pr["state"], "isDraft": pr["isDraft"], "url": pr["url"]});
    }
    if last > 0 {
        v["lastActivityAt"] = json!(format_rfc3339(last));
    }
    v
}

pub fn list(ctx: &Ctx, path: &str) -> Res<Value> {
    let (root, repo) = project(path)?;
    crate::gitgraph::touch(&root);
    let known = Known::load(ctx, &root);
    let all = entries(&root);
    let base_name = default_branch(&root);
    let base = start_point(&root, &base_name);
    let store = store(ctx, &root);
    let managed_root = crate::layout::ManagedRoot::new(&ctx.config.worktree_dir);
    let shared = Shared {
        root: &root,
        repo: &repo,
        base_name: &base_name,
        base: &base,
        store: store.to_string_lossy().into_owned(),
        managed_root,
        stale_ms: ctx.config.worktree_stale_days.max(0) * DAY_MS,
        now_ms: crate::util::now_ms(),
        known: &known,
        paths: all.iter().map(|e| e.path.clone()).collect(),
    };
    let items: Vec<Value> = std::thread::scope(|sc| {
        let hs: Vec<_> = all
            .iter()
            .filter(|e| !e.bare)
            .map(|e| sc.spawn(|| json_of(&shared, e)))
            .collect();
        hs.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    let (mut mine, orphans): (Vec<Value>, Vec<Value>) =
        items.into_iter().partition(|v| v["kind"] != "orphan");
    mine.sort_by(|a, b| {
        (b["kind"] == "main")
            .cmp(&(a["kind"] == "main"))
            .then_with(|| {
                b["lastActivityAt"]
                    .as_str()
                    .cmp(&a["lastActivityAt"].as_str())
            })
    });
    Ok(json!({
        "project": root, "store": shared.store, "base": base_name,
        "worktrees": mine, "orphans": orphans,
    }))
}

pub fn describe(ctx: &Ctx, root: &str, path: &str) -> Res<Value> {
    let l = list(ctx, root)?;
    let c = canon(Path::new(path));
    l["worktrees"]
        .as_array()
        .into_iter()
        .chain(l["orphans"].as_array())
        .flatten()
        .find(|w| w["path"] == path || w["path"] == c.as_str())
        .cloned()
        .ok_or_else(|| (404, format!("{path} is not a worktree of {root}")))
}

fn locate(ctx: &Ctx, id: &str, hint: Option<&str>) -> Res<(String, Value)> {
    for p in hint.into_iter().chain(Path::new(id).is_dir().then_some(id)) {
        if let Ok((root, _)) = project(p) {
            if let Ok(w) = describe(ctx, &root, id) {
                return Ok((root, w));
            }
        }
    }
    let lines = crate::daemon::lines(ctx);
    for p in crate::tasks::projects(ctx, &lines)? {
        let Some(root) = p["path"].as_str() else {
            continue;
        };
        if entries(root).iter().any(|e| e.path == id) {
            return Ok((root.to_string(), describe(ctx, root, id)?));
        }
    }
    Err((404, format!("{id} is not a worktree of a known repository")))
}

pub fn refusal(w: &Value) -> Option<String> {
    let n = |k: &str| w[k].as_u64().unwrap_or(0);
    let base = w["base"].as_str().unwrap_or("the base");
    Some(match () {
        _ if w["kind"] == "main" => "this is the main checkout".into(),
        _ if w["kind"] == "orphan" => {
            "created outside Overseer: it is listed, never touched".into()
        }
        _ if w["locked"] == true => "the worktree is locked (git worktree unlock)".into(),
        _ if w["state"] == "missing" => return None,
        _ if w["state"] == "in_use" => {
            let what = if w["lines"].as_array().is_some_and(|a| !a.is_empty()) {
                "an agent session"
            } else if w["termIds"].as_array().is_some_and(|a| !a.is_empty()) {
                "a running terminal"
            } else {
                "a task in progress"
            };
            format!("in use by {what}")
        }
        _ if n("changes") > 0 => format!("{} uncommitted change(s)", n("changes")),
        _ if n("unpushed") > 0 => format!(
            "{} commit(s) neither pushed nor merged into {base}",
            n("unpushed")
        ),
        _ => return None,
    })
}

pub fn plan(w: &Value) -> Value {
    let id = w["id"].clone();
    if let Some(r) = refusal(w) {
        return json!({"id": id, "refused": r, "remove": [], "keep": []});
    }
    if w["state"] == "missing" {
        return json!({"id": id, "refused": null, "remove": [{"kind": "prune", "path": id}], "keep": []});
    }
    let mut remove = vec![json!({"kind": "worktree", "path": id})];
    let mut keep = vec![];
    if let Some(b) = w["branch"].as_str() {
        if w["merged"] == true || w["ahead"] == 0 {
            remove.push(json!({"kind": "branch", "name": b}));
        } else {
            keep.push(json!({"kind": "branch", "name": b, "reason": format!(
                "not merged into {}", w["base"].as_str().unwrap_or("the base")
            )}));
        }
    }
    json!({"id": id, "refused": null, "remove": remove, "keep": keep})
}

fn prune_one(root: &str, path: &str) -> Result<(), String> {
    let common = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok_or("cannot find the repository")?;
    let want = Path::new(path).join(".git");
    let dir = Path::new(&common).join("worktrees");
    for e in std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let gd = std::fs::read_to_string(e.path().join("gitdir")).unwrap_or_default();
        if Path::new(gd.trim()) == want {
            return std::fs::remove_dir_all(e.path()).map_err(|e| e.to_string());
        }
    }
    Err(format!("no administrative entry for {path}"))
}

pub fn cleanup(ctx: &Ctx, id: &str, dry: bool, hint: Option<&str>) -> Res<Value> {
    cleanup_where(ctx, id, dry, hint, &|_| true)
}

pub fn cleanup_where(
    ctx: &Ctx,
    id: &str,
    dry: bool,
    hint: Option<&str>,
    deletable: &dyn Fn(&str) -> bool,
) -> Res<Value> {
    let (root, w) = locate(ctx, id, hint)?;
    let mut p = plan(&w);
    let mut keep: Vec<Value> = p["keep"].as_array().cloned().unwrap_or_default();
    let mut remove = vec![];
    for item in p["remove"].as_array().cloned().unwrap_or_default() {
        match item["name"].as_str() {
            Some(b) if item["kind"] == "branch" && !deletable(b) => {
                keep.push(json!({"kind": "branch", "name": b, "reason": "not created by Overseer"}))
            }
            _ => remove.push(item),
        }
    }
    p["remove"] = json!(remove);
    p["keep"] = json!(keep);
    p["dryRun"] = json!(dry);
    if dry {
        return Ok(p);
    }
    if let Some(r) = p["refused"].as_str() {
        return Err(conflict(r.to_string()));
    }
    let path = w["path"].as_str().unwrap_or(id).to_string();
    if w["state"] == "missing" {
        prune_one(&root, &path).map_err(conflict)?;
    } else {
        git_out(&root, &["worktree", "remove", &path]).map_err(conflict)?;
    }
    let mut remove = vec![];
    let mut keep: Vec<Value> = p["keep"].as_array().cloned().unwrap_or_default();
    for item in p["remove"].as_array().cloned().unwrap_or_default() {
        if item["kind"] != "branch" {
            remove.push(item);
            continue;
        }
        let b = item["name"].as_str().unwrap_or_default();
        match git_out(&root, &["branch", "-D", b]) {
            Ok(_) => remove.push(item),
            Err(e) => keep.push(json!({"kind": "branch", "name": b, "reason": e})),
        }
    }
    p["remove"] = json!(remove);
    p["keep"] = json!(keep);
    publish("worktree.removed", json!({"project": root, "id": path}));
    Ok(p)
}

fn usable(ctx: &Ctx, id: &str) -> Res<(String, Value)> {
    let (root, w) = locate(ctx, id, None)?;
    if w["state"] == "missing" {
        return Err(conflict("the worktree directory is missing"));
    }
    if w["changes"].as_u64().unwrap_or(0) > 0 {
        return Err(conflict(format!(
            "{} uncommitted change(s): commit or stash them first",
            w["changes"]
        )));
    }
    Ok((root, w))
}

fn conflicts(dir: &str) -> String {
    git(dir, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

pub fn merge_base(ctx: &Ctx, id: &str) -> Res<Value> {
    let (root, w) = usable(ctx, id)?;
    let path = w["path"].as_str().unwrap_or(id).to_string();
    let base = start_point(&root, &default_branch(&root));
    if let Err(e) = git_out(&path, &["merge", "--no-edit", &base]) {
        let files = conflicts(&path);
        let _ = git_out(&path, &["merge", "--abort"]);
        return Err(conflict(if files.is_empty() {
            e
        } else {
            format!("merge conflicts in {files}; the merge was aborted")
        }));
    }
    publish("worktree.updated", json!({"project": root}));
    describe(ctx, &root, &path)
}

pub fn valid_sha(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn cherry_pick(ctx: &Ctx, id: &str, sha: &str) -> Res<Value> {
    if !valid_sha(sha) {
        return Err(bad("sha must be 4-64 hex digits"));
    }
    let (root, w) = usable(ctx, id)?;
    let path = w["path"].as_str().unwrap_or(id).to_string();
    let full = git(
        &root,
        &["rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")],
    )
    .ok_or_else(|| (404, format!("unknown commit {sha}")))?;
    let parents = git(&root, &["rev-list", "--parents", "-n", "1", &full])
        .map(|s| s.split_whitespace().count().saturating_sub(1))
        .unwrap_or(1);
    let mut args = vec!["cherry-pick", "-x"];
    if parents > 1 {
        args.extend(["-m", "1"]);
    }
    args.push(&full);
    if let Err(e) = git_out(&path, &args) {
        let files = conflicts(&path);
        let _ = git_out(&path, &["cherry-pick", "--abort"]);
        return Err(conflict(if files.is_empty() {
            e
        } else {
            format!("conflicts in {files}; the cherry-pick was aborted")
        }));
    }
    let commit = git(&path, &["rev-parse", "HEAD"]).unwrap_or_default();
    publish("worktree.updated", json!({"project": root}));
    Ok(json!({"commit": commit, "worktree": describe(ctx, &root, &path)?}))
}

pub fn for_term(ctx: &Ctx, cwd: &str, agent: &str, opt: &Value) -> Res<String> {
    let s = |k: &str| opt[k].as_str();
    let (root, path, _) = create(
        ctx,
        cwd,
        &New {
            branch: s("branch"),
            base: s("base"),
            slug: s("slug"),
            agent: Some(agent),
        },
    )
    .map_err(|(c, e)| (if c == 400 { 400 } else { 409 }, e))?;
    announce(root, path.clone());
    Ok(path)
}

pub fn routes(ctx: &Ctx, method: &str, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    let id = |s: &str| percent_decode(s);
    Some(match (method, segs) {
        ("GET", ["v1", "projects", p, "worktrees"]) => res(200, list(ctx, &id(p))),
        ("POST", ["v1", "worktrees"]) => {
            let s = |k: &str| body[k].as_str();
            let Some(project) = s("project").filter(|p| !p.is_empty()) else {
                return Some(res(200, Err(bad("project required"))));
            };
            let r = create(
                ctx,
                project,
                &New {
                    branch: s("branch"),
                    base: s("base"),
                    slug: s("slug"),
                    agent: s("agent"),
                },
            )
            .and_then(|(root, path, _)| {
                let w = describe(ctx, &root, &path)?;
                publish("worktree.created", json!({ "worktree": w }));
                Ok(w)
            });
            res(201, r)
        }
        ("POST", ["v1", "worktrees", w, "cleanup"]) => res(
            200,
            cleanup(
                ctx,
                &id(w),
                body["dryRun"] != false,
                body["project"].as_str(),
            ),
        ),
        ("POST", ["v1", "worktrees", w, "merge-base"]) => res(200, merge_base(ctx, &id(w))),
        ("POST", ["v1", "worktrees", w, "cherry-pick"]) => res(
            200,
            cherry_pick(ctx, &id(w), body["sha"].as_str().unwrap_or_default()),
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn run_timeout_kills_the_children_of_a_hung_git() {
        let d = std::env::temp_dir().join(format!("sb-wt-hang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let pid = d.join("pid");
        let alias = format!(
            "alias.hang=!sleep 300 & echo $! > '{}'; wait",
            pid.display()
        );
        let begin = Instant::now();
        assert!(!run_timeout(
            d.to_str().unwrap(),
            &["-c", &alias, "hang"],
            Duration::from_millis(500)
        ));
        assert!(begin.elapsed() < Duration::from_secs(5));
        let child: i32 = std::fs::read_to_string(&pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = (0..100).any(|_| {
            let alive = crate::procs::alive_not_zombie(child);
            if alive {
                std::thread::sleep(Duration::from_millis(20));
            }
            !alive
        });
        assert!(gone, "sleep {child} outlived the timeout");
        assert!(run_timeout(
            d.to_str().unwrap(),
            &["-c", "alias.ok=!true", "ok"],
            Duration::from_secs(5)
        ));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn names() {
        assert_eq!(
            crate::layout::branch_dir_name("task/7-fix-login"),
            "task-7-fix-login"
        );
        assert_eq!(
            crate::layout::branch_dir_name("sb/claude/1029-0930"),
            "sb-claude-1029-0930"
        );
        assert_eq!(
            crate::layout::branch_dir_name("feat/AS-12_x..y"),
            "feat-AS-12_x..y"
        );
        assert_eq!(crate::layout::branch_dir_name("//"), "worktree");
        assert_eq!(crate::layout::store_hash("/home/u/src/x").len(), 8);
        assert_ne!(
            crate::layout::store_hash("/a"),
            crate::layout::store_hash("/b")
        );
        assert_eq!(short_time(0), "0101-0000");
    }

    #[test]
    fn cleanup_plans() {
        let base = json!({"id": "/w/x", "path": "/w/x", "kind": "managed", "state": "stale",
            "base": "main", "changes": 0, "unpushed": 0, "locked": false, "merged": true,
            "branch": "sb/claude/x", "lines": [], "termIds": []});
        let p = plan(&base);
        assert!(p["refused"].is_null());
        assert_eq!(p["remove"].as_array().unwrap().len(), 2);
        let mut w = base.clone();
        w["merged"] = json!(false);
        w["ahead"] = json!(1);
        assert_eq!(plan(&w)["keep"][0]["reason"], "not merged into main");
        w["ahead"] = json!(0);
        assert_eq!(plan(&w)["remove"][1]["kind"], "branch");
        for (k, v, why) in [
            ("kind", json!("main"), "main checkout"),
            ("kind", json!("orphan"), "outside Overseer"),
            ("locked", json!(true), "locked"),
            ("state", json!("in_use"), "in use"),
            ("changes", json!(3), "3 uncommitted"),
            ("unpushed", json!(2), "2 commit(s) neither pushed"),
        ] {
            let mut w = base.clone();
            w[k] = v;
            let p = plan(&w);
            assert!(
                p["refused"].as_str().unwrap().contains(why),
                "{k}: {}",
                p["refused"]
            );
            assert!(p["remove"].as_array().unwrap().is_empty());
        }
        let mut w = base.clone();
        w["state"] = json!("missing");
        assert_eq!(plan(&w)["remove"][0]["kind"], "prune");
    }

    fn sh(d: &Path, args: &[&str]) {
        let mut c = crate::git::fixture::cmd(d);
        c.env("GIT_AUTHOR_DATE", "2020-01-02T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2020-01-02T00:00:00Z");
        crate::git::fixture::checked(c, args);
    }

    fn commit(d: &Path, file: &str, body: &str) {
        std::fs::write(d.join(file), body).unwrap();
        sh(d, &["add", "-A"]);
        sh(d, &["commit", "-q", "-m", file]);
    }

    fn scenario(tag: &str, land: impl Fn(&Path)) -> (PathBuf, bool) {
        let d = std::env::temp_dir().join(format!("sb-landed-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let up = d.join("up");
        std::fs::create_dir_all(&up).unwrap();
        sh(&up, &["init", "-q", "-b", "main"]);
        commit(&up, "a.txt", "a\n");
        sh(&d, &["clone", "-q", "up", "work"]);
        let work = d.join("work");
        sh(&work, &["checkout", "-q", "-b", "feat/x"]);
        commit(&work, "x1.txt", "x1\n");
        commit(&work, "x2.txt", "x2\n");
        sh(&work, &["push", "-q", "origin", "feat/x"]);
        sh(&work, &["checkout", "-q", "main"]);
        land(&up);
        let w = work.to_str().unwrap();
        let before = branch_merged(w, "feat/x");
        sh(&work, &["fetch", "-q", "origin"]);
        assert!(!before, "{tag}: merged before fetch");
        (d, branch_merged(w, "feat/x"))
    }

    #[test]
    fn landed_branches_are_detected_on_the_upstream_default_branch() {
        type Case = (&'static str, bool, fn(&Path));
        let cases: [Case; 7] = [
            ("merge", true, |up| {
                commit(up, "m.txt", "m\n");
                sh(up, &["merge", "-q", "--no-ff", "-m", "merge", "feat/x"]);
            }),
            ("ff", true, |up| {
                sh(up, &["merge", "-q", "--ff-only", "feat/x"])
            }),
            ("squash", true, |up| {
                commit(up, "m.txt", "m\n");
                sh(up, &["merge", "-q", "--squash", "feat/x"]);
                sh(up, &["commit", "-q", "-m", "squash"]);
                commit(up, "later.txt", "l\n");
            }),
            ("rebase", true, |up| {
                commit(up, "m.txt", "m\n");
                sh(up, &["cherry-pick", "feat/x~1", "feat/x"]);
            }),
            ("open", false, |up| commit(up, "m.txt", "m\n")),
            ("elsewhere", false, |up| {
                sh(up, &["checkout", "-q", "-b", "release/next"]);
                sh(up, &["merge", "-q", "--no-ff", "-m", "merge", "feat/x"]);
                sh(up, &["checkout", "-q", "main"]);
            }),
            ("none", false, |_| {}),
        ];
        for (tag, want, land) in cases {
            let (d, got) = scenario(tag, land);
            assert_eq!(got, want, "{tag}");
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn landing_follows_the_pull_request_target_branch() {
        let (d, got) = scenario("pr-base", |up| {
            sh(up, &["checkout", "-q", "-b", "next"]);
            sh(up, &["merge", "-q", "--no-ff", "-m", "merge", "feat/x"]);
            sh(up, &["checkout", "-q", "main"]);
        });
        assert!(!got);
        let w = d.join("work");
        let w = w.to_str().unwrap();
        let open = |base: &str| json!({"state": "OPEN", "base": base});
        assert_eq!(
            landing(w, "feat/x", &open("next"), false),
            (true, "next".into())
        );
        assert_eq!(
            landing(w, "feat/x", &open("main"), false),
            (false, "main".into())
        );
        assert_eq!(
            landing(w, "feat/x", &Value::Null, false),
            (false, "main".into())
        );
        assert_eq!(
            landing(
                w,
                "feat/x",
                &json!({"state": "MERGED", "base": "gone"}),
                false
            ),
            (true, "gone".into())
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn negative_landed_results_expire_before_positive_ones() {
        let old = |s| Instant::now().checked_sub(Duration::from_secs(s)).unwrap();
        assert!(landed_fresh(old(10), false));
        assert!(!landed_fresh(old(60), false));
        assert!(landed_fresh(old(60), true));
        assert!(!landed_fresh(old(400), true));
    }

    #[test]
    fn deleted_and_empty_branches() {
        let (d, got) = scenario("deleted", |up| {
            sh(up, &["merge", "-q", "--no-ff", "-m", "merge", "feat/x"]);
        });
        assert!(got);
        let work = d.join("work");
        let w = work.to_str().unwrap();
        sh(&work, &["branch", "-q", "-D", "feat/x"]);
        assert!(branch_merged(w, "feat/x"));
        sh(&work, &["update-ref", "-d", "refs/remotes/origin/feat/x"]);
        assert!(!branch_merged(w, "feat/x"));
        sh(&work, &["branch", "-q", "feat/empty", "origin/main"]);
        assert!(!branch_merged(w, "feat/empty"));
        assert!(!branch_merged(w, "main"));
        let _ = std::fs::remove_dir_all(d);
    }
}
