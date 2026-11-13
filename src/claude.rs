use crate::procs::{flags_after, launch_session, same_process, strip_resume, Probe};
use crate::util::parse_rfc3339;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const CHUNK: u64 = 256 * 1024;
const MAX_TAIL: u64 = 16 * CHUNK;

#[derive(Clone, Default)]
pub struct Transcript {
    pub session_id: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub started_ms: Option<i64>,
    pub title: Option<String>,
    pub mtime_ms: i64,
}

const VALUE_FLAGS: &[&str] = &[
    "--agent",
    "--agents",
    "--append-system-prompt",
    "--autocompact",
    "--cloud",
    "--debug",
    "-d",
    "--debug-file",
    "--effort",
    "--environment",
    "--fallback-model",
    "--from-pr",
    "--input-format",
    "--json-schema",
    "--max-budget-usd",
    "--model",
    "--name",
    "-n",
    "--output-format",
    "--permission-mode",
    "--permission-prompts",
    "--plugin-dir",
    "--plugin-url",
    "--prompt-suggestions",
    "--remote-control",
    "--remote-control-session-name-prefix",
    "--setting-sources",
    "--settings",
    "--system-prompt",
    "--system-prompt-snapshot",
    "--teleport",
];

const VARIADIC_FLAGS: &[&str] = &[
    "--add-dir",
    "--allowedTools",
    "--allowed-tools",
    "--betas",
    "--disallowedTools",
    "--disallowed-tools",
    "--file",
    "--mcp-config",
    "--tools",
];

// Resume re-passes these options after `--resume`; a positional prompt left in them would be
// submitted again on every relaunch.
pub fn launch_flags(flags: &[String]) -> Vec<String> {
    let mut out = vec![];
    let mut it = flags.iter().peekable();
    while let Some(a) = it.next() {
        if !a.starts_with('-') {
            continue;
        }
        out.push(a.clone());
        if VALUE_FLAGS.contains(&a.as_str()) {
            if let Some(n) = it.next_if(|n| !n.starts_with('-')) {
                out.push(n.clone());
            }
        } else if VARIADIC_FLAGS.contains(&a.as_str()) {
            while let Some(n) = it.next_if(|n| !n.starts_with('-')) {
                out.push(n.clone());
            }
        }
    }
    out
}

pub fn overseer_mcp(exe: &str, state_dir: &str, task_id: &str) -> Vec<String> {
    let config = serde_json::json!({"mcpServers": {"overseer": {
        "command": exe,
        "args": ["mcp"],
        "env": {"OVERSEER_STATE_DIR": state_dir, "OVERSEER_TASK_ID": task_id},
    }}});
    vec![
        "--mcp-config".to_string(),
        config.to_string(),
        "--allowedTools".to_string(),
        "mcp__overseer".to_string(),
    ]
}

pub struct Live {
    pub pid: u32,
    pub status_ms: i64,
    pub session_id: String,
    pub cwd: Option<String>,
    pub busy: bool,
    pub updated_ms: i64,
    pub tmux: Option<String>,
    pub alive: Option<Vec<String>>,
    pub launch_id: Option<String>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "aiTitle")]
    ai_title: Option<String>,
}

pub fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_at(f: &mut File, off: u64, len: u64) -> Vec<u8> {
    let mut buf = vec![];
    if f.seek(SeekFrom::Start(off)).is_ok() {
        let _ = f.take(len).read_to_end(&mut buf);
    }
    buf
}

fn entries(buf: &[u8], skip_first: bool, sid: &str) -> Vec<Entry> {
    buf.split(|b| *b == b'\n')
        .skip(usize::from(skip_first))
        .filter_map(|l| serde_json::from_slice::<Entry>(l).ok())
        .filter(|e| e.session_id.as_deref().is_none_or(|s| s == sid))
        .collect()
}

pub fn read_transcript(path: &Path) -> Option<Transcript> {
    let sid = path.file_stem()?.to_str()?.to_string();
    let mut f = File::open(path).ok()?;
    let size = f.metadata().ok()?.len();
    let mut t = Transcript {
        session_id: sid.clone(),
        mtime_ms: mtime_ms(path),
        ..Default::default()
    };
    let head = entries(&read_at(&mut f, 0, CHUNK), false, &sid);
    for e in &head {
        if t.cwd.is_none() {
            t.cwd = e.cwd.clone().filter(|c| !c.is_empty());
        }
        if t.started_ms.is_none() {
            t.started_ms = e.timestamp.as_deref().and_then(parse_rfc3339);
        }
    }
    let mut len = CHUNK;
    let tail = loop {
        if size <= CHUNK {
            break vec![];
        }
        let start = size.saturating_sub(len);
        let tail = entries(&read_at(&mut f, start, size - start), start > 0, &sid);
        if start == 0 || len >= MAX_TAIL || tail.iter().any(|e| e.git_branch.is_some()) {
            break tail;
        }
        len *= 4;
    };
    let last = |pick: fn(&Entry) -> Option<&String>| {
        tail.iter()
            .rev()
            .chain(head.iter().rev())
            .find_map(|e| pick(e).cloned())
    };
    t.branch = last(|e| e.git_branch.as_ref());
    t.title = last(|e| {
        e.ai_title
            .as_ref()
            .filter(|_| e.kind.as_deref() == Some("ai-title"))
    });
    if t.cwd.is_none() {
        t.cwd = last(|e| e.cwd.as_ref().filter(|c| !c.is_empty()));
    }
    Some(t)
}

type Stamp = (u64, i64);

fn file_stamp(m: &std::fs::Metadata) -> Stamp {
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);
    (m.len(), t)
}

type Parsed = HashMap<PathBuf, (Stamp, Transcript)>;

fn parsed() -> &'static Mutex<Parsed> {
    static C: OnceLock<Mutex<Parsed>> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub fn projects_dir(claude_dir: &Path) -> PathBuf {
    claude_dir.join("projects")
}

pub fn session_file(project_dir: &Path, session_id: &str) -> PathBuf {
    project_dir.join(format!("{session_id}.jsonl"))
}

pub fn subagents_dir(transcript: &Path) -> PathBuf {
    transcript.with_extension("").join("subagents")
}

pub fn subagent_file(subagents_dir: &Path, agent_id: &str) -> PathBuf {
    subagents_dir.join(format!("agent-{agent_id}.jsonl"))
}

pub fn transcripts(claude_dir: &Path) -> Vec<Transcript> {
    let root = projects_dir(claude_dir);
    let Ok(projects) = std::fs::read_dir(&root) else {
        return vec![];
    };
    let mut files: Vec<(PathBuf, Stamp)> = vec![];
    for p in projects.flatten().filter(|p| p.path().is_dir()) {
        let Ok(dir) = std::fs::read_dir(p.path()) else {
            continue;
        };
        for f in dir.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            if let Some(m) = std::fs::metadata(&path).ok().filter(|m| m.is_file()) {
                files.push((path, file_stamp(&m)));
            }
        }
    }
    let seen: HashSet<PathBuf> = files.iter().map(|f| f.0.clone()).collect();
    let mut out = Vec::with_capacity(files.len());
    let mut missing = vec![];
    {
        let c = parsed().lock().unwrap_or_else(|e| e.into_inner());
        for (path, st) in files {
            match c.get(&path).filter(|e| e.0 == st) {
                Some(e) => out.push(e.1.clone()),
                None => missing.push((path, st)),
            }
        }
    }
    let fresh: Vec<(PathBuf, Stamp, Transcript)> = missing
        .into_iter()
        .filter_map(|(p, st)| read_transcript(&p).map(|t| (p, st, t)))
        .collect();
    let mut c = parsed().lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|p, _| !p.starts_with(&root) || seen.contains(p));
    for (p, st, t) in fresh {
        out.push(t.clone());
        c.insert(p, (st, t));
    }
    out
}

pub fn titles(claude_dir: &Path, session_id: &str, n: usize) -> Vec<String> {
    let Some(path) = std::fs::read_dir(projects_dir(claude_dir))
        .into_iter()
        .flatten()
        .flatten()
        .map(|p| session_file(&p.path(), session_id))
        .find(|p| p.is_file())
    else {
        return vec![];
    };
    let Ok(mut f) = File::open(&path) else {
        return vec![];
    };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = size.saturating_sub(4 * CHUNK);
    let mut out: Vec<String> = vec![];
    for e in entries(&read_at(&mut f, start, size - start), start > 0, session_id) {
        if let (Some("ai-title"), Some(t)) = (e.kind.as_deref(), e.ai_title) {
            if out.last() != Some(&t) {
                out.push(t);
            }
        }
    }
    let skip = out.len().saturating_sub(n);
    out.split_off(skip)
}

/// Last parsed content of each session file: Claude rewrites it in place, so a scan can read it
/// torn mid-write, and that one read must not make a live session vanish.
static LAST_GOOD: OnceLock<Mutex<HashMap<PathBuf, Value>>> = OnceLock::new();

fn read_session(path: &Path, seen: &mut HashSet<PathBuf>) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    let mut cache = LAST_GOOD
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    seen.insert(path.to_path_buf());
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(v) => {
            cache.insert(path.to_path_buf(), v.clone());
            Some(v)
        }
        Err(_) => cache.get(path).cloned(),
    }
}

pub fn live(claude_dir: &Path, probe: &dyn Probe) -> Vec<Live> {
    let sessions = claude_dir.join("sessions");
    let Ok(dir) = std::fs::read_dir(&sessions) else {
        return vec![];
    };
    let mut seen = HashSet::new();
    let out = live_in(dir, probe, &mut seen);
    LAST_GOOD
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|p, _| !p.starts_with(&sessions) || seen.contains(p));
    out
}

fn live_in(dir: std::fs::ReadDir, probe: &dyn Probe, seen: &mut HashSet<PathBuf>) -> Vec<Live> {
    let mut out = vec![];
    for e in dir.flatten() {
        let path = e.path();
        let Some(pid) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".json"))
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Some(v) = read_session(&path, seen) else {
            continue;
        };
        let Some(session_id) = v["sessionId"].as_str().map(str::to_string) else {
            continue;
        };
        let proc_start = match &v["procStart"] {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        };
        let started_ms = v["startedAt"].as_i64();
        let flags = probe.info(pid).and_then(|i| {
            let same = match (&i.start, &proc_start) {
                (Some(a), Some(b)) => a == b,
                _ => same_process(started_ms, i.start_ms),
            };
            if !same {
                return None;
            }
            flags_after(&i.argv, |a| {
                crate::agents::runs_as(crate::agents::CLAUDE, a)
            })
        });
        let launch_id = flags.as_deref().and_then(launch_session);
        let alive = flags.map(|f| launch_flags(&strip_resume(&f)));
        out.push(Live {
            pid,
            session_id,
            cwd: v["cwd"].as_str().map(str::to_string),
            busy: v["status"].as_str() == Some("busy"),
            updated_ms: v["updatedAt"].as_i64().unwrap_or(0),
            status_ms: v["statusUpdatedAt"]
                .as_i64()
                .or(v["updatedAt"].as_i64())
                .unwrap_or(0),
            tmux: v["tmux"].as_str().map(str::to_string),
            alive,
            launch_id,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_path_shapes() {
        let main = Path::new("/c/projects/-w/s1.jsonl");
        assert_eq!(projects_dir(Path::new("/c")), Path::new("/c/projects"));
        assert_eq!(session_file(Path::new("/c/projects/-w"), "s1"), main);
        let subs = subagents_dir(main);
        assert_eq!(subs, Path::new("/c/projects/-w/s1/subagents"));
        assert_eq!(
            subagent_file(&subs, "a1"),
            Path::new("/c/projects/-w/s1/subagents/agent-a1.jsonl")
        );
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn launch_flags_drop_the_positional_prompt() {
        assert_eq!(
            launch_flags(&v(&["--effort", "high", "fix the bug"])),
            v(&["--effort", "high"])
        );
        assert_eq!(
            launch_flags(&v(&[
                "--model",
                "opus",
                "-p",
                "do it",
                "--dangerously-skip-permissions"
            ])),
            v(&["--model", "opus", "-p", "--dangerously-skip-permissions"])
        );
        assert_eq!(launch_flags(&v(&["do it"])), Vec::<String>::new());
        assert_eq!(
            launch_flags(&v(&["--settings={\"a\": 1}", "x"])),
            v(&["--settings={\"a\": 1}"])
        );
    }

    #[test]
    fn overseer_mcp_passes_task_and_state_dir_and_pre_approves_the_tools() {
        let a = overseer_mcp("/opt/my overseer", "/home/u/.overseer", "01ABC");
        assert_eq!(a[0], "--mcp-config");
        let v: serde_json::Value = serde_json::from_str(&a[1]).unwrap();
        let s = &v["mcpServers"]["overseer"];
        assert_eq!(s["command"], "/opt/my overseer");
        assert_eq!(s["args"], serde_json::json!(["mcp"]));
        assert_eq!(s["env"]["OVERSEER_TASK_ID"], "01ABC");
        assert_eq!(s["env"]["OVERSEER_STATE_DIR"], "/home/u/.overseer");
        assert_eq!(a[2..], ["--allowedTools", "mcp__overseer"]);
    }

    #[test]
    fn overseer_mcp_survives_a_relaunch() {
        let a = overseer_mcp("/x", "/s", "T");
        let mut flags = vec!["--model".to_string(), "opus".to_string()];
        flags.extend(a.iter().cloned());
        let kept = launch_flags(&flags);
        assert_eq!(kept[2..], a[..]);
    }

    #[test]
    fn launch_flags_keep_the_values_of_flags_that_take_one() {
        let f = v(&[
            "--model",
            "opus",
            "--permission-mode",
            "plan",
            "--mcp-config",
            "a.json",
            "b.json",
            "--allowedTools",
            "Bash",
            "Edit",
            "--append-system-prompt",
            "be brief",
            "-n",
            "work",
        ]);
        assert_eq!(launch_flags(&f), f);
        let with_prompt = [f.clone(), v(&["--effort", "max", "the task"])].concat();
        assert_eq!(
            launch_flags(&with_prompt),
            [f, v(&["--effort", "max"])].concat()
        );
    }

    struct Alive(std::cell::Cell<bool>);
    impl Probe for Alive {
        fn info(&self, pid: u32) -> Option<crate::procs::ProcInfo> {
            (pid == 4242 && self.0.get()).then(|| crate::procs::ProcInfo {
                start: Some("77".into()),
                argv: v(&["claude", "--session-id", "s1"]),
                start_ms: None,
            })
        }
        fn list(&self) -> Vec<(u32, Vec<String>)> {
            vec![]
        }
        fn cwd(&self, _: u32) -> Option<String> {
            None
        }
    }

    #[test]
    fn a_session_file_read_mid_rewrite_keeps_the_session_live() {
        let dir = std::env::temp_dir().join(format!("sb-claude-torn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        let file = dir.join("sessions/4242.json");
        let body = r#"{"pid":4242,"sessionId":"s1","cwd":"/w","procStart":"77","status":"busy"}"#;
        std::fs::write(&file, body).unwrap();
        let probe = Alive(std::cell::Cell::new(true));
        let first = live(&dir, &probe);
        assert_eq!(first.len(), 1);
        assert!(first[0].alive.is_some() && first[0].busy);
        std::fs::write(&file, &body[..body.len() / 2]).unwrap();
        let torn = live(&dir, &probe);
        assert_eq!(torn.len(), 1, "a torn read must not drop the session");
        assert_eq!(torn[0].session_id, "s1");
        assert!(torn[0].alive.is_some() && torn[0].busy);
        probe.0.set(false);
        assert!(live(&dir, &probe).iter().all(|l| l.alive.is_none()));
        std::fs::remove_file(&file).unwrap();
        assert!(live(&dir, &probe).is_empty());
        std::fs::write(&file, "").unwrap();
        probe.0.set(true);
        assert!(
            live(&dir, &probe).is_empty(),
            "no stale cache once the file was gone"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_huge_tail_without_branch_stops_at_the_cap() {
        let dir = std::env::temp_dir().join(format!("sb-claude-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("s9.jsonl");
        let mut text = String::from(
            "{\"type\":\"user\",\"sessionId\":\"s9\",\"cwd\":\"/tmp/p\",\"gitBranch\":\"head\"}\n",
        );
        let filler = format!(
            "{{\"type\":\"assistant\",\"sessionId\":\"s9\",\"x\":\"{}\"}}\n",
            "y".repeat(4000)
        );
        while (text.len() as u64) < MAX_TAIL * 2 {
            text.push_str(&filler);
        }
        std::fs::write(&file, text).unwrap();
        let t = read_transcript(&file).unwrap();
        assert_eq!(t.branch.as_deref(), Some("head"));
        assert_eq!(t.cwd.as_deref(), Some("/tmp/p"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn transcripts_are_reparsed_only_when_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("sb-claude-cache-{}", std::process::id()));
        let proj = dir.join("projects").join("-tmp-p");
        std::fs::create_dir_all(&proj).unwrap();
        let file = proj.join("s1.jsonl");
        let line = |b: &str| {
            format!("{{\"type\":\"user\",\"sessionId\":\"s1\",\"cwd\":\"/tmp/p\",\"gitBranch\":\"{b}\"}}\n")
        };
        std::fs::write(&file, line("one")).unwrap();
        let first = transcripts(&dir);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].branch.as_deref(), Some("one"));
        let cached = parsed().lock().unwrap().get(&file).map(|e| e.0).unwrap();
        assert_eq!(transcripts(&dir)[0].branch.as_deref(), Some("one"));
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .unwrap();
        std::io::Write::write_all(&mut f, line("two").as_bytes()).unwrap();
        drop(f);
        assert_eq!(transcripts(&dir)[0].branch.as_deref(), Some("two"));
        assert_ne!(
            parsed().lock().unwrap().get(&file).map(|e| e.0),
            Some(cached)
        );
        std::fs::remove_file(&file).unwrap();
        assert!(transcripts(&dir).is_empty());
        assert!(!parsed().lock().unwrap().contains_key(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
