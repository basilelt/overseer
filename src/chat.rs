use crate::activity::{codex_failed, command_label};
use crate::procs::now_ms;
use crate::util::{format_rfc3339, parse_rfc3339};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

const KEEP: usize = 2000;
const MAX_SLOTS: usize = if cfg!(test) { 64 } else { 8 };
const IDLE_MS: i64 = 15 * 60_000;
const MAX_LINE: usize = 8 << 20;
const TEXT_CAP: usize = 64 << 10;
const USER_CAP: usize = 16 << 10;
const OUTPUT_CAP: usize = 4 << 10;
const DIFF_CAP: usize = 8 << 10;
const REPORT_CAP: usize = 16 << 10;
const STALE_MS: i64 = 30 * 60_000;
const GONE_MS: i64 = 60_000;
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 500;

static REV: OnceLock<AtomicU64> = OnceLock::new();

fn next_rev() -> u64 {
    REV.get_or_init(|| AtomicU64::new(now_ms().max(0) as u64 * 1000))
        .fetch_add(1, Ordering::Relaxed)
        + 1
}

struct Entry {
    id: String,
    seq: u64,
    rev: u64,
    v: Value,
}

#[derive(Default)]
struct Sub {
    agent_id: Option<String>,
    kind: Option<String>,
    description: String,
    background: bool,
    state: &'static str,
    started: Option<i64>,
    ended: Option<i64>,
    tokens: u64,
    tools: u64,
    activity: Option<String>,
    offset: u64,
    turn: u64,
    waiting: bool,
    resumed: bool,
}

impl Sub {
    fn json(&self) -> Value {
        json!({
            "agentId": self.agent_id,
            "type": self.kind,
            "description": self.description,
            "state": self.state,
            "background": self.background,
            "startedAt": at_json(self.started),
            "endedAt": at_json(self.ended),
            "tokens": self.tokens,
            "toolCount": self.tools,
            "lastActivity": self.activity,
        })
    }

    fn scan(&mut self, v: &Value) {
        match v["type"].as_str() {
            Some("assistant") => {}
            Some("user") => {
                if v["message"]["content"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|c| c["type"] == "tool_result"))
                {
                    self.waiting = false;
                }
                return;
            }
            _ => return,
        }
        let msg = &v["message"];
        let u = &msg["usage"];
        let total: u64 = [
            "input_tokens",
            "output_tokens",
            "cache_read_input_tokens",
            "cache_creation_input_tokens",
        ]
        .iter()
        .filter_map(|k| u[*k].as_u64())
        .fold(0, u64::saturating_add);
        if total > 0 {
            self.tokens = total;
        }
        for c in msg["content"].as_array().into_iter().flatten() {
            match c["type"].as_str() {
                Some("tool_use") => {
                    self.tools += 1;
                    self.waiting = true;
                    let t = claude_tool(c["name"].as_str().unwrap_or(""), &c["input"]);
                    self.activity = t["label"].as_str().map(str::to_string);
                }
                Some("text") => {
                    self.waiting = false;
                    let l = one_line(c["text"].as_str().unwrap_or(""), 160);
                    if !l.is_empty() {
                        self.activity = Some(l);
                    }
                }
                _ => {}
            }
        }
    }
}

struct Chat {
    agent: String,
    side: bool,
    dir: Option<PathBuf>,
    subs: HashMap<String, Sub>,
    metas: HashMap<String, String>,
    meta_stamp: i64,
    offset: u64,
    stamp: (u64, i64),
    entries: Vec<Entry>,
    index: HashMap<String, u64>,
    next_seq: u64,
    evicted: bool,
    base_rev: u64,
    rev: u64,
    model: Option<String>,
    used_ms: i64,
    last_msg: Option<(String, String)>,
    live: Option<bool>,
    turns: u64,
    turn_mid: Option<String>,
    shell: Option<String>,
    line_at: Option<u64>,
    store: Option<crate::vibe::Store>,
}

impl Chat {
    fn new(agent: &str, path: &Path) -> Chat {
        let base = next_rev();
        let side = path
            .parent()
            .and_then(|d| d.file_name())
            .is_some_and(|n| n == "subagents");
        let dir = match (agent, side) {
            ("claude", true) => path.parent().map(Path::to_path_buf),
            ("claude", false) => Some(path.with_extension("").join("subagents")),
            _ => None,
        };
        Chat {
            agent: agent.to_string(),
            side,
            dir,
            subs: HashMap::new(),
            metas: HashMap::new(),
            meta_stamp: 0,
            offset: 0,
            stamp: (0, 0),
            entries: vec![],
            index: HashMap::new(),
            next_seq: 0,
            evicted: false,
            base_rev: base,
            rev: base,
            model: None,
            used_ms: now_ms(),
            last_msg: None,
            live: None,
            turns: 0,
            turn_mid: None,
            shell: None,
            line_at: None,
            store: None,
        }
    }

    fn pos(&self, id: &str) -> Option<usize> {
        let seq = *self.index.get(id)?;
        let first = self.entries.first()?.seq;
        seq.checked_sub(first).map(|i| i as usize)
    }

    fn bump(&mut self) -> u64 {
        self.rev = next_rev();
        self.rev
    }

    fn upsert(&mut self, id: &str, v: Value) {
        if let Some(i) = self.pos(id) {
            if self.entries[i].v != v {
                let rev = self.bump();
                let e = &mut self.entries[i];
                e.v = v;
                e.rev = rev;
            }
            return;
        }
        let rev = self.bump();
        let seq = self.next_seq;
        self.next_seq += 1;
        self.index.insert(id.to_string(), seq);
        self.entries.push(Entry {
            id: id.to_string(),
            seq,
            rev,
            v,
        });
        if self.entries.len() > 2 * KEEP {
            let drop = self.entries.len() - KEEP;
            for e in self.entries.drain(..drop) {
                self.index.remove(&e.id);
            }
            self.evicted = true;
        }
    }

    fn update(&mut self, id: &str, f: impl FnOnce(&mut Value)) {
        let Some(i) = self.pos(id) else { return };
        let mut v = self.entries[i].v.clone();
        f(&mut v);
        self.upsert(id, v);
    }

    fn ingest(&mut self, v: &Value) {
        let at = v["timestamp"].as_str().and_then(parse_rfc3339);
        match self.agent.as_str() {
            "codex" => self.codex(v, at),
            "gemini" => self.gemini(v, at),
            "vibe" => self.vibe(v),
            _ => self.claude(v, at),
        }
    }

    fn json(&self, e: &Entry) -> Value {
        let mut v = json!({"id": e.id, "seq": e.seq, "rev": e.rev});
        if let Value::Object(m) = &e.v {
            for (k, x) in m {
                v[k] = x.clone();
            }
        }
        v
    }

    fn page(&self, q: Query, reset: bool) -> Value {
        let limit = q.limit.clamp(1, MAX_LIMIT);
        let tail = |end: usize| -> (Vec<Value>, bool) {
            let start = end.saturating_sub(limit);
            (
                self.entries[start..end]
                    .iter()
                    .map(|e| self.json(e))
                    .collect(),
                start > 0 || self.evicted,
            )
        };
        let (entries, more, reset) = match q.cursor {
            Cursor::After(c) if !reset && c >= self.base_rev && c <= self.rev => {
                let changed: Vec<&Entry> = self.entries.iter().filter(|e| e.rev > c).collect();
                if changed.len() > MAX_LIMIT {
                    let (e, m) = tail(self.entries.len());
                    (e, m, true)
                } else {
                    (changed.iter().map(|e| self.json(e)).collect(), false, false)
                }
            }
            Cursor::After(_) => {
                let (e, m) = tail(self.entries.len());
                (e, m, true)
            }
            Cursor::Before(s) => {
                let end = self.entries.partition_point(|e| e.seq < s);
                let (e, m) = tail(end);
                (e, m, false)
            }
            Cursor::Tail => {
                let (e, m) = tail(self.entries.len());
                (e, m, reset)
            }
        };
        json!({
            "agent": self.agent,
            "cursor": self.rev,
            "reset": reset,
            "more": more,
            "model": self.model,
            "entries": entries,
        })
    }
}

fn cap(s: &str, n: usize) -> (String, bool) {
    if s.len() <= n {
        return (s.to_string(), false);
    }
    let mut end = n;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}

fn capped(s: &str, n: usize) -> String {
    cap(s, n).0
}

fn base(p: &str) -> &str {
    Path::new(p)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(p)
}

fn one_line(s: &str, n: usize) -> String {
    let first = s.trim().lines().next().unwrap_or("");
    first.chars().take(n).collect()
}

fn at_json(at: Option<i64>) -> Value {
    at.map(|t| json!(format_rfc3339(t))).unwrap_or(Value::Null)
}

fn todos(list: &Value, key: &str) -> Value {
    Value::Array(
        list.as_array()
            .into_iter()
            .flatten()
            .map(|t| json!({"text": t[key], "state": t["status"]}))
            .collect(),
    )
}

fn tag<'a>(s: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let i = s.find(&open)? + open.len();
    let j = s[i..].find(&close)? + i;
    Some(s[i..j].trim())
}

enum UserText {
    Prompt(String),
    Notice(String),
    Skip,
}

fn user_text(s: &str) -> UserText {
    let t = s.trim();
    if t.is_empty() || t.starts_with("Caveat:") {
        return UserText::Skip;
    }
    if let Some(name) = tag(t, "command-name") {
        let args = tag(t, "command-args").unwrap_or("");
        let cmd = if args.is_empty() {
            name.to_string()
        } else {
            format!("{name} {args}")
        };
        return UserText::Prompt(cmd);
    }
    if let Some(out) = tag(t, "local-command-stdout") {
        return if out.is_empty() {
            UserText::Skip
        } else {
            UserText::Notice(capped(out, OUTPUT_CAP))
        };
    }
    if t.starts_with('<') {
        return UserText::Skip;
    }
    if t.starts_with("[Request interrupted") {
        return UserText::Notice("Interrupted".into());
    }
    UserText::Prompt(capped(t, USER_CAP))
}

fn output_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a
            .iter()
            .filter_map(|c| c["text"].as_str().or(c.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        o => o.to_string(),
    }
}

fn set_output(tool: &mut Value, text: &str) {
    let (o, t) = cap(text.trim_end(), OUTPUT_CAP);
    if !o.is_empty() {
        tool["output"] = json!(o);
    }
    if t {
        tool["truncated"] = json!(true);
    }
}

fn diff(old: &str, new: &str) -> Value {
    json!({"old": capped(old, DIFF_CAP), "new": capped(new, DIFF_CAP)})
}

fn tool(name: &str, kind: &str, label: String) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("name".into(), json!(name));
    m.insert("kind".into(), json!(kind));
    m.insert("label".into(), json!(label));
    m.insert("state".into(), json!("running"));
    m
}

fn claude_tool(name: &str, input: &Value) -> Value {
    let s = |k: &str| input[k].as_str().filter(|x| !x.is_empty());
    let mut t = match name {
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            let p = s("file_path").or(s("notebook_path")).unwrap_or("");
            let mut t = tool(name, "edit", format!("Edit {}", base(p)));
            t.insert("path".into(), json!(p));
            if let Some(edits) = input["edits"].as_array() {
                let pick = |k: &str| {
                    edits
                        .iter()
                        .filter_map(|e| e[k].as_str())
                        .collect::<Vec<_>>()
                        .join("\n⋯\n")
                };
                t.insert(
                    "diff".into(),
                    diff(&pick("old_string"), &pick("new_string")),
                );
            } else if let (Some(o), Some(n)) = (
                input["old_string"].as_str(),
                input["new_string"]
                    .as_str()
                    .or(input["new_source"].as_str()),
            ) {
                t.insert("diff".into(), diff(o, n));
            }
            t
        }
        "Write" => {
            let p = s("file_path").unwrap_or("");
            let mut t = tool(name, "write", format!("Write {}", base(p)));
            t.insert("path".into(), json!(p));
            t.insert(
                "diff".into(),
                diff("", input["content"].as_str().unwrap_or("")),
            );
            t
        }
        "Bash" => {
            let cmd = s("command").unwrap_or("");
            let mut t = tool(name, "bash", one_line(s("description").unwrap_or(cmd), 160));
            t.insert("command".into(), json!(capped(cmd, DIFF_CAP)));
            t
        }
        "Read" => {
            let p = s("file_path").unwrap_or("");
            let mut t = tool(name, "read", format!("Read {}", base(p)));
            t.insert("path".into(), json!(p));
            t
        }
        "Grep" | "Glob" | "LS" => tool(
            name,
            "search",
            one_line(s("pattern").or(s("path")).unwrap_or(name), 160),
        ),
        "WebSearch" | "WebFetch" => tool(
            name,
            "web",
            one_line(s("query").or(s("url")).unwrap_or(name), 160),
        ),
        "Task" | "Agent" => {
            let mut t = tool(
                name,
                "task",
                one_line(s("description").unwrap_or("Subagent"), 160),
            );
            if let Some(d) = s("subagent_type") {
                t.insert("detail".into(), json!(d));
            }
            t
        }
        _ => tool(name, "other", name.to_string()),
    };
    t.retain(|_, v| !v.is_null());
    Value::Object(t)
}

impl Chat {
    fn push_user(&mut self, id: &str, at: Option<i64>, text: &str, images: usize) {
        self.last_msg = None;
        if let Some(cmd) = text
            .trim_start()
            .starts_with("<bash-input>")
            .then(|| tag(text, "bash-input"))
            .flatten()
        {
            let mut t = tool("Bash", "bash", one_line(&format!("! {cmd}"), 160));
            t.insert("command".into(), json!(capped(cmd, DIFF_CAP)));
            self.upsert(id, json!({"kind": "tool", "at": at_json(at), "tool": t}));
            self.shell = Some(id.to_string());
            return;
        }
        if let Some(out) = text
            .trim_start()
            .starts_with("<bash-stdout>")
            .then(|| tag(text, "bash-stdout"))
            .flatten()
        {
            let err = tag(text, "bash-stderr").unwrap_or("");
            let all = [out, err]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            match self.shell.take() {
                Some(sid) => self.finish(&sid, out.is_empty() && !err.is_empty(), &all),
                None if !all.is_empty() => self.upsert(
                    id,
                    json!({"kind": "notice", "at": at_json(at), "text": capped(&all, OUTPUT_CAP)}),
                ),
                None => {}
            }
            return;
        }
        match user_text(text) {
            UserText::Prompt(t) => {
                let mut v = json!({"kind": "user", "at": at_json(at), "text": t});
                if images > 0 {
                    v["images"] = json!(images);
                    self.images_at(&mut v);
                }
                self.upsert(id, v);
            }
            UserText::Notice(t) => {
                self.upsert(id, json!({"kind": "notice", "at": at_json(at), "text": t}))
            }
            UserText::Skip if images > 0 => {
                let mut v =
                    json!({"kind": "user", "at": at_json(at), "text": "", "images": images});
                self.images_at(&mut v);
                self.upsert(id, v)
            }
            UserText::Skip => {}
        }
    }

    fn images_at(&self, v: &mut Value) {
        if let Some(o) = self.line_at.filter(|_| !self.side) {
            v["imagesAt"] = json!(o);
        }
    }

    fn push_text(&mut self, id: &str, msg: Option<&str>, at: Option<i64>, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if let (Some(m), Some((lm, lid))) = (msg, self.last_msg.clone()) {
            if lm == m && self.entries.last().is_some_and(|e| e.id == lid) {
                self.update(&lid, |v| {
                    let joined = format!("{}\n\n{}", v["text"].as_str().unwrap_or(""), text);
                    v["text"] = json!(capped(&joined, TEXT_CAP));
                });
                return;
            }
        }
        let mut v = json!({"kind": "assistant", "at": at_json(at), "text": capped(text, TEXT_CAP)});
        if let Some(m) = &self.model {
            v["model"] = json!(m);
        }
        self.upsert(id, v);
        self.last_msg = msg.map(|m| (m.to_string(), id.to_string()));
    }

    fn push_tool(&mut self, id: &str, at: Option<i64>, t: Value) {
        self.last_msg = None;
        self.upsert(id, json!({"kind": "tool", "at": at_json(at), "tool": t}));
    }

    fn push_todos(&mut self, id: &str, at: Option<i64>, list: Value) {
        self.last_msg = None;
        self.upsert(
            id,
            json!({"kind": "todos", "at": at_json(at), "todos": list}),
        );
    }

    fn finish(&mut self, id: &str, failed: bool, out: &str) {
        self.update(id, |v| {
            let t = &mut v["tool"];
            t["state"] = json!(if failed { "failed" } else { "done" });
            set_output(t, out);
        });
    }

    fn sync_sub(&mut self, id: &str) {
        let Some(s) = self.subs.get(id) else { return };
        let a = s.json();
        let st = match s.state {
            "running" => "running",
            "failed" => "failed",
            _ => "done",
        };
        self.update(id, |v| {
            v["tool"]["agent"] = a;
            v["tool"]["state"] = json!(st);
        });
    }

    fn start_sub(&mut self, id: &str, at: Option<i64>, input: &Value) {
        let bg = &input["run_in_background"];
        let s = Sub {
            kind: input["subagent_type"].as_str().map(str::to_string),
            description: one_line(input["description"].as_str().unwrap_or(""), 160),
            background: *bg == true || *bg == "true",
            state: "running",
            started: at,
            turn: self.turns,
            ..Default::default()
        };
        self.subs.insert(id.to_string(), s);
        self.sync_sub(id);
    }

    fn sub_by_agent(&self, agent: &str) -> Option<String> {
        self.subs
            .iter()
            .find(|(_, s)| s.agent_id.as_deref() == Some(agent))
            .map(|(k, _)| k.clone())
    }

    fn sub_result(&mut self, id: &str, at: Option<i64>, c: &Value, r: &Value) {
        let failed = c["is_error"] == true;
        let Some(s) = self.subs.get_mut(id) else {
            return;
        };
        if let Some(a) = r["agentId"].as_str() {
            s.agent_id = Some(a.to_string());
        }
        if !failed && (r["status"] == "async_launched" || r["isAsync"] == true) {
            s.background = true;
        } else {
            s.state = if failed { "failed" } else { "done" };
            s.ended = at;
            if let Some(n) = r["totalTokens"].as_u64() {
                s.tokens = n;
            }
            if let Some(n) = r["totalToolUseCount"].as_u64() {
                s.tools = n;
            }
            self.finish(id, failed, &output_text(&c["content"]));
        }
        self.sync_sub(id);
    }

    fn end_sub(&mut self, id: &str, state: &'static str, at: Option<i64>) {
        let Some(s) = self.subs.get_mut(id) else {
            return;
        };
        if s.state == "running" {
            s.ended = at;
        }
        s.state = state;
        let id = id.to_string();
        self.sync_sub(&id);
    }

    fn notify(&mut self, text: &str, at: Option<i64>) {
        let t = text.trim();
        if let Some((from, body)) = t
            .strip_prefix("<agent-message from=\"")
            .and_then(|r| r.split_once('"'))
        {
            if body.contains("[Subagent hand-back]") {
                let body = body.trim_end().trim_end_matches("</agent-message>");
                self.handback(from, body, at);
            }
            return;
        }
        if let Some(desc) = t
            .strip_prefix("Background agent \"")
            .and_then(|r| r.strip_suffix("\" was stopped by the user."))
        {
            let id = self
                .subs
                .iter()
                .find(|(_, s)| s.state == "running" && s.description == desc)
                .map(|(k, _)| k.clone());
            if let Some(id) = id {
                self.end_sub(&id, "stopped", at);
            }
            return;
        }
        let Some(id) = tag(t, "tool-use-id")
            .filter(|id| self.subs.contains_key(*id))
            .map(str::to_string)
            .or_else(|| tag(t, "task-id").and_then(|a| self.sub_by_agent(a)))
        else {
            return;
        };
        let state = match tag(t, "status") {
            Some("completed") => "done",
            Some("failed") => "failed",
            Some("killed" | "stopped") => "stopped",
            _ => return,
        };
        let num = |k: &str| tag(t, k).and_then(|x| x.parse::<u64>().ok());
        if let Some(s) = self.subs.get_mut(&id) {
            if let Some(n) = num("subagent_tokens") {
                s.tokens = n;
            }
            if let Some(n) = num("tool_uses") {
                s.tools = n;
            }
        }
        if let Some(r) = tag(t, "result").filter(|r| !r.is_empty()) {
            let (o, cut) = cap(r, REPORT_CAP);
            self.update(&id, |v| {
                if v["tool"]["output"].is_null() {
                    v["tool"]["output"] = json!(o);
                    if cut {
                        v["tool"]["truncated"] = json!(true);
                    }
                }
            });
        }
        self.end_sub(&id, state, at);
    }

    fn handback(&mut self, from: &str, body: &str, at: Option<i64>) {
        let Some(id) = self.sub_by_agent(from) else {
            return;
        };
        let report = body
            .split_once("The report follows:\n")
            .map_or(body, |x| x.1);
        let text = report
            .lines()
            .map(|l| l.strip_prefix("  ").unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
        let (o, t) = cap(text.trim(), REPORT_CAP);
        self.update(&id, |v| {
            v["tool"]["output"] = json!(o);
            if t {
                v["tool"]["truncated"] = json!(true);
            }
        });
        self.end_sub(&id, "done", at);
    }

    fn resume_sub(&mut self, to: Option<&str>) {
        let Some(id) = to.and_then(|a| self.sub_by_agent(a)) else {
            return;
        };
        let turns = self.turns;
        if let Some(s) = self.subs.get_mut(&id).filter(|s| s.state != "running") {
            s.state = "running";
            s.ended = None;
            s.turn = turns;
            s.resumed = true;
            self.sync_sub(&id);
        }
    }

    fn read_metas(&mut self, dir: &Path) {
        let Some(m) = stamp(dir).map(|s| s.1) else {
            return;
        };
        if m == self.meta_stamp {
            return;
        }
        let mut ok = true;
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(aid) = name
                .strip_prefix("agent-")
                .and_then(|x| x.strip_suffix(".meta.json"))
            else {
                continue;
            };
            if self.metas.values().any(|v| v == aid) {
                continue;
            }
            let mut buf = String::new();
            let _ = std::fs::File::open(e.path())
                .and_then(|f| f.take(64 << 10).read_to_string(&mut buf));
            match serde_json::from_str::<Value>(&buf)
                .ok()
                .and_then(|v| v["toolUseId"].as_str().map(str::to_string))
            {
                Some(t) => {
                    self.metas.insert(t, aid.to_string());
                }
                None => ok = false,
            }
        }
        if ok {
            self.meta_stamp = m;
        }
    }

    fn track(&mut self, now: i64) {
        let Some(dir) = self.dir.clone() else { return };
        let (live, turns) = (self.live, self.turns);
        let ids: Vec<String> = self
            .subs
            .iter()
            .filter(|(_, s)| s.state == "running")
            .map(|(k, _)| k.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        if ids.iter().any(|k| self.subs[k].agent_id.is_none()) {
            self.read_metas(&dir);
        }
        for id in ids {
            let Some(s) = self.subs.get_mut(&id) else {
                continue;
            };
            if s.agent_id.is_none() {
                s.agent_id = self.metas.get(&id).cloned();
            }
            let file = s
                .agent_id
                .as_ref()
                .map(|a| dir.join(format!("agent-{a}.jsonl")));
            let last = file.as_deref().and_then(|f| {
                let st = stamp(f)?;
                if st.0 > s.offset {
                    let mut off = s.offset;
                    let _ = read_jsonl(f, &mut off, st.0, |v, _| s.scan(&v));
                    s.offset = off;
                }
                Some(st.1)
            });
            let quiet = |ms: i64| last.or(s.started).is_some_and(|t| now - t > ms);
            let blocking = !s.background && !s.resumed;
            let stop = if live == Some(false) || (blocking && turns > s.turn) {
                quiet(GONE_MS)
            } else if live == Some(true) && (blocking || s.waiting) {
                false
            } else {
                quiet(STALE_MS)
            };
            if stop {
                s.state = "stopped";
                s.ended = last;
            }
            self.sync_sub(&id);
        }
    }

    fn claude(&mut self, v: &Value, at: Option<i64>) {
        if v["isSidechain"] == true && !self.side {
            return;
        }
        let uuid = v["uuid"].as_str().unwrap_or("").to_string();
        let msg = &v["message"];
        let content = &msg["content"];
        match v["type"].as_str() {
            Some("assistant") => {
                if let Some(m) = msg["model"].as_str().filter(|m| !m.starts_with('<')) {
                    self.model = Some(m.to_string());
                }
                let mid = msg["id"].as_str();
                if let Some(m) = mid.filter(|m| self.turn_mid.as_deref() != Some(*m)) {
                    self.turns += 1;
                    self.turn_mid = Some(m.to_string());
                }
                for (n, c) in content.as_array().into_iter().flatten().enumerate() {
                    let cid = if uuid.is_empty() {
                        format!("{}:{n}", mid.unwrap_or("a"))
                    } else {
                        format!("{uuid}:{n}")
                    };
                    match c["type"].as_str() {
                        Some("text") => {
                            self.push_text(&cid, mid, at, c["text"].as_str().unwrap_or(""))
                        }
                        Some("tool_use") => {
                            let id = c["id"].as_str().unwrap_or(&cid).to_string();
                            let name = c["name"].as_str().unwrap_or("");
                            let input = &c["input"];
                            match name {
                                "TodoWrite" => {
                                    self.push_todos(&id, at, todos(&input["todos"], "content"))
                                }
                                "SubagentHandback" if self.side => {
                                    self.last_msg = None;
                                    self.push_text(
                                        &id,
                                        None,
                                        at,
                                        input["message"].as_str().unwrap_or(""),
                                    )
                                }
                                _ => {
                                    self.push_tool(&id, at, claude_tool(name, input));
                                    match name {
                                        "Task" | "Agent" => self.start_sub(&id, at, input),
                                        "SendMessage" => self.resume_sub(input["to"].as_str()),
                                        "TaskStop" => {
                                            if let Some(sid) = input["task_id"]
                                                .as_str()
                                                .and_then(|a| self.sub_by_agent(a))
                                            {
                                                self.end_sub(&sid, "stopped", at);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") if v["isMeta"] == true => {
                let o = &v["origin"];
                if let (true, Some(from)) = (o["handback"] == true, o["from"].as_str()) {
                    self.handback(from, o["body"].as_str().unwrap_or(""), at);
                }
            }
            Some("attachment") if v["attachment"]["type"] == "queued_command" => {
                self.notify(v["attachment"]["prompt"].as_str().unwrap_or(""), at)
            }
            Some("user") => {
                let id = if uuid.is_empty() {
                    format!("u{}", self.next_seq)
                } else {
                    uuid.clone()
                };
                if let Some(s) = content.as_str() {
                    if s.starts_with("<task-notification>") || s.starts_with("<agent-message ") {
                        return self.notify(s, at);
                    }
                    return self.push_user(&id, at, s, 0);
                }
                let mut texts = vec![];
                let mut images = 0;
                for c in content.as_array().into_iter().flatten() {
                    match c["type"].as_str() {
                        Some("tool_result") => {
                            let tid = c["tool_use_id"].as_str().unwrap_or("");
                            if self.subs.contains_key(tid) {
                                self.sub_result(tid, at, c, &v["toolUseResult"]);
                            } else {
                                self.finish(
                                    tid,
                                    c["is_error"] == true,
                                    &output_text(&c["content"]),
                                );
                            }
                        }
                        Some("text") => texts.push(c["text"].as_str().unwrap_or("")),
                        Some("image") => images += 1,
                        _ => {}
                    }
                }
                if !texts.is_empty() || images > 0 {
                    self.push_user(&id, at, &texts.join("\n"), images);
                }
            }
            Some("system") if v["subtype"] == "compact_boundary" => {
                self.last_msg = None;
                self.upsert(
                    &uuid,
                    json!({"kind": "notice", "at": at_json(at), "text": "Conversation compacted"}),
                );
            }
            _ => {}
        }
    }

    fn patch_tool(&mut self, id: &str, at: Option<i64>, text: &str) {
        let files: Vec<String> = text
            .lines()
            .filter_map(|l| {
                ["*** Update File: ", "*** Add File: ", "*** Delete File: "]
                    .iter()
                    .find_map(|p| l.strip_prefix(p).map(|f| base(f.trim()).to_string()))
            })
            .collect();
        let label = if files.is_empty() {
            "apply_patch".to_string()
        } else {
            format!("Edit {}", files.join(", "))
        };
        let mut t = tool("apply_patch", "edit", label);
        t.insert("diff".into(), json!({"patch": capped(text, DIFF_CAP)}));
        self.push_tool(id, at, Value::Object(t));
    }

    fn codex(&mut self, v: &Value, at: Option<i64>) {
        let p = &v["payload"];
        let call = p["call_id"].as_str().unwrap_or("");
        match (v["type"].as_str(), p["type"].as_str()) {
            (Some("turn_context"), _) => {
                if let Some(m) = p["model"].as_str() {
                    self.model = Some(m.to_string());
                }
            }
            (Some("event_msg"), Some("user_message")) => {
                let id = format!("u{}", self.next_seq);
                let images = p["images"].as_array().map_or(0, Vec::len);
                self.push_user(&id, at, p["message"].as_str().unwrap_or(""), images);
            }
            (Some("event_msg"), Some("turn_aborted")) => {
                let id = format!("n{}", self.next_seq);
                self.last_msg = None;
                self.upsert(
                    &id,
                    json!({"kind": "notice", "at": at_json(at), "text": "Interrupted"}),
                );
            }
            (Some("response_item"), Some("message")) if p["role"] == "assistant" => {
                let text: Vec<&str> = p["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["text"].as_str())
                    .collect();
                let id = format!("a{}", self.next_seq);
                self.push_text(&id, None, at, &text.join("\n"));
            }
            (Some("response_item"), Some("function_call")) => {
                let raw = p["arguments"].as_str().unwrap_or("");
                let args: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
                let name = p["name"].as_str().unwrap_or("");
                match name {
                    "shell" | "exec_command" | "local_shell" | "shell_command" => {
                        let cmd = command_label(&args);
                        let mut t = tool(name, "bash", one_line(&cmd, 160));
                        t.insert("command".into(), json!(capped(&cmd, DIFF_CAP)));
                        self.push_tool(call, at, Value::Object(t));
                    }
                    "update_plan" => self.push_todos(call, at, todos(&args["plan"], "step")),
                    "apply_patch" => {
                        self.patch_tool(call, at, args["input"].as_str().unwrap_or(raw))
                    }
                    n => self.push_tool(call, at, Value::Object(tool(n, "other", n.into()))),
                }
            }
            (Some("response_item"), Some("custom_tool_call")) => match p["name"].as_str() {
                Some("apply_patch") => self.patch_tool(call, at, p["input"].as_str().unwrap_or("")),
                n => {
                    let n = n.unwrap_or("tool");
                    self.push_tool(call, at, Value::Object(tool(n, "other", n.into())))
                }
            },
            (Some("response_item"), Some("web_search_call")) => {
                let q = p["action"]["query"].as_str().unwrap_or("web search");
                let id = format!("w{}", self.next_seq);
                let mut t = tool("web_search", "web", one_line(q, 160));
                t.insert("state".into(), json!("done"));
                self.push_tool(&id, at, Value::Object(t));
            }
            (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
                let out = &p["output"];
                let parsed = out
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .filter(Value::is_object);
                let text = match &parsed {
                    Some(o) => output_text(&o["output"]),
                    None => output_text(out),
                };
                if self
                    .pos(call)
                    .is_some_and(|i| self.entries[i].v["kind"] == "tool")
                {
                    self.finish(call, codex_failed(out), &text);
                }
            }
            _ => {}
        }
    }

    fn vibe(&mut self, v: &Value) {
        let Some(id) = v["id"].as_str().filter(|i| !i.is_empty()) else {
            return;
        };
        let at = v["createdAt"].as_i64();
        match v["type"].as_str() {
            Some("message") => {
                let text: Vec<&str> = v["content"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["text"].as_str())
                    .collect();
                let text = text.join("\n");
                match v["role"].as_str() {
                    Some("user") => self.push_user(id, at, &text, 0),
                    Some("assistant") => {
                        self.last_msg = None;
                        self.push_text(id, None, at, &text)
                    }
                    _ => {}
                }
            }
            Some("effect") => self.vibe_tool(id, at, v),
            Some("checkpoint") => match v["kind"].as_str() {
                Some("model_change") => {
                    if let Some(m) = v["details"]["model"].as_str() {
                        self.model = Some(m.to_string());
                    }
                }
                Some("compaction") => {
                    self.last_msg = None;
                    self.upsert(
                        id,
                        json!({"kind": "notice", "at": at_json(at), "text": "Context compacted"}),
                    );
                }
                _ => {}
            },
            Some("error") => {
                let text = v["message"]
                    .as_str()
                    .or(v["error"]["message"].as_str())
                    .unwrap_or("Error");
                self.last_msg = None;
                self.upsert(
                    id,
                    json!({"kind": "notice", "at": at_json(at), "text": capped(text, OUTPUT_CAP)}),
                );
            }
            _ => {}
        }
    }

    fn vibe_tool(&mut self, id: &str, at: Option<i64>, v: &Value) {
        let d = &v["detail"];
        let name = d["toolName"]
            .as_str()
            .or(v["title"].as_str())
            .unwrap_or("tool");
        let input = &d["input"];
        let s = |k: &str| input[k].as_str().filter(|x| !x.is_empty());
        let leaf = name.rsplit('.').next().unwrap_or(name);
        if name == "vibe.todo" {
            if input["action"] == "write" {
                self.push_todos(id, at, todos(&input["todos"], "content"));
            }
            return;
        }
        let mut t = match (d["kind"].as_str(), name) {
            (Some("subagent"), _) => {
                let label = d["agentName"].as_str().or(s("task")).unwrap_or("Subagent");
                let mut t = tool(name, "task", one_line(label, 160));
                if let Some(a) = s("agent") {
                    t.insert("detail".into(), json!(a));
                }
                t
            }
            (_, "file_system.bash") => {
                let cmd = s("command").unwrap_or("");
                let mut t = tool(name, "bash", one_line(cmd, 160));
                t.insert("command".into(), json!(capped(cmd, DIFF_CAP)));
                t
            }
            (_, "file_system.read_file") => {
                let p = s("path").or(s("file_path")).unwrap_or("");
                let mut t = tool(name, "read", format!("Read {}", base(p)));
                t.insert("path".into(), json!(p));
                t
            }
            (_, "file_system.write_file") => {
                let p = s("path").or(s("file_path")).unwrap_or("");
                let mut t = tool(name, "write", format!("Write {}", base(p)));
                t.insert("path".into(), json!(p));
                t.insert("diff".into(), diff("", s("content").unwrap_or("")));
                t
            }
            (_, "file_system.edit" | "file_system.search_replace") => {
                let p = s("file_path").or(s("path")).unwrap_or("");
                let mut t = tool(name, "edit", format!("Edit {}", base(p)));
                t.insert("path".into(), json!(p));
                t.insert(
                    "diff".into(),
                    diff(s("old_string").unwrap_or(""), s("new_string").unwrap_or("")),
                );
                t
            }
            _ if leaf.contains("search") || leaf.contains("fetch") || leaf == "open_url" => tool(
                name,
                "web",
                one_line(s("query").or(s("url")).unwrap_or(leaf), 160),
            ),
            _ => {
                let label = d["display"]["summary"]
                    .as_str()
                    .filter(|l| !l.is_empty())
                    .unwrap_or(name);
                tool(name, "other", one_line(label, 160))
            }
        };
        let st = &v["state"];
        let state = match st["status"].as_str() {
            Some("completed") => "done",
            Some("failed" | "skipped") => "failed",
            _ => "running",
        };
        t.insert("state".into(), json!(state));
        let mut tv = Value::Object(t);
        let out = st["outputText"]
            .as_str()
            .filter(|o| !o.is_empty())
            .or(st["error"]["message"].as_str())
            .or(st["reason"].as_str())
            .unwrap_or("");
        set_output(&mut tv, out);
        self.push_tool(id, at, tv);
    }

    fn gemini(&mut self, v: &Value, at: Option<i64>) {
        let id = v["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            return;
        }
        match v["type"].as_str() {
            Some("user") => {
                let text = match &v["content"] {
                    Value::String(s) => s.clone(),
                    Value::Array(a) => a
                        .iter()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                };
                self.push_user(&id, at, &text, 0);
            }
            Some("gemini") => {
                if let Some(m) = v["model"].as_str() {
                    self.model = Some(m.to_string());
                }
                if let Some(t) = v["content"].as_str() {
                    self.last_msg = None;
                    self.push_text(&id, None, at, t);
                }
                for c in v["toolCalls"].as_array().into_iter().flatten() {
                    self.gemini_tool(c, at);
                }
            }
            _ => {}
        }
    }

    fn gemini_tool(&mut self, c: &Value, at: Option<i64>) {
        let Some(id) = c["id"].as_str().filter(|i| !i.is_empty()) else {
            return;
        };
        let at = c["timestamp"].as_str().and_then(parse_rfc3339).or(at);
        let name = c["name"].as_str().unwrap_or("");
        let args = &c["args"];
        let s = |k: &str| args[k].as_str().filter(|x| !x.is_empty());
        if name == "write_todos" {
            return self.push_todos(id, at, todos(&args["todos"], "description"));
        }
        let mut t = match name {
            "write_file" => {
                let p = s("file_path").or(s("absolute_path")).unwrap_or("");
                let mut t = tool(name, "write", format!("Write {}", base(p)));
                t.insert("path".into(), json!(p));
                t.insert("diff".into(), diff("", s("content").unwrap_or("")));
                t
            }
            "replace" | "edit" => {
                let p = s("file_path").or(s("absolute_path")).unwrap_or("");
                let mut t = tool(name, "edit", format!("Edit {}", base(p)));
                t.insert("path".into(), json!(p));
                t.insert(
                    "diff".into(),
                    diff(s("old_string").unwrap_or(""), s("new_string").unwrap_or("")),
                );
                t
            }
            "run_shell_command" => {
                let cmd = s("command").unwrap_or("");
                let mut t = tool(name, "bash", one_line(s("description").unwrap_or(cmd), 160));
                t.insert("command".into(), json!(capped(cmd, DIFF_CAP)));
                t
            }
            "read_file" | "read_many_files" | "list_directory" => {
                let p = s("file_path")
                    .or(s("absolute_path"))
                    .or(s("dir_path"))
                    .unwrap_or("");
                let mut t = tool(name, "read", format!("Read {}", base(p)));
                t.insert("path".into(), json!(p));
                t
            }
            "glob" | "search_file_content" | "grep" => {
                tool(name, "search", one_line(s("pattern").unwrap_or(name), 160))
            }
            "google_web_search" | "web_fetch" => tool(
                name,
                "web",
                one_line(s("query").or(s("prompt")).unwrap_or(name), 160),
            ),
            _ => tool(name, "other", name.to_string()),
        };
        let state = match c["status"].as_str() {
            Some("success") => "done",
            Some("error" | "cancelled") => "failed",
            _ => "running",
        };
        t.insert("state".into(), json!(state));
        let mut v = Value::Object(t);
        if let Some(out) = c["resultDisplay"].as_str() {
            set_output(&mut v, out);
        }
        self.push_tool(id, at, v);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cursor {
    Tail,
    After(u64),
    Before(u64),
}

#[derive(Clone, Copy, Debug)]
pub struct Query {
    pub cursor: Cursor,
    pub limit: usize,
    pub live: Option<bool>,
}

#[derive(Default)]
struct Cache {
    chats: HashMap<PathBuf, Chat>,
    dropped: HashSet<PathBuf>,
    // Cold parses running outside the lock; later callers wait on `parsed()` instead of parsing again.
    loading: HashSet<PathBuf>,
}

impl Cache {
    fn drop_chat(&mut self, path: &Path) {
        running_index()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
        if self.chats.remove(path).is_some() {
            if self.dropped.len() >= 64 {
                self.dropped.clear();
            }
            self.dropped.insert(path.to_path_buf());
        }
    }
}

// Lets tests pause a cold parse and see who waits on it, instead of racing a slow file.
#[cfg(test)]
#[derive(Default)]
struct Probe {
    parses: HashMap<PathBuf, usize>,
    waits: HashMap<PathBuf, usize>,
    held: HashSet<PathBuf>,
}

#[cfg(test)]
fn probe() -> &'static (Mutex<Probe>, Condvar) {
    static P: OnceLock<(Mutex<Probe>, Condvar)> = OnceLock::new();
    P.get_or_init(Default::default)
}

#[cfg(test)]
fn probe_lock() -> MutexGuard<'static, Probe> {
    probe().0.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
fn probe_wait(path: &Path) {
    *probe_lock().waits.entry(path.to_path_buf()).or_default() += 1;
    probe().1.notify_all();
}

#[cfg(test)]
fn probe_parse(path: &Path) {
    let mut p = probe_lock();
    *p.parses.entry(path.to_path_buf()).or_default() += 1;
    probe().1.notify_all();
    while p.held.contains(path) {
        p = probe().1.wait(p).unwrap_or_else(|e| e.into_inner());
    }
}

#[cfg(test)]
fn cold_parses(path: &Path) -> usize {
    probe_lock().parses.get(path).copied().unwrap_or(0)
}

fn slots() -> &'static Mutex<Cache> {
    static S: OnceLock<Mutex<Cache>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn lock() -> MutexGuard<'static, Cache> {
    slots().lock().unwrap_or_else(|e| e.into_inner())
}

fn parsed() -> &'static Condvar {
    static C: Condvar = Condvar::new();
    &C
}

// Clears the loading mark even when the cold parse fails, so waiters never hang.
struct Loading<'a>(&'a Path);

impl Drop for Loading<'_> {
    fn drop(&mut self) {
        lock().loading.remove(self.0);
        parsed().notify_all();
    }
}

fn stamp(path: &Path) -> Option<(u64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    Some((m.len(), t))
}

fn refresh_vibe(chat: &mut Chat, dir: &Path) -> Option<bool> {
    use crate::vibe::Change;
    let change = match chat.store.as_mut() {
        Some(s) => s.refresh(dir)?,
        None => {
            let (s, all) = crate::vibe::load(dir)?;
            chat.store = Some(s);
            Change::Append(all)
        }
    };
    if let Some(m) = chat.store.as_ref().and_then(|s| s.model()) {
        chat.model.get_or_insert_with(|| m.to_string());
    }
    match change {
        Change::Same => Some(false),
        Change::Append(v) => {
            for e in &v {
                chat.ingest(e);
            }
            Some(false)
        }
        Change::Reset(v) => {
            let (live, store) = (chat.live, chat.store.take());
            *chat = Chat::new(&chat.agent, dir);
            chat.live = live;
            chat.model = store.as_ref().and_then(|s| s.model()).map(str::to_string);
            chat.store = store;
            for e in &v {
                chat.ingest(e);
            }
            Some(true)
        }
    }
}

pub fn transcript_stamp(path: &Path) -> Option<u64> {
    if path.is_dir() {
        return Some(crate::vibe::stamp(path));
    }
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn refresh(chat: &mut Chat, path: &Path) -> Option<bool> {
    if chat.agent == "vibe" {
        return refresh_vibe(chat, path);
    }
    let st = stamp(path)?;
    if path.extension().is_some_and(|e| e == "json") {
        if st != chat.stamp {
            let mut buf = Vec::new();
            std::fs::File::open(path)
                .ok()?
                .take(64 << 20)
                .read_to_end(&mut buf)
                .ok()?;
            let v: Value = serde_json::from_slice(&buf).ok()?;
            for m in v["messages"].as_array().into_iter().flatten() {
                chat.ingest(m);
            }
            chat.stamp = st;
        }
        return Some(false);
    }
    let mut reset = false;
    if st.0 < chat.offset {
        let live = chat.live;
        *chat = Chat::new(&chat.agent, path);
        chat.live = live;
        reset = true;
    }
    if st.0 > chat.offset {
        let mut off = chat.offset;
        let r = read_jsonl(path, &mut off, st.0, |v, at| {
            chat.line_at = Some(at);
            chat.ingest(&v)
        });
        chat.offset = off;
        r?;
    }
    chat.stamp = st;
    chat.track(now_ms());
    Some(reset)
}

fn read_jsonl(
    path: &Path,
    offset: &mut u64,
    end: u64,
    mut f: impl FnMut(Value, u64),
) -> Option<()> {
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(*offset)).ok()?;
    let mut r = BufReader::new(file.take(end.saturating_sub(*offset)));
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = (&mut r)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)
            .ok()?;
        if n == 0 || line.last() != Some(&b'\n') {
            if n > MAX_LINE {
                *offset += n as u64;
                continue;
            }
            return Some(());
        }
        let at = *offset;
        *offset += n as u64;
        if let Ok(v) = serde_json::from_slice::<Value>(&line) {
            f(v, at);
        }
    }
}

pub fn image(path: &Path, at: u64, i: usize) -> Result<(String, Vec<u8>), (u16, String)> {
    let gone = || (404, format!("no image {i} at {at}"));
    let mut file = std::fs::File::open(path).map_err(|e| (404, e.to_string()))?;
    file.seek(SeekFrom::Start(at)).map_err(|_| gone())?;
    let mut line = Vec::new();
    BufReader::new(file.take(MAX_LINE as u64))
        .read_until(b'\n', &mut line)
        .map_err(|_| gone())?;
    let v: Value = serde_json::from_slice(&line).map_err(|_| gone())?;
    let p = &v["payload"];
    let found: Option<(String, &str)> = if v["type"] == "user" {
        v["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["type"] == "image")
            .nth(i)
            .and_then(|c| {
                let s = &c["source"];
                Some((s["media_type"].as_str()?.to_string(), s["data"].as_str()?))
            })
    } else if v["type"] == "event_msg" && p["type"] == "user_message" {
        p["images"]
            .as_array()
            .and_then(|a| a.get(i)?.as_str()?.strip_prefix("data:"))
            .and_then(|d| d.split_once(";base64,"))
            .map(|(m, d)| (m.to_string(), d))
    } else {
        None
    };
    let (mime, data) = found.ok_or_else(gone)?;
    if !["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&mime.as_str()) {
        return Err((415, format!("unsupported image type {mime}")));
    }
    let bytes = base64(data).ok_or((422, "image data is not valid base64".to_string()))?;
    Ok((mime, bytes))
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().take_while(|&c| c != b'=') {
        let d = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'\r' | b'\n' => continue,
            _ => return None,
        };
        acc = acc << 6 | d as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn evict(cache: &mut Cache, now: i64) {
    let idle: Vec<PathBuf> = cache
        .chats
        .iter()
        .filter(|(_, c)| now - c.used_ms >= IDLE_MS)
        .map(|(p, _)| p.clone())
        .collect();
    for p in idle {
        cache.drop_chat(&p);
    }
    while cache.chats.len() > MAX_SLOTS {
        let Some(old) = cache
            .chats
            .iter()
            .min_by_key(|(_, c)| (!c.side, c.used_ms))
            .map(|(p, _)| p.clone())
        else {
            break;
        };
        cache.drop_chat(&old);
    }
}

pub fn supported(agent: &str) -> bool {
    matches!(agent, "claude" | "codex" | "gemini" | "vibe")
}

pub fn page(agent: &str, path: &Path, q: Query) -> Option<Value> {
    if !supported(agent) {
        return None;
    }
    let mut g = lock();
    while g.loading.contains(path) {
        #[cfg(test)]
        probe_wait(path);
        g = parsed().wait(g).unwrap_or_else(|e| e.into_inner());
    }
    let fresh = !g.chats.contains_key(path);
    let guard = fresh.then(|| {
        g.loading.insert(path.to_path_buf());
        Loading(path)
    });
    drop(g);
    // A cold transcript can take seconds to parse: do it outside the lock so other chats stay served.
    let loaded = if fresh {
        #[cfg(test)]
        probe_parse(path);
        let mut c = Chat::new(agent, path);
        c.live = q.live;
        refresh(&mut c, path)?;
        Some(c)
    } else {
        None
    };
    let now = now_ms();
    let mut g = lock();
    evict(&mut g, now);
    g.dropped.remove(path);
    let chat = g
        .chats
        .entry(path.to_path_buf())
        .or_insert_with(|| loaded.unwrap_or_else(|| Chat::new(agent, path)));
    chat.used_ms = now;
    if q.live.is_some() {
        chat.live = q.live;
    }
    // Picks up only what was appended during the cold parse.
    let reset = refresh(chat, path);
    note_running(path, chat);
    let reset = reset?;
    let v = chat.page(q, reset || (fresh && q.cursor != Cursor::Tail));
    // Waiters are woken once the chat is cached, so they reuse it instead of parsing again.
    g.loading.remove(path);
    drop(g);
    drop(guard);
    Some(v)
}

pub fn agent_page(main: &Path, agent_id: &str, q: Query) -> Option<Value> {
    if agent_id.is_empty()
        || agent_id.len() > 64
        || !agent_id.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return None;
    }
    let p = main
        .with_extension("")
        .join("subagents")
        .join(format!("agent-{agent_id}.jsonl"));
    let mut v = page("claude", &p, q)?;
    v["agentId"] = json!(agent_id);
    Some(v)
}

fn running_index() -> &'static Mutex<HashMap<PathBuf, Vec<PathBuf>>> {
    static R: OnceLock<Mutex<HashMap<PathBuf, Vec<PathBuf>>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

fn note_running(path: &Path, c: &Chat) {
    let files: Vec<PathBuf> = match &c.dir {
        None => vec![],
        Some(dir) => c
            .subs
            .values()
            .filter(|s| s.state == "running")
            .map(|s| match &s.agent_id {
                Some(a) => dir.join(format!("agent-{a}.jsonl")),
                None => dir.clone(),
            })
            .collect(),
    };
    let mut r = running_index().lock().unwrap_or_else(|e| e.into_inner());
    if files.is_empty() {
        r.remove(path);
    } else {
        r.insert(path.to_path_buf(), files);
    }
}

pub fn agents_stamp(path: &Path) -> u64 {
    let files = running_index()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
        .cloned()
        .unwrap_or_default();
    files
        .iter()
        .filter_map(|f| stamp(f))
        .fold(0u64, |n, s| n.wrapping_add(s.0).wrapping_add(s.1 as u64))
}

pub fn changed(path: &Path) -> Option<u64> {
    let now = now_ms();
    let mut g = lock();
    // A chat being parsed is announced once loaded; a cursor 0 now would only make clients load it again.
    if g.loading.contains(path) {
        return None;
    }
    if g.chats
        .get(path)
        .is_some_and(|c| now - c.used_ms >= IDLE_MS)
    {
        g.drop_chat(path);
    }
    if g.dropped.contains(path) {
        return Some(0);
    }
    let chat = g.chats.get_mut(path)?;
    let r = refresh(chat, path);
    note_running(path, chat);
    r?;
    Some(chat.rev)
}

fn description(path: &Path) -> String {
    let mut s = String::new();
    let _ = std::fs::File::open(path).and_then(|f| f.take(8 << 10).read_to_string(&mut s));
    let mut lines = s.lines();
    let mut first = None;
    if s.starts_with("---") {
        lines.next();
        for l in lines.by_ref() {
            if l.trim() == "---" {
                break;
            }
            if let Some(d) = l.strip_prefix("description:") {
                return d
                    .trim()
                    .trim_matches(['"', '\''])
                    .chars()
                    .take(200)
                    .collect();
            }
        }
    }
    for l in lines {
        let t = l.trim().trim_start_matches('#').trim();
        if !t.is_empty() {
            first = Some(t.chars().take(200).collect());
            break;
        }
    }
    first.unwrap_or_default()
}

fn scan_dir(dir: &Path, prefix: &str, depth: u32, source: &str, out: &mut Vec<Value>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = rd.flatten().collect();
    items.sort_by_key(|e| e.file_name());
    for e in items {
        if out.len() >= 300 {
            return;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && depth > 0 {
            scan_dir(
                &e.path(),
                &format!("{prefix}{name}:"),
                depth - 1,
                source,
                out,
            );
        } else if let Some(stem) = name.strip_suffix(".md").filter(|_| !ft.is_dir()) {
            out.push(json!({
                "name": format!("/{prefix}{stem}"),
                "description": description(&e.path()),
                "source": source,
            }));
        }
    }
}

fn scan_skills(dir: &Path, source: &str, out: &mut Vec<Value>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = rd.flatten().collect();
    items.sort_by_key(|e| e.file_name());
    for e in items {
        let name = e.file_name().to_string_lossy().into_owned();
        let md = e.path().join("SKILL.md");
        if out.len() >= 300 || name.starts_with('.') || !md.is_file() {
            continue;
        }
        let mut head = String::new();
        let _ = std::fs::File::open(&md).and_then(|f| f.take(8 << 10).read_to_string(&mut head));
        if head
            .lines()
            .any(|l| l.replace(' ', "") == "user-invocable:false")
        {
            continue;
        }
        out.push(
            json!({"name": format!("/{name}"), "description": description(&md), "source": source}),
        );
    }
}

pub fn commands(agent: &str, claude_dir: &Path, codex_dir: &Path, cwd: &str) -> Value {
    let mut out = vec![];
    match agent {
        "claude" => {
            if !cwd.is_empty() {
                scan_dir(
                    &Path::new(cwd).join(".claude/commands"),
                    "",
                    3,
                    "project",
                    &mut out,
                );
            }
            scan_dir(&claude_dir.join("commands"), "", 3, "user", &mut out);
        }
        "codex" => scan_dir(&codex_dir.join("prompts"), "prompts:", 0, "user", &mut out),
        "vibe" => {
            if !cwd.is_empty() {
                for sub in [".vibe/skills", ".agents/skills"] {
                    scan_skills(&Path::new(cwd).join(sub), "project", &mut out);
                }
            }
            scan_skills(&crate::vibe::home().join("skills"), "user", &mut out);
            scan_skills(&crate::home().join(".agents/skills"), "user", &mut out);
        }
        _ => {}
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert(c["name"].as_str().unwrap_or("").to_string()));
    json!({ "commands": out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn tmp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("sb-chat-{tag}-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn tail() -> Query {
        Query {
            cursor: Cursor::Tail,
            limit: DEFAULT_LIMIT,
            live: None,
        }
    }

    fn after(c: u64) -> Query {
        Query {
            cursor: Cursor::After(c),
            limit: DEFAULT_LIMIT,
            live: None,
        }
    }

    fn entries(v: &Value) -> &Vec<Value> {
        v["entries"].as_array().unwrap()
    }

    fn find<'a>(v: &'a Value, id: &str) -> &'a Value {
        entries(v).iter().find(|e| e["id"] == id).unwrap()
    }

    fn kinds(v: &Value) -> Vec<&str> {
        entries(v)
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect()
    }

    // macOS keeps the source mtime on copy, Linux does not: stamp now so a fixture checked out
    // more than STALE_MS ago is not read as a stale session.
    fn fresh_copy(from: &Path, to: &Path) {
        std::fs::copy(from, to).unwrap();
        let f = std::fs::File::options().write(true).open(to).unwrap();
        f.set_modified(std::time::SystemTime::now()).unwrap();
    }

    fn copy(name: &str, dir: &Path) -> PathBuf {
        let p = dir.join(Path::new(name).file_name().unwrap());
        fresh_copy(&fixture(name), &p);
        p
    }

    fn append(p: &Path, s: &str) {
        std::fs::OpenOptions::new()
            .append(true)
            .open(p)
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }

    #[test]
    fn image_serves_the_claude_block_at_the_offset() {
        let d = tmp("img-claude");
        let p = copy("chat/claude.jsonl", &d);
        assert_eq!(image(&p, 0, 0), Ok(("image/png".into(), vec![0, 0, 0])));
        assert_eq!(image(&p, 0, 1).unwrap_err().0, 404);
        assert_eq!(image(&p, 5, 0).unwrap_err().0, 404);
        assert_eq!(image(&p, 1 << 40, 0).unwrap_err().0, 404);
    }

    #[test]
    fn image_reads_codex_data_urls_and_rejects_unsafe_types() {
        let d = tmp("img-codex");
        let p = d.join("rollout.jsonl");
        let line = |m: &str| {
            json!({"type": "event_msg", "payload": {"type": "user_message", "message": "",
                "images": [format!("data:{m};base64,aGk=")]}})
            .to_string()
                + "\n"
        };
        let first = line("image/jpeg");
        std::fs::write(
            &p,
            first.clone() + &line("image/svg+xml") + &line("image/png").replace("aGk=", "a*k="),
        )
        .unwrap();
        assert_eq!(image(&p, 0, 0), Ok(("image/jpeg".into(), b"hi".to_vec())));
        assert_eq!(image(&p, first.len() as u64, 0).unwrap_err().0, 415);
        assert_eq!(image(&p, 2 * first.len() as u64 + 3, 0).unwrap_err().0, 422);
    }

    #[test]
    fn base64_decodes_padded_unpadded_and_url_safe() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64("-_8=").unwrap(), [0xfb, 0xff]);
        assert_eq!(base64("aGVs\nbG8=").unwrap(), b"hello");
        assert!(base64("a*").is_none());
    }

    #[test]
    fn codex_user_images_carry_their_line_offset() {
        let d = tmp("img-at");
        let p = d.join("rollout.jsonl");
        let a = json!({"type": "event_msg", "payload": {"type": "user_message", "message": "hi"}})
            .to_string()
            + "\n";
        let b = json!({"type": "event_msg", "payload": {"type": "user_message", "message": "look",
            "images": ["data:image/png;base64,AAAA"]}})
        .to_string();
        std::fs::write(&p, a.clone() + &b + "\n").unwrap();
        let v = page("codex", &p, tail()).unwrap();
        let e = entries(&v);
        assert!(e[0].get("imagesAt").is_none());
        assert_eq!(e[1]["imagesAt"], a.len());
        assert_eq!(image(&p, a.len() as u64, 0).unwrap().1, vec![0, 0, 0]);
    }

    #[test]
    fn claude_conversation() {
        let d = tmp("claude");
        let p = copy("chat/claude.jsonl", &d);
        let v = page("claude", &p, tail()).unwrap();
        assert_eq!(v["model"], "claude-opus-4-1");
        assert_eq!(v["more"], false);
        assert_eq!(
            kinds(&v),
            [
                "user",
                "notice",
                "user",
                "assistant",
                "tool",
                "tool",
                "todos",
                "tool",
                "tool",
                "tool",
                "tool",
                "assistant",
                "user",
                "notice"
            ]
        );
        let e = entries(&v);
        assert_eq!(e[0]["text"], "Fix the login bug in src/auth.rs");
        assert_eq!(e[0]["images"], 1);
        assert_eq!(e[0]["imagesAt"], 0);
        assert!(e[2].get("imagesAt").is_none());
        assert_eq!(e[1]["text"], "cleared");
        assert_eq!(e[2]["text"], "/review src/auth.rs");
        assert_eq!(
            e[3]["text"],
            "Looking at the auth module.\n\nI will read it first."
        );
        assert_eq!(e[3]["model"], "claude-opus-4-1");
        let read = find(&v, "toolu_01");
        assert_eq!(read["tool"]["kind"], "read");
        assert_eq!(read["tool"]["label"], "Read auth.rs");
        assert_eq!(read["tool"]["state"], "done");
        assert_eq!(read["tool"]["output"], "fn login() {}");
        let edit = find(&v, "toolu_04");
        assert_eq!(
            edit["tool"]["diff"],
            json!({"old": "a == b", "new": "a != b"})
        );
        let bash = find(&v, "toolu_05");
        assert_eq!(bash["tool"]["command"], "cargo test");
        assert_eq!(bash["tool"]["label"], "Run tests");
        assert_eq!(bash["tool"]["state"], "failed");
        assert_eq!(bash["tool"]["truncated"], true);
        assert_eq!(bash["tool"]["output"].as_str().unwrap().len(), OUTPUT_CAP);
        let task = find(&v, "toolu_06");
        assert_eq!(task["tool"]["kind"], "task");
        assert_eq!(task["tool"]["detail"], "general-purpose");
        assert_eq!(task["tool"]["output"], "Found 2 callers.");
        let todos = find(&v, "toolu_03");
        assert_eq!(
            todos["todos"],
            json!([{"text": "Fix login", "state": "in_progress"}, {"text": "Write tests", "state": "pending"}])
        );
        let w = find(&v, "toolu_07");
        assert_eq!(w["tool"]["kind"], "write");
        assert_eq!(w["tool"]["state"], "running");
        assert!(!entries(&v).iter().any(|e| e["text"]
            .as_str()
            .is_some_and(|t| t.contains("secret thought"))));
        assert_eq!(entries(&v).last().unwrap()["text"], "Interrupted");
        let seqs: Vec<u64> = entries(&v)
            .iter()
            .map(|e| e["seq"].as_u64().unwrap())
            .collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn shell_commands_become_bash_steps() {
        let d = tmp("shell");
        let p = d.join("s.jsonl");
        let user = |id: &str, c: &str| {
            format!(
                "{}\n",
                json!({"type": "user", "uuid": id, "message": {"content": c}})
            )
        };
        std::fs::write(
            &p,
            [
                user("u1", "<bash-input> git status</bash-input>"),
                user(
                    "u2",
                    "<bash-stdout>On branch main</bash-stdout><bash-stderr></bash-stderr>",
                ),
                user("u3", "<bash-input>nope</bash-input>"),
                user(
                    "u4",
                    "<bash-stdout></bash-stdout><bash-stderr>nope: command not found</bash-stderr>",
                ),
                user(
                    "u5",
                    "<bash-stdout>stray</bash-stdout><bash-stderr></bash-stderr>",
                ),
                user("u6", "explain <bash-input>x</bash-input>"),
            ]
            .concat(),
        )
        .unwrap();
        let v = page("claude", &p, tail()).unwrap();
        assert_eq!(kinds(&v), ["tool", "tool", "notice", "user"]);
        let ok = find(&v, "u1");
        assert_eq!(ok["tool"]["label"], "! git status");
        assert_eq!(ok["tool"]["command"], "git status");
        assert_eq!(ok["tool"]["state"], "done");
        assert_eq!(ok["tool"]["output"], "On branch main");
        let bad = find(&v, "u3");
        assert_eq!(bad["tool"]["state"], "failed");
        assert_eq!(bad["tool"]["output"], "nope: command not found");
        assert_eq!(find(&v, "u5")["text"], "stray");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cursor_returns_new_and_updated_entries() {
        let d = tmp("cursor");
        let p = d.join("s.jsonl");
        std::fs::write(&p, "").unwrap();
        let line = |v: Value| format!("{v}\n");
        append(
            &p,
            &line(
                json!({"type": "user", "uuid": "u1", "timestamp": "2026-10-29T10:00:00Z", "message": {"content": "hi"}}),
            ),
        );
        append(
            &p,
            &line(
                json!({"type": "assistant", "uuid": "a1", "message": {"id": "m1", "model": "claude-x", "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}]}}),
            ),
        );
        let v = page("claude", &p, tail()).unwrap();
        assert_eq!(kinds(&v), ["user", "tool"]);
        let c1 = v["cursor"].as_u64().unwrap();
        let none = page("claude", &p, after(c1)).unwrap();
        assert!(entries(&none).is_empty());
        assert_eq!(none["reset"], false);
        assert_eq!(none["cursor"].as_u64(), Some(c1));
        let result = line(
            json!({"type": "user", "uuid": "u2", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "a\nb"}]}}),
        );
        let (head, rest) = result.split_at(20);
        append(&p, head);
        assert!(entries(&page("claude", &p, after(c1)).unwrap()).is_empty());
        append(&p, rest);
        append(
            &p,
            &line(
                json!({"type": "assistant", "uuid": "a2", "message": {"id": "m2", "content": [{"type": "text", "text": "done"}]}}),
            ),
        );
        let c2 = changed(&p).unwrap();
        assert!(c2 > c1);
        let v = page("claude", &p, after(c1)).unwrap();
        assert_eq!(v["cursor"].as_u64(), Some(c2));
        let ids: Vec<&str> = entries(&v)
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["t1", "a2:0"]);
        assert_eq!(find(&v, "t1")["tool"]["state"], "done");
        assert_eq!(find(&v, "t1")["tool"]["output"], "a\nb");
        assert_eq!(find(&v, "a2:0")["model"], "claude-x");
        let stale = page("claude", &p, after(1)).unwrap();
        assert_eq!(stale["reset"], true);
        assert_eq!(entries(&stale).len(), 3);
        std::fs::write(
            &p,
            line(json!({"type": "user", "uuid": "u9", "message": {"content": "again"}})),
        )
        .unwrap();
        let v = page("claude", &p, after(c2)).unwrap();
        assert_eq!(v["reset"], true);
        assert_eq!(kinds(&v), ["user"]);
        assert_eq!(entries(&v)[0]["text"], "again");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn tail_and_load_earlier() {
        let d = tmp("tail");
        let p = d.join("s.jsonl");
        let mut s = String::new();
        for i in 0..30 {
            s.push_str(&format!(
                "{}\n",
                json!({"type": "user", "uuid": format!("u{i}"), "message": {"content": format!("m{i}")}})
            ));
        }
        std::fs::write(&p, s).unwrap();
        let q = |cursor| Query {
            cursor,
            limit: 10,
            live: None,
        };
        let v = page("claude", &p, q(Cursor::Tail)).unwrap();
        assert_eq!(entries(&v).len(), 10);
        assert_eq!(v["more"], true);
        assert_eq!(entries(&v)[0]["text"], "m20");
        let first = entries(&v)[0]["seq"].as_u64().unwrap();
        let older = page("claude", &p, q(Cursor::Before(first))).unwrap();
        assert_eq!(entries(&older)[0]["text"], "m10");
        assert_eq!(entries(&older)[9]["text"], "m19");
        assert_eq!(older["more"], true);
        let first = entries(&older)[0]["seq"].as_u64().unwrap();
        let oldest = page("claude", &p, q(Cursor::Before(first))).unwrap();
        assert_eq!(entries(&oldest).len(), 10);
        assert_eq!(oldest["more"], false);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn codex_rollout() {
        let d = tmp("codex");
        let p = copy("activity/codex.jsonl", &d);
        append(
            &p,
            &format!(
                "{}\n{}\n",
                json!({"timestamp": "2026-10-29T10:00:13.000Z", "type": "turn_context", "payload": {"model": "gpt-5-codex", "cwd": "/w"}}),
                json!({"timestamp": "2026-10-29T10:00:14.000Z", "type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Next step."}]}})
            ),
        );
        let v = page("codex", &p, tail()).unwrap();
        assert_eq!(
            kinds(&v),
            [
                "user",
                "todos",
                "tool",
                "tool",
                "tool",
                "tool",
                "todos",
                "assistant",
                "assistant"
            ]
        );
        let e = entries(&v);
        assert_eq!(e[0]["text"], "Add a health endpoint to the server");
        let ok = find(&v, "call_1");
        assert_eq!(ok["tool"]["command"], "cargo test");
        assert_eq!(ok["tool"]["state"], "done");
        assert_eq!(ok["tool"]["output"], "ok");
        assert_eq!(find(&v, "call_2")["tool"]["state"], "failed");
        let patch = find(&v, "call_3");
        assert_eq!(patch["tool"]["label"], "Edit main.rs, health.rs");
        assert!(patch["tool"]["diff"]["patch"]
            .as_str()
            .unwrap()
            .contains("*** Update File: src/main.rs"));
        assert_eq!(find(&v, "call_4")["tool"]["state"], "failed");
        assert_eq!(
            e[6]["todos"],
            json!([{"text": "Add route", "state": "completed"}, {"text": "Write tests", "state": "in_progress"}])
        );
        assert_eq!(e[7]["text"], "Added the health endpoint.");
        assert!(e[7].get("model").is_none());
        assert_eq!(e[8]["model"], "gpt-5-codex");
        assert_eq!(v["model"], "gpt-5-codex");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn vibe_sessions_read_the_store_and_follow_the_journal() {
        use crate::vibe::fixture::*;
        let h = tmp("vibe");
        let first = vec![
            message("m1", "user", "Fix the login bug", 1_700_000_000_000),
            effect(
                "e1",
                "file_system.read_file",
                json!({"path": "/w/src/auth.rs"}),
                "completed",
                "fn login()",
                1_700_000_001_000,
            ),
            effect(
                "e2",
                "file_system.edit",
                json!({"file_path": "/w/src/auth.rs", "old_string": "a", "new_string": "b"}),
                "completed",
                "",
                1_700_000_002_000,
            ),
            effect(
                "e3",
                "file_system.bash",
                json!({"command": "cargo test"}),
                "failed",
                "",
                1_700_000_003_000,
            ),
            effect(
                "e4",
                "vibe.todo",
                json!({"action": "write", "todos": [{"id": "1", "content": "Fix it", "status": "in_progress", "priority": "high"}]}),
                "completed",
                "",
                1_700_000_004_000,
            ),
        ];
        let dir =
            crate::vibe::fixture::session(&h, "s1", meta_json("s1", "/w", None), 1, &[first], "");
        let v = page("vibe", &dir, tail()).unwrap();
        assert_eq!(v["model"], "mistral-medium-3.5");
        let e = entries(&v);
        assert_eq!(kinds(&v), ["user", "tool", "tool", "tool", "todos"]);
        assert_eq!(e[0]["text"], "Fix the login bug");
        assert_eq!(e[0]["at"], format_rfc3339(1_700_000_000_000));
        assert_eq!(find(&v, "e1")["tool"]["label"], "Read auth.rs");
        assert_eq!(find(&v, "e1")["tool"]["output"], "fn login()");
        assert_eq!(
            find(&v, "e2")["tool"]["diff"],
            json!({"old": "a", "new": "b"})
        );
        assert_eq!(find(&v, "e3")["tool"]["state"], "failed");
        assert_eq!(find(&v, "e4")["todos"][0]["text"], "Fix it");
        let c = v["cursor"].as_u64().unwrap();
        let seg = dir.join("journal").join(format!("{:016}.jsonl", 101));
        append(
            &seg,
            &delta(
                1,
                json!([{"op": "append_entry", "entry": effect("e5", "file_system.bash", json!({"command": "ls"}), "running", "", 1_700_000_005_000)}]),
            ),
        );
        append(
            &seg,
            &delta(
                2,
                json!([{"op": "replace_entry", "id": "e5", "entry": effect("e5", "file_system.bash", json!({"command": "ls"}), "completed", "a.txt", 1_700_000_005_000)}, {"op": "append_entry", "entry": message("m2", "assistant", "Fixed.", 1_700_000_006_000)}]),
            ),
        );
        let v = page("vibe", &dir, after(c)).unwrap();
        assert_eq!(find(&v, "e5")["tool"]["state"], "done");
        assert_eq!(find(&v, "m2")["text"], "Fixed.");
        assert_eq!(find(&v, "m2")["model"], "mistral-medium-3.5");
        append(&seg, &delta(3, json!([{"op": "remove_entry", "id": "e5"}])));
        let v = page("vibe", &dir, tail()).unwrap();
        assert!(entries(&v).iter().all(|e| e["id"] != "e5"));
    }

    #[test]
    fn gemini_sessions() {
        let d = tmp("gemini");
        let p = copy("activity/gemini.jsonl", &d);
        let v = page("gemini", &p, tail()).unwrap();
        assert_eq!(v["model"], "gemini-2.5-pro");
        let e = entries(&v);
        assert_eq!(e[0]["kind"], "user");
        assert_eq!(e[0]["text"], "Fix the login bug in src/auth.rs");
        assert_eq!(find(&v, "read_file_1")["tool"]["state"], "done");
        assert_eq!(find(&v, "run_shell_command_2")["tool"]["state"], "failed");
        assert_eq!(
            find(&v, "replace_3")["tool"]["diff"],
            json!({"old": "a", "new": "b"})
        );
        assert_eq!(find(&v, "write_todos_4")["kind"], "todos");
        let ids: Vec<&str> = e.iter().map(|x| x["id"].as_str().unwrap()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());

        let j = copy("activity/gemini.json", &d);
        let v = page("gemini", &j, tail()).unwrap();
        assert_eq!(kinds(&v), ["user", "assistant", "tool"]);
        let c = v["cursor"].as_u64().unwrap();
        let body = std::fs::read_to_string(&j)
            .unwrap()
            .replace("\"Done.\"", "\"Done, all good.\"");
        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(&j, format!("{body}\n")).unwrap();
        let v = page("gemini", &j, after(c)).unwrap();
        assert_eq!(kinds(&v), ["assistant"]);
        assert_eq!(entries(&v)[0]["text"], "Done, all good.");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn dropped_chats_still_announce_growth() {
        let d = tmp("idle");
        let p = d.join("s.jsonl");
        let line = |id: &str| {
            format!(
                "{}\n",
                json!({"type": "user", "uuid": id, "message": {"content": id}})
            )
        };
        std::fs::write(&p, line("u1")).unwrap();
        let c = page("claude", &p, tail()).unwrap()["cursor"]
            .as_u64()
            .unwrap();
        slots().lock().unwrap().chats.get_mut(&p).unwrap().used_ms -= IDLE_MS;
        append(&p, &line("u2"));
        assert_eq!(changed(&p), Some(0));
        assert_eq!(changed(&p), Some(0));
        let v = page("claude", &p, after(c)).unwrap();
        assert_eq!(v["reset"], true);
        assert_eq!(kinds(&v), ["user", "user"]);
        assert!(changed(&p).unwrap() > 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn agents_fixture(tag: &str) -> (PathBuf, PathBuf) {
        let d = tmp(tag);
        let p = copy("chat/agents.jsonl", &d);
        let sub = d.join("agents/subagents");
        std::fs::create_dir_all(&sub).unwrap();
        for e in std::fs::read_dir(fixture("chat/agents/subagents")).unwrap() {
            let e = e.unwrap();
            fresh_copy(&e.path(), &sub.join(e.file_name()));
        }
        (d, p)
    }

    #[test]
    fn subagents_summary() {
        let (d, p) = agents_fixture("agents");
        let v = page("claude", &p, tail()).unwrap();
        assert_eq!(kinds(&v), ["user", "tool", "tool", "tool", "tool", "tool"]);
        let bg1 = &find(&v, "toolu_bg1")["tool"];
        assert_eq!(bg1["state"], "done");
        assert_eq!(bg1["output"], "**Parser**\n- lives in src/chat.rs");
        let a = &bg1["agent"];
        assert_eq!(a["agentId"], "a111");
        assert_eq!(a["type"], "Explore");
        assert_eq!(a["description"], "Map the parser");
        assert_eq!(a["state"], "done");
        assert_eq!(a["background"], true);
        assert_eq!(a["tokens"], 76606);
        assert_eq!(a["toolCount"], 24);
        assert_eq!(a["startedAt"], "2026-10-29T10:00:01Z");
        assert_eq!(a["endedAt"], "2026-10-29T10:02:10Z");
        let bg2 = &find(&v, "toolu_bg2")["tool"];
        assert_eq!(bg2["state"], "running");
        assert!(bg2.get("output").is_none());
        assert_eq!(bg2["agent"]["state"], "running");
        assert_eq!(bg2["agent"]["toolCount"], 2);
        assert_eq!(bg2["agent"]["tokens"], 5243);
        assert_eq!(bg2["agent"]["lastActivity"], "Read lib.rs");
        assert_eq!(bg2["agent"]["endedAt"], Value::Null);
        let bg3 = &find(&v, "toolu_bg3")["tool"];
        assert_eq!(
            (bg3["state"].as_str(), bg3["agent"]["state"].as_str()),
            (Some("failed"), Some("failed"))
        );
        let bg4 = &find(&v, "toolu_bg4")["tool"];
        assert_eq!(
            (bg4["state"].as_str(), bg4["agent"]["state"].as_str()),
            (Some("done"), Some("stopped"))
        );
        let fg = &find(&v, "toolu_fg1")["tool"]["agent"];
        assert_eq!(fg["agentId"], "a555");
        assert_eq!(fg["background"], false);
        assert_eq!(fg["state"], "running");
        assert_eq!(fg["lastActivity"], "resize(");

        let c = v["cursor"].as_u64().unwrap();
        let s0 = agents_stamp(&p);
        assert!(s0 > 0);
        assert_eq!(changed(&p), Some(c));
        let sub = d.join("agents/subagents/agent-a222.jsonl");
        append(
            &sub,
            &format!(
                "{}\n",
                json!({"isSidechain": true, "type": "assistant", "uuid": "s5", "message": {"id": "m5", "content": [{"type": "tool_use", "id": "t23", "name": "Edit", "input": {"file_path": "/w/src/chat.rs", "old_string": "a", "new_string": "b"}}], "usage": {"input_tokens": 1, "output_tokens": 1, "cache_read_input_tokens": 6000, "cache_creation_input_tokens": 0}}})
            ),
        );
        assert_ne!(agents_stamp(&p), s0);
        let c2 = changed(&p).unwrap();
        assert!(c2 > c);
        let u = page("claude", &p, after(c)).unwrap();
        let ids: Vec<&str> = entries(&u)
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["toolu_bg2"]);
        let a = &find(&u, "toolu_bg2")["tool"]["agent"];
        assert_eq!(a["lastActivity"], "Edit chat.rs");
        assert_eq!(a["toolCount"], 3);
        assert_eq!(a["tokens"], 6002);

        let line = |v: Value| format!("{v}\n");
        append(
            &p,
            &line(
                json!({"type": "user", "uuid": "n9", "timestamp": "2026-10-29T10:05:00Z", "message": {"content": note("toolu_bg2", "completed")}}),
            ),
        );
        append(
            &p,
            &line(
                json!({"type": "attachment", "uuid": "h9", "timestamp": "2026-10-29T10:05:00Z", "attachment": {"type": "queued_command", "prompt": "<agent-message from=\"a222\">\n[Subagent hand-back] Model output. The report follows:\n  Tests pass\n  - 12 ok\n</agent-message>"}}),
            ),
        );
        append(
            &p,
            &line(
                json!({"type": "assistant", "uuid": "a9", "timestamp": "2026-10-29T10:06:00Z", "message": {"id": "m9", "content": [{"type": "tool_use", "id": "toolu_sm", "name": "SendMessage", "input": {"to": "a111", "message": "more"}}]}}),
            ),
        );
        append(
            &p,
            &line(
                json!({"type": "assistant", "uuid": "a10", "timestamp": "2020-01-01T00:00:00Z", "message": {"id": "m10", "content": [{"type": "tool_use", "id": "toolu_old", "name": "Agent", "input": {"description": "Old", "prompt": "x", "run_in_background": true}}]}}),
            ),
        );
        let v = page("claude", &p, tail()).unwrap();
        assert_eq!(find(&v, "toolu_bg2")["tool"]["agent"]["state"], "done");
        assert_eq!(
            find(&v, "toolu_bg2")["tool"]["output"],
            "Tests pass\n- 12 ok"
        );
        assert_eq!(
            find(&v, "toolu_bg2")["tool"]["agent"]["endedAt"],
            "2026-10-29T10:05:00Z"
        );
        assert_eq!(find(&v, "toolu_bg1")["tool"]["agent"]["state"], "running");
        assert_eq!(
            find(&v, "toolu_bg1")["tool"]["agent"]["endedAt"],
            Value::Null
        );
        assert_eq!(find(&v, "toolu_old")["tool"]["agent"]["state"], "stopped");
        append(
            &p,
            &line(
                json!({"type": "user", "uuid": "n10", "timestamp": "2026-10-29T10:07:00Z", "message": {"content": "<task-notification>\n<task-id>a111</task-id>\n<status>completed</status>\n<result>Interim</result>\n</task-notification>"}}),
            ),
        );
        append(
            &p,
            &line(
                json!({"type": "assistant", "uuid": "a11", "timestamp": "2026-10-29T10:08:00Z", "message": {"id": "m11", "content": [{"type": "tool_use", "id": "toolu_stop", "name": "TaskStop", "input": {"task_id": "a555"}}]}}),
            ),
        );
        let v = page("claude", &p, tail()).unwrap();
        let bg1 = &find(&v, "toolu_bg1")["tool"];
        assert_eq!(bg1["agent"]["state"], "done");
        assert_eq!(bg1["agent"]["endedAt"], "2026-10-29T10:07:00Z");
        assert_eq!(bg1["output"], "**Parser**\n- lives in src/chat.rs");
        assert_eq!(find(&v, "toolu_fg1")["tool"]["agent"]["state"], "stopped");
        assert_eq!(agents_stamp(&p), 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn quiet_sub(
        d: &Path,
        name: &str,
        bg: bool,
        sub_lines: &[Value],
        main_after: &[Value],
    ) -> PathBuf {
        let p = d.join(format!("{name}.jsonl"));
        let tid = format!("toolu_{name}");
        let aid = format!("a{name}");
        let mut main = vec![
            json!({"type": "user", "uuid": "u1", "timestamp": "2020-01-01T00:00:00Z", "message": {"content": "go"}}),
            json!({"type": "assistant", "uuid": "a1", "timestamp": "2020-01-01T00:00:01Z", "message": {"id": "m1", "content": [{"type": "tool_use", "id": tid, "name": "Agent", "input": {"description": name, "prompt": "x", "run_in_background": bg}}]}}),
        ];
        if bg {
            main.push(json!({"type": "user", "uuid": "r1", "timestamp": "2020-01-01T00:00:02Z", "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": "launched"}]}, "toolUseResult": {"status": "async_launched", "agentId": aid}}));
        }
        main.extend(main_after.iter().cloned());
        let text: String = main.iter().map(|v| format!("{v}\n")).collect();
        std::fs::write(&p, text).unwrap();
        let sub = d.join(name).join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join(format!("agent-{aid}.meta.json")),
            json!({"toolUseId": tid}).to_string(),
        )
        .unwrap();
        let f = sub.join(format!("agent-{aid}.jsonl"));
        let text: String = sub_lines.iter().map(|v| format!("{v}\n")).collect();
        std::fs::write(&f, text).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
        std::fs::File::options()
            .append(true)
            .open(&f)
            .unwrap()
            .set_modified(old)
            .unwrap();
        p
    }

    fn sub_state(p: &Path, live: Option<bool>) -> String {
        let q = Query { live, ..tail() };
        let v = page("claude", p, q).unwrap();
        let name = p.file_stem().unwrap().to_string_lossy();
        find(&v, &format!("toolu_{name}"))["tool"]["agent"]["state"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn long_foreground_subagents_stay_running_while_the_session_lives() {
        let d = tmp("stale");
        let bash = json!({"type": "assistant", "uuid": "s1", "message": {"id": "sm1", "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "make test"}}]}});
        let answered = json!({"type": "user", "uuid": "s2", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}});
        let said = json!({"type": "assistant", "uuid": "s3", "message": {"id": "sm2", "content": [{"type": "text", "text": "done"}]}});

        let fg = quiet_sub(&d, "fg", false, std::slice::from_ref(&bash), &[]);
        assert_eq!(sub_state(&fg, Some(true)), "running");
        assert_eq!(sub_state(&fg, None), "running");
        assert_eq!(sub_state(&fg, Some(false)), "stopped");

        let unknown = quiet_sub(&d, "unknown", false, std::slice::from_ref(&bash), &[]);
        assert_eq!(sub_state(&unknown, None), "stopped");

        let moved = json!({"type": "assistant", "uuid": "a2", "timestamp": "2020-01-01T00:10:00Z", "message": {"id": "m2", "content": [{"type": "text", "text": "next"}]}});
        let gone = quiet_sub(&d, "gone", false, std::slice::from_ref(&bash), &[moved]);
        assert_eq!(sub_state(&gone, Some(true)), "stopped");

        let bgwait = quiet_sub(&d, "bgwait", true, std::slice::from_ref(&bash), &[]);
        assert_eq!(sub_state(&bgwait, Some(true)), "running");

        let bgidle = quiet_sub(&d, "bgidle", true, &[bash, answered, said], &[]);
        assert_eq!(sub_state(&bgidle, Some(true)), "stopped");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn agents_stamp_does_not_wait_for_the_chat_cache() {
        let p = PathBuf::from(format!(
            "/nonexistent/sb-stamp-{}.jsonl",
            std::process::id()
        ));
        running_index()
            .lock()
            .unwrap()
            .insert(p.clone(), vec![std::env::temp_dir()]);
        let held = slots().lock().unwrap_or_else(|e| e.into_inner());
        let (tx, rx) = std::sync::mpsc::channel();
        let q = p.clone();
        std::thread::spawn(move || tx.send(agents_stamp(&q)).unwrap());
        let got = rx.recv_timeout(std::time::Duration::from_secs(2));
        drop(held);
        assert!(got.is_ok_and(|s| s > 0));
        slots()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drop_chat(&p);
        assert_eq!(agents_stamp(&p), 0);
    }

    #[test]
    fn huge_subagent_usage_saturates() {
        let mut s = Sub::default();
        s.scan(
            &json!({"type": "assistant", "message": {"content": [], "usage": {
            "input_tokens": u64::MAX, "output_tokens": u64::MAX,
            "cache_read_input_tokens": 1, "cache_creation_input_tokens": 1}}}),
        );
        assert_eq!(s.tokens, u64::MAX);
    }

    fn note(tid: &str, status: &str) -> String {
        format!("<task-notification>\n<tool-use-id>{tid}</tool-use-id>\n<status>{status}</status>\n<usage><subagent_tokens>9000</subagent_tokens><tool_uses>4</tool_uses></usage>\n</task-notification>")
    }

    #[test]
    fn subagent_transcript_pages() {
        let (d, p) = agents_fixture("agentpage");
        let v = agent_page(&p, "a222", tail()).unwrap();
        assert_eq!(v["agentId"], "a222");
        assert_eq!(kinds(&v), ["user", "assistant", "tool", "tool"]);
        assert_eq!(entries(&v)[0]["text"], "Run them");
        assert_eq!(find(&v, "t21")["tool"]["state"], "done");
        assert_eq!(find(&v, "t22")["tool"]["state"], "running");
        let h = agent_page(&p, "a111", tail()).unwrap();
        assert_eq!(kinds(&h), ["user", "tool", "assistant"]);
        assert_eq!(entries(&h)[2]["text"], "**Parser**\n- lives in src/chat.rs");
        let c = v["cursor"].as_u64().unwrap();
        append(
            &d.join("agents/subagents/agent-a222.jsonl"),
            &format!(
                "{}\n",
                json!({"isSidechain": true, "type": "user", "uuid": "s6", "message": {"content": [{"type": "tool_result", "tool_use_id": "t22", "content": "x"}]}})
            ),
        );
        let u = agent_page(&p, "a222", after(c)).unwrap();
        assert_eq!(entries(&u).len(), 1);
        assert_eq!(find(&u, "t22")["tool"]["state"], "done");
        assert!(agent_page(&p, "../x", tail()).is_none());
        assert!(agent_page(&p, "", tail()).is_none());
        assert!(agent_page(&p, "a999", tail()).is_none());
        let main = page("claude", &p, tail()).unwrap();
        assert!(!entries(&main).iter().any(|e| e["text"] == "Run them"));
        std::fs::remove_dir_all(&d).unwrap();
    }

    // Given a cold transcript whose read blocks (a FIFO with no writer yet)
    // When another chat is paged meanwhile
    // Then it is served without waiting for the slow cold parse
    #[cfg(unix)]
    #[test]
    fn cold_parse_does_not_block_other_chats() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::OpenOptionsExt;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{mpsc, Arc};
        use std::time::Duration;
        let d = tmp("cold");
        let slow = d.join("slow.json");
        let path = CString::new(slow.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        let other = d.join("other.jsonl");
        std::fs::write(
            &other,
            format!(
                "{}\n",
                json!({"type": "user", "uuid": "u1", "message": {"content": "hi"}})
            ),
        )
        .unwrap();
        let s = slow.clone();
        let a = std::thread::spawn(move || page("gemini", &s, tail()));
        std::thread::sleep(Duration::from_millis(200));
        let (tx, rx) = mpsc::channel();
        let o = other.clone();
        let b = std::thread::spawn(move || tx.send(page("claude", &o, tail())).unwrap());
        let got = rx.recv_timeout(Duration::from_secs(3));
        let stop = Arc::new(AtomicBool::new(false));
        let (w, halt) = (slow.clone(), stop.clone());
        let feeder = std::thread::spawn(move || {
            while !halt.load(Ordering::Relaxed) {
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&w)
                {
                    let _ = f.write_all(br#"{"messages": []}"#);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        a.join().unwrap();
        b.join().unwrap();
        stop.store(true, Ordering::Relaxed);
        feeder.join().unwrap();
        let got = got.expect("page() on another chat waited for the cold parse");
        assert_eq!(kinds(&got.unwrap()), ["user"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn probe_until(what: &str, done: impl Fn(&Probe) -> bool) {
        let timed_out = probe()
            .1
            .wait_timeout_while(probe_lock(), std::time::Duration::from_secs(10), |p| {
                !done(p)
            })
            .unwrap()
            .1
            .timed_out();
        assert!(!timed_out, "timed out waiting for {what}");
    }

    // Given a cold transcript, opened by two callers while its parse is still running
    // When the second caller arrives during the first one's parse
    // Then it waits for that parse instead of starting its own, and the watcher stays quiet
    #[test]
    fn concurrent_cold_pages_parse_once() {
        let d = tmp("twice");
        let slow = d.join("slow.json");
        std::fs::write(&slow, br#"{"messages": []}"#).unwrap();
        slots().lock().unwrap().dropped.insert(slow.clone());
        probe_lock().held.insert(slow.clone());
        let s = slow.clone();
        let a = std::thread::spawn(move || page("gemini", &s, tail()));
        probe_until("the first cold parse", |p| p.parses.contains_key(&slow));
        let s = slow.clone();
        let b = std::thread::spawn(move || page("gemini", &s, tail()));
        probe_until("the second caller to wait", |p| p.waits.contains_key(&slow));
        assert_eq!(
            changed(&slow),
            None,
            "the watcher announced a chat being loaded"
        );
        probe_lock().held.remove(&slow);
        probe().1.notify_all();
        let (ra, rb) = (a.join().unwrap(), b.join().unwrap());
        assert!(ra.is_some() && rb.is_some());
        assert_eq!(cold_parses(&slow), 1);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn unsupported_and_unopened() {
        let d = tmp("none");
        let p = d.join("x.jsonl");
        std::fs::write(&p, "").unwrap();
        assert!(page("aider", &p, tail()).is_none());
        assert!(changed(&p).is_none());
        assert!(page("claude", &d.join("missing.jsonl"), tail()).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn custom_commands() {
        let d = tmp("cmds");
        let claude = d.join("claude");
        let codex = d.join("codex");
        let proj = d.join("proj");
        std::fs::create_dir_all(claude.join("commands/git")).unwrap();
        std::fs::create_dir_all(proj.join(".claude/commands")).unwrap();
        std::fs::create_dir_all(codex.join("prompts")).unwrap();
        std::fs::write(
            claude.join("commands/review-pr.md"),
            "---\ndescription: \"Review a PR\"\nallowed-tools: Bash\n---\nBody",
        )
        .unwrap();
        std::fs::write(claude.join("commands/git/sync.md"), "\n# Sync the branch\n").unwrap();
        std::fs::write(claude.join("commands/notes.txt"), "x").unwrap();
        std::fs::write(proj.join(".claude/commands/review-pr.md"), "Project review").unwrap();
        std::fs::write(proj.join(".claude/commands/ship.md"), "Ship it").unwrap();
        std::fs::write(codex.join("prompts/plan.md"), "Plan the work").unwrap();
        let v = commands("claude", &claude, &codex, &proj.to_string_lossy());
        assert_eq!(
            v["commands"],
            json!([
                {"name": "/review-pr", "description": "Project review", "source": "project"},
                {"name": "/ship", "description": "Ship it", "source": "project"},
                {"name": "/git:sync", "description": "Sync the branch", "source": "user"},
            ])
        );
        let v = commands("codex", &claude, &codex, "");
        assert_eq!(
            v["commands"],
            json!([{"name": "/prompts:plan", "description": "Plan the work", "source": "user"}])
        );
        assert_eq!(
            commands("gemini", &claude, &codex, "")["commands"],
            json!([])
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}
