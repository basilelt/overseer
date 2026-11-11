use crate::usage::{Meter, Tokens};
use crate::util::{format_rfc3339, parse_rfc3339};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_ENTRIES: usize = 500;
const MAX_DIRS: usize = 256;
const MAX_SWITCHES: usize = 50;

#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub at: i64,
    pub from: String,
    pub to: String,
    pub tokens: Tokens,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spend {
    pub models: Vec<(String, Tokens)>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub switches: Vec<Switch>,
}

struct Entry {
    id: String,
    kind: &'static str,
    label: String,
    path: Option<String>,
    state: &'static str,
    at: i64,
    started: i64,
}

#[derive(Default)]
struct State {
    entries: Vec<Entry>,
    base: usize,
    calls: HashMap<String, Vec<usize>>,
    ai_title: Option<String>,
    summary: Option<String>,
    first_prompt: Option<String>,
    last_prompt: Option<String>,
    model: Option<String>,
    todos: Vec<Value>,
    edited: Vec<String>,
    edited_set: HashSet<String>,
    dirs: Vec<String>,
    last_text: Option<String>,
    last_ts: i64,
    done: HashSet<String>,
    edit_count: HashMap<String, u32>,
    test_fails: HashMap<String, u32>,
    since_test: HashSet<String>,
    test_calls: HashSet<String>,
    meter: Meter,
    spend: Spend,
    turn_model: Option<String>,
}

type Cache = Mutex<HashMap<PathBuf, (u64, State)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

const MISS_RETRY: Duration = Duration::from_secs(2);

type Located = HashMap<(PathBuf, String), Result<PathBuf, Instant>>;

fn located() -> &'static Mutex<Located> {
    static LOCATED: OnceLock<Mutex<Located>> = OnceLock::new();
    LOCATED.get_or_init(Default::default)
}

pub fn find_transcript(
    claude_dir: &Path,
    codex_dir: &Path,
    gemini_dir: &Path,
    agent: &str,
    session_id: &str,
) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.contains('/') || session_id.contains("..") {
        return None;
    }
    let base = match agent {
        "claude" => claude_dir,
        "codex" => codex_dir,
        "gemini" => gemini_dir,
        _ => return None,
    };
    let key = (base.join(agent), session_id.to_string());
    let known = located()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    match known {
        Some(Ok(p)) if p.is_file() => return Some(p),
        Some(Err(at)) if at.elapsed() < MISS_RETRY => return None,
        _ => {}
    }
    let found = locate(base, agent, session_id);
    let mut map = located().lock().unwrap_or_else(|e| e.into_inner());
    if map.len() >= 4096 {
        map.clear();
    }
    map.insert(key, found.clone().ok_or_else(Instant::now));
    found
}

fn locate(base: &Path, agent: &str, session_id: &str) -> Option<PathBuf> {
    match agent {
        "claude" => std::fs::read_dir(base.join("projects"))
            .ok()?
            .flatten()
            .map(|e| e.path().join(format!("{session_id}.jsonl")))
            .find(|p| p.is_file()),
        "codex" => find_rollout(&base.join("sessions"), &format!("-{session_id}.jsonl"), 4),
        "gemini" => crate::gemini::find(base, session_id),
        _ => None,
    }
}

fn find_rollout(dir: &Path, suffix: &str, depth: u32) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let name = e.file_name();
        let name = name.to_string_lossy();
        if ft.is_dir() && depth > 1 {
            if let Some(f) = find_rollout(&e.path(), suffix, depth - 1) {
                return Some(f);
            }
        } else if ft.is_file() && name.starts_with("rollout-") && name.ends_with(suffix) {
            return Some(e.path());
        }
    }
    None
}

pub fn activity(agent: &str, path: &Path, since_ms: Option<i64>) -> Option<Value> {
    with_state(agent, path, |st| st.to_json(since_ms))
}

pub fn is_test_run(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        matches!(
            w,
            "test"
                | "tests"
                | "pytest"
                | "vitest"
                | "jest"
                | "nextest"
                | "rspec"
                | "phpunit"
                | "playwright"
        )
    })
}

pub fn edit_stats(agent: &str, path: &Path) -> HashMap<String, (u32, u32)> {
    let mut out: HashMap<String, (u32, u32)> = HashMap::new();
    for f in transcripts(path) {
        let Some(stats) = with_state(agent, &f, |st| {
            st.edit_count
                .iter()
                .map(|(p, n)| (p.clone(), (*n, st.test_fails.get(p).copied().unwrap_or(0))))
                .collect::<Vec<_>>()
        }) else {
            continue;
        };
        for (p, (e, f)) in stats {
            let slot = out.entry(p).or_default();
            slot.0 += e;
            slot.1 += f;
        }
    }
    out
}

fn transcripts(path: &Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![path.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(path.with_extension("").join("subagents")) {
        files.extend(
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "jsonl")),
        );
    }
    files
}

fn sub_transcripts(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() && depth > 1 => sub_transcripts(&p, depth - 1, out),
            Ok(t) if t.is_file() && p.extension().is_some_and(|x| x == "jsonl") => out.push(p),
            _ => {}
        }
    }
}

pub fn touched(agent: &str, path: &Path) -> (Vec<String>, Vec<String>) {
    let files = transcripts(path);
    let (mut edited, mut dirs) = (vec![], vec![]);
    for f in files {
        if let Some((e, d)) = with_state(agent, &f, |st| (st.edited.clone(), st.dirs.clone())) {
            edited.extend(e);
            dirs.extend(d);
        }
    }
    (edited, dirs)
}

fn with_state<R>(agent: &str, path: &Path, f: impl FnOnce(&State) -> R) -> Option<R> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    let slot = cache
        .entry(path.to_path_buf())
        .or_insert_with(|| (0, State::default()));
    if len < slot.0 {
        *slot = (0, State::default());
    }
    if path.extension().is_some_and(|e| e == "json") {
        if len != slot.0 {
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).ok()?;
            let v: Value = serde_json::from_slice(&buf).ok()?;
            let mut st = State::default();
            if let Some(t) = v["summary"].as_str().filter(|t| !t.is_empty()) {
                st.summary = Some(t.to_string());
            }
            for m in v["messages"].as_array().into_iter().flatten() {
                st.ingest(agent, m);
                let turns = st.meter.gemini(m);
                st.count(turns, false);
            }
            *slot = (len, st);
        }
    } else if len > slot.0 {
        let st = &mut slot.1;
        slot.0 = read_lines(&mut file, slot.0, len, |line| {
            #[cfg(test)]
            INGESTED.with(|n| n.set(n.get() + 1));
            if let Ok(v) = serde_json::from_slice::<Value>(line) {
                st.ingest(agent, &v);
                let turns = st.meter.line(agent, line);
                st.count(turns, v["isSidechain"] == true);
            }
        })?;
    }
    Some(f(&slot.1))
}

#[cfg(test)]
thread_local! {
    pub(crate) static INGESTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub fn spend(agent: &str, path: &Path) -> Option<Spend> {
    let mut out: Option<Spend> = None;
    let mut files = vec![path.to_path_buf()];
    sub_transcripts(&path.with_extension("").join("subagents"), 3, &mut files);
    for (i, f) in files.iter().enumerate() {
        let Some(s) = with_state(agent, f, |st| st.spend.clone()) else {
            continue;
        };
        let o = out.get_or_insert_with(Spend::default);
        if i == 0 {
            o.model = s.model;
            o.effort = s.effort;
            o.switches = s.switches;
        }
        for (m, t) in s.models {
            match o.models.iter_mut().find(|(k, _)| *k == m) {
                Some((_, x)) => x.add(&t),
                None => o.models.push((m, t)),
            }
        }
    }
    out.filter(|s| !s.models.is_empty())
}

pub fn effort_of(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(i, a)| {
        let v = match a.as_str() {
            "--effort" | "--reasoning-effort" => args.get(i + 1)?.as_str(),
            _ => a
                .strip_prefix("--effort=")
                .or_else(|| a.strip_prefix("model_reasoning_effort="))?,
        };
        let v = v.trim_matches(|c| c == '"' || c == '\'');
        (!v.is_empty() && !v.starts_with('-')).then(|| v.to_string())
    })
}

pub fn spend_json(s: &Spend, launch_args: &[String]) -> Value {
    let mut v = s.to_json();
    if v["effort"].is_null() {
        v["effort"] = json!(effort_of(launch_args));
    }
    v
}

fn usd(c: f64) -> f64 {
    (c * 10_000.0).round() / 10_000.0
}

fn tokens_json(t: &Tokens) -> Value {
    json!({
        "input": t.input,
        "cacheWrite": t.cache_write,
        "cacheRead": t.cache_read,
        "output": t.output,
    })
}

impl Spend {
    pub fn total(&self) -> (Tokens, Option<f64>, bool) {
        let mut sum = Tokens::default();
        let (mut cost, mut priced, mut partial) = (0.0, false, false);
        for (m, t) in &self.models {
            sum.add(t);
            match crate::usage::cost(m, t) {
                Some(c) => {
                    cost += c;
                    priced = true;
                }
                None => partial = true,
            }
        }
        (sum, priced.then_some(usd(cost)), partial)
    }

    pub fn to_json(&self) -> Value {
        let (sum, cost, partial) = self.total();
        let mut models: Vec<(f64, Value)> = self
            .models
            .iter()
            .map(|(m, t)| {
                let c = crate::usage::cost(m, t).map(usd);
                let mut j = tokens_json(t);
                j["model"] = json!(m);
                j["cost"] = json!(c);
                (c.unwrap_or(-1.0), j)
            })
            .collect();
        models.sort_by(|a, b| b.0.total_cmp(&a.0));
        let switches: Vec<Value> = self
            .switches
            .iter()
            .map(|s| {
                json!({
                    "at": format_rfc3339(s.at),
                    "from": s.from,
                    "to": s.to,
                    "cacheWrite": s.tokens.cache_write,
                    "cost": crate::usage::cost(&s.to, &s.tokens).map(usd),
                })
            })
            .collect();
        let mut v = tokens_json(&sum);
        v["model"] = json!(self.model);
        v["effort"] = json!(self.effort);
        v["cost"] = json!(cost);
        v["partial"] = json!(partial);
        v["models"] = json!(models.into_iter().map(|(_, j)| j).collect::<Vec<_>>());
        v["switches"] = json!(switches);
        v["pricesAsOf"] = json!(crate::usage::PRICES_AS_OF);
        v
    }
}

pub fn read_lines(
    file: &mut std::fs::File,
    from: u64,
    to: u64,
    mut f: impl FnMut(&[u8]),
) -> Option<u64> {
    use std::io::BufRead;
    if to <= from {
        return Some(from);
    }
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut r = std::io::BufReader::with_capacity(64 * 1024, file.take(to - from));
    let mut line = Vec::new();
    let mut at = from;
    loop {
        line.clear();
        let n = r.read_until(b'\n', &mut line).ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            return Some(at);
        }
        at += n as u64;
        f(&line[..n - 1]);
    }
}

fn keep(slot: &mut Option<String>, v: &Value) {
    if let Some(t) = v.as_str().map(str::trim).filter(|t| !t.is_empty()) {
        *slot = Some(t.to_string());
    }
}

fn trunc(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

fn base(p: &str) -> &str {
    Path::new(p)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(p)
}

fn todos(list: &Value, text_key: &str) -> Vec<Value> {
    list.as_array()
        .into_iter()
        .flatten()
        .map(|t| json!({"text": t[text_key], "state": t["status"]}))
        .collect()
}

pub(crate) fn command_label(args: &Value) -> String {
    match &args["command"] {
        Value::Array(a) => {
            let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [_, "-lc" | "-c", cmd] => cmd.to_string(),
                _ => parts.join(" "),
            }
        }
        Value::String(s) => s.clone(),
        _ => args["cmd"].as_str().unwrap_or("shell").to_string(),
    }
}

pub(crate) fn codex_failed(out: &Value) -> bool {
    let v = match out {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| out.clone()),
        o => o.clone(),
    };
    if let Some(c) = v["metadata"]["exit_code"].as_i64() {
        return c != 0;
    }
    if v.get("success") == Some(&Value::Bool(false)) {
        return true;
    }
    let text = v.as_str().or(v["output"].as_str()).unwrap_or("");
    ["Exit code: ", "exited with code "].iter().any(|p| {
        text.find(p)
            .and_then(|i| {
                text[i + p.len()..]
                    .split(|c: char| !c.is_ascii_digit() && c != '-')
                    .next()?
                    .parse::<i64>()
                    .ok()
            })
            .is_some_and(|c| c != 0)
    })
}

impl State {
    fn count(&mut self, turns: Vec<(String, Tokens, bool)>, side: bool) {
        for (model, t, new) in turns {
            if new && !side {
                if let Some(prev) = self.turn_model.as_ref().filter(|p| **p != model) {
                    if self.spend.switches.len() >= MAX_SWITCHES {
                        self.spend.switches.remove(0);
                    }
                    self.spend.switches.push(Switch {
                        at: self.last_ts,
                        from: prev.clone(),
                        to: model.clone(),
                        tokens: Tokens {
                            input: t.input,
                            cache_write: t.cache_write,
                            cache_write_1h: t.cache_write_1h,
                            ..Tokens::default()
                        },
                    });
                }
                self.turn_model = Some(model.clone());
                self.spend.model = Some(model.clone());
            }
            match self.spend.models.iter_mut().find(|(k, _)| *k == model) {
                Some((_, x)) => x.add(&t),
                None => self.spend.models.push((model, t)),
            }
        }
    }

    fn ingest(&mut self, agent: &str, v: &Value) {
        let ts = v["timestamp"]
            .as_str()
            .and_then(parse_rfc3339)
            .unwrap_or(self.last_ts);
        self.last_ts = self.last_ts.max(ts);
        match agent {
            "codex" => self.codex(v, ts),
            "gemini" => self.gemini(v, ts),
            _ => self.claude(v, ts),
        }
    }

    fn gemini(&mut self, v: &Value, ts: i64) {
        match v["type"].as_str() {
            Some("user") => match &v["content"] {
                Value::String(s) => self.prompt(s),
                Value::Array(a) => {
                    let t: Vec<&str> = a.iter().filter_map(|p| p["text"].as_str()).collect();
                    self.prompt(&t.join("\n"))
                }
                _ => {}
            },
            Some("gemini") => {
                if let Some(t) = v["content"].as_str() {
                    self.text(t);
                }
                for c in v["toolCalls"].as_array().into_iter().flatten() {
                    self.gemini_tool(c, ts);
                }
            }
            _ => {}
        }
    }

    fn gemini_tool(&mut self, c: &Value, ts: i64) {
        let Some(id) = c["id"].as_str().filter(|i| !i.is_empty()) else {
            return;
        };
        if self.done.contains(id) {
            return;
        }
        let at = c["timestamp"]
            .as_str()
            .and_then(parse_rfc3339)
            .unwrap_or(ts);
        let name = c["name"].as_str().unwrap_or("");
        let args = &c["args"];
        let s = |k: &str| args[k].as_str().filter(|x| !x.is_empty());
        if !self.calls.contains_key(id) {
            let (kind, label, path) = match name {
                "write_todos" => {
                    self.todos = todos(&args["todos"], "description");
                    self.done.insert(id.to_string());
                    return;
                }
                "write_file" | "replace" | "edit" => {
                    let p = s("file_path").or(s("absolute_path"));
                    let verb = if name == "write_file" {
                        "Write"
                    } else {
                        "Edit"
                    };
                    ("edit", format!("{verb} {}", base(p.unwrap_or(""))), p)
                }
                "run_shell_command" => {
                    self.shell(s("command").unwrap_or(""));
                    (
                        "bash",
                        trunc(s("description").or(s("command")).unwrap_or(name), 120),
                        None,
                    )
                }
                "read_file" | "read_many_files" | "list_directory" => {
                    let p = s("file_path").or(s("absolute_path")).or(s("dir_path"));
                    ("read", format!("Read {}", base(p.unwrap_or(""))), p)
                }
                "glob" | "search_file_content" | "grep" | "google_web_search" | "web_fetch" => (
                    "search",
                    trunc(
                        s("pattern").or(s("query")).or(s("prompt")).unwrap_or(name),
                        120,
                    ),
                    None,
                ),
                _ => ("other", name.to_string(), None),
            };
            self.push(id, id.into(), kind, label, path.map(str::to_string), at);
        }
        match c["status"].as_str() {
            Some("success") => self.finish(id, false, at),
            Some("error" | "cancelled") => self.finish(id, true, at),
            _ => return,
        }
        self.done.insert(id.to_string());
    }

    fn prompt(&mut self, s: &str) {
        if self.first_prompt.is_none() {
            self.first_prompt = crate::naming::clean_prompt(s);
        }
    }

    fn shell(&mut self, cmd: &str) {
        for t in cmd.split(|c: char| c.is_whitespace() || ";&|()'\"<>=`$".contains(c)) {
            let t = t.trim_end_matches('/');
            if self.dirs.len() < MAX_DIRS
                && t.len() > 1
                && t.starts_with('/')
                && !self.dirs.iter().any(|d| d == t)
            {
                self.dirs.push(t.to_string());
            }
        }
    }

    fn text(&mut self, s: &str) {
        if !s.trim().is_empty() {
            self.last_text = Some(trunc(s, 500));
        }
    }

    fn push(
        &mut self,
        call: &str,
        id: String,
        kind: &'static str,
        label: String,
        path: Option<String>,
        ts: i64,
    ) {
        if let Some(p) = path.as_ref().filter(|_| kind == "edit") {
            if self.edited_set.insert(p.clone()) {
                self.edited.push(p.clone());
            }
            *self.edit_count.entry(p.clone()).or_default() += 1;
            self.since_test.insert(p.clone());
        }
        self.calls
            .entry(call.to_string())
            .or_default()
            .push(self.base + self.entries.len());
        self.entries.push(Entry {
            id,
            kind,
            label,
            path,
            state: "running",
            at: ts,
            started: ts,
        });
        if self.entries.len() > 2 * MAX_ENTRIES {
            let drop = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..drop);
            self.base += drop;
            let base = self.base;
            self.calls.retain(|_, v| {
                v.retain(|i| *i >= base);
                !v.is_empty()
            });
        }
    }

    fn test_call(&mut self, call: &str, text: &str) {
        if is_test_run(text) {
            self.test_calls.insert(call.to_string());
        }
    }

    fn finish(&mut self, call: &str, failed: bool, ts: i64) {
        if self.test_calls.remove(call) {
            if failed {
                for p in &self.since_test {
                    *self.test_fails.entry(p.clone()).or_default() += 1;
                }
            }
            self.since_test.clear();
        }
        for seq in self.calls.remove(call).unwrap_or_default() {
            if let Some(e) = seq
                .checked_sub(self.base)
                .and_then(|i| self.entries.get_mut(i))
            {
                e.state = if failed { "failed" } else { "done" };
                e.at = ts;
            }
        }
    }

    fn claude(&mut self, v: &Value, ts: i64) {
        let content = &v["message"]["content"];
        match v["type"].as_str() {
            Some("ai-title") => keep(&mut self.ai_title, &v["aiTitle"]),
            Some("summary") => keep(&mut self.summary, &v["summary"]),
            Some("last-prompt") => keep(&mut self.last_prompt, &v["lastPrompt"]),
            Some("assistant") => {
                if let Some(m) = v["message"]["model"]
                    .as_str()
                    .filter(|m| !m.is_empty() && !m.starts_with('<'))
                {
                    if self.model.as_deref() != Some(m) {
                        self.model = Some(m.to_string());
                    }
                }
                for c in content.as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("text") => self.text(c["text"].as_str().unwrap_or("")),
                        Some("tool_use") => self.claude_tool(c, ts),
                        _ => {}
                    }
                }
            }
            Some("user") if v["isMeta"] != true => {
                if let Some(s) = content.as_str() {
                    self.prompt(s);
                }
                for c in content.as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("tool_result") => self.finish(
                            c["tool_use_id"].as_str().unwrap_or(""),
                            c["is_error"] == true,
                            ts,
                        ),
                        Some("text") => self.prompt(c["text"].as_str().unwrap_or("")),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn claude_tool(&mut self, c: &Value, ts: i64) {
        let name = c["name"].as_str().unwrap_or("");
        let input = &c["input"];
        let s = |k: &str| input[k].as_str().filter(|x| !x.is_empty());
        let (kind, label, path) = match name {
            "TodoWrite" => {
                self.todos = todos(&input["todos"], "content");
                return;
            }
            "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
                let p = s("file_path").or(s("notebook_path"));
                let verb = if name == "Write" { "Write" } else { "Edit" };
                ("edit", format!("{verb} {}", base(p.unwrap_or(""))), p)
            }
            "Bash" => {
                self.shell(s("command").unwrap_or(""));
                let text = format!(
                    "{} {}",
                    s("command").unwrap_or(""),
                    s("description").unwrap_or("")
                );
                self.test_call(c["id"].as_str().unwrap_or(""), &text);
                (
                    "bash",
                    trunc(s("description").or(s("command")).unwrap_or(name), 120),
                    None,
                )
            }
            "Read" => {
                let p = s("file_path");
                ("read", format!("Read {}", base(p.unwrap_or(""))), p)
            }
            "Grep" | "Glob" | "WebSearch" => (
                "search",
                trunc(s("pattern").or(s("query")).unwrap_or(name), 120),
                None,
            ),
            _ => ("other", name.to_string(), None),
        };
        let id = c["id"].as_str().unwrap_or("");
        self.push(id, id.into(), kind, label, path.map(str::to_string), ts);
    }

    fn codex(&mut self, v: &Value, ts: i64) {
        let p = &v["payload"];
        let call = p["call_id"].as_str().unwrap_or("");
        match (v["type"].as_str(), p["type"].as_str()) {
            (Some("turn_context"), _) => keep(&mut self.spend.effort, &p["effort"]),
            (Some("event_msg"), Some("user_message")) => {
                self.prompt(p["message"].as_str().unwrap_or(""))
            }
            (Some("response_item"), Some("message")) => {
                for c in p["content"].as_array().into_iter().flatten() {
                    let t = c["text"].as_str().unwrap_or("");
                    match p["role"].as_str() {
                        Some("user") => self.prompt(t),
                        Some("assistant") => self.text(t),
                        _ => {}
                    }
                }
            }
            (Some("response_item"), Some("function_call")) => {
                let raw = p["arguments"].as_str().unwrap_or("");
                let args: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
                match p["name"].as_str().unwrap_or("") {
                    "shell" | "exec_command" | "local_shell" | "shell_command" => {
                        let cmd = command_label(&args);
                        self.shell(&cmd);
                        self.test_call(call, &cmd);
                        let label = trunc(&cmd, 120);
                        self.push(call, call.into(), "bash", label, None, ts)
                    }
                    "update_plan" => self.todos = todos(&args["plan"], "step"),
                    "apply_patch" => self.patch(call, args["input"].as_str().unwrap_or(raw), ts),
                    n => self.push(call, call.into(), "other", n.into(), None, ts),
                }
            }
            (Some("response_item"), Some("custom_tool_call")) => match p["name"].as_str() {
                Some("apply_patch") => self.patch(call, p["input"].as_str().unwrap_or(""), ts),
                n => self.push(call, call.into(), "other", n.unwrap_or("").into(), None, ts),
            },
            (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
                self.finish(call, codex_failed(&p["output"]), ts)
            }
            _ => {}
        }
    }

    fn patch(&mut self, call: &str, text: &str, ts: i64) {
        let files: Vec<(&str, &str)> = text
            .lines()
            .filter_map(|l| {
                [
                    ("*** Update File: ", "Edit"),
                    ("*** Add File: ", "Write"),
                    ("*** Delete File: ", "Delete"),
                ]
                .iter()
                .find_map(|(pre, verb)| l.strip_prefix(pre).map(|f| (*verb, f.trim())))
            })
            .collect();
        if files.is_empty() {
            self.push(call, call.into(), "edit", "apply_patch".into(), None, ts);
        }
        for (n, (verb, f)) in files.into_iter().enumerate() {
            self.push(
                call,
                format!("{call}:{n}"),
                "edit",
                format!("{verb} {}", base(f)),
                Some(f.to_string()),
                ts,
            );
        }
    }

    fn to_json(&self, since_ms: Option<i64>) -> Value {
        let start = self.entries.len().saturating_sub(MAX_ENTRIES);
        let work_log: Vec<Value> = self.entries[start..]
            .iter()
            .filter(|e| since_ms.is_none_or(|s| e.at > s))
            .map(|e| {
                let mut j = json!({"id": e.id, "kind": e.kind, "label": e.label});
                if let Some(p) = &e.path {
                    j["path"] = json!(p);
                }
                j["state"] = json!(e.state);
                j["at"] = json!(format_rfc3339(e.at));
                j["startedAt"] = json!(format_rfc3339(e.started));
                j
            })
            .collect();
        let at = self
            .entries
            .iter()
            .map(|e| e.at)
            .max()
            .or((self.last_ts > 0).then_some(self.last_ts));
        json!({
            "title": self.ai_title.clone().or_else(|| self.summary.clone()).or_else(|| self.first_prompt.as_deref().map(|p| trunc(p, 80))),
            "aiTitle": self.ai_title,
            "summary": self.summary,
            "goal": self.first_prompt.as_deref().map(|p| trunc(p, 500)),
            "lastPrompt": self.last_prompt.as_deref().map(|p| trunc(p, 500)),
            "model": self.model,
            "workLog": work_log,
            "todos": self.todos,
            "editedFiles": self.edited,
            "lastAssistantText": self.last_text,
            "at": at.map(format_rfc3339),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/activity")
            .join(name)
    }

    fn ts(s: u32) -> String {
        format!("2026-10-29T10:00:{s:02}Z")
    }

    fn entry<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["workLog"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap()
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sb-activity-{tag}-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn claude_transcript() {
        let v = activity("claude", &fixture("claude.jsonl"), None).unwrap();
        assert_eq!(v["title"], "Fix login bug");
        assert_eq!(v["goal"], "Fix the login bug in src/auth.rs");
        assert_eq!(v["workLog"].as_array().unwrap().len(), 5);
        let read = entry(&v, "toolu_01");
        assert_eq!(read["kind"], "read");
        assert_eq!(read["label"], "Read auth.rs");
        assert_eq!(read["path"], "/w/src/auth.rs");
        assert_eq!(read["state"], "done");
        assert_eq!(read["startedAt"], ts(1));
        assert_eq!(read["at"], ts(2));
        let grep = entry(&v, "toolu_02");
        assert_eq!(
            (grep["kind"].as_str(), grep["label"].as_str()),
            (Some("search"), Some("fn login"))
        );
        let edit = entry(&v, "toolu_04");
        assert_eq!(
            (
                edit["kind"].as_str(),
                edit["label"].as_str(),
                edit["state"].as_str()
            ),
            (Some("edit"), Some("Edit auth.rs"), Some("done"))
        );
        let bash = entry(&v, "toolu_05");
        assert_eq!(
            (
                bash["kind"].as_str(),
                bash["label"].as_str(),
                bash["state"].as_str()
            ),
            (Some("bash"), Some("Run tests"), Some("failed"))
        );
        let running = entry(&v, "toolu_07");
        assert_eq!(
            (running["label"].as_str(), running["state"].as_str()),
            (Some("cargo build"), Some("running"))
        );
        assert_eq!(
            v["todos"],
            json!([{"text": "Fix login", "state": "completed"}, {"text": "Write tests", "state": "in_progress"}])
        );
        assert_eq!(v["editedFiles"], json!(["/w/src/auth.rs"]));
        assert_eq!(v["lastAssistantText"], "x".repeat(500));
        assert_eq!(v["at"], ts(11));
        let since = activity("claude", &fixture("claude.jsonl"), parse_rfc3339(&ts(7))).unwrap();
        let ids: Vec<&str> = since["workLog"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["id"].as_str())
            .collect();
        assert_eq!(ids, ["toolu_05", "toolu_07"]);
        assert_eq!(since["editedFiles"], json!(["/w/src/auth.rs"]));
    }

    #[test]
    fn gemini_sessions() {
        let v = activity("gemini", &fixture("gemini.jsonl"), None).unwrap();
        assert_eq!(v["goal"], "Fix the login bug in src/auth.rs");
        assert_eq!(v["lastAssistantText"], "Patched the check.");
        assert_eq!(v["workLog"].as_array().unwrap().len(), 4);
        let read = entry(&v, "read_file_1");
        assert_eq!(
            (read["kind"].as_str(), read["state"].as_str()),
            (Some("read"), Some("done"))
        );
        assert_eq!(read["label"], "Read auth.rs");
        let sh = entry(&v, "run_shell_command_2");
        assert_eq!(
            (sh["kind"].as_str(), sh["label"].as_str()),
            (Some("bash"), Some("Run the tests"))
        );
        assert_eq!(sh["state"], "failed");
        assert_eq!(sh["startedAt"], ts(4));
        assert_eq!(sh["at"], ts(5));
        assert_eq!(entry(&v, "replace_3")["kind"], "edit");
        assert_eq!(entry(&v, "glob_5")["kind"], "search");
        assert_eq!(v["editedFiles"], json!(["/w/src/auth.rs"]));
        assert_eq!(
            v["todos"],
            json!([{"text": "Fix check", "state": "completed"}, {"text": "Add test", "state": "in_progress"}])
        );

        let w = activity("gemini", &fixture("gemini.json"), None).unwrap();
        assert_eq!(w["title"], "Login fix");
        assert_eq!(w["goal"], "Fix login");
        assert_eq!(entry(&w, "write_file_1")["label"], "Write a.rs");
        assert_eq!(w["editedFiles"], json!(["/w/a.rs"]));

        let home = tmp_dir("gemini");
        let chats = home.join("tmp/repo/chats");
        std::fs::create_dir_all(&chats).unwrap();
        std::fs::write(home.join("tmp/repo/.project_root"), "/w/repo\n").unwrap();
        std::fs::copy(
            fixture("gemini.jsonl"),
            chats.join("session-2026-10-29T10-00-9b2c4e1a.jsonl"),
        )
        .unwrap();
        std::fs::write(
            chats.join("session-x.json"),
            r#"{"sessionId":"s2","kind":"subagent"}"#,
        )
        .unwrap();
        let s = crate::gemini::sessions(&home);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, "9b2c4e1a-0000-4000-8000-000000000001");
        assert_eq!(s[0].cwd.as_deref(), Some("/w/repo"));
        assert_eq!(
            find_transcript(&home, &home, &home, "gemini", &s[0].id),
            Some(chats.join("session-2026-10-29T10-00-9b2c4e1a.jsonl"))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn codex_rollout() {
        let v = activity("codex", &fixture("codex.jsonl"), None).unwrap();
        assert_eq!(v["title"], "Add a health endpoint to the server");
        assert_eq!(v["goal"], "Add a health endpoint to the server");
        assert_eq!(v["workLog"].as_array().unwrap().len(), 5);
        let ok = entry(&v, "call_1");
        assert_eq!(
            (
                ok["kind"].as_str(),
                ok["label"].as_str(),
                ok["state"].as_str()
            ),
            (Some("bash"), Some("cargo test"), Some("done"))
        );
        assert_eq!(entry(&v, "call_2")["state"], "failed");
        let fmt = entry(&v, "call_4");
        assert_eq!(
            (fmt["label"].as_str(), fmt["state"].as_str()),
            (Some("cargo fmt --check"), Some("failed"))
        );
        let a = entry(&v, "call_3:0");
        assert_eq!(
            (
                a["kind"].as_str(),
                a["label"].as_str(),
                a["path"].as_str(),
                a["state"].as_str()
            ),
            (
                Some("edit"),
                Some("Edit main.rs"),
                Some("src/main.rs"),
                Some("done")
            )
        );
        assert_eq!(entry(&v, "call_3:1")["label"], "Write health.rs");
        assert_eq!(v["editedFiles"], json!(["src/main.rs", "src/health.rs"]));
        assert_eq!(
            v["todos"],
            json!([{"text": "Add route", "state": "completed"}, {"text": "Write tests", "state": "in_progress"}])
        );
        assert_eq!(v["lastAssistantText"], "Added the health endpoint.");
        assert_eq!(v["at"], ts(10));
    }

    #[test]
    fn incremental_parse() {
        let dir = tmp_dir("inc");
        let path = dir.join("t.jsonl");
        let use_line = |id: &str, s: u32| {
            format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": ts(s), "message": {"content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "ls"}}]}})
            )
        };
        let result_line = |id: &str, s: u32| {
            format!(
                "{}\n",
                json!({"type": "user", "timestamp": ts(s), "message": {"content": [{"type": "tool_result", "tool_use_id": id, "content": "ok"}]}})
            )
        };
        let append = |s: &str| {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(s.as_bytes())
                .unwrap();
        };
        assert_eq!(activity("claude", &path, None), None);
        append(&use_line("a", 1));
        let partial = result_line("a", 2);
        let (head, tail) = partial.split_at(10);
        append(head);
        let v = activity("claude", &path, None).unwrap();
        assert_eq!(entry(&v, "a")["state"], "running");
        assert_eq!(v["at"], ts(1));
        append(tail);
        append(&use_line("b", 3));
        let v = activity("claude", &path, parse_rfc3339(&ts(1))).unwrap();
        assert_eq!(v["workLog"].as_array().unwrap().len(), 2);
        assert_eq!(entry(&v, "a")["state"], "done");
        assert_eq!(entry(&v, "a")["at"], ts(2));
        assert_eq!(entry(&v, "b")["state"], "running");
        std::fs::write(&path, use_line("c", 4)).unwrap();
        let v = activity("claude", &path, None).unwrap();
        assert_eq!(v["workLog"].as_array().unwrap().len(), 1);
        assert_eq!(entry(&v, "c")["state"], "running");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_transcripts() {
        let dir = tmp_dir("find");
        let claude = dir.join("claude");
        let codex = dir.join("codex");
        std::fs::create_dir_all(claude.join("projects/-w")).unwrap();
        std::fs::write(claude.join("projects/-w/s1.jsonl"), "").unwrap();
        let day = codex.join("sessions/2026/10/29");
        std::fs::create_dir_all(&day).unwrap();
        let rollout = day.join("rollout-2026-10-29T10-00-00-s2.jsonl");
        std::fs::write(&rollout, "").unwrap();
        assert_eq!(
            find_transcript(&claude, &codex, &codex, "claude", "s1"),
            Some(claude.join("projects/-w/s1.jsonl"))
        );
        assert_eq!(
            find_transcript(&claude, &codex, &codex, "codex", "s2"),
            Some(rollout)
        );
        assert_eq!(
            find_transcript(&claude, &codex, &codex, "claude", "s2"),
            None
        );
        assert_eq!(
            find_transcript(&claude, &codex, &codex, "codex", "../s2"),
            None
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_transcripts_after_a_project_rename() {
        let dir = tmp_dir("rename");
        let claude = dir.join("claude");
        let old = claude.join("projects/-home-me-cmux-ide");
        let new = claude.join("projects/-home-me-overseer");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(claude.join("projects/-home-me-other")).unwrap();
        std::fs::write(old.join("s9.jsonl"), "").unwrap();
        let find = || find_transcript(&claude, &dir, &dir, "claude", "s9");
        assert_eq!(find(), Some(old.join("s9.jsonl")));
        assert_eq!(find(), Some(old.join("s9.jsonl")));
        std::fs::rename(&old, &new).unwrap();
        assert_eq!(find(), Some(new.join("s9.jsonl")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn claude_titles_prompts_and_model() {
        let dir = tmp_dir("naming");
        let path = dir.join("t.jsonl");
        let lines = [
            json!({"type": "user", "message": {"content": "<command-name>/clear</command-name>\n<command-message>clear</command-message>"}}),
            json!({"type": "user", "message": {"content": [{"type": "text", "text": "<system-reminder>be nice</system-reminder>\nrepare le bouton de connexion"}]}}),
            json!({"type": "assistant", "message": {"model": "claude-opus-5-5", "content": []}}),
            json!({"type": "last-prompt", "leafUuid": "x"}),
            json!({"type": "last-prompt", "lastPrompt": "and add a test"}),
            json!({"type": "summary", "summary": "Login button repair"}),
        ];
        let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
        std::fs::write(&path, &body).unwrap();
        let v = activity("claude", &path, None).unwrap();
        assert_eq!(v["goal"], "repare le bouton de connexion");
        assert_eq!(v["lastPrompt"], "and add a test");
        assert_eq!(v["summary"], "Login button repair");
        assert_eq!(v["aiTitle"], Value::Null);
        assert_eq!(v["title"], "Login button repair");
        assert_eq!(v["model"], "claude-opus-5-5");
        let more = format!(
            "{}\n{}\n",
            json!({"type": "ai-title", "aiTitle": "First"}),
            json!({"type": "ai-title", "aiTitle": "Login button fix"})
        );
        std::fs::write(&path, format!("{body}{more}")).unwrap();
        let v = activity("claude", &path, None).unwrap();
        assert_eq!(v["aiTitle"], "Login button fix");
        assert_eq!(v["title"], "Login button fix");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn asst(id: &str, model: &str, s: u32, [inp, cw, cw1h, cr, out]: [u64; 5]) -> String {
        json!({"type": "assistant", "timestamp": ts(s), "message": {"id": id, "model": model,
            "content": [{"type": "text", "text": "x"}],
            "usage": {"input_tokens": inp, "cache_creation_input_tokens": cw, "cache_read_input_tokens": cr,
                      "output_tokens": out, "cache_creation": {"ephemeral_1h_input_tokens": cw1h}}}})
        .to_string()
    }

    fn write(p: &Path, lines: &[String]) {
        std::fs::write(p, lines.join("\n") + "\n").unwrap();
    }

    fn model<'a>(v: &'a Value, m: &str) -> &'a Value {
        v["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["model"] == m)
            .unwrap()
    }

    #[test]
    fn claude_spend_counts_each_message_once_across_content_blocks() {
        let d = tmp_dir("spend-dedupe");
        let f = d.join("s.jsonl");
        let a = asst("m1", "claude-opus-5-5", 1, [4, 2000, 2000, 10_000, 100]);
        write(
            &f,
            &[
                a.clone(),
                a.clone(),
                a,
                asst("m2", "claude-opus-5-5", 2, [2, 0, 0, 12_000, 50]),
            ],
        );
        let v = spend("claude", &f).unwrap().to_json();
        assert_eq!(v["input"], 6);
        assert_eq!(v["cacheWrite"], 2000);
        assert_eq!(v["cacheRead"], 22_000);
        assert_eq!(v["output"], 150);
        assert_eq!(v["model"], "claude-opus-5-5");
        let want = (6.0 * 4.0 + 2000.0 * 8.0 + 22_000.0 * 0.2 + 150.0 * 20.0) / 1e6;
        assert!((v["cost"].as_f64().unwrap() - want).abs() < 1e-4);
        assert_eq!(v["partial"], false);
        assert_eq!(v["switches"], json!([]));
        assert_eq!(v["pricesAsOf"], crate::usage::PRICES_AS_OF);
    }

    #[test]
    fn claude_spend_adds_subagents_at_their_own_model_rates() {
        let d = tmp_dir("spend-sub");
        let f = d.join("s.jsonl");
        write(
            &f,
            &[asst("m1", "claude-opus-5-5", 1, [0, 0, 0, 0, 1_000_000])],
        );
        let subs = d.join("s").join("subagents");
        std::fs::create_dir_all(&subs).unwrap();
        write(
            &subs.join("agent-a.jsonl"),
            &[asst("x1", "claude-haiku-5-5", 1, [0, 0, 0, 0, 1_000_000])],
        );
        let v = spend("claude", &f).unwrap().to_json();
        assert_eq!(model(&v, "claude-opus-5-5")["cost"], 20.0);
        assert_eq!(model(&v, "claude-haiku-5-5")["cost"], 0.5);
        assert_eq!(v["cost"], 20.5);
        assert_eq!(v["models"][0]["model"], "claude-opus-5-5");
        assert_eq!(v["model"], "claude-opus-5-5");
    }

    #[test]
    fn claude_spend_adds_workflow_agents_nested_under_subagents() {
        let d = tmp_dir("spend-nested");
        let f = d.join("s.jsonl");
        write(&f, &[asst("m1", "claude-opus-5-5", 1, [0, 0, 0, 0, 10])]);
        let wf = d.join("s").join("subagents").join("workflows").join("wf_1");
        std::fs::create_dir_all(&wf).unwrap();
        write(
            &wf.join("agent-b.jsonl"),
            &[asst("w1", "claude-haiku-5-5", 1, [0, 0, 0, 0, 7])],
        );
        let v = spend("claude", &f).unwrap().to_json();
        assert_eq!(v["output"], 17);
        assert_eq!(model(&v, "claude-haiku-5-5")["output"], 7);
    }

    #[test]
    fn model_switch_is_recorded_with_its_cache_write_cost() {
        let d = tmp_dir("spend-switch");
        let f = d.join("s.jsonl");
        let mut side: Value =
            serde_json::from_str(&asst("sd", "claude-haiku-5-5", 2, [0, 0, 0, 0, 5])).unwrap();
        side["isSidechain"] = json!(true);
        write(
            &f,
            &[
                asst("m1", "claude-opus-5-5", 1, [1, 100, 0, 0, 1]),
                side.to_string(),
                asst("m2", "claude-opus-5-5", 3, [1, 0, 0, 100, 1]),
                asst("m3", "claude-sonnet-5-5", 4, [3, 50_000, 0, 0, 10]),
            ],
        );
        let v = spend("claude", &f).unwrap().to_json();
        let sw = v["switches"].as_array().unwrap();
        assert_eq!(sw.len(), 1);
        assert_eq!(sw[0]["from"], "claude-opus-5-5");
        assert_eq!(sw[0]["to"], "claude-sonnet-5-5");
        assert_eq!(sw[0]["cacheWrite"], 50_000);
        assert_eq!(sw[0]["at"], ts(4));
        let want = (3.0 * 2.0 + 50_000.0 * 2.5) / 1e6;
        assert!((sw[0]["cost"].as_f64().unwrap() - want).abs() < 1e-4);
        assert_eq!(v["model"], "claude-sonnet-5-5");
    }

    #[test]
    fn unknown_model_keeps_tokens_and_marks_cost_partial() {
        let d = tmp_dir("spend-unknown");
        let f = d.join("s.jsonl");
        write(&f, &[asst("m1", "mystery-1", 1, [10, 0, 0, 0, 10])]);
        let v = spend("claude", &f).unwrap().to_json();
        assert_eq!(v["cost"], Value::Null);
        assert_eq!(v["partial"], true);
        assert_eq!(v["input"], 10);
        assert_eq!(model(&v, "mystery-1")["cost"], Value::Null);
    }

    #[test]
    fn transcript_without_usage_has_no_spend() {
        let d = tmp_dir("spend-none");
        let f = d.join("s.jsonl");
        write(
            &f,
            &[
                json!({"type": "user", "timestamp": ts(1), "message": {"content": "hi"}})
                    .to_string(),
            ],
        );
        assert_eq!(spend("claude", &f), None);
    }

    #[test]
    fn codex_spend_diffs_cumulative_totals_per_turn_model_and_reads_effort() {
        let d = tmp_dir("spend-codex");
        let f = d.join("rollout.jsonl");
        let ctx = |m: &str, e: &str| {
            json!({"timestamp": ts(1), "type": "turn_context", "payload": {"model": m, "effort": e}}).to_string()
        };
        let tc = |s: u32, i: u64, c: u64, o: u64| {
            json!({"timestamp": ts(s), "type": "event_msg", "payload": {"type": "token_count",
            "info": {"total_token_usage": {"input_tokens": i, "cached_input_tokens": c, "output_tokens": o}}}}).to_string()
        };
        write(
            &f,
            &[
                ctx("gpt-5", "high"),
                tc(2, 1000, 800, 10),
                tc(3, 1000, 800, 10),
                ctx("gpt-5-mini", "low"),
                tc(4, 3000, 2000, 30),
            ],
        );
        let v = spend("codex", &f).unwrap().to_json();
        assert_eq!(model(&v, "gpt-5")["input"], 200);
        assert_eq!(model(&v, "gpt-5")["cacheRead"], 800);
        assert_eq!(model(&v, "gpt-5-mini")["input"], 800);
        assert_eq!(model(&v, "gpt-5-mini")["cacheRead"], 1200);
        assert_eq!(model(&v, "gpt-5-mini")["output"], 20);
        assert_eq!(v["effort"], "low");
        assert_eq!(v["model"], "gpt-5-mini");
        assert_eq!(v["switches"][0]["to"], "gpt-5-mini");
    }

    #[test]
    fn gemini_spend_reads_message_tokens() {
        let d = tmp_dir("spend-gemini");
        let f = d.join("session.json");
        let msgs = json!([
            {"id": "u1", "type": "user", "timestamp": ts(1), "content": "hi"},
            {"id": "g1", "type": "gemini", "timestamp": ts(2), "model": "gemini-2.5-pro", "content": "ok",
             "tokens": {"input": 1000, "cached": 400, "output": 20, "thoughts": 5, "tool": 0}}
        ]);
        std::fs::write(&f, json!({"messages": msgs}).to_string()).unwrap();
        let v = spend("gemini", &f).unwrap().to_json();
        assert_eq!(v["input"], 600);
        assert_eq!(v["cacheRead"], 400);
        assert_eq!(v["output"], 25);
        assert_eq!(v["model"], "gemini-2.5-pro");
    }

    #[test]
    fn spend_reads_only_appended_lines() {
        let d = tmp_dir("spend-incr");
        let f = d.join("s.jsonl");
        let lines: Vec<String> = (0..200)
            .map(|i| asst(&format!("m{i}"), "claude-opus-5-5", 1, [1, 0, 0, 0, 1]))
            .collect();
        write(&f, &lines);
        INGESTED.with(|n| n.set(0));
        assert_eq!(spend("claude", &f).unwrap().to_json()["output"], 200);
        assert_eq!(INGESTED.with(|n| n.get()), 200);
        assert_eq!(spend("claude", &f).unwrap().to_json()["output"], 200);
        assert_eq!(INGESTED.with(|n| n.get()), 200);
        use std::io::Write;
        let mut h = std::fs::OpenOptions::new().append(true).open(&f).unwrap();
        writeln!(h, "{}", asst("new", "claude-opus-5-5", 2, [1, 0, 0, 0, 1])).unwrap();
        assert_eq!(spend("claude", &f).unwrap().to_json()["output"], 201);
        assert_eq!(INGESTED.with(|n| n.get()), 201);
    }

    #[test]
    fn effort_comes_from_launch_flags_when_the_transcript_has_none() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            effort_of(&v(&["claude", "--effort", "high"])).as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_of(&v(&["codex", "-c", "model_reasoning_effort=xhigh"])).as_deref(),
            Some("xhigh")
        );
        assert_eq!(effort_of(&v(&["claude", "--effort"])), None);
        assert_eq!(effort_of(&v(&["claude"])), None);
        let s = Spend {
            models: vec![(
                "claude-opus-5-5".into(),
                Tokens {
                    output: 1,
                    ..Tokens::default()
                },
            )],
            ..Spend::default()
        };
        assert_eq!(
            spend_json(&s, &v(&["claude", "--effort", "max"]))["effort"],
            "max"
        );
    }
}
