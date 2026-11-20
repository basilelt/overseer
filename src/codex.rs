use crate::claude::mtime_ms;
use crate::util::parse_rfc3339;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Thread {
    pub id: String,
    // Multi-agent v2 sub-agent: the root thread this one was spawned from.
    pub parent: Option<String>,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub started_ms: Option<i64>,
    pub activity_ms: i64,
    pub archived: bool,
}

fn state_db(codex_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(codex_dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let n = name
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            Some((n, e.path()))
        })
        .max_by_key(|(n, _)| *n)
        .map(|(_, p)| p)
}

pub fn threads(codex_dir: &Path, state_dir: &Path) -> Vec<Thread> {
    load(codex_dir, state_dir).0
}

/// The threads and the sub-agent map (thread id -> root id) of one load.
pub fn load(codex_dir: &Path, state_dir: &Path) -> (Vec<Thread>, HashMap<String, String>) {
    let raw = state_db(codex_dir)
        .and_then(|db| from_db(&db, state_dir))
        .unwrap_or_else(|| from_rollouts(codex_dir));
    resolve_roots(raw)
}

pub fn root_in(parents: &HashMap<String, String>, id: &str) -> Option<String> {
    let mut cur = id.to_string();
    for _ in 0..64 {
        match parents.get(&cur) {
            Some(p) if p != &cur => cur = p.clone(),
            _ => break,
        }
    }
    (cur != id).then_some(cur)
}

/// Hook sessions keyed by a sub-agent thread belong to the root session.
pub fn root_hooks(
    hooks: &HashMap<String, crate::registry::HookState>,
    parents: &HashMap<String, String>,
) -> HashMap<String, crate::registry::HookState> {
    let mut out: HashMap<String, crate::registry::HookState> = HashMap::new();
    for (k, h) in hooks {
        let key = root_in(parents, k).unwrap_or_else(|| k.clone());
        match out.get(&key) {
            Some(prev) if prev.at_ms >= h.at_ms => {}
            _ => {
                out.insert(key, h.clone());
            }
        }
    }
    out
}

// Sub-agent threads carry their parent in the rollout's session_meta: never list or resume them
// as sessions of their own, the root thread is the session.
pub fn subagent_parent(v: &Value) -> Option<String> {
    let p = &v["payload"];
    let is_sub = [&v["payload"], v]
        .iter()
        .any(|x| x["thread_source"].as_str() == Some("subagent"))
        || p["source"]["subagent"].is_object()
        || v["source"]["subagent"].is_object();
    if !is_sub {
        return None;
    }
    [&v["payload"], v].iter().find_map(|x| {
        x["source"]["subagent"]["thread_spawn"]["parent_thread_id"]
            .as_str()
            .map(str::to_string)
    })
}

static PARENTS: Mutex<Option<HashMap<PathBuf, Option<String>>>> = Mutex::new(None);

#[cfg(test)]
thread_local!(static HEADER_READS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) });

fn read_parent(path: &Path) -> Option<Option<String>> {
    #[cfg(test)]
    HEADER_READS.with(|c| c.set(c.get() + 1));
    let mut first = String::new();
    BufReader::new(std::fs::File::open(path).ok()?)
        .read_line(&mut first)
        .ok()?;
    Some(subagent_parent(&serde_json::from_str(&first).ok()?))
}

// A thread's parent never changes: read each rollout header once, not on every database change.
fn rollout_parent(path: &Path) -> Option<String> {
    if let Some(p) = PARENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(path))
    {
        return p.clone();
    }
    let p = read_parent(path)?;
    PARENTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(path.to_path_buf(), p.clone());
    p
}

// Threads keyed by their root: a sub-agent and its root are one session, the root's own row wins,
// the most recent activity and any title carry over.
fn resolve_roots(raw: Vec<Thread>) -> (Vec<Thread>, HashMap<String, String>) {
    let mut parent_of: HashMap<String, String> = HashMap::new();
    for t in &raw {
        if let Some(p) = &t.parent {
            parent_of.insert(t.id.clone(), p.clone());
        }
    }
    let mut by_root: HashMap<String, Vec<Thread>> = HashMap::new();
    for t in raw {
        let r = root_in(&parent_of, &t.id).unwrap_or_else(|| t.id.clone());
        by_root.entry(r).or_default().push(t);
    }
    let mut out = Vec::with_capacity(by_root.len());
    for (r, mut group) in by_root {
        group.sort_by_key(|t| (t.id != r, std::cmp::Reverse(t.activity_ms)));
        let mut best = group.remove(0);
        best.activity_ms = group
            .iter()
            .map(|t| t.activity_ms)
            .max()
            .unwrap_or(best.activity_ms);
        if best.title.is_none() {
            best.title = group.iter().find_map(|t| t.title.clone());
        }
        best.id = r;
        best.parent = None;
        out.push(best);
    }
    (out, parent_of)
}

#[derive(Clone)]
struct Row {
    thread: Thread,
    parent: Option<String>,
    rollout: Option<String>,
    updated_ms: i64,
}

type DbStamp = (PathBuf, Option<(u64, i64)>, Option<(u64, i64)>);

fn meta_stamp(p: &Path) -> Option<(u64, i64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);
    Some((m.len(), t))
}

static ROWS: Mutex<Option<(DbStamp, Vec<Row>)>> = Mutex::new(None);
static COPIES: AtomicU64 = AtomicU64::new(0);

fn from_db(db: &Path, state_dir: &Path) -> Option<Vec<Thread>> {
    let name = db.file_name()?.to_str()?;
    let stamp = (
        db.to_path_buf(),
        meta_stamp(db),
        meta_stamp(&db.with_file_name(format!("{name}-wal"))),
    );
    let cached = ROWS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .filter(|c| c.0 == stamp)
        .map(|c| c.1.clone());
    let rows = match cached {
        Some(r) => r,
        None => {
            let tmp = state_dir.join(crate::paths::TMP_DIR).join(format!(
                "codex-{}-{}",
                std::process::id(),
                COPIES.fetch_add(1, Ordering::Relaxed)
            ));
            crate::mkdir_private(&tmp).ok()?;
            let res = copy_and_query(db, &tmp);
            let _ = std::fs::remove_dir_all(&tmp);
            let rows = res?;
            *ROWS.lock().unwrap_or_else(|e| e.into_inner()) = Some((stamp, rows.clone()));
            rows
        }
    };
    Some(
        rows.into_iter()
            .map(|r| {
                let rollout_ms = r.rollout.map(|p| mtime_ms(Path::new(&p))).unwrap_or(0);
                Thread {
                    activity_ms: r.updated_ms.max(rollout_ms),
                    ..r.thread
                }
            })
            .collect(),
    )
}

fn copy_and_query(db: &Path, tmp: &Path) -> Option<Vec<Row>> {
    let name = db.file_name()?.to_str()?;
    let copy = tmp.join(name);
    std::fs::copy(db, &copy).ok()?;
    let wal = db.with_file_name(format!("{name}-wal"));
    if wal.exists() {
        std::fs::copy(&wal, tmp.join(format!("{name}-wal"))).ok()?;
    }
    let conn = rusqlite::Connection::open(&copy).ok()?;
    // Older state databases lack the title columns: read every column by name and let missing
    // ones come back empty instead of failing the whole query.
    let mut stmt = conn.prepare("SELECT * FROM threads").ok()?;
    let rows = stmt
        .query_map([], |r| {
            let text = |col: &str| -> Option<String> {
                r.get::<_, Option<String>>(col)
                    .ok()
                    .flatten()
                    .filter(|s| !s.is_empty())
            };
            let updated: Option<i64> = r.get("updated_at").ok();
            let rollout = text("rollout_path");
            Ok(Row {
                parent: rollout
                    .as_deref()
                    .and_then(|p| rollout_parent(Path::new(p))),
                rollout,
                updated_ms: updated.unwrap_or(0).saturating_mul(1000),
                thread: Thread {
                    id: text("id").unwrap_or_default(),
                    parent: None,
                    title: text("title").or_else(|| {
                        text("first_user_message").map(|m| crate::session_tasks::summarize(&m, 80))
                    }),
                    started_ms: r
                        .get::<_, Option<i64>>("created_at")
                        .ok()
                        .flatten()
                        .map(|s| s * 1000),
                    activity_ms: 0,
                    cwd: text("cwd"),
                    archived: r
                        .get::<_, Option<i64>>("archived")
                        .ok()
                        .flatten()
                        .unwrap_or(0)
                        != 0,
                    branch: text("git_branch"),
                },
            })
        })
        .ok()?;
    let mut rows: Vec<Row> = rows.flatten().collect();
    rows.retain(|r| !r.thread.id.is_empty());
    for r in &mut rows {
        r.thread.parent = r.parent.clone();
    }
    Some(rows)
}

type RolloutStamp = Vec<(PathBuf, u64, i64)>;
static ROLLOUTS: Mutex<Option<(RolloutStamp, Vec<Thread>)>> = Mutex::new(None);

fn from_rollouts(codex_dir: &Path) -> Vec<Thread> {
    let mut files = vec![];
    collect(&codex_dir.join("sessions"), 4, &mut files);
    // Re-read the rollout headers only when a file appeared, grew or was touched.
    let stamp: RolloutStamp = files
        .iter()
        .map(|p| {
            let m = std::fs::metadata(p).ok();
            (
                p.clone(),
                m.as_ref().map(|m| m.len()).unwrap_or(0),
                m.and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos() as i64),
            )
        })
        .collect();
    {
        let c = ROLLOUTS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((s, v)) = c.as_ref() {
            if *s == stamp {
                return v.clone();
            }
        }
    }
    let parsed: Vec<Thread> = files
        .into_iter()
        .filter_map(|path| {
            let mut first = String::new();
            BufReader::new(std::fs::File::open(&path).ok()?)
                .read_line(&mut first)
                .ok()?;
            let v: Value = serde_json::from_str(&first).ok()?;
            if v["type"] != "session_meta" {
                return None;
            }
            let p = &v["payload"];
            Some(Thread {
                id: p["id"].as_str()?.to_string(),
                parent: subagent_parent(&v),
                title: p["title"].as_str().map(str::to_string),
                cwd: p["cwd"].as_str().map(str::to_string),
                branch: p["git"]["branch"].as_str().map(str::to_string),
                started_ms: p["timestamp"].as_str().and_then(parse_rfc3339),
                activity_ms: mtime_ms(&path),
                archived: false,
            })
        })
        .collect();
    *ROLLOUTS.lock().unwrap_or_else(|e| e.into_inner()) = Some((stamp, parsed.clone()));
    parsed
}

fn collect(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if depth > 1 && p.is_dir() {
            collect(&p, depth - 1, out);
        } else if depth == 1
            && p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        {
            out.push(p);
        }
    }
}

// Codex starts MCP servers with a cleared environment, so the task id and state dir must be
// passed in the server's own env or `send_message` and friends lose track of the sender.
// Its tools are pre-approved: with `--ask-for-approval never` (auto-approve) Codex otherwise
// refuses every MCP call, since it cannot ask.
pub fn overseer_mcp(exe: &str, state_dir: &str, task_id: &str) -> Vec<String> {
    let q = |v: &str| serde_json::Value::from(v).to_string();
    [
        format!("mcp_servers.overseer.command={}", q(exe)),
        "mcp_servers.overseer.args=[\"mcp\"]".to_string(),
        format!(
            "mcp_servers.overseer.env.OVERSEER_STATE_DIR={}",
            q(state_dir)
        ),
        format!("mcp_servers.overseer.env.OVERSEER_TASK_ID={}", q(task_id)),
        "mcp_servers.overseer.default_tools_approval_mode=\"approve\"".to_string(),
    ]
    .into_iter()
    .flat_map(|v| ["-c".to_string(), v])
    .collect()
}

// Subcommands codex parses before a prompt that never run a conversation: the Desktop
// backend, servers and tooling. `resume`, `fork`, `exec` and `review` do, so they stay sessions.
const NOT_SESSIONS: &[&str] = &[
    "agents",
    "login",
    "logout",
    "mcp",
    "mcp-server",
    "plugin",
    "app-server",
    "remote-control",
    "app",
    "completion",
    "update",
    "doctor",
    "sandbox",
    "debug",
    "apply",
    "a",
    "queue",
    "archive",
    "delete",
    "migrate-rollouts",
    "migrate-nfs-db",
    "unarchive",
    "cloud",
    "cloud-tasks",
    "exec-server",
    "features",
    "help",
    "responses-api-proxy",
    "stdio-to-uds",
];

const VALUE_FLAGS: &[&str] = &[
    "-c",
    "--config",
    "--enable",
    "--disable",
    "--remote",
    "--remote-auth-token-env",
    "-i",
    "--image",
    "-m",
    "--model",
    "--local-provider",
    "-p",
    "--profile",
    "-s",
    "--sandbox",
    "-C",
    "--cd",
    "--add-dir",
    "-a",
    "--ask-for-approval",
];

// codex is the program itself or the script of a JS runtime or shell. Anywhere else it is an argument:
// the Linux sandbox runs every tool call as `bwrap … -- …/bin/codex --sandbox-policy-cwd …`.
fn args(argv: &[String]) -> Option<&[String]> {
    let runtime = argv.first().is_some_and(|a| {
        matches!(
            a.rsplit('/').next().unwrap_or(a),
            "node" | "bun" | "deno" | "sh" | "bash" | "zsh" | "dash"
        )
    });
    let i = argv
        .iter()
        .take(if runtime { 2 } else { 1 })
        .position(|a| crate::agents::runs_as(crate::agents::CODEX, a))?;
    Some(&argv[i + 1..])
}

// The npm launcher (`node …/bin/codex`) spawns the native codex with the same arguments:
// keep the child, which holds the session's memory and reports its hooks, and drop the wrapper.
// ppid is only asked for processes that share their arguments with another one.
pub fn fold_wrappers(
    procs: Vec<(u32, Vec<String>)>,
    ppid: impl Fn(u32) -> Option<u32>,
) -> Vec<(u32, Vec<String>)> {
    let wrappers: HashSet<u32> = procs
        .iter()
        .filter(|(pid, argv)| procs.iter().any(|(p, b)| p != pid && args(b) == args(argv)))
        .filter_map(|(pid, argv)| {
            let parent = ppid(*pid)?;
            procs
                .iter()
                .any(|(p, b)| *p == parent && args(b) == args(argv))
                .then_some(parent)
        })
        .collect();
    procs
        .into_iter()
        .filter(|(pid, _)| !wrappers.contains(pid))
        .collect()
}

// The options of a codex command line, without its subcommand, the thread it names, `--last`
// or a prompt: resume adds its own `resume <thread>`.
pub fn launch_flags(flags: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut it = flags.iter();
    while let Some(a) = it.next() {
        if VALUE_FLAGS.contains(&a.as_str()) {
            out.push(a.clone());
            out.extend(it.next().cloned());
        } else if a.starts_with('-') && !matches!(a.as_str(), "--last" | "--all") {
            out.push(a.clone());
        }
    }
    out
}

fn subcommand(rest: &[String]) -> Option<&str> {
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        if VALUE_FLAGS.contains(&a.as_str()) {
            it.next();
        } else if !a.starts_with('-') {
            return Some(a);
        }
    }
    None
}

pub fn is_session(argv: &[String]) -> bool {
    args(argv).is_some_and(|rest| subcommand(rest).is_none_or(|s| !NOT_SESSIONS.contains(&s)))
}

// `codex resume` and `codex fork` reopen a thread created before the process; anything else
// starts a new one.
pub fn reopens(argv: &[String]) -> bool {
    args(argv)
        .and_then(subcommand)
        .is_some_and(|s| matches!(s, "resume" | "fork"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn overseer_mcp_passes_task_and_state_dir_as_toml() {
        let a = overseer_mcp("/opt/my overseer", "/home/u/.overseer", "01ABC");
        assert_eq!(
            a,
            [
                "-c",
                "mcp_servers.overseer.command=\"/opt/my overseer\"",
                "-c",
                "mcp_servers.overseer.args=[\"mcp\"]",
                "-c",
                "mcp_servers.overseer.env.OVERSEER_STATE_DIR=\"/home/u/.overseer\"",
                "-c",
                "mcp_servers.overseer.env.OVERSEER_TASK_ID=\"01ABC\"",
                "-c",
                "mcp_servers.overseer.default_tools_approval_mode=\"approve\"",
            ]
        );
        assert_eq!(launch_flags(&a), a);
    }

    fn session(cmd: &str) -> bool {
        is_session(&cmd.split(' ').map(str::to_string).collect::<Vec<_>>())
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-codex-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sessions/2026/11/19")).unwrap();
        d
    }

    fn write_rollout(dir: &Path, sid: &str, ts: &str, parent: Option<&str>, title: Option<&str>) {
        let source = match parent {
            Some(p) => json!({
                "subagent": {"thread_spawn": {"parent_thread_id": p}, "agent_role": "worker"},
            }),
            None => json!("vscode"),
        };
        let mut payload = json!({
            "id": sid, "timestamp": format!("2026-11-19T{ts}Z"), "cwd": "/w",
            "source": source, "thread_source": parent.map(|_| "subagent"),
        });
        if let Some(t) = title {
            payload["title"] = json!(t);
        }
        let meta = json!({"timestamp": format!("2026-11-19T{ts}Z"), "type": "session_meta", "payload": payload});
        std::fs::write(
            dir.join(format!("sessions/2026/11/19/rollout-{ts}-{sid}.jsonl")),
            format!("{meta}\n"),
        )
        .unwrap();
    }

    #[test]
    fn subagent_parent_is_read_from_the_rollout_meta() {
        let root = json!({"type": "session_meta", "payload": {"id": "r", "source": "vscode"}});
        assert_eq!(subagent_parent(&root), None);
        let sub = json!({
            "type": "session_meta",
            "payload": {
                "id": "s", "thread_source": "subagent", "depth": 2,
                "source": {"subagent": {"thread_spawn": {"parent_thread_id": "r"}}},
            }
        });
        assert_eq!(subagent_parent(&sub).as_deref(), Some("r"));
        // top-level spelling
        let sub2 = json!({
            "type": "session_meta",
            "thread_source": "subagent",
            "payload": {"id": "s2", "source": {"subagent": {"thread_spawn": {"parent_thread_id": "r2"}}}},
        });
        assert_eq!(subagent_parent(&sub2).as_deref(), Some("r2"));
    }

    #[test]
    fn sub_agent_threads_resolve_to_their_root_and_keep_one_line() {
        let d = tmp("roots");
        write_rollout(&d, "root-1", "10-00-00", None, Some("Fix the login"));
        write_rollout(&d, "mid-1", "11-00-00", Some("root-1"), None);
        write_rollout(&d, "leaf-1", "12-00-00", Some("mid-1"), None);
        write_rollout(&d, "other-2", "12-30-00", None, None);
        let (ts, roots) = load(&d, &d.join("state"));
        let mut ids: Vec<&str> = ts.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        assert_eq!(
            ids,
            ["other-2", "root-1"],
            "sub-agents collapse into their root"
        );
        let root = ts.iter().find(|t| t.id == "root-1").unwrap();
        assert_eq!(root.title.as_deref(), Some("Fix the login"));
        assert_eq!(root_in(&roots, "leaf-1").as_deref(), Some("root-1"));
        assert_eq!(root_in(&roots, "mid-1").as_deref(), Some("root-1"));
        assert_eq!(root_in(&roots, "root-1"), None);
        assert_eq!(root_in(&roots, "unknown"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rollout_headers_are_read_once_per_thread_not_on_every_db_change() {
        let d = tmp("header-reads");
        let day = d.join("sessions/2026/11/19");
        let db = rusqlite::Connection::open(d.join("state_5.sqlite")).unwrap();
        db.execute_batch(
            "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT, title TEXT, archived INTEGER, git_branch TEXT);",
        )
        .unwrap();
        let add = |i: usize| {
            let id = format!("t-{i}");
            write_rollout(&d, &id, &format!("1{i}-00-00"), None, None);
            let path = day.join(format!("rollout-1{i}-00-00-{id}.jsonl"));
            db.execute(
                "INSERT INTO threads VALUES (?1, ?2, 1, 1, '/w', 'x', 0, NULL)",
                rusqlite::params![id, path.to_string_lossy()],
            )
            .unwrap();
        };
        for i in 0..5 {
            add(i);
        }
        let state = d.join("state");
        let reads = || HEADER_READS.with(|c| c.get());
        let (ts, _) = load(&d, &state);
        assert_eq!(ts.len(), 5);
        let after_first = reads();
        add(5);
        let (ts, _) = load(&d, &state);
        assert_eq!(ts.len(), 6);
        assert!(
            reads() - after_first <= 1,
            "only the new thread's rollout is read, got {} reads",
            reads() - after_first
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_sub_agent_without_its_root_still_points_at_the_root() {
        let d = tmp("orphan-sub");
        write_rollout(&d, "leaf-9", "12-00-00", Some("gone-root"), None);
        let (ts, roots) = load(&d, &d.join("state"));
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].id, "gone-root");
        assert_eq!(root_in(&roots, "leaf-9").as_deref(), Some("gone-root"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn codex_hook_sessions_of_sub_agents_are_keyed_by_the_root() {
        let mut hooks = HashMap::new();
        hooks.insert(
            "leaf-1".to_string(),
            crate::registry::HookState {
                at_ms: 100,
                busy: true,
                ..Default::default()
            },
        );
        hooks.insert(
            "root-1".to_string(),
            crate::registry::HookState {
                at_ms: 50,
                ..Default::default()
            },
        );
        let d = tmp("hooks");
        write_rollout(&d, "root-1", "10-00-00", None, None);
        write_rollout(&d, "leaf-1", "12-00-00", Some("root-1"), None);
        let (_, roots) = load(&d, &d.join("state"));
        let remapped = root_hooks(&hooks, &roots);
        assert_eq!(remapped.len(), 1, "{remapped:?}");
        assert!(remapped.contains_key("root-1"));
        assert!(remapped["root-1"].busy, "the latest hook wins");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_state_db_title_and_first_user_message_name_the_thread() {
        let d = tmp("db-title");
        let db = d.join("state_9.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
               title TEXT, archived INTEGER, git_branch TEXT, first_user_message TEXT);",
        )
        .unwrap();
        let t = parse_rfc3339("2026-11-19T10:00:00Z").unwrap() / 1000;
        conn.execute(
            "INSERT INTO threads VALUES ('cx-t', NULL, ?1, ?1, '/w', 'Thread title', 0, NULL, 'prompt text')",
            rusqlite::params![t],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads VALUES ('cx-p', NULL, ?1, ?1, '/w', NULL, 0, NULL, 'Fix the login bug in src/auth.rs please')",
            rusqlite::params![t],
        )
        .unwrap();
        drop(conn);
        let ts = threads(&d, &d.join("state"));
        let titled = ts.iter().find(|t| t.id == "cx-t").unwrap();
        assert_eq!(titled.title.as_deref(), Some("Thread title"));
        let prompted = ts.iter().find(|t| t.id == "cx-p").unwrap();
        assert_eq!(
            prompted.title.as_deref(),
            Some("Fix the login bug in src/auth.rs please")
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_older_state_db_without_title_columns_still_loads() {
        let d = tmp("db-old");
        let db = d.join("state_1.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
               archived INTEGER, git_branch TEXT);",
        )
        .unwrap();
        let t = parse_rfc3339("2026-11-19T10:00:00Z").unwrap() / 1000;
        conn.execute(
            "INSERT INTO threads VALUES ('cx-old', NULL, ?1, ?1, '/w', 0, 'main')",
            rusqlite::params![t],
        )
        .unwrap();
        drop(conn);
        let ts = threads(&d, &d.join("state"));
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].id, "cx-old");
        assert_eq!(ts[0].branch.as_deref(), Some("main"));
        assert_eq!(ts[0].title, None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_state_db_sub_agent_row_points_at_its_root_via_its_rollout() {
        let d = tmp("db-sub");
        write_rollout(&d, "root-r", "10-00-00", None, None);
        write_rollout(&d, "sub-s", "12-00-00", Some("root-r"), None);
        let sub_rollout = d.join("sessions/2026/11/19/rollout-12-00-00-sub-s.jsonl");
        let db = d.join("state_2.sqlite");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE threads (id TEXT, rollout_path TEXT, created_at INTEGER, updated_at INTEGER, cwd TEXT,
               title TEXT, archived INTEGER, git_branch TEXT, first_user_message TEXT);",
        )
        .unwrap();
        let t = parse_rfc3339("2026-11-19T10:00:00Z").unwrap() / 1000;
        conn.execute(
            "INSERT INTO threads VALUES ('root-r', ?2, ?1, ?1, '/w', 'Root', 0, NULL, NULL)",
            rusqlite::params![
                t,
                d.join("sessions/2026/11/19/rollout-10-00-00-root-r.jsonl")
                    .to_string_lossy()
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads VALUES ('sub-s', ?2, ?1, ?1, '/w', 'Review', 0, NULL, NULL)",
            rusqlite::params![t, sub_rollout.to_string_lossy()],
        )
        .unwrap();
        drop(conn);
        let ts = threads(&d, &d.join("state"));
        assert_eq!(
            ts.len(),
            1,
            "the sub-agent row collapses into the root: {ts:?}"
        );
        assert_eq!(ts[0].id, "root-r");
        assert_eq!(
            ts[0].title.as_deref(),
            Some("Root"),
            "the root's title wins"
        );
        let (_, roots) = load(&d, &d.join("state"));
        assert_eq!(root_in(&roots, "sub-s").as_deref(), Some("root-r"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn same_command_in_two_terminals_is_two_sessions() {
        let argv = |s: &str| s.split(' ').map(str::to_string).collect::<Vec<_>>();
        let procs = vec![
            (10, argv("codex resume --last")),
            (20, argv("codex resume --last")),
        ];
        let kept = fold_wrappers(procs, |pid| Some(pid - 1));
        assert_eq!(kept.iter().map(|p| p.0).collect::<Vec<_>>(), [10, 20]);
    }

    #[test]
    fn only_conversations_are_sessions() {
        for cmd in [
            "codex",
            "codex --model o3",
            "codex resume 01a181a9",
            "node /n/bin/codex resume --last",
            "/bin/sh /u/.asdf/shims/codex",
            "codex -m o3 fork --last",
            "codex fix the login bug",
            "codex -c model=o3 exec do it",
            "codex review",
        ] {
            assert!(session(cmd), "{cmd}");
        }
        for cmd in [
            "/u/.codex/packages/standalone/releases/0.166.0/bin/codex app",
            "node /n/bin/codex app-server",
            "/n/codex-darwin-arm64/vendor/codex/codex app-server --listen stdio://",
            "codex -c model=o3 mcp-server",
            "codex --enable x mcp list",
            "codex login",
            "codex completion zsh",
            "bash -c ls",
            "bwrap --new-session --ro-bind / / --argv0 codex-linux-sandbox -- /u/.codex/packages/app-server-daemon/releases/0.166.0/bin/codex --sandbox-policy-cwd /r --command-cwd /r -- /bin/zsh -c ls",
            "rg codex src",
            "sh -c codex",
        ] {
            assert!(!session(cmd), "{cmd}");
        }
    }
}
