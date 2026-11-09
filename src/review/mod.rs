pub mod anchor;
pub mod deliver;
pub mod diff;
pub mod graph;
pub mod lang;
pub mod map;
pub mod symbols;
pub mod triage;
pub mod watch;

use crate::board::{decode, publish};
use crate::daemon::{q, Request};
use crate::registry::Registry;
use crate::util::{format_rfc3339, ulid};
use crate::{Ctx, Line};
use anchor::{capture, locate, Anchor, Found};
use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::sync::Mutex;

type Res<T> = Result<T, (u16, String)>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS review_threads (
  id TEXT PRIMARY KEY,
  worktree TEXT NOT NULL,
  path TEXT NOT NULL,
  side TEXT NOT NULL,
  line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  orig_line INTEGER NOT NULL,
  orig_end_line INTEGER NOT NULL,
  code TEXT NOT NULL,
  ctx_before TEXT NOT NULL,
  ctx_after TEXT NOT NULL,
  status TEXT NOT NULL,
  outdated INTEGER NOT NULL DEFAULT 0,
  changed INTEGER NOT NULL DEFAULT 0,
  sent INTEGER NOT NULL DEFAULT 0,
  line_id TEXT,
  task_id TEXT,
  delivery_id TEXT,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS review_threads_worktree ON review_threads (worktree);
CREATE TABLE IF NOT EXISTS review_comments (
  id TEXT PRIMARY KEY,
  thread_id TEXT NOT NULL,
  author TEXT NOT NULL,
  body TEXT NOT NULL,
  pending INTEGER NOT NULL,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS review_comments_thread ON review_comments (thread_id);
CREATE TABLE IF NOT EXISTS review_deliveries (
  id TEXT PRIMARY KEY,
  worktree TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  target TEXT,
  reason TEXT,
  line_id TEXT,
  task_id TEXT,
  text TEXT NOT NULL,
  threads TEXT NOT NULL,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS review_viewed (
  worktree TEXT NOT NULL,
  path TEXT NOT NULL,
  hash TEXT NOT NULL,
  PRIMARY KEY (worktree, path)
);
";

const TCOLS: &str = "id, worktree, path, side, line, end_line, orig_line, orig_end_line, code, ctx_before, ctx_after, status, outdated, changed, sent, line_id, task_id, delivery_id, created_ms, updated_ms";

static LINES: Mutex<Vec<Line>> = Mutex::new(vec![]);
static DELIVER: Mutex<()> = Mutex::new(());
static QUEUE: crate::deliver::Queue = crate::deliver::Queue::new();

#[derive(Clone, Debug, Default)]
pub struct Thread {
    pub id: String,
    pub worktree: String,
    pub path: String,
    pub side: String,
    pub line: usize,
    pub end: usize,
    pub orig_line: usize,
    pub orig_end: usize,
    pub code: Vec<String>,
    pub before: Vec<u64>,
    pub after: Vec<u64>,
    pub status: String,
    pub outdated: bool,
    pub changed: bool,
    pub sent: bool,
    pub line_id: Option<String>,
    pub task_id: Option<String>,
    pub delivery_id: Option<String>,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn hashes(s: &str) -> Vec<u64> {
    s.split(',')
        .filter_map(|x| u64::from_str_radix(x, 16).ok())
        .collect()
}

fn unhash(v: &[u64]) -> String {
    v.iter()
        .map(|h| format!("{h:x}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn trow(r: &Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get(0)?,
        worktree: r.get(1)?,
        path: r.get(2)?,
        side: r.get(3)?,
        line: r.get::<_, i64>(4)? as usize,
        end: r.get::<_, i64>(5)? as usize,
        orig_line: r.get::<_, i64>(6)? as usize,
        orig_end: r.get::<_, i64>(7)? as usize,
        code: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or_default(),
        before: hashes(&r.get::<_, String>(9)?),
        after: hashes(&r.get::<_, String>(10)?),
        status: r.get(11)?,
        outdated: r.get(12)?,
        changed: r.get(13)?,
        sent: r.get(14)?,
        line_id: r.get(15)?,
        task_id: r.get(16)?,
        delivery_id: r.get(17)?,
        created_ms: r.get(18)?,
        updated_ms: r.get(19)?,
    })
}

fn sql<T>(r: rusqlite::Result<T>) -> Res<T> {
    r.map_err(|e| (500, e.to_string()))
}

fn bad(m: impl Into<String>) -> (u16, String) {
    (400, m.into())
}

pub fn open(ctx: &Ctx) -> Res<Registry> {
    let reg = Registry::open(&ctx.state_dir).map_err(|e| (500, e))?;
    sql(reg.conn.execute_batch(SCHEMA))?;
    Ok(reg)
}

fn save(reg: &Registry, t: &Thread) -> Res<()> {
    sql(reg.conn.execute(
        &format!("INSERT OR REPLACE INTO review_threads ({TCOLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)"),
        params![
            t.id, t.worktree, t.path, t.side, t.line as i64, t.end as i64, t.orig_line as i64,
            t.orig_end as i64, serde_json::to_string(&t.code).unwrap_or_default(), unhash(&t.before),
            unhash(&t.after), t.status, t.outdated, t.changed, t.sent, t.line_id, t.task_id,
            t.delivery_id, t.created_ms, t.updated_ms
        ],
    ))
    .map(|_| ())
}

fn threads_where(reg: &Registry, cond: &str, p: &[&dyn rusqlite::ToSql]) -> Res<Vec<Thread>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "SELECT {TCOLS} FROM review_threads {cond} ORDER BY worktree, path, line, created_ms"
    )))?;
    let rows = sql(st.query_map(p, trow))?;
    sql(rows.collect())
}

fn thread(reg: &Registry, id: &str) -> Res<Thread> {
    threads_where(reg, "WHERE id = ?1", &[&id])?
        .pop()
        .ok_or_else(|| (404, format!("no review thread {id}")))
}

#[derive(Clone, Debug)]
pub struct Comment {
    pub id: String,
    pub thread_id: String,
    pub author: String,
    pub body: String,
    pub pending: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn comments(reg: &Registry, thread_id: &str) -> Res<Vec<Comment>> {
    let mut st = sql(reg.conn.prepare(
        "SELECT id, thread_id, author, body, pending, created_ms, updated_ms FROM review_comments
         WHERE thread_id = ?1 ORDER BY created_ms, id",
    ))?;
    let rows = sql(st.query_map([thread_id], |r| {
        Ok(Comment {
            id: r.get(0)?,
            thread_id: r.get(1)?,
            author: r.get(2)?,
            body: r.get(3)?,
            pending: r.get(4)?,
            created_ms: r.get(5)?,
            updated_ms: r.get(6)?,
        })
    }))?;
    sql(rows.collect())
}

fn add_comment(
    reg: &Registry,
    thread_id: &str,
    author: &str,
    body: &str,
    pending: bool,
) -> Res<()> {
    let now = crate::procs::now_ms();
    sql(reg.conn.execute(
        "INSERT INTO review_comments (id, thread_id, author, body, pending, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            format!("rc-{}", ulid(now)),
            thread_id,
            author,
            body,
            pending,
            now
        ],
    ))
    .map(|_| ())
}

fn delivery_json(reg: &Registry, id: &str) -> Res<Option<Value>> {
    sql(reg
        .conn
        .query_row(
            "SELECT id, kind, status, target, reason, threads, updated_ms FROM review_deliveries WHERE id = ?1",
            [id],
            |r| {
                let threads: String = r.get(5)?;
                Ok(json!({
                    "id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?,
                    "status": r.get::<_, String>(2)?, "target": r.get::<_, Option<String>>(3)?,
                    "reason": r.get::<_, Option<String>>(4)?,
                    "threads": serde_json::from_str::<Value>(&threads).unwrap_or(json!([])),
                    "at": format_rfc3339(r.get(6)?),
                }))
            },
        )
        .optional())
}

pub fn to_json(reg: &Registry, t: &Thread) -> Res<Value> {
    let cs: Vec<Value> = comments(reg, &t.id)?
        .iter()
        .map(|c| {
            json!({"id": c.id, "author": c.author, "body": c.body, "pending": c.pending,
                   "createdAt": format_rfc3339(c.created_ms), "updatedAt": format_rfc3339(c.updated_ms)})
        })
        .collect();
    let mut v = json!({
        "id": t.id, "worktree": t.worktree, "path": t.path, "side": t.side,
        "line": t.line, "endLine": t.end, "originalLine": t.orig_line, "originalEndLine": t.orig_end,
        "code": t.code.join("\n"), "status": t.status, "outdated": t.outdated, "changed": t.changed,
        "createdAt": format_rfc3339(t.created_ms), "updatedAt": format_rfc3339(t.updated_ms),
        "comments": cs,
    });
    for (k, x) in [("lineId", &t.line_id), ("taskId", &t.task_id)] {
        if let Some(x) = x {
            v[k] = json!(x);
        }
    }
    if let Some(d) = t.delivery_id.as_deref() {
        if let Some(d) = delivery_json(reg, d)? {
            v["delivery"] = d;
        }
    }
    Ok(v)
}

fn emit(reg: &Registry, t: &Thread) {
    if let Ok(v) = to_json(reg, t) {
        publish("review.thread", json!({"thread": v}));
    }
}

pub struct Target {
    pub worktree: String,
    pub line_id: Option<String>,
    pub task_id: Option<String>,
    pub branch: Option<String>,
}

fn cached_lines(ctx: &Ctx) -> Vec<Line> {
    let l = LINES.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if l.is_empty() {
        crate::current_lines(ctx).unwrap_or_default()
    } else {
        l
    }
}

fn missing(path: &str) -> (u16, String) {
    (410, format!("the worktree folder {path} no longer exists"))
}

fn top(path: &str) -> Res<String> {
    if !Path::new(path).is_absolute() {
        return Err(bad(format!("{path} is not an absolute path")));
    }
    if !Path::new(path).is_dir() {
        return Err(missing(path));
    }
    crate::git::git(path, &["rev-parse", "--show-toplevel"])
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad(format!("{path} is not in a git worktree")))
}

pub fn resolve(
    ctx: &Ctx,
    worktree: Option<&str>,
    line: Option<&str>,
    task: Option<&str>,
) -> Res<Target> {
    match (worktree, line, task) {
        (Some(w), None, None) => Ok(Target {
            worktree: top(w)?,
            line_id: None,
            task_id: None,
            branch: None,
        }),
        (None, Some(id), None) => {
            let mut lines = cached_lines(ctx);
            let fresh = crate::actions::find_line(&lines, id)
                .ok()
                .and_then(line_dir)
                .is_some();
            if !fresh {
                lines = crate::scan(ctx).map_err(|e| (500, e))?;
            }
            let l = crate::actions::find_line(&lines, id).map_err(|e| (404, e))?;
            let dir = line_dir(l).ok_or_else(|| missing(&l.worktree_path))?;
            Ok(Target {
                worktree: top(dir)?,
                line_id: Some(l.id.clone()),
                task_id: None,
                branch: None,
            })
        }
        (None, None, Some(id)) => {
            let reg = Registry::open(&ctx.state_dir).map_err(|e| (500, e))?;
            let t = crate::tasks::get(&reg, id)?;
            let wt = t
                .worktree_path
                .clone()
                .ok_or_else(|| (409, format!("task #{} has no worktree yet", t.number)))?;
            Ok(Target {
                worktree: top(&wt)?,
                line_id: t.line_id,
                task_id: Some(t.id),
                branch: None,
            })
        }
        _ => Err(bad("give exactly one of worktree, line, task")),
    }
}

pub fn line_dir(l: &Line) -> Option<&str> {
    [l.worktree_path.as_str(), l.cwd.as_str()]
        .into_iter()
        .find(|p| Path::new(p).is_absolute() && Path::new(p).is_dir())
}

fn local_branch(wt: &str, b: &str) -> Res<String> {
    let ok = !b.is_empty()
        && !b.starts_with('-')
        && crate::git::git(
            wt,
            &["rev-parse", "--verify", "-q", &format!("refs/heads/{b}")],
        )
        .is_some();
    if ok {
        Ok(b.to_string())
    } else {
        Err((404, format!("no local branch {b}")))
    }
}

fn target_q(ctx: &Ctx, r: &Request) -> Res<Target> {
    let d = |k| q(r, k).map(decode);
    let mut t = resolve(
        ctx,
        d("worktree").as_deref(),
        d("line").as_deref(),
        d("task").as_deref(),
    )?;
    if let Some(b) = d("branch").filter(|b| !b.is_empty()) {
        t.branch = Some(local_branch(&t.worktree, &b)?);
    }
    Ok(t)
}

fn target_b(ctx: &Ctx, b: &Value) -> Res<Target> {
    let s = |k: &str| b[k].as_str().filter(|s| !s.is_empty());
    resolve(ctx, s("worktree"), s("lineId"), s("taskId"))
}

fn task_of(ctx: &Ctx, wt: &str) -> Option<crate::tasks::Task> {
    let reg = Registry::open(&ctx.state_dir).ok()?;
    let want = Path::new(wt).canonicalize().ok()?;
    crate::tasks::all(&reg)
        .ok()?
        .into_iter()
        .filter(|t| {
            t.worktree_path
                .as_deref()
                .is_some_and(|p| p == wt || Path::new(p).canonicalize().is_ok_and(|c| c == want))
        })
        .max_by_key(|t| (!t.from_session(), t.updated_ms))
}

fn base_hint(ctx: &Ctx, wt: &str) -> Option<String> {
    let t = task_of(ctx, wt)?;
    crate::git::git(&t.project, &["symbolic-ref", "--short", "-q", "HEAD"])
        .filter(|b| !b.is_empty())
}

fn rel(path: &str) -> Res<&str> {
    let p = Path::new(path);
    if path.is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(bad(format!("bad path {path}")));
    }
    Ok(path)
}

pub fn read_lines(wt: &str, path: &str) -> Res<Vec<String>> {
    let full = Path::new(wt).join(rel(path)?);
    let canon = full
        .canonicalize()
        .map_err(|_| (404, format!("{path} does not exist")))?;
    let root = Path::new(wt)
        .canonicalize()
        .map_err(|e| (500, e.to_string()))?;
    if !canon.starts_with(&root) {
        return Err(bad(format!("{path} is outside the worktree")));
    }
    let meta = std::fs::metadata(&canon).map_err(|e| (404, e.to_string()))?;
    if !meta.is_file() || meta.len() > 2 << 20 {
        return Err((409, format!("{path} is not a text file under 2 MiB")));
    }
    let bytes = std::fs::read(&canon).map_err(|e| (404, e.to_string()))?;
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return Err((409, format!("{path} is binary")));
    }
    Ok(String::from_utf8_lossy(&bytes)
        .split_terminator('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect())
}

fn viewed(reg: &Registry, wt: &str) -> Res<BTreeMap<String, String>> {
    let mut st = sql(reg
        .conn
        .prepare("SELECT path, hash FROM review_viewed WHERE worktree = ?1"))?;
    let rows = sql(st.query_map([wt], |r| Ok((r.get(0)?, r.get(1)?))))?;
    sql(rows.collect())
}

fn hint_of(ctx: &Ctx, wt: &str) -> (Option<String>, &'static str) {
    match crate::git::git(wt, &["symbolic-ref", "--short", "-q", "HEAD"])
        .and_then(|b| diff::created_from(wt, &b))
    {
        Some(c) => (Some(c), "created"),
        None => (base_hint(ctx, wt), "task"),
    }
}

fn named(rv: &mut diff::Review, label: &'static str) {
    if rv.base_source == Some("hint") {
        rv.base_source = Some(label);
    }
}

fn superproject(wt: &str) -> Option<String> {
    crate::git::git(wt, &["rev-parse", "--show-superproject-working-tree"])
        .filter(|s| !s.is_empty())
}

pub fn review(ctx: &Ctx, t: &Target, base: Option<&str>) -> Res<Value> {
    watch::touch(&t.worktree);
    let sup = superproject(&t.worktree);
    if let Some(b) = &t.branch {
        let mut rv = diff::assemble_branch(&t.worktree, b, base).map_err(|e| (409, e))?;
        named(&mut rv, "explicit");
        return Ok(review_json(&rv, &BTreeMap::new(), true, sup));
    }
    let (hint, label) = match base {
        Some(b) => (Some(b.to_string()), "explicit"),
        None => hint_of(ctx, &t.worktree),
    };
    let mut rv = diff::assemble(&t.worktree, hint.as_deref()).map_err(|e| (409, e))?;
    named(&mut rv, label);
    let reg = open(ctx)?;
    let seen = viewed(&reg, &t.worktree)?;
    reanchor(&reg, &t.worktree);
    let mut v = review_json(&rv, &seen, false, sup);
    v["linked"] = links(ctx, t)
        .into_iter()
        .map(|w| {
            let branch = crate::git::git(&w, &["symbolic-ref", "--short", "-q", "HEAD"]);
            json!({"worktree": w, "branch": branch})
        })
        .collect();
    Ok(v)
}

fn repo_root(p: &Path) -> Option<&Path> {
    let mut r = p.ancestors().skip(1).find(|d| d.join(".git").exists())?;
    while std::fs::read_to_string(r.join(".git")).is_ok_and(|g| g.contains("/.git/modules/")) {
        r = r.ancestors().skip(1).find(|d| d.join(".git").exists())?;
    }
    Some(r)
}

pub fn edited_roots(wt: &str, files: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for f in files {
        let Some(r) = repo_root(Path::new(f)) else {
            continue;
        };
        let r = r.to_string_lossy().into_owned();
        if r != wt && !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

fn is_linked_worktree(root: &str) -> bool {
    std::fs::read_to_string(Path::new(root).join(".git"))
        .is_ok_and(|g| g.contains("/.git/worktrees/"))
}

fn line_links(ctx: &Ctx, l: &Line, wt: &str) -> Vec<String> {
    let Some(t) = crate::activity::find_transcript(
        &ctx.claude_dir,
        &ctx.codex_dir,
        &ctx.gemini_dir,
        l.agent,
        &l.session_id,
    ) else {
        return vec![];
    };
    let (edited, dirs) = crate::activity::touched(l.agent, &t);
    let mut out = edited_roots(wt, &edited);
    for r in edited_roots(wt, &dirs) {
        if is_linked_worktree(&r) && !out.contains(&r) {
            out.push(r);
        }
    }
    out.retain(|r| Path::new(r).is_dir());
    out
}

fn agents_at<'a>(ctx: &Ctx, wt: &str, lines: &'a [Line]) -> Vec<&'a Line> {
    let task_line = task_of(ctx, wt).and_then(|t| t.line_id);
    lines
        .iter()
        .filter(|l| (l.alive() && line_dir(l) == Some(wt)) || Some(&l.id) == task_line.as_ref())
        .collect()
}

fn links(ctx: &Ctx, t: &Target) -> Vec<String> {
    let lines = cached_lines(ctx);
    let mut out: Vec<String> = vec![];
    let mut agents = agents_at(ctx, &t.worktree, &lines);
    if let Some(l) = t
        .line_id
        .as_ref()
        .and_then(|id| lines.iter().find(|l| &l.id == id))
    {
        agents.push(l);
    }
    for l in agents {
        for r in line_links(ctx, l, &t.worktree) {
            if !out.contains(&r) {
                out.push(r);
            }
        }
    }
    out
}

fn linker<'a>(ctx: &Ctx, wt: &str, lines: &'a [Line]) -> Option<&'a Line> {
    if lines.iter().any(|l| l.alive() && line_dir(l) == Some(wt)) {
        return None;
    }
    lines
        .iter()
        .filter(|l| l.alive())
        .find(|l| line_dir(l).is_some_and(|d| line_links(ctx, l, d).iter().any(|r| r == wt)))
}

fn review_json(
    rv: &diff::Review,
    seen: &BTreeMap<String, String>,
    read_only: bool,
    superproject: Option<String>,
) -> Value {
    let commits = |v: &[diff::Commit]| v.iter().map(diff::Commit::to_json).collect::<Vec<_>>();
    let tr = triage::analyze(&rv.files);
    json!({
        "worktree": rv.worktree, "branch": rv.branch, "base": rv.base, "mergeBase": rv.merge_base,
        "baseSource": rv.base_source, "superproject": superproject,
        "head": rv.head, "revision": rv.revision(), "truncated": rv.truncated, "readOnly": read_only,
        "commits": commits(&rv.commits), "recent": commits(&rv.recent),
        "files": rv.files.iter().zip(&tr.files).map(|(f, t)| t.annotate(f.to_json(seen.get(&f.path) == Some(&f.hash())))).collect::<Vec<_>>(),
        "triage": tr.to_json(),
    })
}

pub fn bases(t: &Target) -> Res<Value> {
    let wt = &t.worktree;
    let branch = crate::git::git(wt, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let (base, source) = diff::base_ref(wt, branch.as_deref(), None, diff::Infer::Full);
    let out = crate::git::git(
        wt,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--count=200",
            "--format=%(refname)%1f%(objectname)%1f%(committerdate:unix)%1f%(subject)",
            "refs/heads",
            "refs/remotes",
        ],
    )
    .unwrap_or_default();
    let list: Vec<Value> = out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\x1f');
            let full = f.next()?;
            let (name, remote) = match full.strip_prefix("refs/heads/") {
                Some(n) => (n, false),
                None => (full.strip_prefix("refs/remotes/")?, true),
            };
            if name.ends_with("/HEAD") || Some(name) == branch.as_deref() {
                return None;
            }
            let head = f.next().unwrap_or_default();
            let at = f.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0) * 1000;
            Some(json!({
                "name": name, "remote": remote, "head": head, "at": format_rfc3339(at),
                "subject": f.next().unwrap_or_default(),
            }))
        })
        .collect();
    Ok(json!({
        "base": base, "baseSource": base.as_ref().map(|_| source),
        "default": crate::worktrees::default_branch_cached(wt), "branches": list,
    }))
}

pub struct Summary {
    pub files: usize,
    pub dirty: usize,
    pub ahead: usize,
    pub base: Option<String>,
}

pub fn summary(path: &str, defaults: &diff::Defaults) -> Summary {
    let status = crate::git::git(
        path,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )
    .unwrap_or_default();
    let entries: Vec<&str> = status.split('\0').filter(|e| e.len() > 3).collect();
    let untracked = entries.iter().filter(|e| e.starts_with("??")).count();
    let branch =
        crate::git::git(path, &["symbolic-ref", "--short", "-q", "HEAD"]).filter(|b| !b.is_empty());
    let base = diff::base_with(path, branch.as_deref(), defaults);
    let mb = base
        .as_deref()
        .and_then(|b| crate::git::git(path, &["merge-base", "HEAD", b]))
        .filter(|s| !s.is_empty());
    let ahead = mb
        .as_deref()
        .and_then(|m| crate::git::git(path, &["rev-list", "--count", &format!("{m}..HEAD"), "--"]))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);
    let from = mb.as_deref().unwrap_or("HEAD");
    let tracked = crate::git::git(
        path,
        &[
            "diff",
            "--no-ext-diff",
            "-M",
            "--name-only",
            "-z",
            "--ignore-submodules=none",
            from,
            "--",
        ],
    )
    .map(|s| s.split('\0').filter(|x| !x.is_empty()).count())
    .unwrap_or(entries.len() - untracked);
    Summary {
        files: tracked + untracked,
        dirty: entries.len(),
        ahead,
        base,
    }
}

fn recency(path: &str) -> i64 {
    let admin = crate::worktrees::admin_dir(path);
    let fs = ["index", "HEAD", "logs/HEAD"]
        .iter()
        .filter_map(|f| {
            admin
                .as_ref()
                .and_then(|a| crate::worktrees::mtime_ms(&a.join(f)))
        })
        .max()
        .unwrap_or(0);
    let commit = crate::git::git(path, &["log", "-1", "--format=%ct", "HEAD", "--"])
        .and_then(|s| s.trim().parse::<i64>().ok())
        .map_or(0, |s| s * 1000);
    fs.max(commit)
}

pub fn related(current: &str, lines: &[Line]) -> Value {
    let current = current.to_string();
    let mut out: Vec<(i64, Value)> = vec![];
    let entries = crate::worktrees::entries(&current);
    let main = entries.first().map(|e| e.path.clone());
    let defaults = diff::Defaults::of(&current);
    let live: Vec<&str> = lines
        .iter()
        .filter(|l| l.alive())
        .filter_map(line_dir)
        .collect();
    for e in entries.into_iter().filter(|e| !e.bare && !e.prunable) {
        if e.path != current && !live.contains(&e.path.as_str()) {
            continue;
        }
        let Ok(path) = top(&e.path) else { continue };
        let is_current = path == current;
        let Summary {
            files,
            dirty,
            ahead,
            base,
        } = summary(&path, &defaults);
        let agents = live.iter().filter(|d| **d == path).count();
        if !is_current && (agents == 0 || (files == 0 && ahead == 0)) {
            continue;
        }
        let at = recency(&path);
        out.push((
            at,
            json!({
                "path": path, "branch": e.branch, "head": e.head, "base": base,
                "files": files, "dirty": dirty, "ahead": ahead, "agents": agents, "current": is_current,
                "main": main.as_deref() == Some(e.path.as_str()),
                "at": format_rfc3339(at),
            }),
        ));
    }
    out.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    json!({
        "current": current,
        "worktrees": out.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
    })
}

pub fn branches(t: &Target) -> Res<Value> {
    let wt = &t.worktree;
    let default = crate::worktrees::default_branch(wt);
    let base = crate::worktrees::start_point(wt, &default);
    if base == "HEAD" {
        return Ok(json!({"base": null, "branches": []}));
    }
    let out = crate::git::git(
        wt,
        &[
            "for-each-ref",
            &format!("--no-merged={base}"),
            "--sort=-committerdate",
            "--count=60",
            "--format=%(refname:short)%1f%(objectname)%1f%(committerdate:unix)%1f%(worktreepath)%1f%(subject)",
            "refs/heads",
        ],
    )
    .unwrap_or_default();
    let list: Vec<Value> = out
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\x1f');
            let name = f.next().filter(|s| !s.is_empty())?;
            let head = f.next().unwrap_or_default();
            let at = f.next().and_then(|t| t.parse::<i64>().ok()).unwrap_or(0) * 1000;
            let worktree = f.next().filter(|s| !s.is_empty());
            Some(json!({
                "name": name, "head": head, "at": format_rfc3339(at),
                "worktree": worktree, "subject": f.next().unwrap_or_default(),
            }))
        })
        .collect();
    Ok(json!({"base": base, "branches": list}))
}

pub fn reanchor(reg: &Registry, wt: &str) {
    let Ok(ts) = threads_where(
        reg,
        "WHERE worktree = ?1 AND side = 'new' AND status != 'resolved'",
        &[&wt],
    ) else {
        return;
    };
    let mut files: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();
    for mut t in ts {
        let lines = files
            .entry(t.path.clone())
            .or_insert_with(|| read_lines(wt, &t.path).ok());
        let a = Anchor {
            line: t.line,
            end: t.end,
            code: t.code.clone(),
            before: t.before.clone(),
            after: t.after.clone(),
        };
        let (line, end, outdated, changed) = match lines.as_deref().map(|l| locate(l, &a)) {
            Some(Found::Same) => (t.line, t.end, false, false),
            Some(Found::Moved(s, e)) => (s, e, false, false),
            Some(Found::Changed(s, e)) => (s, e, false, true),
            Some(Found::Outdated) | None => (t.line, t.end, true, t.changed),
        };
        if (line, end, outdated, changed) != (t.line, t.end, t.outdated, t.changed) {
            (t.line, t.end, t.outdated, t.changed) = (line, end, outdated, changed);
            t.updated_ms = crate::procs::now_ms();
            if save(reg, &t).is_ok() {
                emit(reg, &t);
            }
        }
    }
}

fn base_side(ctx: &Ctx, wt: &str, path: &str, base: Option<&str>) -> Res<Vec<String>> {
    let hint = base.map(str::to_string).or_else(|| hint_of(ctx, wt).0);
    let rv = diff::assemble(wt, hint.as_deref()).map_err(|e| (409, e))?;
    let f = rv
        .file(path)
        .ok_or_else(|| bad(format!("{path} has no changes")))?;
    let (dir, rev, rel) =
        diff::blob_at(&rv, true, &f.old_path).ok_or_else(|| bad("no base to comment on"))?;
    let base = diff::base_lines(&dir, &rev, &rel);
    base.ok_or_else(|| bad(format!("{path} is not in the base")))
}

fn create(ctx: &Ctx, b: &Value) -> Res<Value> {
    let body = b["body"]
        .as_str()
        .map(str::trim_end)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| bad("body required"))?;
    let reg = open(ctx)?;
    if let Some(tid) = b["threadId"].as_str() {
        let mut t = thread(&reg, tid)?;
        add_comment(&reg, &t.id, "user", body, true)?;
        t.updated_ms = crate::procs::now_ms();
        save(&reg, &t)?;
        emit(&reg, &t);
        return to_json(&reg, &t);
    }
    let tg = target_b(ctx, b)?;
    let path = rel(b["path"].as_str().ok_or_else(|| bad("path required"))?)?;
    let side = match b["side"].as_str().unwrap_or("new") {
        s @ ("new" | "old") => s,
        _ => return Err(bad("side must be new or old")),
    };
    let line = b["line"]
        .as_u64()
        .filter(|n| *n >= 1)
        .ok_or_else(|| bad("line must be a positive integer"))? as usize;
    let end = match &b["endLine"] {
        Value::Null => line,
        v => v
            .as_u64()
            .filter(|n| *n as usize >= line)
            .ok_or_else(|| bad("endLine must be >= line"))? as usize,
    };
    let lines = if side == "new" {
        read_lines(&tg.worktree, path)?
    } else {
        base_side(
            ctx,
            &tg.worktree,
            path,
            b["base"].as_str().filter(|s| !s.is_empty()),
        )?
    };
    let a = capture(&lines, line, end)
        .ok_or_else(|| bad(format!("{path} has {} lines", lines.len())))?;
    let task = tg
        .task_id
        .clone()
        .or_else(|| task_of(ctx, &tg.worktree).map(|t| t.id));
    let now = crate::procs::now_ms();
    let t = Thread {
        id: format!("rt-{}", ulid(now)),
        worktree: tg.worktree.clone(),
        path: path.to_string(),
        side: side.into(),
        line,
        end,
        orig_line: line,
        orig_end: end,
        code: a.code,
        before: a.before,
        after: a.after,
        status: "pending".into(),
        line_id: tg.line_id.clone(),
        task_id: task,
        created_ms: now,
        updated_ms: now,
        ..Default::default()
    };
    save(&reg, &t)?;
    add_comment(&reg, &t.id, "user", body, true)?;
    watch::touch(&t.worktree);
    emit(&reg, &t);
    to_json(&reg, &t)
}

fn comment_by_id(reg: &Registry, id: &str) -> Res<Option<Comment>> {
    Ok(sql(reg
        .conn
        .query_row(
            "SELECT thread_id FROM review_comments WHERE id = ?1",
            [id],
            |r| r.get::<_, String>(0),
        )
        .optional())?
    .and_then(|tid| comments(reg, &tid).ok())
    .and_then(|cs| cs.into_iter().find(|c| c.id == id)))
}

fn patch(ctx: &Ctx, id: &str, b: &Value) -> Res<Value> {
    let reg = open(ctx)?;
    let now = crate::procs::now_ms();
    if let Some(c) = comment_by_id(&reg, id)? {
        let body = b["body"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| bad("body required"))?;
        if !c.pending {
            return Err((409, "only pending comments can be edited".into()));
        }
        sql(reg.conn.execute(
            "UPDATE review_comments SET body = ?2, updated_ms = ?3 WHERE id = ?1",
            params![id, body.trim_end(), now],
        ))?;
        let mut t = thread(&reg, &c.thread_id)?;
        t.updated_ms = now;
        save(&reg, &t)?;
        emit(&reg, &t);
        return to_json(&reg, &t);
    }
    let mut t = thread(&reg, id)?;
    match b["status"].as_str() {
        Some("resolved") => t.status = "resolved".into(),
        Some("open") => t.status = if t.sent { "sent" } else { "pending" }.into(),
        Some("addressed") => {
            if t.status == "pending" {
                return Err((409, "the thread was not sent to the agent yet".into()));
            }
            let note = b["note"].as_str().map(str::trim).filter(|s| !s.is_empty());
            add_comment(&reg, &t.id, "agent", note.unwrap_or("Addressed."), false)?;
            if t.status != "resolved" {
                t.status = "addressed".into();
            }
        }
        _ => return Err(bad("status must be resolved, open or addressed")),
    }
    t.updated_ms = now;
    save(&reg, &t)?;
    if t.status != "resolved" {
        reanchor(&reg, &t.worktree);
        t = thread(&reg, id)?;
    }
    emit(&reg, &t);
    to_json(&reg, &t)
}

fn delete(ctx: &Ctx, id: &str) -> Res<Value> {
    let reg = open(ctx)?;
    let tid = match comment_by_id(&reg, id)? {
        Some(c) => {
            if !c.pending {
                return Err((409, "only pending comments can be deleted".into()));
            }
            sql(reg
                .conn
                .execute("DELETE FROM review_comments WHERE id = ?1", [id]))?;
            c.thread_id
        }
        None => {
            let t = thread(&reg, id)?;
            if t.status != "pending" || comments(&reg, id)?.iter().any(|c| !c.pending) {
                return Err((409, "only pending threads can be deleted".into()));
            }
            sql(reg
                .conn
                .execute("DELETE FROM review_comments WHERE thread_id = ?1", [id]))?;
            t.id
        }
    };
    let t = thread(&reg, &tid)?;
    if comments(&reg, &tid)?.is_empty() {
        sql(reg
            .conn
            .execute("DELETE FROM review_threads WHERE id = ?1", [&tid]))?;
        publish(
            "review.thread_deleted",
            json!({"id": tid, "worktree": t.worktree}),
        );
    } else {
        emit(&reg, &t);
    }
    Ok(json!({"deleted": id}))
}

fn apply(ctx: &Ctx, id: &str) -> Res<Value> {
    let reg = open(ctx)?;
    let mut t = thread(&reg, id)?;
    if t.side != "new" || t.outdated {
        return Err((409, "the commented lines are outdated".into()));
    }
    let sugg = comments(&reg, id)?
        .iter()
        .rev()
        .find_map(|c| deliver::split_suggestion(&c.body).1)
        .ok_or_else(|| (409, "the thread has no suggestion".to_string()))?;
    let full = Path::new(&t.worktree).join(rel(&t.path)?);
    let raw = std::fs::read_to_string(&full).map_err(|e| (409, e.to_string()))?;
    let mut lines = read_lines(&t.worktree, &t.path)?;
    if t.end > lines.len() || lines[t.line - 1..t.end] != t.code[..] {
        return Err((409, "the lines changed since the comment".into()));
    }
    let n = sugg.len();
    lines.splice(t.line - 1..t.end, sugg.iter().cloned());
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = lines.join(eol);
    if raw.ends_with('\n') || lines.is_empty() {
        out.push_str(eol);
    }
    std::fs::write(&full, out).map_err(|e| (500, e.to_string()))?;
    let now = crate::procs::now_ms();
    if let Some(a) = capture(&lines, t.line, (t.line + n).saturating_sub(1).max(t.line)) {
        if n > 0 {
            t.code = a.code;
            t.end = t.line + n - 1;
        }
    }
    t.status = "resolved".into();
    t.changed = false;
    t.updated_ms = now;
    save(&reg, &t)?;
    emit(&reg, &t);
    to_json(&reg, &t)
}

fn pending_items(reg: &Registry, wt: &str) -> Res<Vec<(Thread, Vec<Comment>)>> {
    let mut out = vec![];
    for t in threads_where(reg, "WHERE worktree = ?1", &[&wt])? {
        let cs: Vec<Comment> = comments(reg, &t.id)?
            .into_iter()
            .filter(|c| c.pending)
            .collect();
        if !cs.is_empty() {
            out.push((t, cs));
        }
    }
    Ok(out)
}

fn submit(ctx: &Ctx, b: &Value) -> Res<Value> {
    let kind = match b["kind"].as_str() {
        Some(k @ ("comment" | "request_changes" | "approve")) => k,
        _ => return Err(bad("kind must be comment, request_changes or approve")),
    };
    let body = b["body"].as_str().map(str::trim).filter(|s| !s.is_empty());
    let mut tg = target_b(ctx, b)?;
    let reg = open(ctx)?;
    let items = pending_items(&reg, &tg.worktree)?;
    let all = cached_lines(ctx);
    let via = linker(ctx, &tg.worktree, &all);
    if let Some(l) = via {
        tg.line_id = Some(l.id.clone());
    }
    let now = crate::procs::now_ms();
    let did = format!("rd-{}", ulid(now));
    let fmt: Vec<deliver::Item> = items
        .iter()
        .map(|(t, cs)| deliver::Item {
            id: t.id.clone(),
            path: match via {
                Some(_) => format!("{}/{}", tg.worktree, t.path),
                None => t.path.clone(),
            },
            side: t.side.clone(),
            line: t.line,
            end: t.end,
            code: t.code.clone(),
            comments: cs.iter().map(|c| c.body.clone()).collect(),
        })
        .collect();
    let text = deliver::format(kind, &fmt, body);
    let ids: Vec<String> = items.iter().map(|(t, _)| t.id.clone()).collect();
    let task = task_of(ctx, &tg.worktree);
    if kind == "request_changes" {
        if let Some(t) = task.as_ref().filter(|t| t.status == "in_testing") {
            let _ = crate::tasks::review(ctx, &t.id, &json!({"action": "reject"}));
        }
    }
    if items.is_empty() && body.is_none() {
        return Ok(json!({
            "delivery": {"id": did, "status": "none", "kind": kind, "target": null, "at": format_rfc3339(now), "threads": []},
            "threads": [], "text": "",
        }));
    }
    sql(reg.conn.execute(
        "INSERT INTO review_deliveries (id, worktree, kind, status, target, reason, line_id, task_id, text, threads, created_ms, updated_ms)
         VALUES (?1, ?2, ?3, 'queued', NULL, 'waiting for the agent', ?4, ?5, ?6, ?7, ?8, ?8)",
        params![did, tg.worktree, kind, tg.line_id, task.as_ref().map(|t| t.id.clone()), text, json!(ids).to_string(), now],
    ))?;
    for (mut t, _) in items {
        sql(reg.conn.execute(
            "UPDATE review_comments SET pending = 0, updated_ms = ?2 WHERE thread_id = ?1 AND pending = 1",
            params![t.id, now],
        ))?;
        t.status = "sent".into();
        t.sent = true;
        t.delivery_id = Some(did.clone());
        t.updated_ms = now;
        save(&reg, &t)?;
    }
    QUEUE.push(crate::deliver::Job {
        id: did.clone(),
        target: tg.worktree.clone(),
        text: text.clone(),
        at_ms: now,
    });
    let lines = cached_lines(ctx);
    if !attempt(ctx, &reg, &did, &lines) {
        QUEUE.remove(&did);
    }
    let threads: Vec<Value> = ids
        .iter()
        .filter_map(|id| thread(&reg, id).ok())
        .map(|t| {
            emit(&reg, &t);
            to_json(&reg, &t)
        })
        .collect::<Res<_>>()?;
    let d = delivery_json(&reg, &did)?.unwrap_or(Value::Null);
    Ok(json!({"delivery": d, "threads": threads, "text": text}))
}

fn attempt(ctx: &Ctx, reg: &Registry, id: &str, lines: &[Line]) -> bool {
    let _g = DELIVER.lock().unwrap_or_else(|e| e.into_inner());
    let row = reg.conn.query_row(
        "SELECT worktree, status, line_id, task_id, text, reason, target FROM review_deliveries WHERE id = ?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?,
            ))
        },
    );
    let Ok((wt, status, line_id, task_id, text, reason, prev_target)) = row else {
        return false;
    };
    if status != "queued" {
        return false;
    }
    let task = task_id.as_deref().and_then(|t| {
        Registry::open(&ctx.state_dir)
            .ok()
            .and_then(|r| crate::tasks::get(&r, t).ok())
    });
    let mut prefer: Vec<&str> = vec![];
    if let Some(l) = line_id.as_deref() {
        prefer.push(l);
    }
    if let Some(l) = task.as_ref().and_then(|t| t.line_id.as_deref()) {
        prefer.push(l);
    }
    let at = line_id
        .as_deref()
        .and_then(|id| lines.iter().find(|l| l.id == id && l.alive()))
        .and_then(line_dir)
        .unwrap_or(&wt);
    let (line, tgt) = deliver::target(
        at,
        &prefer,
        task.as_ref().and_then(|t| t.term_id.as_deref()),
        lines,
    );
    let tmux = ctx.tmux.as_ref();
    let (st, why, label) = match &tgt {
        None => (
            "queued",
            Some("no agent is running in this worktree".to_string()),
            None,
        ),
        Some(x) => match deliver::ready(line, x, tmux)
            .and_then(|_| crate::deliver::send(x, &text, tmux, &format!("sb-{id}")))
        {
            Ok(()) => ("delivered", None, Some(x.label())),
            Err(e) => ("queued", Some(e), Some(x.label())),
        },
    };
    if st == "queued" && why == reason && label == prev_target {
        return true;
    }
    let now = crate::procs::now_ms();
    let _ = reg.conn.execute(
        "UPDATE review_deliveries SET status = ?2, reason = ?3, target = ?4, updated_ms = ?5,
           text = CASE WHEN ?2 = 'delivered' THEN '' ELSE text END WHERE id = ?1",
        params![id, st, why, label, now],
    );
    if let Ok(Some(d)) = delivery_json(reg, id) {
        publish("review.delivery", json!({"delivery": d, "worktree": wt}));
    }
    st == "queued"
}

pub fn tick(ctx: &Ctx, lines: &[Line]) {
    *LINES.lock().unwrap_or_else(|e| e.into_inner()) = lines.to_vec();
    let Ok(reg) = open(ctx) else {
        return;
    };
    let ids: Vec<String> = reg
        .conn
        .prepare("SELECT id FROM review_deliveries WHERE status = 'queued' ORDER BY created_ms")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .unwrap_or_default();
    for id in ids.into_iter().filter(|id| !QUEUE.contains(id)) {
        QUEUE.push(crate::deliver::Job {
            target: String::new(),
            text: String::new(),
            at_ms: crate::procs::now_ms(),
            id,
        });
    }
    QUEUE.drain(|j| attempt(ctx, &reg, &j.id, lines));
}

pub fn open_worktrees(ctx: &Ctx) -> Vec<String> {
    let Ok(reg) = open(ctx) else {
        return vec![];
    };
    reg.conn
        .prepare("SELECT DISTINCT worktree FROM review_threads WHERE status != 'resolved'")
        .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
        .unwrap_or_default()
}

fn list(ctx: &Ctx, r: &Request) -> Res<Value> {
    let has = ["worktree", "line", "task"]
        .iter()
        .any(|k| q(r, k).is_some());
    let wt = if has {
        Some(target_q(ctx, r)?.worktree)
    } else {
        None
    };
    if let Some(w) = &wt {
        watch::touch(w);
    }
    let status = q(r, "status").map(decode);
    let wanted: Vec<&str> = status
        .as_deref()
        .map(|s| s.split(',').collect())
        .unwrap_or_default();
    let reg = open(ctx)?;
    let ts = match &wt {
        Some(w) => threads_where(&reg, "WHERE worktree = ?1", &[w])?,
        None => threads_where(&reg, "", &[])?,
    };
    ts.iter()
        .filter(|t| wanted.is_empty() || wanted.contains(&t.status.as_str()))
        .map(|t| to_json(&reg, t))
        .collect::<Res<Vec<_>>>()
        .map(Value::from)
}

fn set_viewed(ctx: &Ctx, b: &Value) -> Res<Value> {
    let tg = target_b(ctx, b)?;
    let path = rel(b["path"].as_str().ok_or_else(|| bad("path required"))?)?;
    let hash = b["hash"].as_str().unwrap_or_default();
    let on = b["viewed"]
        .as_bool()
        .ok_or_else(|| bad("viewed must be a boolean"))?;
    let reg = open(ctx)?;
    if on {
        sql(reg.conn.execute(
            "INSERT OR REPLACE INTO review_viewed (worktree, path, hash) VALUES (?1, ?2, ?3)",
            params![tg.worktree, path, hash],
        ))?;
    } else {
        sql(reg.conn.execute(
            "DELETE FROM review_viewed WHERE worktree = ?1 AND path = ?2",
            params![tg.worktree, path],
        ))?;
    }
    Ok(json!({"path": path, "hash": hash, "viewed": on}))
}

pub fn routes(r: &Request, segs: &[&str], body: &Value, ctx: &Ctx) -> Option<(u16, Value)> {
    let out = |code: u16, v: Res<Value>| match v {
        Ok(v) => (code, v),
        Err((c, e)) => (c, json!({"error": e})),
    };
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "review"]) => out(
            200,
            target_q(ctx, r).and_then(|t| review(ctx, &t, q(r, "base").map(decode).as_deref())),
        ),
        ("GET", ["v1", "review", "file"]) => out(
            200,
            target_q(ctx, r).and_then(|t| {
                let p = q(r, "path")
                    .map(decode)
                    .ok_or_else(|| bad("path required"))?;
                let lines = match q(r, "rev").map(decode) {
                    Some(rev) if crate::worktrees::valid_sha(&rev) => {
                        diff::lines_at(&t.worktree, &rev, rel(&p)?)
                            .ok_or_else(|| (404, format!("{p} is not in {rev}")))?
                    }
                    Some(rev) => return Err(bad(format!("bad revision {rev}"))),
                    None => read_lines(&t.worktree, &p)?,
                };
                Ok(json!({"path": p, "lines": lines}))
            }),
        ),
        ("GET", ["v1", "review", "graph"]) => out(
            200,
            target_q(ctx, r)
                .and_then(|t| graph::route(ctx, &t, q(r, "base").map(decode).as_deref())),
        ),
        ("GET", ["v1", "review", "map"]) => out(
            200,
            target_q(ctx, r).and_then(|t| {
                let with: Vec<String> = r
                    .query
                    .iter()
                    .filter(|(k, _)| k == "with")
                    .map(|(_, v)| decode(v))
                    .collect();
                map::route(ctx, &t, &with)
            }),
        ),
        ("GET", ["v1", "review", "worktrees"]) => out(
            200,
            target_q(ctx, r).map(|t| related(&t.worktree, &cached_lines(ctx))),
        ),
        ("GET", ["v1", "review", "branches"]) => {
            out(200, target_q(ctx, r).and_then(|t| branches(&t)))
        }
        ("GET", ["v1", "review", "bases"]) => out(200, target_q(ctx, r).and_then(|t| bases(&t))),
        ("PUT", ["v1", "review", "viewed"]) => out(200, set_viewed(ctx, body)),
        ("GET", ["v1", "review", "comments"]) => out(200, list(ctx, r)),
        ("POST", ["v1", "review", "comments"]) => out(201, create(ctx, body)),
        ("PATCH", ["v1", "review", "comments", id]) => out(200, patch(ctx, id, body)),
        ("DELETE", ["v1", "review", "comments", id]) => out(200, delete(ctx, id)),
        ("POST", ["v1", "review", "comments", id, "apply"]) => out(200, apply(ctx, id)),
        ("POST", ["v1", "review", "submit"]) => out(200, submit(ctx, body)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(d: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(d)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["-c", "user.email=t@example.com", "-c", "user.name=T"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "protocol.file.allow=always",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sb-rv-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    #[test]
    fn an_agent_links_the_other_worktrees_it_edited_but_not_submodules() {
        let d = tmp("links");
        let repo = |name: &str| {
            let p = d.join(name);
            std::fs::create_dir_all(&p).unwrap();
            run(&p, &["init", "-q", "-b", "main"]);
            std::fs::write(p.join("a.txt"), "1\n").unwrap();
            run(&p, &["add", "-A"]);
            run(&p, &["commit", "-q", "-m", "init"]);
            p
        };
        let main = repo("main");
        let lib = repo("lib");
        run(
            &main,
            &["submodule", "add", "-q", lib.to_str().unwrap(), "sub"],
        );
        let side = d.join("main-wt");
        run(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat",
                side.to_str().unwrap(),
            ],
        );
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let edited: Vec<String> = [
            main.join("a.txt"),
            main.join("sub/a.txt"),
            side.join("dir/new.rs"),
            side.join("a.txt"),
            d.join("loose/x.txt"),
        ]
        .iter()
        .map(|p| s(p))
        .collect();
        assert_eq!(edited_roots(&s(&main), &edited), [s(&side)]);

        let claude = d.join("claude");
        let proj = claude.join("projects/p");
        std::fs::create_dir_all(&proj).unwrap();
        let edit = json!({"type": "assistant", "timestamp": "2026-10-30T10:00:00Z", "message": {"id": "m1", "role": "assistant", "content": [
            {"type": "tool_use", "id": "t1", "name": "Edit", "input": {"file_path": s(&side.join("a.txt")), "old_string": "1", "new_string": "2"}}
        ]}});
        std::fs::write(proj.join("s1.jsonl"), format!("{edit}\n")).unwrap();
        let other = d.join("main-wt2");
        run(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat2",
                other.to_str().unwrap(),
            ],
        );
        let bash = json!({"type": "assistant", "timestamp": "2026-10-30T10:01:00Z", "message": {"id": "m2", "role": "assistant", "content": [
            {"type": "tool_use", "id": "t2", "name": "Bash", "input": {"command": format!("cd {}/dir && sed -i s/a/b/ x; cat {}/a.txt", s(&other), s(&lib))}}
        ]}});
        std::fs::create_dir_all(proj.join("s1/subagents")).unwrap();
        std::fs::write(proj.join("s1/subagents/agent-x.jsonl"), format!("{bash}\n")).unwrap();
        let mut ctx = Ctx::from_env(&crate::procs::System);
        ctx.claude_dir = claude;
        ctx.state_dir = d.join("state");
        let l = Line {
            id: "l1".into(),
            agent: "claude",
            session_id: "s1".into(),
            worktree_path: s(&main),
            cwd: s(&main),
            pid: Some(1),
            ..Default::default()
        };
        assert_eq!(line_links(&ctx, &l, &s(&main)), [s(&side), s(&other)]);
        let lines = vec![l];
        assert_eq!(
            linker(&ctx, &s(&side), &lines).map(|l| l.id.as_str()),
            Some("l1")
        );
        assert!(linker(&ctx, &s(&main), &lines).is_none());
        assert!(linker(&ctx, &s(&lib), &lines).is_none());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_line_with_a_vanished_worktree_falls_back_to_its_live_cwd_or_reports_gone() {
        let d = tmp("line");
        let gone = d.join("cmux-ide").to_string_lossy().into_owned();
        let live = d.to_string_lossy().into_owned();
        let mut l = Line {
            worktree_path: gone.clone(),
            cwd: live.clone(),
            ..Default::default()
        };
        assert_eq!(line_dir(&l), Some(live.as_str()));
        l.cwd = gone.clone();
        assert_eq!(line_dir(&l), None);
        l.worktree_path = "unknown".into();
        assert_eq!(line_dir(&l), None);
        let (code, msg) = top(&gone).unwrap_err();
        assert_eq!(code, 410);
        assert!(msg.contains("no longer exists"), "{msg}");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn submodule_changes_are_reviewed_only_on_a_branch_the_agent_created() {
        let d = tmp("sub");
        let init = |name: &str, file: &str| {
            let p = d.join(name);
            std::fs::create_dir_all(&p).unwrap();
            run(&p, &["init", "-q", "-b", "main"]);
            std::fs::write(p.join(file), "1\n").unwrap();
            run(&p, &["add", "-A"]);
            run(&p, &["commit", "-q", "-m", "init"]);
            p
        };
        let lib = init("lib", "a.txt");
        std::fs::write(lib.join("x.rs"), "fn a() {}\n").unwrap();
        run(&lib, &["add", "-A"]);
        run(&lib, &["commit", "-q", "-m", "rust"]);
        let other = init("other", "o.txt");
        let pinned = init("pinned", "p.txt");
        let sup = init("sup", "x.txt");
        for (src, at) in [
            (&lib, "vendor/lib"),
            (&other, "vendor/other"),
            (&pinned, "vendor/pinned"),
        ] {
            run(&sup, &["submodule", "add", "-q", src.to_str().unwrap(), at]);
        }
        run(&sup, &["commit", "-q", "-m", "add submodules"]);
        let sub = sup.join("vendor/lib");
        let old = run(&sub, &["rev-parse", "HEAD"]);
        run(&sup, &["checkout", "-q", "-b", "feat"]);
        run(&sub, &["checkout", "-q", "-b", "agent"]);
        std::fs::write(sub.join("b.txt"), "b\n").unwrap();
        std::fs::write(sub.join("x.rs"), "fn b() {}\n").unwrap();
        run(&sub, &["add", "-A"]);
        run(&sub, &["commit", "-q", "-m", "lib work"]);
        let new = run(&sub, &["rev-parse", "HEAD"]);
        std::fs::write(sub.join("a.txt"), "dirty\n").unwrap();
        std::fs::write(sub.join("loose.txt"), "u\n").unwrap();
        std::fs::write(other.join("up.txt"), "upstream\n").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "someone else"]);
        run(&sup.join("vendor/other"), &["pull", "-q"]);
        std::fs::write(sup.join("vendor/other/o.txt"), "dirty\n").unwrap();
        let pin = sup.join("vendor/pinned");
        std::fs::write(pin.join("old.txt"), "old\n").unwrap();
        run(&pin, &["add", "-A"]);
        run(&pin, &["commit", "-q", "-m", "old release work"]);
        run(&pin, &["checkout", "-q", "--detach"]);
        std::fs::write(sup.join("x.txt"), "2\n").unwrap();
        let s = sup.to_string_lossy().into_owned();

        let rv = diff::assemble(&s, None).unwrap();
        let st: Vec<(&str, &str)> = rv
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect();
        assert_eq!(
            st,
            [
                ("vendor/lib", "submodule"),
                ("vendor/lib/a.txt", "modified"),
                ("vendor/lib/b.txt", "added"),
                ("vendor/lib/loose.txt", "untracked"),
                ("vendor/lib/x.rs", "modified"),
                ("x.txt", "modified")
            ]
        );
        let a = rv.file("vendor/lib/a.txt").unwrap();
        assert_eq!((a.additions, a.deletions), (1, 1));
        let subs = sub.to_string_lossy().into_owned();
        assert_eq!(
            diff::blob_at(&rv, true, "vendor/lib/a.txt"),
            Some((subs.clone(), old.clone(), "a.txt".into()))
        );
        assert_eq!(
            base_side(
                &Ctx::from_env(&crate::procs::System),
                &s,
                "vendor/lib/a.txt",
                Some("main")
            )
            .unwrap(),
            ["1"]
        );
        let xi = rv
            .files
            .iter()
            .position(|f| f.path == "vendor/lib/x.rs")
            .unwrap();
        let src = graph::sources(&rv, true);
        assert_eq!(src.old[xi].as_deref(), Some(&["fn a() {}".to_string()][..]));
        assert_eq!(src.new[xi].as_deref(), Some(&["fn b() {}".to_string()][..]));
        assert_eq!(
            read_lines(&sup.to_string_lossy(), "vendor/lib/a.txt").unwrap(),
            ["dirty"]
        );
        let f = rv.file("vendor/lib").unwrap();
        assert!(f.hunks.is_empty());
        assert_eq!(f.submodule, Some((Some(old.clone()), Some(new.clone()))));
        assert_eq!(f.to_json(false)["submodule"]["from"], old.as_str());
        assert_eq!(summary(&s, &diff::Defaults::of(&s)).files, 4);
        assert_eq!(superproject(&s), None);

        assert_eq!(top(&subs).unwrap(), subs);
        assert_eq!(superproject(&subs).as_deref(), Some(s.as_str()));
        let l = Line {
            worktree_path: s.clone(),
            cwd: subs.clone(),
            ..Default::default()
        };
        assert_eq!(top(line_dir(&l).unwrap()).unwrap(), s);
        let own = diff::assemble(&subs, Some(&old)).unwrap();
        let paths: Vec<&str> = own.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "b.txt", "loose.txt", "x.rs"]);

        let swt = d.join("lib-wt");
        run(
            &sub,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "feat/sub",
                swt.to_str().unwrap(),
            ],
        );
        std::fs::write(swt.join("c.txt"), "c\n").unwrap();
        run(&swt, &["add", "-A"]);
        run(&swt, &["commit", "-q", "-m", "sub wt work"]);
        let w = swt.to_string_lossy().into_owned();
        assert_eq!(top(&w).unwrap(), w);
        let rw = diff::assemble(&w, Some("agent")).unwrap();
        let paths: Vec<&str> = rw.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["c.txt"]);
        assert_eq!(rw.commits.len(), 1);

        run(&sup, &["add", "vendor/lib"]);
        run(&sup, &["commit", "-q", "-m", "bump lib"]);
        run(&sup, &["checkout", "-q", "main"]);
        let rb = diff::assemble_branch(&s, "feat", Some("main")).unwrap();
        let paths: Vec<&str> = rb.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["vendor/lib", "vendor/lib/b.txt", "vendor/lib/x.rs"]);
        let src = graph::sources(&rb, false);
        assert_eq!(src.old[2].as_deref(), Some(&["fn a() {}".to_string()][..]));
        assert_eq!(src.new[2].as_deref(), Some(&["fn b() {}".to_string()][..]));
        assert_eq!(
            diff::lines_at(&s, rb.head.as_deref().unwrap(), "vendor/lib/x.rs").unwrap(),
            ["fn b() {}"]
        );
        assert_eq!(
            diff::lines_at(&s, rb.head.as_deref().unwrap(), "vendor/nope/x.rs"),
            None
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn bases_list_local_and_remote_branches_without_the_current_one() {
        let d = tmp("bases");
        let origin = d.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        run(&origin, &["init", "-q", "-b", "main"]);
        std::fs::write(origin.join("a.txt"), "a\n").unwrap();
        run(&origin, &["add", "-A"]);
        run(&origin, &["commit", "-q", "-m", "base"]);
        run(&origin, &["branch", "release"]);
        let clone = d.join("clone");
        run(
            &d,
            &[
                "clone",
                "-q",
                origin.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        run(&clone, &["checkout", "-q", "-b", "feat", "origin/release"]);
        let t = Target {
            worktree: clone.to_string_lossy().into_owned(),
            line_id: None,
            task_id: None,
            branch: None,
        };
        let v = bases(&t).unwrap();
        assert_eq!(v["base"], "origin/release");
        assert_eq!(v["baseSource"], "created");
        assert_eq!(v["default"], "main");
        let names: Vec<(&str, bool)> = v["branches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| (b["name"].as_str().unwrap(), b["remote"].as_bool().unwrap()))
            .collect();
        assert!(names.contains(&("main", false)), "{names:?}");
        assert!(names.contains(&("origin/release", true)), "{names:?}");
        assert!(
            !names
                .iter()
                .any(|(n, _)| *n == "feat" || n.ends_with("/HEAD")),
            "{names:?}"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn related_lists_changed_worktrees_by_recency_and_unmerged_branches() {
        let d = tmp("rel");
        let repo = d.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        run(&repo, &["add", "-A"]);
        run(&repo, &["commit", "-q", "-m", "base"]);
        let wt = |name: &str, branch: &str| {
            let p = d.join(name);
            run(
                &repo,
                &["worktree", "add", "-q", "-b", branch, p.to_str().unwrap()],
            );
            p
        };
        let clean = wt("clean", "feat/clean");
        let busy = wt("busy", "feat/busy");
        let dirty = wt("dirty", "fix/dirty");
        std::fs::write(busy.join("b.txt"), "b\n").unwrap();
        run(&busy, &["add", "-A"]);
        run(&busy, &["commit", "-q", "-m", "busy work"]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dirty.join("a.txt"), "changed\n").unwrap();
        run(&dirty, &["add", "a.txt"]);
        let closed = wt("closed", "feat/closed");
        std::fs::write(closed.join("a.txt"), "left over\n").unwrap();
        run(&repo, &["branch", "orphan-work", "feat/busy"]);
        let root = repo.to_string_lossy().into_owned();
        let line = |p: &Path, pid: Option<u32>| Line {
            worktree_path: p.to_string_lossy().into_owned(),
            pid,
            ..Default::default()
        };
        let lines = [
            line(&clean, Some(1)),
            line(&busy, Some(2)),
            line(&dirty, Some(3)),
            line(&closed, None),
        ];

        assert_eq!(
            related(&root, &[])["worktrees"].as_array().unwrap().len(),
            1
        );
        let v = related(&root, &lines);
        let list = v["worktrees"].as_array().unwrap();
        let names: Vec<&str> = list.iter().map(|w| w["branch"].as_str().unwrap()).collect();
        assert_eq!(names, ["fix/dirty", "feat/busy", "main"]);
        assert_eq!(
            (list[0]["dirty"].as_u64(), list[0]["files"].as_u64()),
            (Some(1), Some(1))
        );
        assert_eq!(
            (list[1]["ahead"].as_u64(), list[1]["files"].as_u64()),
            (Some(1), Some(1))
        );
        std::fs::write(busy.join("c.txt"), "c\n").unwrap();
        std::fs::write(busy.join("a.txt"), "a2\n").unwrap();
        let s = summary(busy.to_str().unwrap(), &diff::Defaults::of(&root));
        let full = diff::assemble(busy.to_str().unwrap(), None).unwrap();
        assert_eq!(s.files, full.files.len());
        assert_eq!((s.files, s.dirty, s.ahead), (3, 2, 1));
        assert_eq!(list[1]["base"], "main");
        assert_eq!(list[2]["current"], true);
        assert_eq!(list[2]["main"], true);
        assert!(!names.contains(&"feat/clean") && !names.contains(&"feat/closed"));
        assert_eq!(list[0]["agents"], 1);
        assert!(clean.is_dir());

        let t = Target {
            worktree: root.clone(),
            line_id: None,
            task_id: None,
            branch: None,
        };
        let b = branches(&t).unwrap();
        assert_eq!(b["base"], "main");
        let bn: Vec<&str> = b["branches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["name"].as_str().unwrap())
            .collect();
        assert!(
            bn.contains(&"feat/busy") && bn.contains(&"orphan-work"),
            "{bn:?}"
        );
        assert!(
            !bn.contains(&"feat/clean") && !bn.contains(&"main"),
            "{bn:?}"
        );
        let orphan = b["branches"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["name"] == "orphan-work")
            .unwrap();
        assert!(orphan["worktree"].is_null());

        assert_eq!(local_branch(&root, "orphan-work").unwrap(), "orphan-work");
        assert_eq!(local_branch(&root, "--all").unwrap_err().0, 404);
        let rv = review_json(
            &diff::assemble_branch(&root, "orphan-work", None).unwrap(),
            &BTreeMap::new(),
            true,
            None,
        );
        assert_eq!(rv["readOnly"], true);
        assert_eq!(rv["base"], "main");
        assert_eq!(rv["files"][0]["path"], "b.txt");
        assert_eq!(rv["commits"][0]["subject"], "busy work");
        std::fs::remove_dir_all(d).unwrap();
    }
}
