pub mod actions;
pub mod activity;
pub mod agents;
pub mod autostart;
pub mod board;
pub mod broadcast;
pub mod chat;
pub mod claude;
pub mod claude_limits;
pub mod cli;
pub mod codex;
pub mod daemon;
pub mod deliver;
pub mod discover;
pub mod dlog;
pub mod doctor;
pub mod exec;
pub mod feedback;
pub mod files;
pub mod fsdirs;
pub mod gemini;
pub mod gh;
pub mod git;
pub mod gitgraph;
pub mod github;
pub mod gitops;
pub mod hold;
pub mod hooks;
pub mod http;
pub mod import;
pub mod inbox;
pub mod install;
pub mod layout;
pub mod legacy;
pub mod lifecycle;
pub mod mcp;
pub mod naming;
pub mod notes;
pub mod organize;
pub mod paths;
pub mod ports;
pub mod power;
pub mod procs;
pub mod profiles;
pub mod proxy;
pub mod quorum;
pub mod registry;
pub mod remote;
pub mod review;
pub mod runner;
pub mod scanner;
pub mod screen;
pub mod session_tasks;
pub mod shellenv;
pub mod status;
pub mod statusline;
pub mod sync;
pub mod sys;
pub mod sysmon;
pub mod task_agents;
pub mod task_sources;
pub mod task_transition;
pub mod tasks;
pub mod term;
pub mod termtheme;
pub mod tmux;
pub mod uninstall;
pub mod upgrade;
pub mod uploads;
pub mod usage;
pub mod util;
pub mod vibe;
pub mod workspaces;
pub mod worktrees;

use procs::{flags_after, Probe};
use registry::HookState;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sys::DirBuilderExt;
use util::{format_rfc3339, shell_quote, ticket};

pub struct Config {
    pub station_id: String,
    pub owner: String,
    pub roots: Vec<PathBuf>,
    pub idle_after_ms: i64,
    pub mcp_hints: Vec<String>,
    pub accept_remote_commands: bool,
    pub ssh: BTreeMap<String, String>,
    pub remote_command: String,
    pub repos: BTreeMap<String, PathBuf>,
    pub worktree_dir: PathBuf,
    pub start_args: Vec<String>,
    pub memory_policy: Option<f64>,
    pub auto_concurrency: usize,
    pub auto_organize: bool,
    pub worktree_fetch: bool,
    pub worktree_stale_days: i64,
    pub session_tasks_days: i64,
    pub discover_roots: Vec<PathBuf>,
}

pub struct Ctx<'a> {
    pub claude_dir: PathBuf,
    pub codex_dir: PathBuf,
    pub gemini_dir: PathBuf,
    pub vibe_dir: PathBuf,
    pub state_dir: PathBuf,
    pub snapshot: PathBuf,
    pub now_ms: i64,
    pub probe: &'a dyn Probe,
    pub tmux: Option<tmux::Tmux>,
    pub config: Config,
}

#[derive(Default, Clone, Debug)]
pub struct Line {
    pub id: String,
    pub owner: String,
    pub station_id: String,
    pub agent: &'static str,
    pub session_id: String,
    pub key: String,
    pub ticket: Option<String>,
    pub repo: String,
    pub branch: String,
    pub worktree_path: String,
    pub state: &'static str,
    pub held_reason: Option<&'static str>,
    pub last_activity_ms: i64,
    pub cwd: String,
    pub cwd_missing: bool,
    pub volatile: bool,
    pub pid: Option<u32>,
    pub busy: bool,
    pub busy_known: bool,
    pub launch_args: Vec<String>,
    pub resume_command: String,
    pub title: Option<String>,
    pub title_override: Option<String>,
    pub tmux: Option<String>,
    pub pane: Option<String>,
    pub held_pane: Option<String>,
    pub pinned: bool,
    pub needs_input: bool,
    pub orphan: bool,
    pub duplicate: bool,
    pub rss: Option<u64>,
    pub hook: Option<HookState>,
    pub offline: bool,
    pub origin: Option<String>,
    pub env_ref: Option<String>,
    pub term_id: Option<String>,
    pub workspace_id: Option<String>,
    pub hold_pending: bool,
    pub held_at: Option<String>,
    pub notes: Vec<Value>,
}

struct Found {
    agent: &'static str,
    key: String,
    session_id: String,
    cwd: String,
    branch: Option<String>,
    activity_ms: i64,
    alive: Option<(u32, Vec<String>)>,
    busy: bool,
    busy_known: bool,
    duplicate: bool,
    archived: bool,
    title: Option<String>,
    tmux: Option<String>,
    hook: Option<HookState>,
    shared: bool,
}

impl Found {
    fn new(agent: &'static str, session_id: String) -> Found {
        Found {
            agent,
            key: session_id.clone(),
            session_id,
            cwd: String::new(),
            branch: None,
            activity_ms: 0,
            alive: None,
            busy: false,
            busy_known: false,
            duplicate: false,
            archived: false,
            title: None,
            tmux: None,
            hook: None,
            shared: false,
        }
    }
}

pub fn mkdir_private(p: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

pub fn env_dir(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(default))
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

fn hostname() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        raw_hostname()
            .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok())
            .and_then(|h| h.trim().split('.').next().map(str::to_string))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "station".into())
    })
    .clone()
}

#[cfg(unix)]
fn raw_hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    let r = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if r != 0 {
        return None;
    }
    let n = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

#[cfg(not(unix))]
fn raw_hostname() -> Option<String> {
    exec::output_within(
        &mut std::process::Command::new("hostname"),
        exec::PROBE_TIMEOUT,
    )
    .ok()
    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

const EMAIL_TTL: std::time::Duration = std::time::Duration::from_secs(60);

fn git_email() -> Option<String> {
    type Slot = Option<(std::time::Instant, Option<String>)>;
    static EMAIL: std::sync::Mutex<Slot> = std::sync::Mutex::new(None);
    if let Some((at, v)) = EMAIL.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if at.elapsed() < EMAIL_TTL {
            return v.clone();
        }
    }
    let v =
        git::git(&home().to_string_lossy(), &["config", "user.email"]).filter(|e| !e.is_empty());
    *EMAIL.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), v.clone()));
    v
}

pub fn load_config(state_dir: &Path) -> Config {
    let v: Value = std::fs::read(state_dir.join("config.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let s = |k: &str| v[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let strs = |k: &str| -> Vec<String> {
        v[k].as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    Config {
        station_id: s("stationId").unwrap_or_else(hostname),
        owner: s("owner")
            .or_else(git_email)
            .or_else(|| std::env::var("USER").ok().filter(|u| !u.is_empty()))
            .unwrap_or_else(|| "unknown".into()),
        roots: strs("roots").iter().map(|r| expand(r)).collect(),
        idle_after_ms: v["idleAfterSeconds"].as_i64().unwrap_or(600) * 1000,
        mcp_hints: strs("mcpCommands"),
        accept_remote_commands: v["acceptRemoteCommands"].as_bool().unwrap_or(false),
        ssh: v["ssh"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, t)| Some((k.clone(), t.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
        remote_command: s("remoteCommand").unwrap_or_else(|| "overseer".into()),
        repos: v["repos"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, p)| Some((k.to_ascii_lowercase(), expand(p.as_str()?))))
                    .collect()
            })
            .unwrap_or_default(),
        worktree_dir: s("worktreeDir")
            .map(|p| expand(&p))
            .unwrap_or_else(paths::default_worktrees_dir),
        start_args: strs("startArgs"),
        memory_policy: (v["memoryPolicy"]["enabled"] == true).then(|| {
            v["memoryPolicy"]["thresholdPercent"]
                .as_f64()
                .unwrap_or(85.0)
        }),
        auto_concurrency: v["autoConcurrency"].as_u64().unwrap_or(2) as usize,
        auto_organize: v["autoOrganize"].as_bool().unwrap_or(false),
        worktree_fetch: v["worktreeFetch"].as_bool().unwrap_or(true),
        worktree_stale_days: v["worktreeStaleDays"].as_i64().unwrap_or(14),
        session_tasks_days: v["sessionTasksDays"].as_i64().unwrap_or(7),
        discover_roots: match v["discoverRoots"].as_array() {
            Some(a) => a.iter().filter_map(|r| r.as_str()).map(expand).collect(),
            None => [
                "~/Documents/Github",
                "~/src",
                "~/code",
                "~/projects",
                "~/dev",
            ]
            .iter()
            .map(|r| expand(r))
            .filter(|p| p.is_dir())
            .collect(),
        },
    }
}

pub fn default_state_dir() -> PathBuf {
    std::env::var_os("OVERSEER_STATE_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(platform_state_dir)
}

pub fn platform_state_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(d).join(wire::STATE_DIR_NAME);
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    if cfg!(target_os = "macos") {
        return home.join("Library/Application Support/overseer");
    }
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join(wire::STATE_DIR_NAME)
}

impl<'a> Ctx<'a> {
    pub fn from_env(probe: &'a dyn Probe) -> Ctx<'a> {
        let state_dir = default_state_dir();
        Ctx {
            claude_dir: agents::config_dir("claude"),
            codex_dir: agents::config_dir(agents::CODEX),
            gemini_dir: agents::config_dir(agents::GEMINI),
            vibe_dir: agents::config_dir(agents::VIBE),
            snapshot: std::env::var_os("OVERSEER_TMUX_SNAPSHOT")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".local/state/tmux-snapshot/latest/state.json")),
            config: load_config(&state_dir),
            state_dir,
            now_ms: std::env::var("OVERSEER_NOW")
                .ok()
                .and_then(|s| util::parse_rfc3339(&s))
                .unwrap_or_else(procs::now_ms),
            probe,
            tmux: Some(tmux::Tmux::from_env()),
        }
    }
}

pub fn line_state(
    archived: bool,
    alive: bool,
    busy: bool,
    activity_ms: i64,
    now_ms: i64,
    idle_after_ms: i64,
) -> (&'static str, Option<&'static str>) {
    if archived {
        (wire::STATE_HUNG_UP, None)
    } else if !alive {
        (wire::STATE_HELD, Some("lost"))
    } else if busy || now_ms - activity_ms <= idle_after_ms {
        (wire::STATE_ACTIVE, None)
    } else {
        (wire::STATE_IDLE, None)
    }
}

pub fn apply_marks(
    state: (&'static str, Option<&'static str>),
    held_by_user: bool,
    closed: bool,
) -> (&'static str, Option<&'static str>) {
    match state {
        (wire::STATE_HELD, _) if closed => (wire::STATE_HUNG_UP, None),
        (wire::STATE_HELD, _) if held_by_user => (wire::STATE_HELD, Some("user")),
        s => s,
    }
}

fn is_volatile(path: &str) -> bool {
    let tmp = std::env::temp_dir();
    ["/tmp/", "/private/tmp/", "/var/folders/"]
        .iter()
        .any(|p| path.starts_with(p))
        || Path::new(path).starts_with(&tmp) && tmp != Path::new("/")
}

pub fn live_cwd(recorded: &str, live: impl IntoIterator<Item = Option<String>>) -> String {
    if !recorded.is_empty() && Path::new(recorded).is_dir() {
        return recorded.to_string();
    }
    let live: Vec<String> = live
        .into_iter()
        .flatten()
        .filter(|c| !c.is_empty())
        .collect();
    live.iter()
        .find(|c| Path::new(c).is_dir())
        .or_else(|| live.first().filter(|_| recorded.is_empty()))
        .cloned()
        .unwrap_or_else(|| recorded.to_string())
}

pub fn process_cwd(recorded: &str, live: impl IntoIterator<Item = Option<String>>) -> String {
    live.into_iter()
        .flatten()
        .find(|c| !c.is_empty() && Path::new(c).is_dir())
        .unwrap_or_else(|| recorded.to_string())
}

fn claude_found(
    ctx: &Ctx,
    hooks: &HashMap<String, HookState>,
    remembered: &HashMap<String, String>,
) -> Vec<Found> {
    claude_found_from(
        claude::transcripts(&ctx.claude_dir),
        claude::live(&ctx.claude_dir, ctx.probe),
        hooks,
        remembered,
    )
}

fn claude_found_from(
    transcripts: Vec<claude::Transcript>,
    mut live: Vec<claude::Live>,
    hooks: &HashMap<String, HookState>,
    remembered: &HashMap<String, String>,
) -> Vec<Found> {
    let mut by_sid: BTreeMap<String, Found> = BTreeMap::new();
    let mut seen: HashMap<String, (String, Option<String>, i64, Option<String>)> = HashMap::new();
    for t in transcripts {
        let activity = t.mtime_ms.max(t.started_ms.unwrap_or(0));
        let cwd = t.cwd.unwrap_or_default();
        seen.insert(
            t.session_id.clone(),
            (cwd.clone(), t.branch.clone(), activity, t.title.clone()),
        );
        let f = by_sid
            .entry(t.session_id.clone())
            .or_insert_with(|| Found::new(agents::CLAUDE, t.session_id.clone()));
        f.cwd = cwd;
        f.branch = t.branch;
        f.activity_ms = activity;
        f.title = t.title;
    }
    live.sort_by_key(|l| (l.launch_id.as_deref() != Some(l.session_id.as_str()), l.pid));
    let mut reported: HashMap<String, usize> = HashMap::new();
    for l in live.iter().filter(|l| l.alive.is_some()) {
        *reported.entry(l.session_id.clone()).or_default() += 1;
    }
    // A process resumed on the session a line was last in goes back to that line, unless a
    // live process still answers to the line's own key.
    let own: HashSet<&str> = live
        .iter()
        .filter(|l| l.alive.is_some())
        .flat_map(|l| [Some(l.session_id.as_str()), l.launch_id.as_deref()])
        .flatten()
        .collect();
    let mut lines_now: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (key, now) in remembered {
        if key != now && !own.contains(key.as_str()) {
            lines_now.entry(now).or_default().insert(key);
        }
    }
    let mut taken: HashSet<String> = HashSet::new();
    let line_of = |sid: &str, taken: &HashSet<String>| {
        lines_now
            .get(sid)?
            .iter()
            .find(|k| !taken.contains(**k))
            .map(|k| k.to_string())
    };
    let mut status_ms: HashMap<String, i64> = HashMap::new();
    for l in live {
        let Some(args) = l.alive else {
            let f = by_sid
                .entry(l.session_id.clone())
                .or_insert_with(|| Found::new(agents::CLAUDE, l.session_id.clone()));
            f.cwd = live_cwd(&f.cwd, [l.cwd.clone()]);
            f.activity_ms = f.activity_ms.max(l.updated_ms);
            continue;
        };
        let key = l
            .launch_id
            .as_deref()
            .and_then(|k| line_of(k, &taken))
            .unwrap_or_else(|| match l.launch_id.as_deref() {
                Some(k) if !taken.contains(k) => k.to_string(),
                _ => line_of(&l.session_id, &taken).unwrap_or_else(|| {
                    if taken.contains(&l.session_id) {
                        format!("{}~{}", l.session_id, l.pid)
                    } else {
                        l.session_id.clone()
                    }
                }),
            });
        taken.insert(key.clone());
        let f = by_sid
            .entry(key.clone())
            .or_insert_with(|| Found::new(agents::CLAUDE, key.clone()));
        if f.session_id != l.session_id {
            f.session_id = l.session_id.clone();
            if let Some((cwd, branch, activity, title)) = seen.get(&l.session_id) {
                f.cwd = cwd.clone();
                f.branch = branch.clone();
                f.activity_ms = *activity;
                f.title = title.clone();
            }
        }
        f.shared = reported.get(&l.session_id).copied().unwrap_or(0) > 1;
        f.duplicate = f.shared;
        f.cwd = live_cwd(&f.cwd, [l.cwd.clone()]);
        f.activity_ms = f.activity_ms.max(l.updated_ms);
        f.alive = Some((l.pid, args));
        f.busy = l.busy;
        f.busy_known = true;
        f.tmux = l.tmux;
        status_ms.insert(key, l.status_ms);
    }
    let mut claimed: HashSet<String> = HashSet::new();
    for (key, now) in remembered {
        if taken.contains(key) || key == now {
            continue;
        }
        let Some((cwd, branch, activity, title)) = by_sid
            .get(now)
            .filter(|f| f.alive.is_none())
            .map(|f| {
                (
                    f.cwd.clone(),
                    f.branch.clone(),
                    f.activity_ms,
                    f.title.clone(),
                )
            })
            .or_else(|| seen.get(now).cloned())
        else {
            continue;
        };
        let f = by_sid
            .entry(key.clone())
            .or_insert_with(|| Found::new(agents::CLAUDE, key.clone()));
        f.session_id = now.clone();
        f.cwd = cwd;
        f.branch = branch;
        f.activity_ms = activity;
        f.title = title;
        claimed.insert(now.clone());
    }
    by_sid.retain(|k, f| {
        f.alive.is_some()
            || taken.contains(k)
            || *k != f.session_id
            || !(reported.contains_key(k) || claimed.contains(k))
    });
    for f in by_sid.values_mut() {
        let Some(h) = hooks.get(&f.session_id) else {
            continue;
        };
        if f.shared && h.pid != f.alive.as_ref().map(|a| a.0) {
            continue;
        }
        f.activity_ms = f.activity_ms.max(h.at_ms);
        if f.alive.is_some() && h.at_ms > status_ms.get(&f.key).copied().unwrap_or(0) {
            f.busy = h.busy;
            f.busy_known = true;
        }
    }
    by_sid.into_values().collect()
}

// Codex TUIs share one app-server that runs the hooks, so a hook pid never names a TUI and a
// fresh `codex` shows no thread id: those are paired with threads in their cwd on times that do
// not move. A fresh process only owns a thread created after it started, or one used since.
const CODEX_SLACK_MS: i64 = 2_000;
const CODEX_TYPED_MS: i64 = 3_000;

fn codex_found(ctx: &Ctx, hooks: &HashMap<String, HookState>) -> Vec<Found> {
    let threads = codex::threads(&ctx.codex_dir, &ctx.state_dir);
    let mut procs: Vec<(u32, Vec<String>, Option<i64>)> = codex::fold_wrappers(
        ctx.probe
            .list()
            .into_iter()
            .filter(|(_, argv)| codex::is_session(argv))
            .collect(),
        |pid| ctx.probe.ppid(pid),
    )
    .into_iter()
    .map(|(pid, argv)| {
        let start = ctx.probe.info(pid).and_then(|i| i.start_ms);
        (pid, argv, start)
    })
    .collect();
    procs.sort_by_key(|p| (p.2, p.0));
    let ids: HashSet<&str> = threads.iter().map(|t| t.id.as_str()).collect();
    let mut owner: Vec<Option<usize>> = vec![None; threads.len()];
    let mut claimed = HashSet::new();
    for (i, t) in threads.iter().enumerate().filter(|(_, t)| !t.archived) {
        let free = |j: &usize| !claimed.contains(j);
        let hit = hooks
            .get(&t.id)
            .and_then(|h| h.pid)
            .and_then(|p| (0..procs.len()).filter(free).find(|j| procs[*j].0 == p))
            .or_else(|| {
                (0..procs.len())
                    .filter(free)
                    .find(|j| procs[*j].1.contains(&t.id))
            });
        if let Some(j) = hit {
            claimed.insert(j);
            owner[i] = Some(j);
        }
    }
    let terms: Vec<Option<Arc<term::Term>>> = procs
        .iter()
        .map(|(pid, ..)| term::of_pid(*pid, ctx.probe))
        .collect();
    for (i, t) in threads.iter().enumerate().filter(|(_, t)| !t.archived) {
        if owner[i].is_some() {
            continue;
        }
        let hit = (0..procs.len()).find(|j| {
            !claimed.contains(j)
                && terms[*j].as_ref().and_then(|x| x.thread()).as_deref() == Some(&t.id)
        });
        if let Some(j) = hit {
            claimed.insert(j);
            owner[i] = Some(j);
        }
    }
    let open = |i: usize| !threads[i].archived && threads[i].cwd.is_some();
    let names_other =
        |argv: &[String], id: &str| argv.iter().any(|a| a != id && ids.contains(a.as_str()));
    let mut newest: Vec<usize> = (0..threads.len()).filter(|i| open(*i)).collect();
    newest.sort_by_key(|i| std::cmp::Reverse(threads[*i].started_ms));
    let mut recent: Vec<usize> = (0..threads.len()).filter(|i| open(*i)).collect();
    recent.sort_by_key(|i| std::cmp::Reverse(threads[*i].activity_ms));
    let typed_at = |j: usize, at: Option<i64>| {
        let s = terms[j].as_ref().map_or(0, |x| x.submit_ms());
        s > 0 && at.is_some_and(|at| (s - at).abs() <= CODEX_TYPED_MS)
    };
    let mut cwds = BTreeMap::new();
    let mut in_cwd = |pid: u32, cwd: &str| {
        cwds.entry(pid)
            .or_insert_with(|| ctx.probe.cwd(pid))
            .as_deref()
            == Some(cwd)
    };
    let mut fits_new = vec![vec![false; procs.len()]; threads.len()];
    let mut fits_used = fits_new.clone();
    for i in newest.iter().chain(recent.iter()).copied() {
        let t = &threads[i];
        let cwd = t.cwd.as_deref().unwrap_or_default();
        for (j, (pid, argv, start)) in procs.iter().enumerate() {
            if names_other(argv, &t.id) || !in_cwd(*pid, cwd) {
                continue;
            }
            fits_new[i][j] = !codex::reopens(argv)
                && start
                    .zip(t.started_ms)
                    .is_none_or(|(s, c)| c + CODEX_SLACK_MS >= s);
            fits_used[i][j] =
                codex::reopens(argv) || start.is_none_or(|s| t.activity_ms + CODEX_SLACK_MS >= s);
        }
    }
    for (order, fits, stamp) in [(newest, &fits_new, true), (recent, &fits_used, false)] {
        loop {
            let mut moved = false;
            for &i in &order {
                if owner[i].is_some() {
                    continue;
                }
                let free: Vec<usize> = (0..procs.len())
                    .filter(|j| !claimed.contains(j) && fits[i][*j])
                    .collect();
                let at = if stamp {
                    threads[i].started_ms
                } else {
                    Some(threads[i].activity_ms)
                };
                let typed: Vec<usize> = free.iter().copied().filter(|j| typed_at(*j, at)).collect();
                let pick = match (free.as_slice(), typed.as_slice()) {
                    ([j], _) | (_, [j]) => *j,
                    _ => continue,
                };
                if let Some(x) = &terms[pick] {
                    x.bind_thread(&threads[i].id);
                }
                claimed.insert(pick);
                owner[i] = Some(pick);
                moved = true;
            }
            if !moved {
                break;
            }
        }
    }
    threads
        .into_iter()
        .zip(owner)
        .map(|(t, j)| {
            let mut f = Found::new(agents::CODEX, t.id.clone());
            let hook = hooks.get(&t.id);
            if let Some((pid, argv, _)) = j.map(|j| &procs[j]) {
                let flags =
                    flags_after(argv, |a| agents::runs_as(agents::CODEX, a)).unwrap_or_default();
                f.alive = Some((*pid, codex::launch_flags(&flags)));
            }
            if let Some(h) = hook {
                f.busy = h.busy;
                f.busy_known = f.alive.is_some();
            }
            f.cwd = t.cwd.unwrap_or_default();
            f.branch = t.branch;
            f.activity_ms = t
                .activity_ms
                .max(t.started_ms.unwrap_or(0))
                .max(hook.map_or(0, |h| h.at_ms));
            f.archived = t.archived;
            f
        })
        .collect()
}

fn gemini_found(ctx: &Ctx, hooks: &HashMap<String, HookState>) -> Vec<Found> {
    let mut sessions = gemini::sessions(&ctx.gemini_dir);
    if sessions.is_empty() {
        return vec![];
    }
    sessions.sort_by_key(|s| -s.activity_ms);
    let all: Vec<(u32, Vec<String>)> = ctx
        .probe
        .list()
        .into_iter()
        .filter(|(_, argv)| {
            argv.iter()
                .take(3)
                .any(|a| agents::runs_as(agents::GEMINI, a))
        })
        .collect();
    let procs: Vec<&(u32, Vec<String>)> = all
        .iter()
        .filter(|(pid, _)| {
            ctx.probe
                .ppid(*pid)
                .is_none_or(|pp| !all.iter().any(|(p, _)| *p == pp))
        })
        .collect();
    let mut claimed = HashSet::new();
    let mut cwd_taken = HashSet::new();
    let mut cwds = BTreeMap::new();
    sessions
        .into_iter()
        .map(|s| {
            let mut f = Found::new(agents::GEMINI, s.id.clone());
            let first_in_cwd = s.cwd.as_ref().is_some_and(|c| cwd_taken.insert(c.clone()));
            let hit = procs
                .iter()
                .find(|(pid, argv)| !claimed.contains(pid) && argv.contains(&s.id))
                .or_else(|| {
                    let cwd = s.cwd.as_deref().filter(|_| first_in_cwd)?;
                    procs.iter().find(|(pid, argv)| {
                        !claimed.contains(pid)
                            && !argv
                                .iter()
                                .any(|a| a == "--session-id" || a == "--resume" || a == "-r")
                            && cwds
                                .entry(*pid)
                                .or_insert_with(|| ctx.probe.cwd(*pid))
                                .as_deref()
                                == Some(cwd)
                    })
                });
            if let Some((pid, argv)) = hit {
                claimed.insert(*pid);
                let flags =
                    flags_after(argv, |a| agents::runs_as(agents::GEMINI, a)).unwrap_or_default();
                f.alive = Some((*pid, without_prompt(&procs::strip_resume(&flags))));
            }
            if let Some(h) = hooks.get(&s.id) {
                f.busy = h.busy;
                f.busy_known = f.alive.is_some();
            }
            f.cwd = s.cwd.unwrap_or_default();
            f.activity_ms = s
                .activity_ms
                .max(s.started_ms.unwrap_or(0))
                .max(hooks.get(&s.id).map_or(0, |h| h.at_ms));
            f
        })
        .collect()
}

fn vibe_found(ctx: &Ctx, hooks: &HashMap<String, HookState>) -> Vec<Found> {
    let mut sessions = vibe::sessions(&ctx.vibe_dir);
    if sessions.is_empty() {
        return vec![];
    }
    sessions.sort_by_key(|s| -s.activity_ms);
    let procs: HashMap<u32, Vec<String>> = ctx
        .probe
        .list()
        .into_iter()
        .filter(|(_, argv)| {
            argv.iter()
                .take(3)
                .any(|a| agents::runs_as(agents::VIBE, a))
        })
        .collect();
    let tui = |pid: u32| {
        let mut p = Some(pid);
        for _ in 0..4 {
            let cur = p?;
            if procs.contains_key(&cur) {
                return Some(cur);
            }
            p = ctx.probe.ppid(cur);
        }
        None
    };
    let mut claimed = HashSet::new();
    sessions
        .into_iter()
        .map(|s| {
            let mut f = Found::new(agents::VIBE, s.id.clone());
            let leased = s
                .pid
                .filter(|p| ctx.probe.info(*p).is_some())
                .and_then(tui)
                .filter(|t| !claimed.contains(t));
            let named = || {
                procs
                    .iter()
                    .find(|(pid, argv)| !claimed.contains(*pid) && argv.contains(&s.id))
                    .map(|(pid, _)| *pid)
            };
            let live = leased.or_else(named);
            if let Some(t) = live {
                claimed.insert(t);
                let flags = procs
                    .get(&t)
                    .and_then(|argv| flags_after(argv, |a| agents::runs_as(agents::VIBE, a)))
                    .unwrap_or_default();
                f.alive = Some((t, without_prompt(&procs::strip_resume(&flags))));
            }
            let hook = hooks.get(&s.id);
            if let Some(h) = hook {
                f.busy = h.busy;
                f.busy_known = f.alive.is_some();
            }
            if let (Some(h), Some((pid, _))) = (hook, &f.alive) {
                if let Some(t) = term::of_pid(*pid, ctx.probe) {
                    f.busy = term::effective_busy(
                        h.busy,
                        h.at_ms,
                        t.submit_ms(),
                        t.turn_busy(ctx.now_ms),
                    );
                }
            }
            f.cwd = s.cwd.unwrap_or_default();
            f.title = s.title;
            f.archived = s.archived;
            f.activity_ms = s
                .activity_ms
                .max(s.started_ms.unwrap_or(0))
                .max(hook.map_or(0, |h| h.at_ms));
            f
        })
        .collect()
}

fn without_prompt(flags: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut it = flags.iter();
    while let Some(f) = it.next() {
        match f.as_str() {
            "-i" | "--prompt-interactive" | "-p" | "--prompt" => {
                it.next();
            }
            "--" => break,
            s if s.starts_with("--prompt-interactive=") || s.starts_with("--prompt=") => {}
            _ => out.push(f.clone()),
        }
    }
    out
}

fn term_found(states: &HashMap<(&'static str, String), HookState>, now_ms: i64) -> Vec<Found> {
    let mut by_sid: BTreeMap<(&'static str, String), (bool, i64, Found)> = BTreeMap::new();
    for t in term::all() {
        let i = t.info();
        let Some(a) = i
            .agent
            .as_deref()
            .and_then(agents::get)
            .filter(|a| a.polled())
        else {
            continue;
        };
        let sid = i.session.clone().unwrap_or_else(|| i.id.clone());
        let mut f = Found::new(a.id, sid.clone());
        f.cwd = i.cwd.clone();
        f.activity_ms = t.last_output_ms().max(i.created_ms);
        f.title = Some(i.title.clone());
        f.alive = (!i.exited && i.pid > 0).then(|| (i.pid, i.flags.clone()));
        let hook = states
            .get(&(a.id, hooks::term_key(&i.id)))
            .filter(|h| h.at_ms >= i.created_ms);
        let detector = t.turn_busy(now_ms).filter(|_| f.alive.is_some());
        if let Some(h) = hook {
            f.busy = term::effective_busy(h.busy, h.at_ms, t.submit_ms(), detector);
            f.busy_known = f.alive.is_some();
            f.activity_ms = f.activity_ms.max(h.at_ms);
            f.hook = Some(h.clone());
        } else if let Some(b) = detector {
            f.busy = b;
            f.busy_known = true;
        }
        let key = (a.id, sid);
        let rank = (!i.exited, i.created_ms);
        if by_sid.get(&key).is_none_or(|(l, c, _)| (*l, *c) < rank) {
            by_sid.insert(key, (rank.0, rank.1, f));
        }
    }
    by_sid.into_values().map(|(_, _, f)| f).collect()
}

pub fn snapshot_args(path: &Path) -> HashMap<String, Vec<String>> {
    let v: Value = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let mut out: HashMap<String, (bool, Vec<String>)> = HashMap::new();
    for p in v["panes"].as_array().into_iter().flatten() {
        let (Some(sid), Some(args)) = (p["claude_session"].as_str(), p["claude_args"].as_array())
        else {
            continue;
        };
        let guessed = p["claude_session_guessed"].as_bool().unwrap_or(false);
        let args: Vec<String> = args
            .iter()
            .filter_map(|a| a.as_str().map(str::to_string))
            .collect();
        let args = procs::strip_resume(&args);
        match out.get(sid) {
            Some((false, _)) if guessed => {}
            _ => {
                out.insert(sid.to_string(), (guessed, args));
            }
        }
    }
    out.into_iter().map(|(k, (_, a))| (k, a)).collect()
}

pub fn resume_flags(agent: &str, flags: &[String]) -> Vec<String> {
    let flags = procs::rejoin_split_json(flags);
    let Some(a) = agents::get(agent) else {
        return flags;
    };
    match a.repair {
        agents::Repair::Codex => {
            let mut flags = codex::launch_flags(&flags);
            let mode = a.mode_flags("", &flags);
            flags.splice(0..0, mode);
            flags
        }
        agents::Repair::Claude => claude::launch_flags(&flags),
        agents::Repair::None => flags,
    }
}

pub fn resume_argv(agent: &str, session_id: &str, flags: &[String]) -> Vec<String> {
    agents::get(agent)
        .and_then(|a| a.resume_argv(session_id, &resume_flags(agent, flags)))
        .unwrap_or_else(|| vec![agent.to_string()])
}

pub fn resume_command(agent: &str, cwd: &str, session_id: &str, flags: &[String]) -> String {
    let argv: Vec<String> = resume_argv(agent, session_id, flags)
        .iter()
        .map(|a| shell_quote(a))
        .collect();
    format!("cd {} && {}", shell_quote(cwd), argv.join(" "))
}

fn scan_linked(ctx: &Ctx) -> Result<Vec<Line>, String> {
    let lines = scan_now(ctx)?;
    term::link_lines(&lines);
    Ok(lines)
}

fn term_hosts_session(session_id: &str) -> bool {
    !session_id.is_empty()
        && term::all().iter().any(|t| {
            let i = t.info();
            !i.exited && i.session.as_deref() == Some(session_id)
        })
}

pub fn scan(ctx: &Ctx) -> Result<Vec<Line>, String> {
    scanner::fresh(|| scan_linked(ctx))
}

pub fn current_lines(ctx: &Ctx) -> Result<Vec<Line>, String> {
    scanner::current(|| scan_linked(ctx))
}

pub fn session_at_exit(t: &term::Info) -> Option<(String, String)> {
    let agent = t.agent.as_deref()?;
    let ctx = Ctx::from_env(&procs::System);
    let lines = scan(&ctx).ok()?;
    let pick = lines
        .iter()
        .find(|l| l.term_id.as_deref() == Some(t.id.as_str()))
        .or_else(|| {
            lines
                .iter()
                .filter(|l| {
                    l.term_id.is_none()
                        && l.pid.is_none()
                        && l.agent == agent
                        && l.cwd == t.cwd
                        && l.last_activity_ms >= t.created_ms
                })
                .max_by_key(|l| l.last_activity_ms)
        })?;
    Some((pick.session_id.clone(), pick.resume_command.clone()))
}

type UpsertKey = (PathBuf, u64, &'static str, String);
type Upsert = (u64, String, Vec<String>);
type Upserted = HashMap<UpsertKey, Upsert>;

fn db_id(state_dir: &Path) -> u64 {
    let Ok(m) = std::fs::metadata(state_dir.join(paths::REGISTRY_DB)) else {
        return 0;
    };
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let ino = 0u64;
    let born = m
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64);
    ino ^ born.rotate_left(17)
}

static UPSERTED: std::sync::Mutex<Option<Upserted>> = std::sync::Mutex::new(None);

fn upsert_once(
    ctx: &Ctx,
    reg: &registry::Registry,
    l: &Line,
    alive: Option<&Vec<String>>,
    fallback: Option<&Vec<String>>,
    db: u64,
    done: &mut Vec<(UpsertKey, Upsert)>,
) -> Result<(String, Vec<String>), String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (
        &l.owner,
        &l.station_id,
        &l.repo,
        &l.branch,
        &l.worktree_path,
        &l.cwd,
        &l.ticket,
        l.state,
        l.held_reason,
        l.last_activity_ms,
        &l.session_id,
    )
        .hash(&mut h);
    (alive, fallback).hash(&mut h);
    let fp = h.finish();
    let key = (
        ctx.state_dir.clone(),
        db,
        l.agent,
        l.registry_key().to_string(),
    );
    let hit = UPSERTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&key))
        .filter(|e| e.0 == fp)
        .map(|e| (e.1.clone(), e.2.clone()));
    if let Some(r) = hit {
        return Ok(r);
    }
    let r = reg.upsert_line(l, alive, fallback, ctx.now_ms)?;
    done.push((key, (fp, r.0.clone(), r.1.clone())));
    Ok(r)
}

type WorktreeRow = (String, Option<String>, Option<String>, bool);
type SeenTrees = HashMap<(u64, String), (WorktreeRow, i64)>;

static SEEN_TREES: std::sync::Mutex<Option<SeenTrees>> = std::sync::Mutex::new(None);
const TREE_REFRESH_MS: i64 = 3_600_000;

fn tree_unchanged(db: u64, path: &str, row: &WorktreeRow, now_ms: i64) -> bool {
    SEEN_TREES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&(db, path.to_string())))
        .is_some_and(|(r, at)| r == row && now_ms - at < TREE_REFRESH_MS)
}

fn remember_trees(done: Vec<((u64, String), (WorktreeRow, i64))>) {
    if done.is_empty() {
        return;
    }
    let mut g = SEEN_TREES.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() + done.len() > 10_000 {
        m.clear();
    }
    m.extend(done);
}

fn remember_upserts(done: Vec<(UpsertKey, Upsert)>) {
    if done.is_empty() {
        return;
    }
    let mut g = UPSERTED.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() + done.len() > 50_000 {
        m.clear();
    }
    m.extend(done);
}

fn scan_now(ctx: &Ctx) -> Result<Vec<Line>, String> {
    mkdir_private(&ctx.state_dir)
        .map_err(|e| format!("cannot create {}: {e}", ctx.state_dir.display()))?;
    let reg = registry::Registry::open(&ctx.state_dir)?;
    reg.conn
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    hooks::ingest(&ctx.state_dir, &reg, ctx.probe)?;
    reg.conn
        .execute_batch("COMMIT")
        .map_err(|e| e.to_string())?;
    let hook_map = |agent| -> Result<HashMap<String, HookState>, String> {
        Ok(reg.hook_sessions(agent)?.into_iter().collect())
    };
    let mut session_hooks: HashMap<&'static str, HashMap<String, HookState>> = HashMap::new();
    for a in agents::AGENTS.iter().filter(|a| a.sessions()) {
        session_hooks.insert(a.id, hook_map(a.id)?);
    }
    let no_hooks = HashMap::new();
    let hooks_of = |id: &str| session_hooks.get(id).unwrap_or(&no_hooks);
    let mut term_hooks = HashMap::new();
    for a in agents::AGENTS.iter().filter(|a| a.polled()) {
        for (k, h) in reg.hook_sessions(a.id)? {
            if k.starts_with("term:") {
                term_hooks.insert((a.id, k), h);
            }
        }
    }
    let snap = snapshot_args(&ctx.snapshot);
    let panes = ctx.tmux.as_ref().map(|t| t.panes()).unwrap_or_default();
    let mut g = git::Git::default();
    let mut repo_dirs: BTreeSet<(String, String)> = BTreeSet::new();
    let mut found = vec![];
    for a in agents::AGENTS.iter().filter(|a| a.sessions()) {
        let hooks = hooks_of(a.id);
        match a.transcript {
            Some(agents::Transcript::Claude) => {
                found.extend(claude_found(ctx, hooks, &reg.divergent(a.id)?))
            }
            Some(agents::Transcript::Codex) => found.extend(codex_found(ctx, hooks)),
            Some(agents::Transcript::Gemini) => found.extend(gemini_found(ctx, hooks)),
            Some(agents::Transcript::Vibe) => found.extend(vibe_found(ctx, hooks)),
            None => {}
        }
    }
    found.extend(term_found(&term_hooks, ctx.now_ms));
    let probed: Vec<_> = found
        .into_iter()
        .map(|mut f| {
            let pid = f.alive.as_ref().map(|a| a.0);
            let term_id = pid.and_then(|p| term::owner_of(p, ctx.probe));
            if let Some(p) = pid {
                let term_cwd = term_id.as_deref().and_then(term::get).map(|t| t.info().cwd);
                f.cwd = process_cwd(&f.cwd, [ctx.probe.cwd(p), term_cwd]);
            }
            let info = (!f.cwd.is_empty()).then(|| g.info(&f.cwd)).flatten();
            if let Some(i) = &info {
                repo_dirs.insert((i.common.clone(), i.top.clone()));
            }
            let pane = pid
                .and_then(|p| tmux::pane_of(p, &panes, ctx.probe))
                .cloned();
            let rss = pid.and_then(|p| ctx.probe.rss(p));
            (f, pid, term_id, info, pane, rss)
        })
        .collect();
    for root in &ctx.config.roots {
        let mut candidates = vec![root.clone()];
        if let Ok(rd) = std::fs::read_dir(root) {
            candidates.extend(rd.flatten().map(|e| e.path()));
        }
        for c in candidates.into_iter().filter(|c| c.join(".git").exists()) {
            if let Some(i) = g.info(&c.to_string_lossy()) {
                repo_dirs.insert((i.common.clone(), i.top.clone()));
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut trees = vec![];
    for (common, top) in repo_dirs {
        if !seen.insert(common) {
            continue;
        }
        let repo = g
            .info(&top)
            .map(|i| i.repo)
            .unwrap_or_else(|| "unknown".into());
        trees.push((repo, git::worktrees(&top)));
    }
    reg.conn
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|e| e.to_string())?;
    let mut lines = vec![];
    let mut upserted = vec![];
    let db = db_id(&ctx.state_dir);
    // An empty process list is a failed probe (pid 1 always exists): it proves no loss.
    let listed = std::cell::OnceCell::new();
    let probe_ok = || *listed.get_or_init(|| !ctx.probe.list().is_empty());
    for (f, pid, term_id, info, pane, rss) in probed {
        let branch = f
            .branch
            .filter(|b| !b.is_empty() && b != "HEAD")
            .or_else(|| info.as_ref().and_then(|i| i.branch.clone()))
            .unwrap_or_default();
        let marks = reg.marks(f.agent, &f.key)?;
        let alive = f.alive.is_some();
        let (state, held_reason) = apply_marks(
            line_state(
                f.archived,
                alive,
                f.busy,
                f.activity_ms,
                ctx.now_ms,
                ctx.config.idle_after_ms,
            ),
            marks.held_pid != 0,
            marks.closed,
        );
        let worktree_path = info
            .as_ref()
            .map(|i| i.top.clone())
            .unwrap_or_else(|| f.cwd.clone());
        let hook = if agents::get(f.agent).is_some_and(|a| a.sessions()) {
            hooks_of(f.agent)
                .get(&f.session_id)
                .filter(|h| !f.shared || h.pid == pid)
        } else {
            f.hook.as_ref()
        }
        .cloned();
        let mut l = Line {
            owner: marks.owner.clone().unwrap_or_else(|| {
                info.as_ref()
                    .and_then(|i| i.email.clone())
                    .unwrap_or_else(|| ctx.config.owner.clone())
            }),
            origin: marks.origin.clone(),
            env_ref: marks.env_ref.clone(),
            workspace_id: marks.workspace_id.clone(),
            station_id: ctx.config.station_id.clone(),
            agent: f.agent,
            ticket: marks.ticket_override.clone().or_else(|| ticket(&branch)),
            repo: info
                .as_ref()
                .map(|i| i.repo.clone())
                .unwrap_or_else(|| "unknown".into()),
            branch,
            volatile: is_volatile(&worktree_path),
            worktree_path: if worktree_path.is_empty() {
                "unknown".into()
            } else {
                worktree_path
            },
            state,
            held_reason,
            last_activity_ms: f.activity_ms,
            cwd_missing: f.cwd.is_empty() || !Path::new(&f.cwd).is_dir(),
            cwd: f.cwd,
            pid,
            busy: f.busy,
            busy_known: f.busy_known,
            title: f.title,
            title_override: marks.title_override.clone(),
            notes: marks.notes.clone(),
            tmux: f.tmux,
            orphan: pane.as_ref().is_some_and(|p| !p.attached),
            pane: pane.map(|p| p.id),
            duplicate: f.duplicate,
            pinned: marks.pinned,
            needs_input: alive && hook.as_ref().is_some_and(|h| h.needs_input),
            rss,
            hook,
            term_id,
            key: f.key,
            session_id: f.session_id,
            ..Default::default()
        };
        let fallback = agents::get(f.agent)
            .is_some_and(|a| a.tmux_snapshot)
            .then(|| snap.get(&l.session_id))
            .flatten();
        let alive_args = f.alive.as_ref().map(|a| &a.1);
        let (id, args) = upsert_once(ctx, &reg, &l, alive_args, fallback, db, &mut upserted)?;
        if alive && (marks.held_pid != 0 || marks.closed) && pid != Some(marks.held_pid as u32) {
            reg.set_mark(&id, "held_by_user", None)?;
            reg.set_mark(&id, "closed_at", None)?;
        }
        // The line left the held state (resumed) or hung up: drop stale pause marks.
        let still_held = marks.held_pid != 0 && pid == Some(marks.held_pid as u32);
        l.held_at = marks.held_at.clone();
        if l.state == wire::STATE_HELD {
            if l.held_at.is_none() && (marks.held_pid != 0 || probe_ok()) {
                let at = format_rfc3339(ctx.now_ms);
                reg.set_mark(&id, "held_at", Some(&at))?;
                l.held_at = Some(at);
            }
        } else if l.held_at.is_some() && !still_held {
            reg.set_mark(&id, "held_at", None)?;
            l.held_at = None;
        }
        l.hold_pending = marks.hold_pending;
        let restarting = !alive && term_hosts_session(&l.session_id);
        if l.hold_pending && (marks.closed || !alive && !restarting && probe_ok()) {
            reg.set_mark(&id, "hold_pending", None)?;
            l.hold_pending = false;
        }
        l.held_pane = panes
            .iter()
            .find(|p| p.held.as_deref() == Some(id.as_str()) && !p.dead)
            .map(|p| p.id.clone());
        l.id = id;
        if l.workspace_id.is_none() {
            let from_term = l.term_id.as_deref().and_then(term::workspace_of);
            if let Some(w) = from_term {
                reg.set_mark(&l.id, "workspace_id", Some(&w))?;
                l.workspace_id = Some(w);
            }
        }
        l.resume_command = resume_command(l.agent, &l.cwd, &l.session_id, &args);
        l.launch_args = args;
        lines.push(l);
    }
    let mut wrote = vec![];
    for (repo, list) in trees {
        for w in list {
            let t = w.branch.as_deref().and_then(ticket);
            let row: WorktreeRow = (
                repo.clone(),
                w.branch.clone(),
                t.clone(),
                w.prunable || is_volatile(&w.path),
            );
            if tree_unchanged(db, &w.path, &row, ctx.now_ms) {
                continue;
            }
            reg.upsert_worktree(
                &w.path,
                &repo,
                w.branch.as_deref(),
                t.as_deref(),
                row.3,
                ctx.now_ms,
            )?;
            wrote.push(((db, w.path.clone()), (row, ctx.now_ms)));
        }
    }
    reg.conn
        .execute_batch("COMMIT")
        .map_err(|e| e.to_string())?;
    remember_upserts(upserted);
    remember_trees(wrote);
    lines.sort_by_key(|l| -l.last_activity_ms);
    Ok(lines)
}

pub mod wire {
    include!("../contract/wire.rs");
}

pub const RECENT_MS: i64 = 7 * 24 * 3600 * 1000;

#[derive(Default)]
pub struct LineFilter {
    all: bool,
    ticket: Option<String>,
    repo: Option<String>,
    state: Option<String>,
}

impl LineFilter {
    pub fn new(
        all: bool,
        ticket: Option<&str>,
        repo: Option<&str>,
        state: Option<&str>,
    ) -> LineFilter {
        LineFilter {
            all,
            ticket: ticket.map(|t| util::ticket(t).unwrap_or_else(|| t.to_ascii_uppercase())),
            repo: repo.map(str::to_ascii_lowercase),
            state: state.map(str::to_string),
        }
    }

    pub fn matches(&self, l: &Line, now_ms: i64) -> bool {
        (self.all || l.recent(now_ms))
            && self
                .ticket
                .as_ref()
                .is_none_or(|t| l.ticket.as_ref() == Some(t))
            && self
                .repo
                .as_ref()
                .is_none_or(|r| l.repo.to_ascii_lowercase().contains(r.as_str()))
            && self.state.as_ref().is_none_or(|s| l.state == *s)
    }
}

impl Line {
    pub fn registry_key(&self) -> &str {
        if self.key.is_empty() {
            &self.session_id
        } else {
            &self.key
        }
    }

    pub fn alive(&self) -> bool {
        self.pid.is_some()
    }

    pub fn recent(&self, now_ms: i64) -> bool {
        self.alive()
            || self.pinned
            || self.held_reason == Some("user")
            || now_ms - self.last_activity_ms <= RECENT_MS
    }

    pub fn to_json(&self) -> Value {
        let mut line = json!({
            "id": self.id,
            "owner": self.owner,
            "stationId": self.station_id,
            "agent": self.agent,
            "agentSessionId": self.session_id,
            "repo": self.repo,
            "branch": self.branch,
            "worktreePath": self.worktree_path,
            "origin": self.origin.as_deref().unwrap_or("dev"),
            "state": self.state,
            "lastActivityAt": format_rfc3339(self.last_activity_ms),
        });
        if let Some(t) = &self.ticket {
            line["ticketKey"] = json!(t);
        }
        if let Some(e) = &self.env_ref {
            line["envRef"] = json!(e);
        }
        let mut local = json!({
            "busy": self.busy,
            "needsInput": self.needs_input,
            "pinned": self.pinned,
            "orphan": self.orphan,
            "duplicate": self.duplicate,
            "cwd": self.cwd,
            "cwdMissing": self.cwd_missing,
            "volatile": self.volatile,
            "launchArgs": self.launch_args,
            "resumeCommand": self.resume_command,
            "resumeVerified": agents::get(self.agent).is_some_and(|a| a.resume_verified),
            "titleOverride": self.title_override,
            "notes": self.notes,
        });
        if let Some(p) = self.pid {
            local["pid"] = json!(p);
        }
        if let Some(r) = self.rss {
            local["rssBytes"] = json!(r);
        }
        if let Some(r) = self.held_reason {
            local["heldReason"] = json!(r);
        }
        if let Some(a) = &self.held_at {
            local["heldAt"] = json!(a);
        }
        if self.hold_pending {
            local["holdPending"] = json!(true);
        }
        let live =
            |n: fn(&HookState) -> i64| self.hook.as_ref().filter(|_| self.alive()).map_or(0, n);
        local["subagents"] = json!(live(|h| h.subagents));
        local["tasks"] = json!(live(|h| h.tasks));
        if let Some(t) = &self.title {
            local["title"] = json!(t);
        }
        if let Some(t) = &self.term_id {
            local["termId"] = json!(t);
        }
        if let Some(w) = &self.workspace_id {
            local["workspaceId"] = json!(w);
        }
        if let Some(p) = self.pane.as_ref().or(self.held_pane.as_ref()) {
            local["tmuxPane"] = json!(p);
        }
        if let Some(t) = &self.tmux {
            if let Some((session, pane)) = t.split_once(':') {
                local["tmuxSession"] = json!(session);
                if local.get("tmuxPane").is_none() {
                    if let Some((_, p)) = pane.rsplit_once('.') {
                        local["tmuxPane"] = json!(p);
                    }
                }
            }
        }
        json!({ "line": line, "local": local })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_resume_config_override_selects_embedded_mode() {
        let flags = vec![
            "-c".to_string(),
            "mcp_servers.overseer.args=[\"mcp\"]".to_string(),
        ];
        assert_eq!(
            resume_flags("codex", &flags),
            ["--no-daemon", "-c", "mcp_servers.overseer.args=[\"mcp\"]",]
        );
    }

    #[test]
    fn unchanged_worktrees_are_not_rewritten_every_scan() {
        let row: WorktreeRow = ("github.com/a/b".into(), Some("main".into()), None, false);
        let db = 0xfeed_u64 ^ std::process::id() as u64;
        assert!(!tree_unchanged(db, "/w/x", &row, 1_000));
        remember_trees(vec![((db, "/w/x".into()), (row.clone(), 1_000))]);
        assert!(tree_unchanged(db, "/w/x", &row, 2_000));
        assert!(!tree_unchanged(db + 1, "/w/x", &row, 2_000));
        let mut moved = row.clone();
        moved.1 = Some("feat/y".into());
        assert!(!tree_unchanged(db, "/w/x", &moved, 2_000));
        assert!(!tree_unchanged(db, "/w/x", &row, 1_000 + TREE_REFRESH_MS));
    }
}
