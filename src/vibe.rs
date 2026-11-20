use crate::claude::mtime_ms;
use crate::util::parse_rfc3339;
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const STORE_FORMAT: &str = "mistral.vibe.unified-session-store/v1";
const STORE_MINOR: u64 = 10;
const MAX_DOC: u64 = 64 << 20;
const MAX_META: u64 = 4 << 20;

pub fn home() -> PathBuf {
    crate::agents::config_dir("vibe")
}

pub fn root(home: &Path) -> PathBuf {
    home.join("logs").join("session")
}

pub fn valid_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 128
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
}

pub fn session_dir(home: &Path, id: &str) -> Option<PathBuf> {
    valid_id(id)
        .then(|| root(home).join("unified").join(id))
        .filter(|d| d.join("CURRENT").is_file())
}

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub cwd: Option<String>,
    pub started_ms: Option<i64>,
    pub activity_ms: i64,
    pub dir: PathBuf,
    pub title: Option<String>,
    pub archived: bool,
    pub model: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Clone)]
struct Meta {
    id: String,
    parent: bool,
    cwd: Option<String>,
    started_ms: Option<i64>,
    title: Option<String>,
    archived: bool,
    model: Option<String>,
}

type Metas = HashMap<PathBuf, (i64, Option<Meta>)>;

static METAS: Mutex<Option<Metas>> = Mutex::new(None);
static KINDS: Mutex<Option<HashMap<PathBuf, bool>>> = Mutex::new(None);

fn read_capped(path: &Path, cap: u64) -> Option<Vec<u8>> {
    let mut buf = vec![];
    std::fs::File::open(path)
        .ok()?
        .take(cap)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

fn json(path: &Path, cap: u64) -> Option<Value> {
    serde_json::from_slice(&read_capped(path, cap)?).ok()
}

fn str_of(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn meta(path: &Path) -> Option<Meta> {
    let v = json(path, MAX_META)?;
    let id = v["session_id"]
        .as_str()
        .filter(|i| valid_id(i))?
        .to_string();
    Some(Meta {
        id,
        parent: str_of(&v["parent_session_id"]).is_some(),
        cwd: str_of(&v["environment"]["working_directory"])
            .or_else(|| str_of(&v["origin_directory"])),
        started_ms: v["start_time"].as_str().and_then(parse_rfc3339),
        title: str_of(&v["title"]),
        archived: !v["archived_at"].is_null(),
        model: str_of(&v["config"]["active_model"]),
    })
}

fn cached_meta(path: &Path, mtime: i64) -> Option<Meta> {
    let mut g = METAS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some((t, h)) = m.get(path) {
        if *t == mtime {
            return h.clone();
        }
    }
    let h = meta(path);
    m.insert(path.to_path_buf(), (mtime, h.clone()));
    h
}

struct Head {
    current: Vec<u8>,
    generation: PathBuf,
    manifest: Value,
}

fn head(dir: &Path) -> Option<Head> {
    let current = read_capped(&dir.join("CURRENT"), 4096)?;
    let c: Value = serde_json::from_slice(&current).ok()?;
    if c["store_format"] != STORE_FORMAT
        || c["store_format_minor"].as_u64().unwrap_or(1) > STORE_MINOR
    {
        return None;
    }
    let generation = c["generation"]
        .as_str()
        .filter(|g| g.bytes().all(|b| b.is_ascii_digit()))?;
    let generation = dir.join("generations").join(generation);
    let manifest = json(&generation.join("manifest.json"), MAX_DOC)?;
    Some(Head {
        current,
        generation,
        manifest,
    })
}

fn file_name(v: &Value) -> Option<&str> {
    v.as_str()
        .filter(|p| !p.is_empty() && !p.contains(['/', '\\']) && *p != "." && *p != "..")
}

fn journal_path(dir: &Path, manifest: &Value) -> Option<PathBuf> {
    let p = manifest["recovery_journal_segment"]["path"].as_str()?;
    let name = p.strip_prefix("journal/")?;
    file_name(&Value::from(name))?;
    Some(dir.join("journal").join(name))
}

fn is_subagent(dir: &Path) -> bool {
    let mut g = KINDS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(k) = m.get(dir) {
        return *k;
    }
    let Some(h) = head(dir) else {
        return true;
    };
    let Some(rs) = file_name(&h.manifest["runtime_state"]["path"]) else {
        return true;
    };
    let Some(v) = json(&h.generation.join(rs), MAX_DOC) else {
        return true;
    };
    let sub = v["identity"]["kind"] != "fork" && v["identity"]["kind"] != "root";
    m.insert(dir.to_path_buf(), sub);
    sub
}

pub fn leases(root: &Path) -> HashMap<String, u32> {
    let Ok(rd) = std::fs::read_dir(root.join("active")) else {
        return HashMap::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_suffix(".lock.json")?;
            let v = json(&e.path(), 4096)?;
            let pid = v["process_id"]
                .as_u64()
                .filter(|p| *p > 0 && *p <= u32::MAX as u64)?;
            valid_id(id).then(|| (id.to_string(), pid as u32))
        })
        .collect()
}

pub fn open(home: &Path, id: &str, is_alive: impl Fn(u32) -> bool) -> bool {
    if !valid_id(id) {
        return false;
    }
    let lock = root(home).join("active").join(format!("{id}.lock.json"));
    if !lock.is_file() {
        return false;
    }
    match json(&lock, 4096).and_then(|v| v["process_id"].as_u64()) {
        Some(p) => p > 0 && p <= u32::MAX as u64 && is_alive(p as u32),
        None => true,
    }
}

fn newest_journal(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir.join("journal"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .max()
}

pub fn sessions(home: &Path) -> Vec<Session> {
    let root = root(home);
    let Ok(rd) = std::fs::read_dir(root.join("unified")) else {
        return vec![];
    };
    let leases = leases(&root);
    let mut out = vec![];
    for e in rd.flatten() {
        let dir = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if !valid_id(&name) {
            continue;
        }
        let meta_path = dir.join("meta.json");
        let mtime = mtime_ms(&meta_path);
        if mtime == 0 {
            continue;
        }
        let Some(m) = cached_meta(&meta_path, mtime).filter(|m| m.id == name) else {
            continue;
        };
        if m.parent && is_subagent(&dir) {
            continue;
        }
        let pid = leases.get(&m.id).copied();
        let mut activity = mtime.max(mtime_ms(&dir.join("CURRENT")));
        if pid.is_some() {
            activity = activity.max(newest_journal(&dir).map_or(0, |j| mtime_ms(&j)));
        }
        out.push(Session {
            id: m.id,
            cwd: m.cwd,
            started_ms: m.started_ms,
            activity_ms: activity,
            dir,
            title: m.title,
            archived: m.archived,
            model: m.model,
            pid,
        });
    }
    out
}

pub struct Usage {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub at: i64,
    pub model: Option<String>,
}

pub fn usage(dir: &Path) -> Option<Usage> {
    let h = head(dir)?;
    let v = json(&h.generation.join("projection-state.json"), MAX_DOC)?;
    let s = &v["snapshot"]["session"];
    let t = &s["tokenUsage"];
    if !t.is_object() {
        return None;
    }
    let n = |k: &str| t[k].as_u64().unwrap_or(0);
    Some(Usage {
        input: n("inputTokens"),
        cached: n("cachedInputTokens"),
        output: n("outputTokens"),
        at: s["updatedAt"].as_i64().unwrap_or(0),
        model: meta(&dir.join("meta.json")).and_then(|m| m.model),
    })
}

pub fn find(home: &Path, session_id: &str) -> Option<PathBuf> {
    session_dir(home, session_id)
}

pub fn pending(home: &Path, known: &[Session], is_alive: impl Fn(u32) -> bool) -> Vec<Session> {
    let root = root(home);
    let mut taken: std::collections::HashSet<u32> = known.iter().filter_map(|s| s.pid).collect();
    let mut out = vec![];
    let mut found: Vec<(String, u32)> = leases(&root).into_iter().collect();
    found.sort();
    for (id, pid) in found {
        let dir = root.join("unified").join(&id);
        if dir.join("meta.json").exists() || !is_alive(pid) || !taken.insert(pid) {
            continue;
        }
        let lock = root.join("active").join(format!("{id}.lock.json"));
        let started = json(&lock, 4096)
            .and_then(|v| parse_rfc3339(v["acquired_at"].as_str()?))
            .filter(|ms| *ms > 0);
        out.push(Session {
            id,
            cwd: None,
            started_ms: started,
            activity_ms: mtime_ms(&lock),
            dir,
            title: None,
            archived: false,
            model: None,
            pid: Some(pid),
        });
    }
    out
}

pub fn watch_leases(home: &Path, poke: impl Fn()) {
    use notify::{RecursiveMode, Watcher};
    let root = root(home);
    let active = root.join("active");
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let Ok(mut w) = notify::recommended_watcher(tx) else {
        return;
    };
    let mut armed = w.watch(&active, RecursiveMode::NonRecursive).is_ok();
    if w.watch(&root, RecursiveMode::NonRecursive).is_err() && !armed {
        return;
    }
    for ev in rx {
        let Ok(ev) = ev else {
            continue;
        };
        if !armed {
            armed = w.watch(&active, RecursiveMode::NonRecursive).is_ok();
        }
        let lease = |p: &PathBuf| {
            p == &active
                || p.parent() == Some(active.as_path())
                    && p.file_name()
                        .is_some_and(|n| n.to_string_lossy().ends_with(".lock.json"))
        };
        if !ev.kind.is_access() && ev.paths.iter().any(lease) {
            poke();
        }
    }
}

pub fn stamp(dir: &Path) -> u64 {
    let st = |p: &Path| {
        std::fs::metadata(p).ok().map_or(0u64, |m| {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            m.len().wrapping_add(t)
        })
    };
    let j = newest_journal(dir).map_or(0, |p| st(&p));
    st(&dir.join("CURRENT")).wrapping_add(j)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    pub running: bool,
    pub done_ms: i64,
}

const SUBMIT_GRACE_MS: i64 = 5_000;
const TURN_TAIL: u64 = 512 << 10;
const TURN_KEY: &[u8] = b"\"latestTurn\":{";

type Turns = HashMap<PathBuf, (u64, Option<Turn>)>;

static TURNS: Mutex<Option<Turns>> = Mutex::new(None);

fn turn_of(v: &Value, fallback_ms: i64) -> Option<Turn> {
    let status = v["status"].as_str()?;
    Some(Turn {
        running: matches!(status, "in_progress" | "running"),
        done_ms: v["completedAt"].as_i64().unwrap_or(fallback_ms),
    })
}

fn last_turn(bytes: &[u8], fallback_ms: i64) -> Option<Turn> {
    let at = bytes.windows(TURN_KEY.len()).rposition(|w| w == TURN_KEY)? + TURN_KEY.len() - 1;
    let end = at + bytes[at..].iter().position(|b| *b == b'}')? + 1;
    turn_of(&serde_json::from_slice(&bytes[at..end]).ok()?, fallback_ms)
}

fn read_turn(dir: &Path) -> Option<Turn> {
    let h = head(dir)?;
    let journal = journal_path(dir, &h.manifest);
    let fallback_ms = journal
        .as_deref()
        .map_or(0, mtime_ms)
        .max(mtime_ms(&dir.join("CURRENT")));
    if let Some(p) = &journal {
        let mut f = std::fs::File::open(p).ok()?;
        let len = f.metadata().ok()?.len();
        let whole = len <= TURN_TAIL;
        let mut buf = vec![];
        f.seek(SeekFrom::Start(len.saturating_sub(TURN_TAIL)))
            .ok()?;
        f.take(TURN_TAIL).read_to_end(&mut buf).ok()?;
        if let Some(t) = last_turn(&buf, fallback_ms) {
            return Some(t);
        }
        if !whole {
            return None;
        }
    }
    let ps = file_name(&h.manifest["projection_state"]["path"])?;
    let doc = json(&h.generation.join(ps), MAX_DOC)?;
    turn_of(&doc["snapshot"]["latestTurn"], fallback_ms)
}

pub fn idle(turn: Option<Turn>, hook: (bool, i64), submit_ms: i64, now_ms: i64) -> bool {
    let (hook_busy, hook_at) = hook;
    turn.is_some_and(|t| {
        !t.running && !(hook_busy && hook_at > t.done_ms) && now_ms - submit_ms > SUBMIT_GRACE_MS
    })
}

pub fn turn(dir: &Path) -> Option<Turn> {
    let key = stamp(dir);
    let mut g = TURNS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some((k, t)) = m.get(dir) {
        if *k == key {
            return *t;
        }
    }
    if m.len() > 4096 {
        m.clear();
    }
    let t = read_turn(dir);
    m.insert(dir.to_path_buf(), (key, t));
    t
}

fn hash(v: &Value) -> u64 {
    let mut h = DefaultHasher::new();
    v.to_string().hash(&mut h);
    h.finish()
}

fn id_of(v: &Value) -> String {
    v["id"].as_str().unwrap_or("").to_string()
}

pub enum Change {
    Same,
    Append(Vec<Value>),
    Reset(Vec<Value>),
}

pub struct Store {
    current: Vec<u8>,
    journal: PathBuf,
    offset: u64,
    index: Vec<(String, u64)>,
    model: Option<String>,
    parsed: usize,
}

enum Op {
    Upsert(Value),
    Rebuild,
}

fn ops(record: &Value) -> Vec<Op> {
    let p = &record["payload"];
    match record["type"].as_str() {
        Some("projection_delta" | "session_relocated") => p["delta"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|op| match op["op"].as_str() {
                Some("append_entry" | "replace_entry") => Some(Op::Upsert(op["entry"].clone())),
                Some("remove_entry" | "set_history_entries") => Some(Op::Rebuild),
                _ => None,
            })
            .collect(),
        Some("projection_advanced") => vec![Op::Rebuild],
        _ => vec![],
    }
}

fn apply(entries: &mut Vec<Value>, record: &Value) {
    let p = &record["payload"];
    if record["type"] == "projection_advanced" {
        *entries = p["snapshot"]["history"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        return;
    }
    if !matches!(
        record["type"].as_str(),
        Some("projection_delta" | "session_relocated")
    ) {
        return;
    }
    for op in p["delta"].as_array().into_iter().flatten() {
        match op["op"].as_str() {
            Some("append_entry") => entries.push(op["entry"].clone()),
            Some("replace_entry") => {
                let id = op["id"].as_str().unwrap_or("");
                if let Some(e) = entries.iter_mut().rev().find(|e| e["id"] == id) {
                    *e = op["entry"].clone();
                }
            }
            Some("remove_entry") => {
                let id = op["id"].as_str().unwrap_or("");
                entries.retain(|e| e["id"] != id);
            }
            Some("set_history_entries") => {
                *entries = op["entries"].as_array().cloned().unwrap_or_default()
            }
            _ => {}
        }
    }
}

fn read_records(path: &Path, offset: &mut u64, mut f: impl FnMut(Option<Value>)) -> Option<()> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len < *offset {
        return None;
    }
    if len == *offset {
        return Some(());
    }
    file.seek(SeekFrom::Start(*offset)).ok()?;
    let mut buf = vec![];
    file.take((len - *offset).min(MAX_DOC * 2))
        .read_to_end(&mut buf)
        .ok()?;
    let complete = buf.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    for line in buf[..complete]
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
    {
        f(serde_json::from_slice(line).ok());
    }
    *offset += complete as u64;
    Some(())
}

fn model_of(dir: &Path) -> Option<String> {
    json(&dir.join("meta.json"), MAX_META).and_then(|v| str_of(&v["config"]["active_model"]))
}

fn load_once(dir: &Path) -> Option<(Store, Vec<Value>)> {
    let h = head(dir)?;
    let ps = &h.manifest["projection_state"];
    let mut doc = json(&h.generation.join(file_name(&ps["path"])?), MAX_DOC)?;
    let mut parsed = 1;
    let mut entries = match ps["chunks"].as_array() {
        Some(chunks) => {
            parsed += chunks.len();
            let mut all = vec![];
            for c in chunks {
                let d = c
                    .as_str()
                    .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
                match json(&dir.join("chunks").join(format!("{d}.json")), MAX_DOC)? {
                    Value::Array(a) => all.extend(a),
                    _ => return None,
                }
            }
            all
        }
        None => doc["snapshot"]["history"]["entries"]
            .take()
            .as_array()
            .cloned()
            .unwrap_or_default(),
    };
    let journal = journal_path(dir, &h.manifest)?;
    let mut offset = 0;
    let mut broken = false;
    if journal.is_file() {
        read_records(&journal, &mut offset, |r| {
            parsed += 1;
            match r {
                Some(r) => apply(&mut entries, &r),
                None => broken = true,
            }
        })?;
    }
    if broken || read_capped(&dir.join("CURRENT"), 4096)? != h.current {
        return None;
    }
    let store = Store {
        current: h.current,
        journal,
        offset,
        index: entries.iter().map(|e| (id_of(e), hash(e))).collect(),
        model: model_of(dir),
        parsed,
    };
    Some((store, entries))
}

pub fn load(dir: &Path) -> Option<(Store, Vec<Value>)> {
    load_once(dir).or_else(|| load_once(dir))
}

pub fn entries(dir: &Path) -> Option<Vec<Value>> {
    load(dir).map(|(_, e)| e)
}

impl Store {
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn parsed(&self) -> usize {
        self.parsed
    }

    pub fn refresh(&mut self, dir: &Path) -> Option<Change> {
        let current = read_capped(&dir.join("CURRENT"), 4096)?;
        if current != self.current {
            return self.reload(dir);
        }
        let mut touched = vec![];
        let mut rebuild = false;
        let mut offset = self.offset;
        let mut parsed = 0;
        read_records(&self.journal, &mut offset, |r| match r {
            None => rebuild = true,
            Some(r) => {
                parsed += 1;
                for op in ops(&r) {
                    match op {
                        Op::Upsert(e) => touched.push(e),
                        Op::Rebuild => rebuild = true,
                    }
                }
            }
        })?;
        if rebuild {
            return self.reload(dir);
        }
        self.offset = offset;
        self.parsed = parsed;
        if touched.is_empty() {
            return Some(Change::Same);
        }
        for e in &touched {
            let (id, h) = (id_of(e), hash(e));
            match self.index.iter_mut().rev().find(|(i, _)| *i == id) {
                Some(slot) => slot.1 = h,
                None => self.index.push((id, h)),
            }
        }
        Some(Change::Append(touched))
    }

    fn reload(&mut self, dir: &Path) -> Option<Change> {
        let (fresh, all) = load(dir)?;
        let n = self.index.len();
        let prefix = fresh.index.len() >= n && fresh.index[..n] == self.index[..];
        *self = fresh;
        Some(if prefix {
            Change::Append(all.into_iter().skip(n).collect())
        } else {
            Change::Reset(all)
        })
    }
}

#[cfg(test)]
pub mod fixture {
    use serde_json::{json, Value};
    use std::path::Path;

    pub fn message(id: &str, role: &str, text: &str, at: i64) -> Value {
        json!({"type": "message", "id": id, "sessionId": "s", "turnId": "t1", "createdAt": at, "updatedAt": at,
            "generationStatus": "completed", "relatedEntryId": null, "inputEntryId": null, "role": role,
            "content": [{"type": "text", "text": text}], "source": "user"})
    }

    pub fn effect(id: &str, tool: &str, input: Value, status: &str, out: &str, at: i64) -> Value {
        json!({"type": "effect", "id": id, "sessionId": "s", "turnId": "t1", "createdAt": at, "updatedAt": at,
            "generationStatus": if status == "running" { "in_progress" } else { "completed" },
            "title": tool, "detail": {"kind": "tool", "toolName": tool, "input": input,
                "display": {"summary": tool, "verb": "Running", "message": tool}},
            "state": {"status": status, "outputText": out}})
    }

    pub fn delta(seq: u64, ops: Value) -> String {
        format!(
            "{}\n",
            json!({"recovery_journal_record_version": 1, "sequence": seq, "previous_record_sha256": null,
                "record_sha256": "0".repeat(64), "type": "projection_delta",
                "payload": {"watermark": seq, "delta": ops}})
        )
    }

    pub fn session(
        home: &Path,
        id: &str,
        meta: Value,
        generation: u64,
        chunks: &[Vec<Value>],
        journal: &str,
    ) -> std::path::PathBuf {
        let dir = super::root(home).join("unified").join(id);
        let g = format!("{generation:016}");
        let gdir = dir.join("generations").join(&g);
        std::fs::create_dir_all(&gdir).unwrap();
        std::fs::create_dir_all(dir.join("chunks")).unwrap();
        std::fs::create_dir_all(dir.join("journal")).unwrap();
        let mut digests = vec![];
        for (i, c) in chunks.iter().enumerate() {
            let d = format!("{:0>64x}", generation * 1000 + i as u64);
            std::fs::write(
                dir.join("chunks").join(format!("{d}.json")),
                Value::Array(c.clone()).to_string(),
            )
            .unwrap();
            digests.push(d);
        }
        let seg = format!("{:016}.jsonl", generation * 100 + 1);
        std::fs::write(
            gdir.join("projection-state.json"),
            json!({"projection_state_version": 1, "session_id": id,
            "snapshot_sequence": generation * 100, "watermark": 0,
            "snapshot": {"history": {"entries": []}}})
            .to_string(),
        )
        .unwrap();
        std::fs::write(gdir.join("runtime-state.json"), json!({"runtime_state_version": 3, "session_id": id,
            "identity": {"kind": if meta["parent_session_id"].is_null() { "root" } else { "subagent" }}}).to_string()).unwrap();
        std::fs::write(gdir.join("manifest.json"), json!({"manifest_version": 1, "session_id": id, "generation": g,
            "snapshot_sequence": generation * 100, "execution_state": "quiescent",
            "runtime_state": {"path": "runtime-state.json", "sha256": "0".repeat(64)},
            "projection_state": {"path": "projection-state.json", "sha256": "0".repeat(64), "chunks": digests},
            "recovery_journal_segment": {"path": format!("journal/{seg}"), "first_sequence": generation * 100 + 1}}).to_string()).unwrap();
        std::fs::write(dir.join("journal").join(&seg), journal).unwrap();
        std::fs::write(dir.join("CURRENT"), format!("{}\n", json!({"store_format": super::STORE_FORMAT, "store_format_minor": 10,
            "session_id": id, "generation": g, "snapshot_sequence": generation * 100, "manifest_sha256": "0".repeat(64)}))).unwrap();
        std::fs::write(
            dir.join("meta.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
        dir
    }

    pub fn meta_json(id: &str, cwd: &str, parent: Option<&str>) -> Value {
        json!({"session_id": id, "parent_session_id": parent, "start_time": "2026-11-11T10:00:00+00:00",
            "end_time": "2026-11-11T10:05:00+00:00", "git_commit": null, "git_branch": null,
            "environment": {"working_directory": cwd}, "origin_directory": cwd, "username": "u",
            "child_sessions": [], "loops": [], "title": "Fix the login bug", "title_source": "auto",
            "archived_at": null, "config": {"active_model": "mistral-medium-3.5", "model_name": "mistral-medium-latest"}})
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::*;
    use super::*;
    use serde_json::json;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ov-vibe-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn ids(v: &[Value]) -> Vec<String> {
        v.iter().map(id_of).collect()
    }

    fn lease(h: &Path, id: &str, pid: u32) {
        let a = root(h).join("active");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(
            a.join(format!("{id}.lock.json")),
            json!({"acquired_at": "2026-11-11T10:00:00.000Z", "lease_version": 1,
                "process_id": pid, "session_id": id})
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn a_lease_without_a_store_yet_is_a_pending_session() {
        let h = tmp("pending");
        lease(&h, "p1", 4242);
        let p = pending(&h, &sessions(&h), |pid| pid == 4242);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id, "p1");
        assert_eq!(p[0].pid, Some(4242));
        assert!(p[0].cwd.is_none());
        assert!(p[0].started_ms.is_some_and(|ms| ms > 0));
    }

    #[test]
    fn pending_skips_stored_dead_and_shared_process_leases() {
        let h = tmp("pending-skip");
        session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        lease(&h, "s1", 10);
        lease(&h, "dead", 11);
        lease(&h, "second", 10);
        lease(&h, "ok", 12);
        let known = sessions(&h);
        let p = pending(&h, &known, |pid| pid != 11);
        assert_eq!(p.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["ok"]);
        assert!(pending(&h, &known, |_| false).is_empty());
    }

    #[test]
    fn a_lease_appearing_or_going_wakes_the_scanner() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let h = tmp("watch");
        std::fs::create_dir_all(root(&h)).unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let n = hits.clone();
        let home = h.clone();
        std::thread::spawn(move || {
            watch_leases(&home, || {
                n.fetch_add(1, Ordering::SeqCst);
            })
        });
        let wait = |want: usize| {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while hits.load(Ordering::SeqCst) < want {
                assert!(std::time::Instant::now() < end, "no wake-up");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        std::thread::sleep(std::time::Duration::from_millis(200));
        let a = root(&h).join("active");
        std::fs::create_dir_all(&a).unwrap();
        wait(1);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let before = hits.load(Ordering::SeqCst);
        lease(&h, "w1", 1);
        wait(before + 1);
        let before = hits.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let before = before.max(hits.load(Ordering::SeqCst));
        std::fs::write(a.join(".registry"), "x").unwrap();
        std::fs::create_dir_all(root(&h).join("unified/x")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(hits.load(Ordering::SeqCst), before);
        std::fs::remove_file(a.join("w1.lock.json")).unwrap();
        wait(before + 1);
    }

    #[test]
    fn rejects_unsafe_session_ids() {
        assert!(valid_id("2aea6685-2327-ee7a-3ac1-0a194fdd8de9"));
        for bad in ["", "../x", "a/b", "-a", ".."] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn lists_root_and_fork_sessions_but_not_subagents() {
        let h = tmp("list");
        session(
            &h,
            "root1",
            meta_json("root1", "/w/repo", None),
            1,
            &[vec![message("m1", "user", "hi", 1)]],
            "",
        );
        session(
            &h,
            "sub1",
            meta_json("sub1", "/w/repo", Some("root1")),
            1,
            &[],
            "",
        );
        let fork = session(
            &h,
            "fork1",
            meta_json("fork1", "/w/repo", Some("root1")),
            1,
            &[],
            "",
        );
        let rs = fork
            .join("generations")
            .join(format!("{:016}", 1))
            .join("runtime-state.json");
        std::fs::write(rs, json!({"identity": {"kind": "fork"}}).to_string()).unwrap();
        std::fs::create_dir_all(root(&h).join("active")).unwrap();
        std::fs::write(
            root(&h).join("active/root1.lock.json"),
            json!({"lease_version": 1, "session_id": "root1", "process_id": 4242}).to_string(),
        )
        .unwrap();
        let mut s = sessions(&h);
        s.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(
            s.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            ["fork1", "root1"]
        );
        let r = &s[1];
        assert_eq!(r.cwd.as_deref(), Some("/w/repo"));
        assert_eq!(r.pid, Some(4242));
        assert_eq!(r.model.as_deref(), Some("mistral-medium-3.5"));
        assert_eq!(r.title.as_deref(), Some("Fix the login bug"));
        assert!(r.started_ms.is_some());
        assert_eq!(s[0].pid, None);
        assert_eq!(find(&h, "root1"), Some(root(&h).join("unified/root1")));
        assert_eq!(find(&h, "../root1"), None);
    }

    #[test]
    fn loads_chunks_then_replays_the_journal() {
        let h = tmp("load");
        let j = delta(
            1,
            json!([{"op": "append_entry", "entry": effect("e1", "file_system.bash", json!({"command": "ls"}), "running", "", 3)}]),
        ) + &delta(
            2,
            json!([{"op": "replace_entry", "id": "e1", "entry": effect("e1", "file_system.bash", json!({"command": "ls"}), "completed", "a.txt", 3)}]),
        ) + "{\"partial";
        let dir = session(
            &h,
            "s1",
            meta_json("s1", "/w", None),
            1,
            &[
                vec![message("m1", "user", "hi", 1)],
                vec![message("m2", "assistant", "ok", 2)],
            ],
            &j,
        );
        let (store, e) = load(&dir).unwrap();
        assert_eq!(ids(&e), ["m1", "m2", "e1"]);
        assert_eq!(e[2]["state"]["status"], "completed");
        assert_eq!(store.model(), Some("mistral-medium-3.5"));
    }

    #[test]
    fn refresh_returns_only_new_journal_entries() {
        let h = tmp("refresh");
        let dir = session(
            &h,
            "s1",
            meta_json("s1", "/w", None),
            1,
            &[vec![message("m1", "user", "hi", 1)]],
            "",
        );
        let (mut store, _) = load(&dir).unwrap();
        assert!(matches!(store.refresh(&dir), Some(Change::Same)));
        let seg = dir.join("journal").join(format!("{:016}.jsonl", 101));
        let mut f = std::fs::OpenOptions::new().append(true).open(&seg).unwrap();
        std::io::Write::write_all(
            &mut f,
            delta(
                1,
                json!([{"op": "append_entry", "entry": message("m2", "assistant", "done", 2)}]),
            )
            .as_bytes(),
        )
        .unwrap();
        match store.refresh(&dir) {
            Some(Change::Append(v)) => assert_eq!(ids(&v), ["m2"]),
            _ => panic!("expected an append"),
        }
        std::io::Write::write_all(
            &mut f,
            delta(2, json!([{"op": "remove_entry", "id": "m1"}])).as_bytes(),
        )
        .unwrap();
        match store.refresh(&dir) {
            Some(Change::Reset(v)) => assert_eq!(ids(&v), ["m2"]),
            _ => panic!("expected a reset"),
        }
    }

    #[test]
    fn a_new_generation_with_the_same_history_appends_only_the_tail() {
        let h = tmp("gen");
        let first = vec![message("m1", "user", "hi", 1)];
        let dir = session(
            &h,
            "s1",
            meta_json("s1", "/w", None),
            1,
            std::slice::from_ref(&first),
            "",
        );
        let (mut store, _) = load(&dir).unwrap();
        session(
            &h,
            "s1",
            meta_json("s1", "/w", None),
            2,
            &[first, vec![message("m2", "assistant", "ok", 2)]],
            "",
        );
        match store.refresh(&dir) {
            Some(Change::Append(v)) => assert_eq!(ids(&v), ["m2"]),
            _ => panic!("expected an append"),
        }
        session(
            &h,
            "s1",
            meta_json("s1", "/w", None),
            3,
            &[vec![message("m9", "user", "new", 9)]],
            "",
        );
        assert!(matches!(store.refresh(&dir), Some(Change::Reset(_))));
    }

    #[test]
    fn refuses_a_newer_store_format() {
        let h = tmp("minor");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let c = std::fs::read_to_string(dir.join("CURRENT"))
            .unwrap()
            .replace("\"store_format_minor\":10", "\"store_format_minor\":11");
        std::fs::write(dir.join("CURRENT"), c).unwrap();
        assert!(load(&dir).is_none());
    }

    #[test]
    fn stamp_moves_when_the_journal_grows() {
        let h = tmp("stamp");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let a = stamp(&dir);
        let seg = dir.join("journal").join(format!("{:016}.jsonl", 101));
        std::fs::write(&seg, delta(1, json!([]))).unwrap();
        assert_ne!(stamp(&dir), a);
    }

    #[test]
    fn open_follows_the_lease_lock_file() {
        let h = tmp("open");
        session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let active = root(&h).join("active");
        std::fs::create_dir_all(&active).unwrap();
        let lock = active.join("s1.lock.json");
        assert!(!open(&h, "s1", |_| true));
        assert!(!open(&h, "../s1", |_| true));
        std::fs::write(
            &lock,
            json!({"lease_version": 1, "process_id": 42}).to_string(),
        )
        .unwrap();
        assert!(open(&h, "s1", |p| p == 42));
        assert!(!open(&h, "s1", |_| false));
        std::fs::write(&lock, "{\"process_id\":").unwrap();
        assert!(open(&h, "s1", |_| false));
        std::fs::write(&lock, json!({"lease_version": 1}).to_string()).unwrap();
        assert!(open(&h, "s1", |_| false));
    }

    fn envelope(status: &str, completed: Option<i64>) -> String {
        let done = completed.map_or(String::new(), |c| format!("\"completedAt\":{c},"));
        format!(
            "{{\"payload\":{{\"delta\":[{{\"op\":\"set_envelope\",\"state\":{{\"activeCallbacks\":[],\"history\":{{\"entries\":[]}},\"latestTurn\":{{{done}\"id\":\"turn-1\",\"queueItemId\":\"q\",\"startedAt\":1795077719099,\"status\":\"{status}\"}},\"session\":{{\"id\":\"s1\"}}}}}}]}},\"sequence\":7,\"type\":\"projection_delta\"}}\n"
        )
    }

    fn journal_of(dir: &Path) -> PathBuf {
        std::fs::read_dir(dir.join("journal"))
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path()
    }

    #[test]
    fn turn_follows_the_last_envelope_of_the_journal() {
        let h = tmp("turn");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        assert_eq!(turn(&dir).map(|t| t.running), None);
        let j = journal_of(&dir);
        std::fs::write(&j, envelope("in_progress", None).repeat(3)).unwrap();
        assert_eq!(turn(&dir).map(|t| t.running), Some(true));
        let all = format!(
            "{}{}",
            envelope("in_progress", None),
            envelope("completed", Some(1795084789346))
        );
        std::fs::write(&j, all).unwrap();
        assert_eq!(
            turn(&dir),
            Some(Turn {
                running: false,
                done_ms: 1795084789346
            })
        );
    }

    #[test]
    fn turn_reads_the_snapshot_when_the_journal_was_rotated_empty() {
        let h = tmp("turn-snap");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let ps = dir.join("generations/0000000000000001/projection-state.json");
        std::fs::write(
            &ps,
            json!({"snapshot": {"history": {"entries": []},
                "latestTurn": {"completedAt": 42, "id": "t", "startedAt": 1, "status": "completed", "stopReason": null}}})
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            turn(&dir),
            Some(Turn {
                running: false,
                done_ms: 42
            })
        );
    }

    #[test]
    fn turn_without_a_completion_time_uses_the_store_mtime_and_a_cancelled_turn_is_over() {
        let h = tmp("turn-cancel");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        std::fs::write(journal_of(&dir), envelope("cancelled", None)).unwrap();
        let t = turn(&dir).unwrap();
        assert!(!t.running && t.done_ms > 0);
    }

    #[test]
    fn turn_is_unknown_when_a_long_journal_hides_its_envelope() {
        let h = tmp("turn-long");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let ps = dir.join("generations/0000000000000001/projection-state.json");
        std::fs::write(
            &ps,
            json!({"snapshot": {"latestTurn": {"completedAt": 42, "status": "completed"}}})
                .to_string(),
        )
        .unwrap();
        let filler = format!("{{\"pad\":\"{}\"}}\n", "x".repeat(TURN_TAIL as usize));
        std::fs::write(journal_of(&dir), filler).unwrap();
        assert_eq!(turn(&dir), None);
    }

    #[test]
    fn turn_is_cached_until_the_store_changes() {
        let h = tmp("turn-cache");
        let dir = session(&h, "s1", meta_json("s1", "/w", None), 1, &[], "");
        let j = journal_of(&dir);
        std::fs::write(&j, envelope("in_progress", None)).unwrap();
        assert_eq!(turn(&dir).map(|t| t.running), Some(true));
        std::fs::write(&j, envelope("completed", Some(5))).unwrap();
        assert_eq!(turn(&dir).map(|t| t.running), Some(false));
    }

    #[test]
    fn idle_needs_a_finished_turn_older_than_the_last_submit_grace_and_any_busy_hook() {
        let over = Some(Turn {
            running: false,
            done_ms: 1_000,
        });
        let running = Some(Turn {
            running: true,
            done_ms: 0,
        });
        assert!(idle(over, (false, 1_100), 0, 100_000));
        assert!(idle(over, (true, 900), 0, 100_000));
        assert!(!idle(over, (true, 2_000), 0, 100_000));
        assert!(!idle(running, (false, 1_100), 0, 100_000));
        assert!(!idle(None, (false, 1_100), 0, 100_000));
        assert!(!idle(over, (false, 1_100), 98_000, 100_000));
        assert!(idle(over, (false, 1_100), 90_000, 100_000));
    }
}
