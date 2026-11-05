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

pub fn transcripts(claude_dir: &Path) -> Vec<Transcript> {
    let root = claude_dir.join("projects");
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
    let name = format!("{session_id}.jsonl");
    let Some(path) = std::fs::read_dir(claude_dir.join("projects"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|p| p.path().join(&name))
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

pub fn is_claude(arg: &str) -> bool {
    arg.contains("claude")
}

pub fn live(claude_dir: &Path, probe: &dyn Probe) -> Vec<Live> {
    let Ok(dir) = std::fs::read_dir(claude_dir.join("sessions")) else {
        return vec![];
    };
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
        let Some(v) = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
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
            flags_after(&i.argv, is_claude)
        });
        let launch_id = flags.as_deref().and_then(launch_session);
        let alive = flags.map(|f| strip_resume(&f));
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
