use crate::claude::mtime_ms;
use crate::util::parse_rfc3339;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

#[derive(Clone)]
pub struct Thread {
    pub id: String,
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
    state_db(codex_dir)
        .and_then(|db| from_db(&db, state_dir))
        .unwrap_or_else(|| from_rollouts(codex_dir))
}

#[derive(Clone)]
struct Row {
    thread: Thread,
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
    let mut stmt = conn
        .prepare("SELECT id, rollout_path, created_at, updated_at, cwd, archived, git_branch FROM threads")
        .ok()?;
    let rows = stmt
        .query_map([], |r| {
            let updated: Option<i64> = r.get(3)?;
            Ok(Row {
                rollout: r.get(1)?,
                updated_ms: updated.unwrap_or(0).saturating_mul(1000),
                thread: Thread {
                    id: r.get(0)?,
                    started_ms: r.get::<_, Option<i64>>(2)?.map(|s| s * 1000),
                    activity_ms: 0,
                    cwd: r.get::<_, Option<String>>(4)?.filter(|c| !c.is_empty()),
                    archived: r.get::<_, Option<i64>>(5)?.unwrap_or(0) != 0,
                    branch: r.get::<_, Option<String>>(6)?.filter(|b| !b.is_empty()),
                },
            })
        })
        .ok()?;
    Some(rows.flatten().collect())
}

fn from_rollouts(codex_dir: &Path) -> Vec<Thread> {
    let mut files = vec![];
    collect(&codex_dir.join("sessions"), 4, &mut files);
    files
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
                cwd: p["cwd"].as_str().map(str::to_string),
                branch: p["git"]["branch"].as_str().map(str::to_string),
                started_ms: p["timestamp"].as_str().and_then(parse_rfc3339),
                activity_ms: mtime_ms(&path),
                archived: false,
            })
        })
        .collect()
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
