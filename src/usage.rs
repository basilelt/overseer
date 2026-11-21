use crate::activity::read_lines;
use crate::agents::Transcript;
use crate::claude_limits::{outcome, read_creds, Auth, Outcome, Token, ACTIVE_MS, REFRESH_MS};
use crate::http::err;
use crate::util::{format_rfc3339, parse_rfc3339};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub const BUCKET_MS: i64 = 900_000;
const DAY_MS: i64 = 86_400_000;
const KEEP_MS: i64 = 31 * DAY_MS;
const IDLE_GAP_MS: i64 = 300_000;
const FULL_SWEEP_MS: i64 = 60_000;
const HOT_MS: i64 = 2 * 3_600_000;
const DEMAND_MS: i64 = 15 * 60_000;
const DEBOUNCE_MS: u64 = 2_000;
const LIMIT_HIT_MS: i64 = 5 * 3_600_000;

pub struct Price {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub write_5m: f64,
    pub write_1h: f64,
}

const fn p(input: f64, output: f64, cache_read: f64, write_5m: f64, write_1h: f64) -> Price {
    Price {
        input,
        output,
        cache_read,
        write_5m,
        write_1h,
    }
}

pub const PRICES_AS_OF: &str = "2026-11-11";

pub const PRICES: &[(&str, Price)] = &[
    ("claude-fable-5-2", p(8.0, 40.0, 0.2, 10.0, 16.0)),
    ("claude-mythos-5-2", p(8.0, 40.0, 0.2, 10.0, 16.0)),
    ("claude-fable-", p(10.0, 50.0, 0.25, 12.5, 20.0)),
    ("claude-mythos-", p(10.0, 50.0, 0.25, 12.5, 20.0)),
    ("claude-opus-5-5", p(4.0, 20.0, 0.2, 5.0, 8.0)),
    ("claude-opus-5", p(5.0, 25.0, 0.5, 6.25, 10.0)),
    ("claude-opus-4-5", p(5.0, 25.0, 0.5, 6.25, 10.0)),
    ("claude-opus-4-6", p(5.0, 25.0, 0.5, 6.25, 10.0)),
    ("claude-opus-4-7", p(5.0, 25.0, 0.5, 6.25, 10.0)),
    ("claude-opus-4-8", p(5.0, 25.0, 0.5, 6.25, 10.0)),
    ("claude-opus-4", p(15.0, 75.0, 1.5, 18.75, 30.0)),
    ("claude-sonnet-5-5", p(2.0, 10.0, 0.1, 2.5, 4.0)),
    ("claude-sonnet-5", p(2.0, 10.0, 0.2, 2.5, 4.0)),
    ("claude-sonnet-4", p(3.0, 15.0, 0.3, 3.75, 6.0)),
    ("claude-3-7-sonnet", p(3.0, 15.0, 0.3, 3.75, 6.0)),
    ("claude-haiku-5-5", p(0.1, 0.5, 0.01, 0.125, 0.2)),
    ("claude-haiku-4", p(1.0, 5.0, 0.1, 1.25, 2.0)),
    ("claude-3-5-haiku", p(0.8, 4.0, 0.08, 1.0, 1.6)),
    ("gpt-5.5", p(5.0, 30.0, 0.5, 0.0, 0.0)),
    ("gpt-5.4-mini", p(0.75, 4.5, 0.075, 0.0, 0.0)),
    ("gpt-5.4", p(2.5, 15.0, 0.25, 0.0, 0.0)),
    ("gpt-5.3-codex", p(1.75, 14.0, 0.175, 0.0, 0.0)),
    ("gpt-5.2", p(1.75, 14.0, 0.175, 0.0, 0.0)),
    ("gpt-5-mini", p(0.25, 2.0, 0.025, 0.0, 0.0)),
    ("gpt-5-nano", p(0.05, 0.4, 0.005, 0.0, 0.0)),
    ("gpt-5", p(1.25, 10.0, 0.125, 0.0, 0.0)),
    ("codex-mini", p(1.5, 6.0, 0.375, 0.0, 0.0)),
    ("o4-mini", p(1.1, 4.4, 0.275, 0.0, 0.0)),
    ("o3", p(2.0, 8.0, 0.5, 0.0, 0.0)),
    ("gpt-4.1", p(2.0, 8.0, 0.5, 0.0, 0.0)),
    ("gemini-3-pro", p(2.0, 12.0, 0.2, 0.0, 0.0)),
    ("gemini-3-flash", p(0.5, 3.0, 0.05, 0.0, 0.0)),
    ("gemini-2.5-pro", p(1.25, 10.0, 0.31, 0.0, 0.0)),
    ("gemini-2.5-flash-lite", p(0.1, 0.4, 0.025, 0.0, 0.0)),
    ("gemini-2.5-flash", p(0.3, 2.5, 0.075, 0.0, 0.0)),
];

pub fn price(model: &str) -> Option<&'static Price> {
    let m = model.trim().to_ascii_lowercase();
    let m = m.rsplit('/').next().unwrap_or(&m);
    PRICES
        .iter()
        .filter(|(k, _)| {
            m.strip_prefix(k)
                .is_some_and(|rest| k.ends_with('-') || rest.is_empty() || rest.starts_with('-'))
        })
        .max_by_key(|(k, _)| k.len())
        .map(|(_, p)| p)
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cache_write_1h: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }

    pub(crate) fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.cache_write_1h += o.cache_write_1h;
    }

    fn minus(&self, o: &Tokens) -> Tokens {
        Tokens {
            input: self.input.saturating_sub(o.input),
            output: self.output.saturating_sub(o.output),
            cache_read: self.cache_read.saturating_sub(o.cache_read),
            cache_write: self.cache_write.saturating_sub(o.cache_write),
            cache_write_1h: self.cache_write_1h.saturating_sub(o.cache_write_1h),
        }
    }

    fn is_zero(&self) -> bool {
        self.total() == 0
    }
}

pub fn cost(model: &str, t: &Tokens) -> Option<f64> {
    let p = price(model)?;
    let w1h = t.cache_write_1h.min(t.cache_write);
    let w5m = t.cache_write - w1h;
    Some(
        (t.input as f64 * p.input
            + t.output as f64 * p.output
            + t.cache_read as f64 * p.cache_read
            + w5m as f64 * p.write_5m
            + w1h as f64 * p.write_1h)
            / 1e6,
    )
}

#[derive(Default, Clone, Debug, PartialEq)]
struct Agg {
    tokens: Tokens,
    messages: u64,
    active_ms: i64,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Window {
    pub key: String,
    pub minutes: i64,
    pub used: f64,
    pub resets_at: i64,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Limits {
    pub at: i64,
    pub windows: Vec<Window>,
    pub plan: Option<String>,
    #[serde(default)]
    pub credits: Option<Credits>,
    #[serde(default)]
    pub reached: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Credits {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<f64>,
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
struct LimitHit {
    at: i64,
    text: String,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Cursor {
    offset: u64,
    len: u64,
    mtime: i64,
    last_id: String,
    last: Tokens,
    model: String,
    prev_total: Tokens,
    last_ts: i64,
}

struct Tracked {
    agent: &'static str,
    session: String,
    cwd: String,
    sub: bool,
    cursor: Cursor,
}

#[derive(Default)]
struct Batch {
    buckets: HashMap<(i64, String), Agg>,
    limits: Option<Limits>,
    hit: Option<LimitHit>,
    ok_at: i64,
    cwd: Option<String>,
}

impl Batch {
    fn add(&mut self, ts: i64, model: &str, t: &Tokens, msg: bool, active: i64) {
        let a = self
            .buckets
            .entry((ts.div_euclid(BUCKET_MS), model.to_string()))
            .or_default();
        a.tokens.add(t);
        a.messages += msg as u64;
        a.active_ms += active;
    }
}

fn contains(line: &[u8], needle: &str) -> bool {
    std::str::from_utf8(line).is_ok_and(|s| s.contains(needle))
}

fn ts_of(v: &Value) -> Option<i64> {
    v["timestamp"].as_str().and_then(parse_rfc3339)
}

fn active(c: &mut Cursor, ts: i64, sub: bool) -> i64 {
    let gap = ts - c.last_ts;
    let a = if !sub && c.last_ts > 0 && gap > 0 && gap <= IDLE_GAP_MS {
        gap
    } else {
        0
    };
    c.last_ts = c.last_ts.max(ts);
    a
}

#[derive(Deserialize)]
struct CUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_creation: Option<CCreation>,
}

#[derive(Deserialize)]
struct CCreation {
    #[serde(default)]
    ephemeral_1h_input_tokens: u64,
}

#[derive(Deserialize)]
struct CMsg {
    id: Option<String>,
    model: Option<String>,
    usage: Option<CUsage>,
}

#[derive(Deserialize)]
struct CLine {
    timestamp: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    message: Option<CMsg>,
}

fn claude_line(c: &mut Cursor, sub: bool, line: &[u8], b: &mut Batch) {
    let Ok(s) = std::str::from_utf8(line) else {
        return;
    };
    if s.contains("\"error\":\"rate_limit\"") {
        if let Ok(v) = serde_json::from_str::<Value>(s) {
            if let Some(ts) = ts_of(&v) {
                let text = v["message"]["content"][0]["text"]
                    .as_str()
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(200)
                    .collect();
                if b.hit.as_ref().is_none_or(|h| ts >= h.at) {
                    b.hit = Some(LimitHit { at: ts, text });
                }
            }
        }
        return;
    }
    let wants_cwd = b.cwd.is_none() && s.contains("\"cwd\"");
    if !s.contains("\"usage\"") && !wants_cwd {
        return;
    }
    let Ok(l) = serde_json::from_str::<CLine>(s) else {
        return;
    };
    if wants_cwd {
        b.cwd = l.cwd.filter(|c| !c.is_empty());
    }
    let Some(m) = l.message else { return };
    let (Some(u), Some(model)) = (m.usage, m.model) else {
        return;
    };
    if model.starts_with('<') {
        return;
    }
    let Some(ts) = l.timestamp.as_deref().and_then(parse_rfc3339) else {
        return;
    };
    let t = Tokens {
        input: u.input_tokens,
        output: u.output_tokens,
        cache_read: u.cache_read_input_tokens,
        cache_write: u.cache_creation_input_tokens,
        cache_write_1h: u.cache_creation.map_or(0, |c| c.ephemeral_1h_input_tokens),
    };
    let id = m.id.or(l.request_id).unwrap_or_default();
    let same = !id.is_empty() && id == c.last_id;
    let delta = if same { t.minus(&c.last) } else { t };
    if same {
        c.last = Tokens {
            input: c.last.input.max(t.input),
            output: c.last.output.max(t.output),
            cache_read: c.last.cache_read.max(t.cache_read),
            cache_write: c.last.cache_write.max(t.cache_write),
            cache_write_1h: c.last.cache_write_1h.max(t.cache_write_1h),
        };
    } else {
        c.last_id = id;
        c.last = t;
    }
    b.ok_at = b.ok_at.max(ts);
    if same && delta.is_zero() {
        return;
    }
    let a = if same { 0 } else { active(c, ts, sub) };
    b.add(ts, &model, &delta, !same, a);
}

fn codex_tokens(v: &Value) -> Tokens {
    let n = |k: &str| v[k].as_u64().unwrap_or(0);
    let cached = n("cached_input_tokens");
    Tokens {
        input: n("input_tokens").saturating_sub(cached),
        output: n("output_tokens"),
        cache_read: cached,
        ..Tokens::default()
    }
}

fn window_key(minutes: i64) -> String {
    const MONTH_MINUTES: i64 = 30 * 24 * 60;
    match minutes {
        300 => "5h".into(),
        10_080 => "wk".into(),
        m if m > 0 && (m - MONTH_MINUTES).abs() <= MONTH_MINUTES / 20 => "mo".into(),
        m if m > 0 && m % 1440 == 0 => format!("{}d", m / 1440),
        m if m > 0 && m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

fn codex_limits(rl: &Value, ts: i64) -> Option<Limits> {
    let windows: Vec<Window> = ["primary", "secondary"]
        .iter()
        .filter_map(|k| {
            let w = &rl[*k];
            let used = w["used_percent"].as_f64()?;
            let minutes = w["window_minutes"].as_i64().unwrap_or(0);
            let resets_at = w["resets_at"]
                .as_i64()
                .map(|s| s * 1000)
                .or_else(|| w["resets_in_seconds"].as_i64().map(|s| ts + s * 1000))
                .unwrap_or(0);
            Some(Window {
                key: window_key(minutes),
                minutes,
                used,
                resets_at,
            })
        })
        .collect();
    (!windows.is_empty()).then(|| Limits {
        at: ts,
        windows,
        plan: rl["plan_type"].as_str().map(str::to_string),
        credits: rl["credits"].is_object().then(|| {
            let c = &rl["credits"];
            Credits {
                has_credits: c["has_credits"].as_bool().unwrap_or(false),
                unlimited: c["unlimited"].as_bool().unwrap_or(false),
                balance: c["balance"]
                    .as_str()
                    .and_then(|b| b.parse().ok())
                    .or_else(|| c["balance"].as_f64()),
            }
        }),
        reached: rl["rate_limit_reached_type"].as_str().map(str::to_string),
    })
}

fn codex_line(c: &mut Cursor, line: &[u8], b: &mut Batch) {
    if !(contains(line, "\"token_count\"")
        || contains(line, "\"turn_context\"")
        || contains(line, "\"session_meta\""))
    {
        return;
    }
    let Ok(v) = serde_json::from_slice::<Value>(line) else {
        return;
    };
    let p = &v["payload"];
    match (v["type"].as_str(), p["type"].as_str()) {
        (Some("session_meta"), _) => {
            if let Some(cwd) = p["cwd"].as_str().filter(|c| !c.is_empty()) {
                b.cwd.get_or_insert_with(|| cwd.to_string());
            }
        }
        (Some("turn_context"), _) => {
            if let Some(m) = p["model"].as_str().filter(|m| !m.is_empty()) {
                c.model = m.to_string();
            }
        }
        (Some("event_msg"), Some("token_count")) => {
            let Some(ts) = ts_of(&v) else { return };
            if let Some(l) = codex_limits(&p["rate_limits"], ts) {
                if b.limits.as_ref().is_none_or(|o| l.at >= o.at) {
                    b.limits = Some(l);
                }
            }
            let total = &p["info"]["total_token_usage"];
            if total.is_null() {
                return;
            }
            let t = codex_tokens(total);
            let delta = if t.input + t.cache_read < c.prev_total.input + c.prev_total.cache_read {
                t
            } else {
                t.minus(&c.prev_total)
            };
            c.prev_total = t;
            if delta.is_zero() {
                return;
            }
            let model = if c.model.is_empty() {
                "codex".to_string()
            } else {
                c.model.clone()
            };
            let a = active(c, ts, false);
            b.add(ts, &model, &delta, true, a);
        }
        _ => {}
    }
}

fn vibe_session(c: &mut Cursor, dir: &Path, fallback_ts: i64, b: &mut Batch) {
    let Some(u) = crate::vibe::usage(dir) else {
        return;
    };
    let cur = Tokens {
        input: u.input.saturating_sub(u.cached),
        output: u.output,
        cache_read: u.cached,
        ..Tokens::default()
    };
    let delta = cur.minus(&c.last);
    c.last = cur;
    if delta.is_zero() {
        return;
    }
    let ts = if u.at > 0 { u.at } else { fallback_ts };
    let model = u.model.unwrap_or_else(|| "mistral".to_string());
    b.add(ts, &model, &delta, false, 0);
}

fn copilot_line(c: &mut Cursor, line: &[u8], b: &mut Batch) {
    let wants_cwd = b.cwd.is_none() && contains(line, "\"session.start\"");
    if !(wants_cwd || contains(line, "\"session.usage_record\"")) {
        return;
    }
    let Ok(v) = serde_json::from_slice::<Value>(line) else {
        return;
    };
    DECODED.fetch_add(1, Ordering::Relaxed);
    match v["type"].as_str() {
        Some("session.start") => {
            if let Some(cwd) = v["data"]["context"]["cwd"]
                .as_str()
                .filter(|c| !c.is_empty())
            {
                b.cwd.get_or_insert_with(|| cwd.to_string());
            }
        }
        Some("session.usage_record") => {
            let u = &v["data"]["usage"];
            let Some(ts) = ts_of(&v) else { return };
            let n = |k: &str| u[k].as_u64().unwrap_or(0);
            let (read, write) = (n("cacheReadTokens"), n("cacheWriteTokens"));
            let t = Tokens {
                input: n("inputTokens").saturating_sub(read + write),
                output: n("outputTokens"),
                cache_read: read,
                cache_write: write,
                ..Tokens::default()
            };
            if t.is_zero() {
                return;
            }
            let model = u["model"].as_str().unwrap_or("copilot");
            let a = active(c, ts, false);
            b.add(ts, model, &t, true, a);
        }
        _ => {}
    }
}

static DECODED: AtomicU64 = AtomicU64::new(0);

pub fn decoded() -> u64 {
    DECODED.load(Ordering::Relaxed)
}

enum Pb<'a> {
    Int(u64),
    Msg(&'a [u8]),
}

fn pb_varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let x = *b.get(*i)?;
        *i += 1;
        v |= u64::from(x & 0x7f) << shift;
        if x < 0x80 {
            return Some(v);
        }
    }
    None
}

fn pb_fields(b: &[u8]) -> Vec<(u64, Pb<'_>)> {
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let Some(k) = pb_varint(b, &mut i) else { break };
        match k & 7 {
            0 => {
                let Some(v) = pb_varint(b, &mut i) else { break };
                out.push((k >> 3, Pb::Int(v)));
            }
            2 => {
                let Some(l) = pb_varint(b, &mut i) else { break };
                let Some(d) = i.checked_add(l as usize).and_then(|e| b.get(i..e)) else {
                    break;
                };
                i += l as usize;
                out.push((k >> 3, Pb::Msg(d)));
            }
            1 => i += 8,
            5 => i += 4,
            _ => break,
        }
    }
    out
}

fn pb_msg(b: &[u8], field: u64) -> Option<&[u8]> {
    pb_fields(b).into_iter().find_map(|(f, v)| match v {
        Pb::Msg(m) if f == field => Some(m),
        _ => None,
    })
}

fn pb_int(b: &[u8], field: u64) -> u64 {
    pb_fields(b)
        .into_iter()
        .find_map(|(f, v)| match v {
            Pb::Int(n) if f == field => Some(n),
            _ => None,
        })
        .unwrap_or(0)
}

fn agy_row(c: &mut Cursor, blob: &[u8], fallback_ts: i64, b: &mut Batch) {
    DECODED.fetch_add(1, Ordering::Relaxed);
    let Some(head) = pb_msg(blob, 1) else { return };
    let Some(stats) = pb_msg(head, 4) else { return };
    let t = Tokens {
        input: pb_int(stats, 2),
        output: pb_int(stats, 3),
        cache_read: pb_int(stats, 5),
        ..Tokens::default()
    };
    if t.is_zero() {
        return;
    }
    let secs = pb_msg(head, 9)
        .and_then(|m| pb_msg(m, 4))
        .map_or(0, |m| pb_int(m, 1));
    let ts = if secs > 0 {
        secs as i64 * 1000
    } else {
        fallback_ts
    };
    let model = pb_msg(head, 19)
        .and_then(|m| std::str::from_utf8(m).ok())
        .filter(|m| !m.is_empty())
        .unwrap_or("antigravity");
    let a = active(c, ts, false);
    b.add(ts, model, &t, true, a);
}

fn agy_conversation(c: &mut Cursor, path: &Path, fallback_ts: i64, b: &mut Batch) {
    let Ok(conn) = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return;
    };
    let _ = conn.busy_timeout(Duration::from_millis(500));
    let Ok(mut st) =
        conn.prepare("SELECT idx, data FROM gen_metadata WHERE idx >= ?1 ORDER BY idx")
    else {
        return;
    };
    let Ok(rows) = st.query_map([c.offset as i64], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
    }) else {
        return;
    };
    for (idx, blob) in rows.flatten() {
        agy_row(c, &blob, fallback_ts, b);
        c.offset = c.offset.max(idx as u64 + 1);
    }
}

struct OcSession {
    id: String,
    cwd: String,
    model: String,
    sub: bool,
    tokens: Tokens,
    updated: i64,
}

fn opencode_model(raw: Option<String>) -> String {
    let Some(raw) = raw.filter(|m| !m.is_empty()) else {
        return "opencode".to_string();
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(Value::Object(o)) => o
            .get("id")
            .and_then(Value::as_str)
            .map_or_else(|| "opencode".to_string(), str::to_string),
        Ok(Value::String(s)) => s,
        _ => raw,
    }
}

fn opencode_sessions(db: &Path) -> Vec<OcSession> {
    let Ok(conn) = Connection::open_with_flags(
        db,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return vec![];
    };
    let _ = conn.busy_timeout(Duration::from_millis(500));
    let Ok(mut st) = conn.prepare(
        "SELECT id, directory, model, parent_id IS NOT NULL, tokens_input, tokens_output,
                tokens_reasoning, tokens_cache_read, tokens_cache_write, time_updated
         FROM session",
    ) else {
        return vec![];
    };
    let Ok(rows) = st.query_map([], |r| {
        let n = |i: usize| r.get::<_, i64>(i).unwrap_or(0).max(0) as u64;
        Ok(OcSession {
            id: r.get(0)?,
            cwd: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            model: opencode_model(r.get::<_, Option<String>>(2).ok().flatten()),
            sub: r.get::<_, i64>(3)? != 0,
            tokens: Tokens {
                input: n(4),
                output: n(5) + n(6),
                cache_read: n(7),
                cache_write: n(8),
                ..Tokens::default()
            },
            updated: r.get(9)?,
        })
    }) else {
        return vec![];
    };
    rows.flatten().collect()
}

fn opencode_session(c: &mut Cursor, s: &OcSession, ts: i64, b: &mut Batch) {
    let delta = s.tokens.minus(&c.last);
    c.last = s.tokens;
    if delta.is_zero() {
        return;
    }
    DECODED.fetch_add(1, Ordering::Relaxed);
    b.add(ts, &s.model, &delta, false, 0);
}

fn gemini_msg(c: &mut Cursor, v: &Value, b: &mut Batch) {
    if v["type"] != "gemini" {
        return;
    }
    let tk = &v["tokens"];
    if !tk.is_object() {
        return;
    }
    let Some(ts) = ts_of(v) else { return };
    let n = |k: &str| tk[k].as_u64().unwrap_or(0);
    let cached = n("cached");
    let t = Tokens {
        input: n("input").saturating_sub(cached) + n("tool"),
        output: n("output") + n("thoughts"),
        cache_read: cached,
        ..Tokens::default()
    };
    let model = v["model"].as_str().unwrap_or("gemini").to_string();
    let id = v["id"].as_str().unwrap_or("").to_string();
    let same = !id.is_empty() && id == c.last_id;
    let delta = if same { t.minus(&c.last) } else { t };
    c.last_id = id;
    c.last = t;
    if delta.is_zero() {
        return;
    }
    let a = if same { 0 } else { active(c, ts, false) };
    b.add(ts, &model, &delta, !same, a);
}

#[derive(Default)]
pub(crate) struct Meter {
    c: Cursor,
}

impl Meter {
    pub(crate) fn line(
        &mut self,
        kind: Option<Transcript>,
        line: &[u8],
    ) -> Vec<(String, Tokens, bool)> {
        let mut b = Batch::default();
        match kind {
            Some(Transcript::Codex) => codex_line(&mut self.c, line, &mut b),
            Some(Transcript::Gemini) => {
                if let Ok(v) = serde_json::from_slice::<Value>(line) {
                    gemini_msg(&mut self.c, &v, &mut b);
                }
            }
            Some(Transcript::Claude) | Some(Transcript::Vibe) | None => {
                if !contains(line, "\"usage\"") {
                    return vec![];
                }
                claude_line(&mut self.c, true, line, &mut b)
            }
        }
        Self::turns(b)
    }

    pub(crate) fn gemini(&mut self, v: &Value) -> Vec<(String, Tokens, bool)> {
        let mut b = Batch::default();
        gemini_msg(&mut self.c, v, &mut b);
        Self::turns(b)
    }

    fn turns(b: Batch) -> Vec<(String, Tokens, bool)> {
        b.buckets
            .into_iter()
            .map(|((_, model), a)| (model, a.tokens, a.messages > 0))
            .collect()
    }
}

fn stamp(p: &Path) -> Option<(i64, u64)> {
    let (m, l) = mtime_len(p)?;
    if p.extension().is_none_or(|e| e != "db") {
        return Some((m, l));
    }
    let mut wal = p.as_os_str().to_os_string();
    wal.push("-wal");
    Some(match mtime_len(Path::new(&wal)) {
        Some((wm, wl)) => (m.max(wm), l + wl),
        None => (m, l),
    })
}

fn mtime_len(p: &Path) -> Option<(i64, u64)> {
    let m = std::fs::metadata(p).ok()?;
    let t = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((t, m.len()))
}

#[derive(Clone)]
pub struct Dirs {
    pub claude: PathBuf,
    pub codex: PathBuf,
    pub gemini: PathBuf,
    pub vibe: PathBuf,
    pub copilot: PathBuf,
    pub opencode: PathBuf,
    pub agy: PathBuf,
}

impl Dirs {
    pub fn from_env(claude: PathBuf, codex: PathBuf, gemini: PathBuf) -> Dirs {
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::home().join(".local/share"));
        Dirs {
            claude,
            codex,
            gemini,
            vibe: crate::vibe::home(),
            copilot: crate::agents::config_dir(crate::agents::COPILOT),
            opencode: data.join("opencode"),
            agy: crate::home().join(".gemini/antigravity-cli"),
        }
    }
}

struct Found {
    path: PathBuf,
    agent: &'static str,
    session: String,
    cwd: String,
    sub: bool,
}

fn walk_jsonl(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() && depth > 0 {
            walk_jsonl(&p, depth - 1, out);
        } else if ft.is_file() && p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

fn discover(d: &Dirs, cutoff: i64) -> Vec<Found> {
    let mut out = vec![];
    let fresh = |p: &Path| mtime_len(p).is_some_and(|(m, _)| m >= cutoff);
    if let Ok(projects) = std::fs::read_dir(crate::claude::projects_dir(&d.claude)) {
        for proj in projects.flatten().filter(|e| e.path().is_dir()) {
            let Ok(rd) = std::fs::read_dir(proj.path()) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let Ok(ft) = e.file_type() else { continue };
                let name = e.file_name().to_string_lossy().to_string();
                if ft.is_file() && name.ends_with(".jsonl") {
                    if fresh(&p) {
                        out.push(Found {
                            session: name.trim_end_matches(".jsonl").to_string(),
                            path: p,
                            agent: "claude",
                            cwd: String::new(),
                            sub: false,
                        });
                    }
                } else if ft.is_dir() {
                    let mut subs = vec![];
                    walk_jsonl(&p.join("subagents"), 3, &mut subs);
                    for s in subs.into_iter().filter(|s| fresh(s)) {
                        out.push(Found {
                            path: s,
                            agent: "claude",
                            session: name.clone(),
                            cwd: String::new(),
                            sub: true,
                        });
                    }
                }
            }
        }
    }
    let mut rollouts = vec![];
    walk_jsonl(&d.codex.join("sessions"), 3, &mut rollouts);
    for p in rollouts.into_iter().filter(|p| fresh(p)) {
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if !name.starts_with("rollout-") {
            continue;
        }
        let session = name
            .get(name.len().saturating_sub(36)..)
            .unwrap_or("")
            .to_string();
        out.push(Found {
            path: p,
            agent: "codex",
            session,
            cwd: String::new(),
            sub: false,
        });
    }
    for s in crate::gemini::sessions(&d.gemini) {
        if s.activity_ms >= cutoff {
            out.push(Found {
                path: s.path,
                agent: "gemini",
                session: s.id,
                cwd: s.cwd.unwrap_or_default(),
                sub: false,
            });
        }
    }
    for s in crate::vibe::sessions(&d.vibe) {
        if s.activity_ms >= cutoff {
            out.push(Found {
                path: s.dir.join("CURRENT"),
                agent: crate::agents::VIBE,
                session: s.id,
                cwd: s.cwd.unwrap_or_default(),
                sub: false,
            });
        }
    }
    if let Ok(rd) = std::fs::read_dir(d.copilot.join("session-state")) {
        for e in rd.flatten() {
            let p = e.path().join("events.jsonl");
            if fresh(&p) {
                out.push(Found {
                    path: p,
                    agent: crate::agents::COPILOT,
                    session: e.file_name().to_string_lossy().to_string(),
                    cwd: String::new(),
                    sub: false,
                });
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(d.agy.join("conversations")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "db")
                && stamp(&p).is_some_and(|(m, _)| m >= cutoff)
            {
                out.push(Found {
                    session: p
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    path: p,
                    agent: "agy",
                    cwd: String::new(),
                    sub: false,
                });
            }
        }
    }
    out
}

fn opencode_dbs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "db")
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("opencode"))
        })
        .collect()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS files (
  path TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  session TEXT NOT NULL,
  cwd TEXT NOT NULL,
  sub INTEGER NOT NULL,
  cursor TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS buckets (
  path TEXT NOT NULL,
  bucket INTEGER NOT NULL,
  model TEXT NOT NULL,
  input INTEGER NOT NULL,
  output INTEGER NOT NULL,
  cache_read INTEGER NOT NULL,
  cache_write INTEGER NOT NULL,
  cache_write_1h INTEGER NOT NULL,
  messages INTEGER NOT NULL,
  active_ms INTEGER NOT NULL,
  PRIMARY KEY (path, bucket, model)
);
CREATE INDEX IF NOT EXISTS buckets_by_time ON buckets (bucket);
CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v TEXT NOT NULL);
";

static OPEN: Mutex<()> = Mutex::new(());

fn open_db(path: &Path) -> Result<Connection, String> {
    use crate::sys::OpenOptionsExt;
    // Switching a fresh file to WAL fails at once with "database is locked" when another connection does the same, busy_timeout does not apply.
    let _open = OPEN.lock().unwrap_or_else(|e| e.into_inner());
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    let c = Connection::open(path).map_err(|e| e.to_string())?;
    c.busy_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    c.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| e.to_string())?;
    c.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    Ok(c)
}

fn kv_get<T: for<'de> Deserialize<'de>>(c: &Connection, k: &str) -> Option<T> {
    let s: String = c
        .query_row("SELECT v FROM kv WHERE k = ?1", [k], |r| r.get(0))
        .optional()
        .ok()??;
    serde_json::from_str(&s).ok()
}

fn kv_set<T: Serialize>(c: &Connection, k: &str, v: &T) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO kv (k, v) VALUES (?1, ?2) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
        params![k, serde_json::to_string(v).unwrap_or_default()],
    )
    .map(|_| ())
}

pub struct Index {
    pub db_path: PathBuf,
    conn: Connection,
    dirs: Dirs,
    files: HashMap<PathBuf, Tracked>,
    db_stamps: HashMap<PathBuf, (i64, u64)>,
    last_full: i64,
    last_prune: i64,
    pub indexed: bool,
}

impl Index {
    pub fn open(db_path: &Path, dirs: Dirs) -> Result<Index, String> {
        let conn = open_db(db_path)?;
        let mut files = HashMap::new();
        {
            let mut st = conn
                .prepare("SELECT path, agent, session, cwd, sub, cursor FROM files")
                .map_err(|e| e.to_string())?;
            let rows = st
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, String>(5)?,
                    ))
                })
                .map_err(|e| e.to_string())?;
            for (path, agent, session, cwd, sub, cursor) in rows.flatten() {
                let Some(agent) = crate::agents::get(&agent)
                    .filter(|a| a.metered)
                    .map(|a| a.id)
                else {
                    continue;
                };
                files.insert(
                    PathBuf::from(path),
                    Tracked {
                        agent,
                        session,
                        cwd,
                        sub: sub != 0,
                        cursor: serde_json::from_str(&cursor).unwrap_or_default(),
                    },
                );
            }
        }
        let indexed = kv_get::<bool>(&conn, "indexed").unwrap_or(false);
        Ok(Index {
            db_path: db_path.to_path_buf(),
            conn,
            dirs,
            files,
            db_stamps: HashMap::new(),
            last_full: 0,
            last_prune: 0,
            indexed,
        })
    }

    pub fn sweep(&mut self, now: i64, full: bool, pace: Option<Duration>) -> Result<bool, String> {
        let cutoff = now - KEEP_MS;
        let mut changed = false;
        let candidates: Vec<(PathBuf, &'static str, String, String, bool)> =
            if full || now - self.last_full >= FULL_SWEEP_MS {
                self.last_full = now;
                discover(&self.dirs, cutoff)
                    .into_iter()
                    .map(|f| (f.path, f.agent, f.session, f.cwd, f.sub))
                    .collect()
            } else {
                self.files
                    .iter()
                    .filter(|(_, t)| t.cursor.mtime >= now - HOT_MS)
                    .map(|(p, t)| (p.clone(), t.agent, t.session.clone(), t.cwd.clone(), t.sub))
                    .collect()
            };
        for (path, agent, session, cwd, sub) in candidates {
            let Some((mtime, len)) = stamp(&path) else {
                continue;
            };
            let known = self.files.get(&path);
            if known.is_some_and(|t| t.cursor.mtime == mtime && t.cursor.len == len) {
                continue;
            }
            if known.is_none() {
                self.files.insert(
                    path.clone(),
                    Tracked {
                        agent,
                        session,
                        cwd,
                        sub,
                        cursor: Cursor::default(),
                    },
                );
            }
            changed |= self.ingest(&path, mtime, len, None)?;
            if let Some(d) = pace {
                std::thread::sleep(d);
            }
        }
        changed |= self.sweep_opencode(now, cutoff, pace)?;
        if full && now - self.last_prune > 3_600_000 {
            self.last_prune = now;
            self.prune(cutoff)?;
        }
        if !self.indexed {
            self.indexed = true;
            let _ = kv_set(&self.conn, "indexed", &true);
        }
        Ok(changed)
    }

    fn sweep_opencode(
        &mut self,
        now: i64,
        cutoff: i64,
        pace: Option<Duration>,
    ) -> Result<bool, String> {
        let mut changed = false;
        for db in opencode_dbs(&self.dirs.opencode) {
            let Some(st) = stamp(&db) else { continue };
            if self.db_stamps.get(&db) == Some(&st) {
                continue;
            }
            for s in opencode_sessions(&db) {
                let path = db.join(format!("#{}", s.id));
                let known = self.files.get(&path);
                if known.is_some_and(|t| t.cursor.last == s.tokens)
                    || (known.is_none() && s.updated < cutoff)
                {
                    continue;
                }
                let ts = if known.is_some() {
                    now
                } else {
                    s.updated.min(now)
                };
                if known.is_none() {
                    self.files.insert(
                        path.clone(),
                        Tracked {
                            agent: crate::agents::OPENCODE,
                            session: s.id.clone(),
                            cwd: s.cwd.clone(),
                            sub: s.sub,
                            cursor: Cursor::default(),
                        },
                    );
                }
                changed |= self.ingest(&path, ts, s.tokens.total(), Some((&s, ts)))?;
                if let Some(d) = pace {
                    std::thread::sleep(d);
                }
            }
            self.db_stamps.insert(db, st);
        }
        Ok(changed)
    }

    fn prune(&mut self, cutoff: i64) -> Result<(), String> {
        let gone: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|(p, t)| {
                let real = if t.agent == crate::agents::OPENCODE {
                    p.parent()
                } else {
                    Some(p.as_path())
                };
                t.cursor.mtime < cutoff || !real.is_some_and(Path::exists)
            })
            .map(|(p, _)| p.clone())
            .collect();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        for p in &gone {
            let s = p.to_string_lossy();
            tx.execute("DELETE FROM files WHERE path = ?1", [&s])
                .map_err(|e| e.to_string())?;
            self.files.remove(p);
        }
        tx.execute(
            "DELETE FROM buckets WHERE bucket < ?1",
            [cutoff.div_euclid(BUCKET_MS)],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM buckets WHERE path NOT IN (SELECT path FROM files)",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    fn ingest(
        &mut self,
        path: &Path,
        mtime: i64,
        len: u64,
        oc: Option<(&OcSession, i64)>,
    ) -> Result<bool, String> {
        let Some(t) = self.files.get_mut(path) else {
            return Ok(false);
        };
        let vibe = t.agent == crate::agents::VIBE;
        let whole = path.extension().is_some_and(|e| e == "json");
        let reset = whole || (t.agent != "agy" && len < t.cursor.offset);
        let mut b = Batch {
            cwd: (!t.cwd.is_empty()).then(|| t.cwd.clone()),
            ..Batch::default()
        };
        let had_cwd = b.cwd.is_some();
        if reset {
            t.cursor = Cursor::default();
        }
        let mut c = std::mem::take(&mut t.cursor);
        if let Some((s, ts)) = oc {
            opencode_session(&mut c, s, ts, &mut b);
        } else if vibe {
            if let Some(dir) = path.parent() {
                vibe_session(&mut c, dir, mtime, &mut b);
            }
        } else if t.agent == "agy" {
            agy_conversation(&mut c, path, mtime, &mut b);
        } else if whole {
            if let Some(v) = std::fs::read(path)
                .ok()
                .and_then(|buf| serde_json::from_slice::<Value>(&buf).ok())
            {
                for m in v["messages"].as_array().into_iter().flatten() {
                    gemini_msg(&mut c, m, &mut b);
                }
            }
            c.offset = len;
        } else {
            let Ok(mut f) = std::fs::File::open(path) else {
                t.cursor = c;
                return Ok(false);
            };
            let (agent, sub) = (t.agent, t.sub);
            let kind = crate::agents::transcript_of(agent);
            let copilot = agent == crate::agents::COPILOT;
            let end = read_lines(&mut f, c.offset, len, |line| match kind {
                _ if copilot => copilot_line(&mut c, line, &mut b),
                Some(Transcript::Codex) => codex_line(&mut c, line, &mut b),
                Some(Transcript::Gemini) => {
                    if let Ok(v) = serde_json::from_slice::<Value>(line) {
                        gemini_msg(&mut c, &v, &mut b);
                    }
                }
                Some(Transcript::Claude) | Some(Transcript::Vibe) | None => {
                    claude_line(&mut c, sub, line, &mut b)
                }
            });
            if let Some(e) = end {
                c.offset = e;
            }
        }
        c.mtime = mtime;
        c.len = len;
        if !had_cwd {
            if let Some(cwd) = &b.cwd {
                t.cwd = cwd.clone();
            }
        }
        t.cursor = c;
        let (agent, session, cwd, sub) = (t.agent, t.session.clone(), t.cwd.clone(), t.sub);
        let cursor = serde_json::to_string(&t.cursor).unwrap_or_default();
        let ps = path.to_string_lossy().to_string();
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        if reset {
            tx.execute("DELETE FROM buckets WHERE path = ?1", [&ps])
                .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT INTO files (path, agent, session, cwd, sub, cursor) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (path) DO UPDATE SET cwd = excluded.cwd, cursor = excluded.cursor",
            params![ps, agent, session, cwd, sub as i64, cursor],
        )
        .map_err(|e| e.to_string())?;
        let changed = !b.buckets.is_empty() || reset;
        for ((bucket, model), a) in &b.buckets {
            tx.execute(
                "INSERT INTO buckets (path, bucket, model, input, output, cache_read, cache_write, cache_write_1h, messages, active_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (path, bucket, model) DO UPDATE SET
                   input = input + excluded.input, output = output + excluded.output,
                   cache_read = cache_read + excluded.cache_read, cache_write = cache_write + excluded.cache_write,
                   cache_write_1h = cache_write_1h + excluded.cache_write_1h,
                   messages = messages + excluded.messages, active_ms = active_ms + excluded.active_ms",
                params![
                    ps,
                    bucket,
                    model,
                    a.tokens.input as i64,
                    a.tokens.output as i64,
                    a.tokens.cache_read as i64,
                    a.tokens.cache_write as i64,
                    a.tokens.cache_write_1h as i64,
                    a.messages as i64,
                    a.active_ms
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        let mut limits_changed = false;
        if let Some(l) = b.limits {
            let key = format!("limits.{agent}");
            let old: Option<Limits> = kv_get(&tx, &key);
            if old.as_ref().is_none_or(|o| l.at >= o.at && *o != l) {
                kv_set(&tx, &key, &l).map_err(|e| e.to_string())?;
                limits_changed = true;
            }
        }
        if let Some(h) = b.hit {
            let key = format!("hit.{agent}");
            let old: Option<LimitHit> = kv_get(&tx, &key);
            if old.as_ref().is_none_or(|o| h.at > o.at) {
                kv_set(&tx, &key, &h).map_err(|e| e.to_string())?;
                limits_changed = true;
            }
        }
        if b.ok_at > 0 {
            let key = format!("ok.{agent}");
            if kv_get::<i64>(&tx, &key).is_none_or(|o| b.ok_at > o) {
                kv_set(&tx, &key, &b.ok_at).map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(changed || limits_changed)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Range {
    Today,
    Days(i64),
}

impl Range {
    pub fn parse(s: Option<&str>) -> Option<Range> {
        match s.unwrap_or("today") {
            "today" => Some(Range::Today),
            "7d" => Some(Range::Days(7)),
            "30d" => Some(Range::Days(30)),
            _ => None,
        }
    }

    fn days(self) -> i64 {
        match self {
            Range::Today => 1,
            Range::Days(n) => n,
        }
    }

    fn name(self) -> String {
        match self {
            Range::Today => "today".into(),
            Range::Days(n) => format!("{n}d"),
        }
    }
}

pub struct Query<'a> {
    pub range: Range,
    pub by: &'a str,
    pub tz_offset_min: i64,
    pub session: Option<(&'a str, &'a str)>,
    pub now: i64,
    pub oauth: bool,
    pub tap: Option<Limits>,
}

pub struct LineInfo {
    pub id: String,
    pub repo: String,
    pub title: Option<String>,
}

pub fn local_day(ms: i64, tz_offset_min: i64) -> i64 {
    (ms + tz_offset_min * 60_000).div_euclid(DAY_MS)
}

fn day_label(day: i64) -> String {
    format_rfc3339(day * DAY_MS)[..10].to_string()
}

#[derive(Default)]
struct Sum {
    tokens: Tokens,
    cost: f64,
    unpriced: u64,
    messages: u64,
    active_ms: i64,
    last: i64,
}

impl Sum {
    fn add(&mut self, model: &str, a: &Agg, last: i64) {
        self.tokens.add(&a.tokens);
        match cost(model, &a.tokens) {
            Some(c) => self.cost += c,
            None => self.unpriced += a.tokens.total(),
        }
        self.messages += a.messages;
        self.active_ms += a.active_ms;
        self.last = self.last.max(last);
    }

    fn json(&self) -> Value {
        let t = &self.tokens;
        json!({
            "input": t.input,
            "output": t.output,
            "cacheRead": t.cache_read,
            "cacheWrite": t.cache_write,
            "total": t.total(),
            "cost": if self.unpriced > 0 && self.cost == 0.0 { Value::Null } else { json!((self.cost * 10_000.0).round() / 10_000.0) },
            "costPartial": self.unpriced > 0,
            "messages": self.messages,
            "activeMs": self.active_ms,
        })
    }
}

struct Row {
    agent: String,
    session: String,
    cwd: String,
    model: String,
    day: i64,
    agg: Agg,
    last: i64,
}

fn project_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "—".into())
}

fn repo_label(repo: &str) -> String {
    repo.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(repo)
        .to_string()
}

pub fn query(
    conn: &Connection,
    q: &Query,
    lines: &HashMap<(String, String), LineInfo>,
) -> Result<Value, String> {
    let today = local_day(q.now, q.tz_offset_min);
    let first = today - (q.range.days() - 1);
    let off = q.tz_offset_min * 60_000;
    let from_bucket = (first * DAY_MS - off).div_euclid(BUCKET_MS);
    let mut sql = String::from(
        "SELECT f.agent, f.session, f.cwd, b.model, (b.bucket * ?2 + ?3) / ?4 - ?5 AS d,
                SUM(b.input), SUM(b.output), SUM(b.cache_read), SUM(b.cache_write), SUM(b.cache_write_1h),
                SUM(b.messages), SUM(b.active_ms), MAX(b.bucket)
         FROM buckets b JOIN files f ON f.path = b.path
         WHERE b.bucket >= ?1",
    );
    if q.session.is_some() {
        sql.push_str(" AND f.agent = ?6 AND f.session = ?7");
    }
    sql.push_str(" GROUP BY f.agent, f.session, f.cwd, b.model, d");
    let shift = 1_000_000 * DAY_MS;
    let mut st = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let (sa, ss) = q.session.unwrap_or(("", ""));
    let map = |r: &rusqlite::Row| -> rusqlite::Result<Row> {
        let n = |i: usize| r.get::<_, i64>(i).map(|v| v.max(0) as u64);
        Ok(Row {
            agent: r.get(0)?,
            session: r.get(1)?,
            cwd: r.get(2)?,
            model: r.get(3)?,
            day: r.get(4)?,
            agg: Agg {
                tokens: Tokens {
                    input: n(5)?,
                    output: n(6)?,
                    cache_read: n(7)?,
                    cache_write: n(8)?,
                    cache_write_1h: n(9)?,
                },
                messages: n(10)?,
                active_ms: r.get(11)?,
            },
            last: r.get::<_, i64>(12)? * BUCKET_MS + BUCKET_MS,
        })
    };
    let rows: Vec<Row> = if q.session.is_some() {
        st.query_map(
            params![
                from_bucket,
                BUCKET_MS,
                off + shift,
                DAY_MS,
                shift / DAY_MS,
                sa,
                ss
            ],
            map,
        )
        .map_err(|e| e.to_string())?
        .flatten()
        .collect()
    } else {
        st.query_map(
            params![from_bucket, BUCKET_MS, off + shift, DAY_MS, shift / DAY_MS],
            map,
        )
        .map_err(|e| e.to_string())?
        .flatten()
        .collect()
    };
    let mut total = Sum::default();
    let mut days: BTreeMap<i64, Sum> = (first..=today).map(|d| (d, Sum::default())).collect();
    let mut groups: HashMap<String, (Sum, Value)> = HashMap::new();
    let mut agents: BTreeMap<String, Sum> = BTreeMap::new();
    let mut seen_agents: HashSet<String> = HashSet::new();
    for Row {
        agent,
        session,
        cwd,
        model,
        day,
        agg: a,
        last,
    } in &rows
    {
        if *day < first || *day > today {
            continue;
        }
        let last = (*last).min(q.now);
        total.add(model, a, last);
        if let Some(s) = days.get_mut(day) {
            s.add(model, a, last);
        }
        seen_agents.insert(agent.clone());
        if *day == today {
            agents.entry(agent.clone()).or_default().add(model, a, last);
        }
        let line = lines.get(&(agent.clone(), session.clone()));
        let project = line
            .map(|l| repo_label(&l.repo))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| project_name(cwd));
        let (key, meta) = match q.by {
            "model" => (model.clone(), json!({"model": model})),
            "project" => (project.clone(), json!({"project": project})),
            "session" => (
                format!("{agent}:{session}"),
                json!({
                    "agent": agent,
                    "sessionId": session,
                    "lineId": line.map(|l| l.id.clone()),
                    "title": line.and_then(|l| l.title.clone()),
                    "project": project,
                }),
            ),
            _ => (agent.clone(), json!({"agent": agent})),
        };
        groups
            .entry(key)
            .or_insert_with(|| (Sum::default(), meta))
            .0
            .add(model, a, last);
    }
    let mut groups: Vec<(String, Sum, Value)> =
        groups.into_iter().map(|(k, (s, m))| (k, s, m)).collect();
    groups.sort_by(|a, b| {
        b.1.tokens
            .total()
            .cmp(&a.1.tokens.total())
            .then_with(|| a.0.cmp(&b.0))
    });
    groups.truncate(if q.by == "session" { 50 } else { 100 });
    let groups: Vec<Value> = groups
        .into_iter()
        .map(|(k, s, mut m)| {
            m["key"] = json!(k);
            m["usage"] = s.json();
            if s.last > 0 {
                m["lastAt"] = json!(format_rfc3339(s.last));
            }
            m
        })
        .collect();
    let days: Vec<Value> = days
        .iter()
        .map(|(d, s)| json!({"day": day_label(*d), "usage": s.json()}))
        .collect();
    let mut agent_names: Vec<String> = seen_agents.into_iter().collect();
    for agent in crate::agents::AGENTS
        .iter()
        .filter(|a| !crate::agents::is_shell(a.id))
    {
        if !agent_names.iter().any(|x| x == agent.id) {
            agent_names.push(agent.id.to_string());
        }
    }
    agent_names.sort();
    let agents: Vec<Value> = agent_names
        .iter()
        .map(|a| {
            let today = agents
                .get(a)
                .map(Sum::json)
                .unwrap_or(Sum::default().json());
            json!({
                "agent": a,
                "today": today,
                "limits": limits_json(conn, a, q.now, q.oauth, q.tap
                    .as_ref()
                    .filter(|_| crate::agents::get(a).is_some_and(|x| x.oauth_limits))),
            })
        })
        .collect();
    Ok(json!({
        "range": q.range.name(),
        "by": q.by,
        "tzOffset": q.tz_offset_min,
        "from": format_rfc3339(first * DAY_MS - off),
        "to": format_rfc3339(q.now),
        "bucketMs": BUCKET_MS,
        "totals": total.json(),
        "days": days,
        "groups": groups,
        "agents": agents,
    }))
}

fn limits_json(
    conn: &Connection,
    agent: &str,
    now: i64,
    oauth: bool,
    tap: Option<&Limits>,
) -> Value {
    let fetched: Option<Limits> = oauth
        .then(|| kv_get(conn, &format!("oauth.{agent}")))
        .flatten();
    let via_tap = tap.is_some_and(|t| fetched.as_ref().is_none_or(|f| t.at >= f.at));
    let lim: Option<Limits> = if via_tap {
        tap.cloned()
    } else {
        fetched.or_else(|| kv_get(conn, &format!("limits.{agent}")))
    };
    let auth: Option<Auth> = oauth
        .then(|| kv_get(conn, &format!("oauth.{agent}.auth")))
        .flatten();
    let hit: Option<LimitHit> = kv_get(conn, &format!("hit.{agent}"));
    let ok: i64 = kv_get(conn, &format!("ok.{agent}")).unwrap_or(0);
    let hit = hit
        .filter(|h| h.at > ok && now - h.at < LIMIT_HIT_MS)
        .map(|h| json!({"at": format_rfc3339(h.at), "message": h.text}));
    let mut v = match lim {
        Some(l) => json!({
            "source": agent,
            "observedAt": format_rfc3339(l.at),
            "plan": l.plan,
            "windows": l.windows.iter().map(|w| {
                let reset = w.resets_at > 0 && w.resets_at <= now;
                json!({
                    "key": w.key,
                    "windowMinutes": w.minutes,
                    "usedPercent": if reset { 0.0 } else { w.used },
                    "resetsAt": (w.resets_at > 0).then(|| format_rfc3339(w.resets_at)),
                    "stale": reset,
                })
            }).collect::<Vec<_>>(),
            "hit": hit,
            "credits": l.credits.as_ref().map(|c| json!({
                "hasCredits": c.has_credits,
                "unlimited": c.unlimited,
                "balance": c.balance,
            })),
            "reached": l.reached,
            "via": if via_tap { "statusline" } else { agent },
        }),
        None => json!({
            "source": Value::Null,
            "windows": [],
            "hit": hit,
        }),
    };
    if let Some(a) = auth.filter(|_| !via_tap) {
        v["auth"] = json!(a);
    }
    v
}

fn last_claude_activity(conn: &Connection, now: i64) -> Option<i64> {
    let from = (now - ACTIVE_MS).div_euclid(BUCKET_MS) - 1;
    conn.query_row(
        "SELECT MAX(b.bucket) FROM buckets b JOIN files f ON f.path = b.path
         WHERE f.agent = 'claude' AND b.bucket >= ?1",
        [from],
        |r| r.get::<_, Option<i64>>(0),
    )
    .ok()
    .flatten()
    .map(|b| ((b + 1) * BUCKET_MS).min(now))
}

fn refresh_claude(
    conn: &Connection,
    claude_dir: &Path,
    now: i64,
    http: impl FnOnce(&Token) -> Option<(u16, Vec<u8>)>,
) -> Result<bool, String> {
    let before: (Option<Limits>, Option<Auth>) = (
        kv_get(conn, "oauth.claude"),
        kv_get(conn, "oauth.claude.auth"),
    );
    match outcome(read_creds(claude_dir), now, http) {
        Outcome::Fresh(l) => {
            kv_set(conn, "oauth.claude", &l).map_err(|e| e.to_string())?;
            kv_set(conn, "oauth.claude.auth", &Auth::Ok).map_err(|e| e.to_string())?;
        }
        Outcome::Failed(a) => kv_set(conn, "oauth.claude.auth", &a).map_err(|e| e.to_string())?,
        Outcome::NoLogin => {
            conn.execute(
                "DELETE FROM kv WHERE k IN ('oauth.claude', 'oauth.claude.auth')",
                [],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    let after: (Option<Limits>, Option<Auth>) = (
        kv_get(conn, "oauth.claude"),
        kv_get(conn, "oauth.claude.auth"),
    );
    Ok(before != after)
}

struct Shared {
    dirs: Option<Dirs>,
    db: PathBuf,
    started: bool,
    poked: bool,
    full: bool,
    demand_at: i64,
    indexing: bool,
    claude_dir: PathBuf,
    oauth_wanted_at: i64,
    oauth_attempt_at: i64,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
static WAKE: Condvar = Condvar::new();

pub fn init(state_dir: &Path, dirs: Dirs) {
    let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = Some(Shared {
            claude_dir: dirs.claude.clone(),
            oauth_wanted_at: 0,
            oauth_attempt_at: 0,
            dirs: Some(dirs),
            db: state_dir.join(crate::paths::USAGE_DB),
            started: false,
            poked: false,
            full: true,
            demand_at: 0,
            indexing: false,
        });
    }
}

fn demanded(s: &Shared, now: i64) -> bool {
    s.demand_at > 0 && (now - s.demand_at < DEMAND_MS || crate::board::subscribers() > 0)
}

pub fn tap_changed(state_dir: &Path, seen: &mut Option<std::time::SystemTime>) -> bool {
    let now = std::fs::metadata(crate::statusline::path(state_dir))
        .and_then(|m| m.modified())
        .ok();
    if now == *seen {
        return false;
    }
    *seen = now;
    true
}

pub fn poke(full: bool) {
    let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = g.as_mut() else { return };
    s.poked = true;
    s.full |= full;
    if s.started {
        WAKE.notify_all();
    }
}

fn demand(now: i64, oauth: bool) -> Option<(PathBuf, bool)> {
    let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    let s = g.as_mut()?;
    let stale = now - s.demand_at > FULL_SWEEP_MS;
    s.demand_at = now;
    if oauth {
        s.oauth_wanted_at = now;
        if s.started && now - s.oauth_attempt_at >= REFRESH_MS {
            s.poked = true;
            WAKE.notify_all();
        }
    }
    if !s.started {
        s.started = true;
        s.indexing = true;
        let dirs = s.dirs.take()?;
        let db = s.db.clone();
        if !crate::dlog::supervise("usage", move || worker(db.clone(), dirs.clone())) {
            return None;
        }
    } else if stale {
        s.poked = true;
        s.full = true;
        WAKE.notify_all();
    }
    Some((s.db.clone(), s.indexing))
}

fn worker(db: PathBuf, dirs: Dirs) {
    let mut idx = match Index::open(&db, dirs) {
        Ok(i) => i,
        Err(e) => {
            crate::dlog::line("error", &format!("usage: {e}"));
            return;
        }
    };
    let first = !idx.indexed;
    let pace = first.then_some(Duration::from_millis(2));
    match idx.sweep(crate::util::now_ms(), true, pace) {
        Ok(_) => crate::board::publish("usage.updated", json!({})),
        Err(e) => crate::dlog::line("error", &format!("usage: {e}")),
    }
    if let Some(s) = SHARED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.indexing = false;
    }
    let tap = |db: &Path| db.parent().and_then(crate::statusline::read);
    let mut tap_at = tap(&db).map(|t| t.at);
    if claude_limits_due(&idx, tap(&db).as_ref()) {
        crate::board::publish("usage.updated", json!({}));
    }
    loop {
        let full = {
            let mut g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                let now = crate::util::now_ms();
                match g.as_mut() {
                    Some(s) if s.poked && demanded(s, now) => {
                        s.poked = false;
                        let f = s.full;
                        s.full = false;
                        break f;
                    }
                    _ => {}
                }
                g = WAKE.wait(g).unwrap_or_else(|e| e.into_inner());
            }
        };
        std::thread::sleep(Duration::from_millis(DEBOUNCE_MS));
        let swept = match idx.sweep(crate::util::now_ms(), full, None) {
            Ok(c) => c,
            Err(e) => {
                crate::dlog::line("error", &format!("usage: {e}"));
                false
            }
        };
        let tapped = tap(&db);
        let tap_changed = tapped.as_ref().map(|t| t.at) != tap_at;
        tap_at = tapped.as_ref().map(|t| t.at);
        if claude_limits_due(&idx, tapped.as_ref()) || swept || tap_changed {
            crate::board::publish("usage.updated", json!({}));
        }
    }
}

fn claude_limits_due(idx: &Index, tap: Option<&Limits>) -> bool {
    let now = crate::util::now_ms();
    if crate::claude_limits::tap_covers(tap.map(|t| t.at), now) {
        return false;
    }
    let (wanted_at, attempt_at, dir) = {
        let g = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        let Some(s) = g.as_ref() else { return false };
        (s.oauth_wanted_at, s.oauth_attempt_at, s.claude_dir.clone())
    };
    let wanted = wanted_at > 0 && now - wanted_at < DEMAND_MS && crate::board::subscribers() > 0;
    if !crate::claude_limits::due(
        wanted,
        now,
        attempt_at,
        last_claude_activity(&idx.conn, now),
    ) {
        return false;
    }
    if let Some(s) = SHARED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        s.oauth_attempt_at = now;
    }
    match refresh_claude(&idx.conn, &dir, now, crate::claude_limits::fetch) {
        Ok(changed) => changed,
        Err(e) => {
            crate::dlog::line("error", &format!("usage: claude limits: {e}"));
            false
        }
    }
}

pub fn api(r: &crate::http::Request, lines: &[crate::Line]) -> (u16, Value) {
    let now = crate::util::now_ms();
    let Some(range) = Range::parse(crate::http::q(r, "range")) else {
        return err(400, "range must be today, 7d or 30d");
    };
    let by = crate::http::q(r, "by").unwrap_or("agent");
    if !matches!(by, "agent" | "project" | "model" | "session") {
        return err(400, "by must be agent, project, model or session");
    }
    let tz = match crate::http::q(r, "tzOffset").map(|s| s.parse::<i64>()) {
        None => 0,
        Some(Ok(m)) if (-14 * 60..=14 * 60).contains(&m) => m,
        _ => return err(400, "tzOffset must be minutes east of UTC"),
    };
    let wanted = crate::http::q(r, "line");
    let session = match wanted {
        None => None,
        Some(id) => match lines.iter().find(|l| l.id == id) {
            Some(l) => Some((l.agent, l.session_id.as_str())),
            None => return err(404, format!("no line {id}")),
        },
    };
    let oauth = r.is_one("claudeLimits");
    let Some((db, indexing)) = demand(now, oauth) else {
        return err(503, "usage is not available");
    };
    let conn = match open_db(&db) {
        Ok(c) => c,
        Err(e) => return err(500, e),
    };
    let map: HashMap<(String, String), LineInfo> = lines
        .iter()
        .map(|l| {
            (
                (l.agent.to_string(), l.session_id.clone()),
                LineInfo {
                    id: l.id.clone(),
                    repo: l.repo.clone(),
                    title: l.title.clone(),
                },
            )
        })
        .collect();
    let q = Query {
        range,
        by,
        tz_offset_min: tz,
        session,
        now,
        oauth,
        tap: db.parent().and_then(crate::statusline::read),
    };
    match query(&conn, &q, &map) {
        Ok(mut v) => {
            v["indexing"] = json!(indexing);
            (200, v)
        }
        Err(e) => err(500, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/usage")
            .join(name)
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sb-usage-{tag}-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_new_or_rewritten_statusline_file_asks_for_one_refresh() {
        let d = tmp("tap-poke");
        let mut seen = None;
        assert!(!tap_changed(&d, &mut seen));
        std::fs::write(crate::statusline::path(&d), "{}").unwrap();
        assert!(tap_changed(&d, &mut seen));
        assert!(!tap_changed(&d, &mut seen));
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(crate::statusline::path(&d))
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(tap_changed(&d, &mut seen));
        std::fs::remove_file(crate::statusline::path(&d)).unwrap();
        assert!(tap_changed(&d, &mut seen));
        assert!(!tap_changed(&d, &mut seen));
    }

    fn home(tag: &str) -> (PathBuf, Index) {
        let d = tmp(tag);
        let dirs = Dirs {
            claude: d.join("claude"),
            codex: d.join("codex"),
            gemini: d.join("gemini"),
            vibe: d.join("vibe"),
            copilot: d.join("copilot"),
            opencode: d.join("opencode"),
            agy: d.join("agy"),
        };
        std::fs::create_dir_all(dirs.claude.join("projects/-w-app")).unwrap();
        std::fs::create_dir_all(dirs.codex.join("sessions/2026/10/29")).unwrap();
        std::fs::create_dir_all(&dirs.gemini).unwrap();
        let idx = Index::open(&d.join("usage.db"), dirs).unwrap();
        (d, idx)
    }

    #[test]
    fn many_threads_opening_a_fresh_database_at_once_all_get_a_connection() {
        for round in 0..20 {
            let db = tmp(&format!("race{round}")).join("usage.db");
            let gate = std::sync::Barrier::new(8);
            let opened: Vec<Result<Connection, String>> = std::thread::scope(|s| {
                let hs: Vec<_> = (0..8)
                    .map(|_| {
                        s.spawn(|| {
                            gate.wait();
                            open_db(&db)
                        })
                    })
                    .collect();
                hs.into_iter().map(|h| h.join().unwrap()).collect()
            });
            let errs: Vec<_> = opened.iter().filter_map(|r| r.as_ref().err()).collect();
            assert!(errs.is_empty(), "round {round}: {errs:?}");
        }
    }

    fn now() -> i64 {
        parse_rfc3339("2026-10-29T18:00:00Z").unwrap()
    }

    fn touch_now(p: &Path) {
        let f = std::fs::File::options().append(true).open(p).unwrap();
        f.set_modified(std::time::SystemTime::now()).unwrap();
    }

    fn get(idx: &Index, range: Range, by: &str, tz: i64) -> Value {
        query(
            &idx.conn,
            &Query {
                range,
                by,
                tz_offset_min: tz,
                session: None,
                now: now(),
                oauth: false,
                tap: None,
            },
            &HashMap::new(),
        )
        .unwrap()
    }

    fn append(p: &Path, s: &str) {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap()
            .write_all(s.as_bytes())
            .unwrap();
    }

    fn agent_of<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["agent"] == id)
            .unwrap()
    }

    fn real_now() -> i64 {
        crate::util::now_ms()
    }

    #[test]
    fn prices() {
        assert_eq!(price("claude-opus-5-5").unwrap().input, 4.0);
        assert_eq!(price("claude-opus-5").unwrap().input, 5.0);
        assert_eq!(price("claude-opus-4-1-20250805").unwrap().input, 15.0);
        assert_eq!(price("claude-opus-4-6").unwrap().input, 5.0);
        assert_eq!(price("claude-fable-5-1").unwrap().output, 50.0);
        assert_eq!(price("claude-sonnet-4-6").unwrap().input, 3.0);
        assert_eq!(price("gpt-5.4").unwrap().input, 2.5);
        assert_eq!(price("gpt-5.5").unwrap().output, 30.0);
        assert_eq!(price("gpt-5.4-mini").unwrap().input, 0.75);
        assert_eq!(price("gpt-5-codex").unwrap().input, 1.25);
        assert!(price("gpt-5.9").is_none());
        assert!(price("gpt-6-luna").is_none());
        assert_eq!(price("claude-fable-5-2").unwrap().input, 8.0);
        assert_eq!(price("claude-sonnet-5-5").unwrap().cache_read, 0.1);
        assert_eq!(price("claude-haiku-5-5").unwrap().output, 0.5);
        assert_eq!(price("gpt-5-mini").unwrap().input, 0.25);
        assert_eq!(price("gemini-2.5-flash-lite").unwrap().input, 0.1);
        assert_eq!(price("models/gemini-2.5-pro").unwrap().input, 1.25);
        assert!(price("llama-3").is_none());
        let t = Tokens {
            input: 1_000_000,
            output: 1_000_000,
            cache_read: 1_000_000,
            cache_write: 2_000_000,
            cache_write_1h: 1_000_000,
        };
        assert!(
            (cost("claude-opus-5-5", &t).unwrap() - (4.0 + 20.0 + 0.2 + 5.0 + 8.0)).abs() < 1e-9
        );
        assert_eq!(cost("mystery", &t), None);
    }

    #[test]
    fn day_buckets_follow_the_client_timezone() {
        let at = parse_rfc3339("2026-10-29T23:30:00Z").unwrap();
        assert_eq!(day_label(local_day(at, 0)), "2026-10-29");
        assert_eq!(day_label(local_day(at, 60)), "2026-10-30");
        assert_eq!(day_label(local_day(at, -300)), "2026-10-29");
        assert_eq!(day_label(local_day(at, 330)), "2026-10-30");
        let early = parse_rfc3339("2026-10-30T02:00:00Z").unwrap();
        assert_eq!(day_label(local_day(early, -300)), "2026-10-29");
    }

    #[test]
    fn claude_transcript_dedupes_and_buckets() {
        let (d, mut idx) = home("claude");
        let t = d.join("claude/projects/-w-app/s1.jsonl");
        std::fs::copy(fixture("claude.jsonl"), &t).unwrap();
        touch_now(&t);
        let sub = d.join("claude/projects/-w-app/s1/subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::copy(fixture("claude-sub.jsonl"), sub.join("agent-a1.jsonl")).unwrap();
        touch_now(&sub.join("agent-a1.jsonl"));
        assert!(idx.sweep(now(), true, None).unwrap());

        let v = get(&idx, Range::Today, "model", 0);
        let tot = &v["totals"];
        assert_eq!(tot["input"], 10 + 20 + 5);
        assert_eq!(tot["output"], 300 + 40 + 7);
        assert_eq!(tot["cacheRead"], 1000 + 2000);
        assert_eq!(tot["cacheWrite"], 500 + 100);
        assert_eq!(tot["messages"], 3);
        assert_eq!(tot["costPartial"], false);
        assert_eq!(tot["activeMs"], 60_000);
        let models: Vec<&str> = v["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["model"].as_str().unwrap())
            .collect();
        assert_eq!(models, ["claude-opus-5-5", "claude-haiku-4-5"]);
        let opus = &v["groups"][0]["usage"];
        let expect = (10.0 + 20.0) * 4.0 + (300.0 + 40.0) * 20.0 + 3000.0 * 0.2 + 500.0 * 8.0;
        assert!((opus["cost"].as_f64().unwrap() - expect / 1e6).abs() < 1e-4);

        let s = get(&idx, Range::Today, "session", 0);
        assert_eq!(s["groups"].as_array().unwrap().len(), 1);
        assert_eq!(s["groups"][0]["sessionId"], "s1");
        assert_eq!(s["groups"][0]["project"], "app");

        let hit = &agent_of(&v, "claude")["limits"]["hit"];
        assert!(hit.is_null());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn response_lists_every_supported_agent() {
        let (d, mut idx) = home("allagents");
        let t = d.join("claude/projects/-w-app/s1.jsonl");
        std::fs::copy(fixture("claude.jsonl"), &t).unwrap();
        touch_now(&t);
        idx.sweep(now(), true, None).unwrap();
        let v = get(&idx, Range::Today, "agent", 0);
        let ids: Vec<&str> = v["agents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["agent"].as_str().unwrap())
            .collect();
        let mut expected: Vec<&str> = crate::agents::AGENTS
            .iter()
            .filter(|a| !crate::agents::is_shell(a.id))
            .map(|a| a.id)
            .collect();
        expected.sort_unstable();
        assert_eq!(ids, expected);
        assert!(!ids.contains(&"shell"));
        let vibe = agent_of(&v, "vibe");
        assert_eq!(vibe["today"]["total"], 0);
        assert!(vibe["limits"]["source"].is_null());
        assert!(vibe["limits"]["windows"].as_array().unwrap().is_empty());
        assert!(vibe["limits"]["hit"].is_null());
        assert!(agent_of(&v, "claude")["today"]["total"].as_u64().unwrap() > 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn vibe_usage(d: &Path, id: &str, generation: u64, input: u64, cached: u64, output: u64) {
        use crate::vibe::fixture::*;
        let dir = crate::vibe::fixture::session(
            &d.join("vibe"),
            id,
            meta_json(id, "/w/app", None),
            generation,
            &[],
            "",
        );
        std::fs::write(
            dir.join("generations")
                .join(format!("{generation:016}"))
                .join("projection-state.json"),
            json!({"projection_state_version": 1, "session_id": id, "snapshot_sequence": 1, "watermark": 0,
                "snapshot": {"history": {"entries": []}, "session": {"id": id, "title": "t",
                    "tokenUsage": {"cachedInputTokens": cached, "inputTokens": input, "outputTokens": output, "totalTokens": input + output},
                    "updatedAt": now()}}})
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn vibe_sessions_are_metered_from_their_store_by_delta() {
        let (d, mut idx) = home("vibe");
        vibe_usage(&d, "v1", 5, 1000, 400, 50);
        idx.sweep(now(), true, None).unwrap();
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 600);
        assert_eq!(v["totals"]["cacheRead"], 400);
        assert_eq!(v["totals"]["output"], 50);
        assert_eq!(agent_of(&v, "vibe")["today"]["total"], 1050);
        let m = get(&idx, Range::Today, "model", 0);
        assert_eq!(m["groups"][0]["model"], "mistral-medium-3.5");

        vibe_usage(&d, "v1", 6, 1500, 700, 80);
        idx.sweep(now(), true, None).unwrap();
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 800);
        assert_eq!(v["totals"]["cacheRead"], 700);
        assert_eq!(v["totals"]["output"], 80);

        assert!(!idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["output"], 80);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn later(p: &Path, secs: u64) {
        let f = std::fs::File::options().append(true).open(p).unwrap();
        f.set_modified(std::time::SystemTime::now() + Duration::from_secs(secs))
            .unwrap();
    }

    fn copilot_events(d: &Path) -> PathBuf {
        let dir = d.join("copilot/session-state/11111111-2222-3333-4444-555555555555");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("events.jsonl");
        std::fs::copy(fixture("copilot-events.jsonl"), &p).unwrap();
        touch_now(&p);
        p
    }

    fn copilot_record(model: &str, input: u64, output: u64, read: u64, write: u64) -> String {
        format!(
            "{}\n",
            json!({"type": "session.usage_record", "timestamp": "2026-10-29T17:00:00.000Z",
                "data": {"usage": {"model": model, "inputTokens": input, "outputTokens": output,
                    "cacheReadTokens": read, "cacheWriteTokens": write}}})
        )
    }

    #[test]
    fn copilot_usage_records_split_fresh_input_from_cache() {
        let (d, mut idx) = home("copilot");
        copilot_events(&d);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 6);
        assert_eq!(v["totals"]["output"], 440);
        assert_eq!(v["totals"]["cacheRead"], 18972);
        assert_eq!(v["totals"]["cacheWrite"], 20078);
        assert_eq!(v["totals"]["messages"], 2);
        assert_eq!(
            agent_of(&v, "copilot")["today"]["total"],
            6 + 440 + 18972 + 20078
        );
        let s = get(&idx, Range::Today, "session", 0);
        assert_eq!(s["groups"][0]["project"], "app");
        assert_eq!(
            s["groups"][0]["sessionId"],
            "11111111-2222-3333-4444-555555555555"
        );
        let m = get(&idx, Range::Today, "model", 0);
        assert_eq!(m["groups"][0]["model"], "gpt-5.6-luna");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn copilot_appends_are_read_once_and_unchanged_files_are_skipped() {
        let (d, mut idx) = home("copilot-inc");
        let p = copilot_events(&d);
        idx.sweep(now(), true, None).unwrap();
        assert!(!idx.sweep(now(), true, None).unwrap());
        append(&p, &copilot_record("gpt-5.6-luna", 1000, 10, 400, 100));
        later(&p, 5);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 6 + 500);
        assert_eq!(v["totals"]["output"], 450);
        assert_eq!(v["totals"]["cacheRead"], 18972 + 400);
        assert_eq!(v["totals"]["cacheWrite"], 20078 + 100);
        assert!(!idx.sweep(now(), true, None).unwrap());
        append(
            &p,
            "{\"type\":\"session.usage_record\",\"timestamp\":\"2026-10-29T17:\n",
        );
        assert!(!idx.sweep(now(), true, None).unwrap());
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn varint(mut v: u64, out: &mut Vec<u8>) {
        while v >= 0x80 {
            out.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn pb_int_field(f: u64, v: u64, out: &mut Vec<u8>) {
        varint(f << 3, out);
        varint(v, out);
    }

    fn pb_bytes_field(f: u64, b: &[u8], out: &mut Vec<u8>) {
        varint(f << 3 | 2, out);
        varint(b.len() as u64, out);
        out.extend_from_slice(b);
    }

    fn agy_blob(model: &str, input: u64, output: u64, read: u64, secs: u64) -> Vec<u8> {
        let mut stats = vec![];
        pb_int_field(1, 1132, &mut stats);
        pb_int_field(2, input, &mut stats);
        pb_int_field(3, output, &mut stats);
        if read > 0 {
            pb_int_field(5, read, &mut stats);
        }
        pb_int_field(6, 24, &mut stats);
        pb_int_field(10, 49, &mut stats);
        pb_bytes_field(11, b"anon", &mut stats);
        let mut ts = vec![];
        pb_int_field(1, secs, &mut ts);
        pb_int_field(2, 956_181_649, &mut ts);
        let mut at = vec![];
        pb_int_field(2, u64::MAX, &mut at);
        pb_bytes_field(4, &ts, &mut at);
        let mut head = vec![];
        pb_int_field(3, 1132, &mut head);
        pb_bytes_field(4, &stats, &mut head);
        pb_bytes_field(9, &at, &mut head);
        pb_bytes_field(19, model.as_bytes(), &mut head);
        let mut blob = vec![];
        pb_bytes_field(2, &[2, 3], &mut blob);
        pb_bytes_field(1, &head, &mut blob);
        blob
    }

    fn agy_db(d: &Path, id: &str) -> PathBuf {
        let dir = d.join("agy/conversations");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{id}.db"));
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE gen_metadata (idx integer, data blob, size integer NOT NULL DEFAULT 0, PRIMARY KEY (idx));
             CREATE TABLE steps (idx integer, step_type integer NOT NULL DEFAULT 0);",
        )
        .unwrap();
        p
    }

    fn agy_add(p: &Path, idx: i64, blob: &[u8]) {
        let c = Connection::open(p).unwrap();
        c.execute(
            "INSERT INTO gen_metadata (idx, data, size) VALUES (?1, ?2, ?3)",
            params![idx, blob, blob.len() as i64],
        )
        .unwrap();
    }

    #[test]
    fn antigravity_conversation_rows_are_metered_incrementally() {
        let (d, mut idx) = home("agy");
        let p = agy_db(&d, "c1");
        let t = now().div_euclid(1000) as u64 - 3600;
        agy_add(&p, 0, &agy_blob("gemini-3-flash-a", 18193, 241, 0, t));
        agy_add(
            &p,
            1,
            &agy_blob("gemini-3-flash-a", 2446, 112, 16277, t + 2),
        );
        agy_add(&p, 2, &[0xff, 0xff]);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 18193 + 2446);
        assert_eq!(v["totals"]["output"], 241 + 112);
        assert_eq!(v["totals"]["cacheRead"], 16277);
        assert_eq!(v["totals"]["messages"], 2);
        assert_eq!(
            agent_of(&v, "agy")["today"]["total"],
            18193 + 2446 + 353 + 16277
        );
        let m = get(&idx, Range::Today, "model", 0);
        assert_eq!(m["groups"][0]["model"], "gemini-3-flash-a");
        let s = get(&idx, Range::Today, "session", 0);
        assert_eq!(s["groups"][0]["sessionId"], "c1");

        assert!(!idx.sweep(now(), true, None).unwrap());
        agy_add(&p, 3, &agy_blob("gemini-3-flash-a", 100, 10, 50, t + 9));
        later(&p, 5);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 18193 + 2446 + 100);
        assert_eq!(v["totals"]["cacheRead"], 16277 + 50);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn antigravity_rows_land_on_their_own_day() {
        let (d, mut idx) = home("agy-days");
        let p = agy_db(&d, "c2");
        let t = now().div_euclid(1000) as u64;
        agy_add(
            &p,
            0,
            &agy_blob("gemini-3-flash-a", 700, 7, 0, t - 3 * 86_400),
        );
        agy_add(&p, 1, &agy_blob("gemini-3-flash-a", 300, 3, 0, t - 60));
        idx.sweep(now(), true, None).unwrap();
        let today = get(&idx, Range::Today, "agent", 0);
        assert_eq!(today["totals"]["input"], 300);
        let week = get(&idx, Range::Days(7), "agent", 0);
        assert_eq!(week["totals"]["input"], 1000);
        std::fs::remove_dir_all(&d).unwrap();
    }

    fn opencode_db(d: &Path) -> (PathBuf, Connection) {
        let dir = d.join("opencode");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("opencode.db");
        let c = Connection::open(&p).unwrap();
        c.execute_batch(
            "CREATE TABLE session (id text PRIMARY KEY, project_id text NOT NULL, parent_id text,
               directory text NOT NULL, title text NOT NULL, time_created integer NOT NULL,
               time_updated integer NOT NULL, model text, cost real DEFAULT 0 NOT NULL,
               tokens_input integer DEFAULT 0 NOT NULL, tokens_output integer DEFAULT 0 NOT NULL,
               tokens_reasoning integer DEFAULT 0 NOT NULL, tokens_cache_read integer DEFAULT 0 NOT NULL,
               tokens_cache_write integer DEFAULT 0 NOT NULL);",
        )
        .unwrap();
        (p, c)
    }

    fn oc_session(
        c: &Connection,
        id: &str,
        parent: Option<&str>,
        model: Option<&str>,
        t: [i64; 5],
    ) {
        c.execute(
            "INSERT OR REPLACE INTO session (id, project_id, parent_id, directory, title, time_created,
               time_updated, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read,
               tokens_cache_write)
             VALUES (?1, 'p', ?2, '/w/app', 't', ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, parent, now() - 1000, model, t[0], t[1], t[2], t[3], t[4]],
        )
        .unwrap();
    }

    #[test]
    fn opencode_sessions_are_metered_by_delta_without_touching_unchanged_ones() {
        let (d, mut idx) = home("opencode");
        let (p, c) = opencode_db(&d);
        let model = r#"{"id":"claude-sonnet-4-5","providerID":"anthropic"}"#;
        oc_session(&c, "ses_a", None, Some(model), [1000, 200, 30, 400, 50]);
        oc_session(&c, "ses_b", Some("ses_a"), None, [10, 5, 0, 0, 0]);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 1010);
        assert_eq!(v["totals"]["output"], 235);
        assert_eq!(v["totals"]["cacheRead"], 400);
        assert_eq!(v["totals"]["cacheWrite"], 50);
        assert_eq!(
            agent_of(&v, "opencode")["today"]["total"],
            1010 + 235 + 400 + 50
        );
        let m = get(&idx, Range::Today, "model", 0);
        let models: Vec<&str> = m["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| g["model"].as_str().unwrap())
            .collect();
        assert_eq!(models, ["claude-sonnet-4-5", "opencode"]);
        let s = get(&idx, Range::Today, "session", 0);
        assert_eq!(s["groups"].as_array().unwrap().len(), 2);
        assert_eq!(s["groups"][0]["project"], "app");

        assert!(!idx.sweep(now(), true, None).unwrap());
        c.execute(
            "UPDATE session SET tokens_input = tokens_input + 500, tokens_output = tokens_output + 20 WHERE id = 'ses_a'",
            [],
        )
        .unwrap();
        later(&p, 5);
        assert!(idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 1510);
        assert_eq!(v["totals"]["output"], 255);
        assert!(!idx.sweep(now(), true, None).unwrap());

        drop(idx);
        let dirs = Dirs {
            claude: d.join("claude"),
            codex: d.join("codex"),
            gemini: d.join("gemini"),
            vibe: d.join("vibe"),
            copilot: d.join("copilot"),
            opencode: d.join("opencode"),
            agy: d.join("agy"),
        };
        let mut idx = Index::open(&d.join("usage.db"), dirs).unwrap();
        assert!(!idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["input"], 1510);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn opencode_sessions_idle_for_a_month_are_not_indexed() {
        let (d, mut idx) = home("opencode-old");
        let (_, c) = opencode_db(&d);
        oc_session(&c, "ses_old", None, None, [10, 1, 0, 0, 0]);
        c.execute(
            "UPDATE session SET time_updated = ?1",
            [now() - 40 * DAY_MS],
        )
        .unwrap();
        assert!(!idx.sweep(now(), true, None).unwrap());
        let v = get(&idx, Range::Days(30), "agent", 0);
        assert_eq!(v["totals"]["input"], 0);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_opencode_database_or_session_table_is_not_an_error() {
        let (d, mut idx) = home("opencode-missing");
        assert!(!idx.sweep(now(), true, None).unwrap());
        std::fs::create_dir_all(d.join("opencode")).unwrap();
        Connection::open(d.join("opencode/opencode.db"))
            .unwrap()
            .execute_batch("CREATE TABLE other (x integer)")
            .unwrap();
        assert!(!idx.sweep(now(), true, None).unwrap());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn incremental_appends_only_read_new_lines() {
        let (d, mut idx) = home("inc");
        let t = d.join("claude/projects/-w-app/s2.jsonl");
        let line = |id: &str, ts: &str, out: u64| {
            format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": ts, "cwd": "/w/app", "message": {"id": id, "model": "claude-opus-5-5", "usage": {"input_tokens": 1, "output_tokens": out, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}})
            )
        };
        append(&t, &line("m1", "2026-10-29T10:00:00Z", 10));
        let partial = line("m1", "2026-10-29T10:00:01Z", 25);
        let (head, tail) = partial.split_at(20);
        append(&t, head);
        idx.sweep(now(), true, None).unwrap();
        assert_eq!(get(&idx, Range::Today, "agent", 0)["totals"]["output"], 10);
        let off = idx.files[&t].cursor.offset;
        append(&t, tail);
        append(&t, &line("m2", "2026-10-29T10:01:00Z", 5));
        assert!(idx.sweep(now(), false, None).unwrap());
        assert!(idx.files[&t].cursor.offset > off);
        let v = get(&idx, Range::Today, "agent", 0);
        assert_eq!(v["totals"]["output"], 25 + 5);
        assert_eq!(v["totals"]["messages"], 2);
        assert!(!idx.sweep(now(), false, None).unwrap());

        let reopened = Index::open(
            &d.join("usage.db"),
            Dirs {
                claude: d.join("claude"),
                codex: d.join("codex"),
                gemini: d.join("gemini"),
                vibe: d.join("vibe"),
                copilot: d.join("copilot"),
                opencode: d.join("opencode"),
                agy: d.join("agy"),
            },
        )
        .unwrap();
        assert_eq!(
            reopened.files[&t].cursor.offset,
            idx.files[&t].cursor.offset
        );
        assert!(reopened.indexed);

        std::fs::write(&t, line("m9", "2026-10-29T11:00:00Z", 3)).unwrap();
        idx.sweep(now(), false, None).unwrap();
        assert_eq!(get(&idx, Range::Today, "agent", 0)["totals"]["output"], 3);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn codex_rollout_tokens_and_limits() {
        let (d, mut idx) = home("codex");
        let r = d.join("codex/sessions/2026/10/29/rollout-2026-10-29T10-00-00-019f3d2a-f525-7523-9503-b51b6d205741.jsonl");
        std::fs::copy(fixture("codex.jsonl"), &r).unwrap();
        touch_now(&r);
        idx.sweep(now(), true, None).unwrap();
        let v = get(&idx, Range::Today, "session", 0);
        let g = &v["groups"][0];
        assert_eq!(g["agent"], "codex");
        assert_eq!(g["sessionId"], "019f3d2a-f525-7523-9503-b51b6d205741");
        assert_eq!(g["project"], "api");
        let u = &g["usage"];
        assert_eq!(u["input"], 15_000 - 9_000);
        assert_eq!(u["cacheRead"], 9_000);
        assert_eq!(u["output"], 400);
        assert_eq!(u["messages"], 2);
        let expect = (6_000.0 * 2.5 + 9_000.0 * 0.25 + 400.0 * 15.0) / 1e6;
        assert!((u["cost"].as_f64().unwrap() - expect).abs() < 1e-4);
        let m = get(&idx, Range::Today, "model", 0);
        assert_eq!(m["groups"][0]["model"], "gpt-5.4");

        let lim = &agent_of(&v, "codex")["limits"];
        assert_eq!(lim["source"], "codex");
        assert_eq!(lim["plan"], "plus");
        let w = lim["windows"].as_array().unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(
            (w[0]["key"].as_str(), w[0]["usedPercent"].as_f64()),
            (Some("5h"), Some(29.0))
        );
        assert_eq!(w[0]["resetsAt"], "2026-10-29T20:00:00Z");
        assert_eq!(w[0]["stale"], false);
        assert_eq!(
            (w[1]["key"].as_str(), w[1]["usedPercent"].as_f64()),
            (Some("wk"), Some(38.0))
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn codex_go_monthly_window_preserves_its_usage_and_billing_reset() {
        let (d, mut idx) = home("codexgo");
        let r = d.join("codex/sessions/2026/10/29/rollout-2026-10-29T18-00-00-019f3d2a-f525-7523-9503-b51b6d205741.jsonl");
        let line = json!({
            "timestamp": "2026-10-29T18:00:00Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": 1000,
                        "cached_input_tokens": 200,
                        "output_tokens": 40,
                        "total_tokens": 1040
                    }
                },
                "rate_limits": {
                    "limit_id": "codex",
                    "plan_type": "go",
                    "primary": {
                        "used_percent": 34.0,
                        "window_minutes": 43_200,
                        "resets_at": 1_795_982_400_i64
                    },
                    "secondary": null
                }
            }
        })
        .to_string();
        std::fs::write(&r, format!("{line}\n")).unwrap();
        touch_now(&r);
        idx.sweep(now(), true, None).unwrap();

        let v = get(&idx, Range::Today, "agent", 0);
        let codex = v["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["agent"] == "codex")
            .unwrap();
        let limits = &codex["limits"];
        assert_eq!(limits["plan"], "go");
        assert_eq!(limits["windows"].as_array().unwrap().len(), 1);
        assert_eq!(limits["windows"][0]["key"], "mo");
        assert_eq!(limits["windows"][0]["usedPercent"], 34.0);
        assert_eq!(limits["windows"][0]["resetsAt"], "2026-11-29T20:00:00Z");
        assert_eq!(limits["windows"][0]["stale"], false);

        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn codex_spent_plan_window_reports_the_credits_still_in_use() {
        let (d, mut idx) = home("codexcredits");
        let r = d.join("codex/sessions/2026/10/29/rollout-2026-11-09T08-43-52-01a1bf7a-68b2-71c2-bc92-0e5477d8ed37.jsonl");
        let line = json!({
            "timestamp": "2026-11-09T08:45:34.126Z",
            "type": "event_msg",
            "payload": {
                "type": "token_count",
                "info": {"total_token_usage": {"input_tokens": 10, "output_tokens": 2}},
                "rate_limits": {
                    "limit_id": "codex",
                    "limit_name": null,
                    "primary": {"used_percent": 100.0, "window_minutes": 43_200, "resets_at": 1_795_622_413_i64},
                    "secondary": null,
                    "credits": {"has_credits": true, "unlimited": false, "balance": "1272.1821530000"},
                    "individual_limit": null,
                    "spend_control_reached": null,
                    "plan_type": "go",
                    "rate_limit_reached_type": null
                }
            }
        })
        .to_string();
        std::fs::write(&r, format!("{line}\n")).unwrap();
        touch_now(&r);
        idx.sweep(now(), true, None).unwrap();

        let v = get(&idx, Range::Today, "agent", 0);
        let codex = v["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["agent"] == "codex")
            .unwrap();
        let limits = &codex["limits"];
        assert_eq!(limits["windows"][0]["usedPercent"], 100.0);
        assert_eq!(limits["credits"]["hasCredits"], true);
        assert_eq!(limits["credits"]["unlimited"], false);
        assert_eq!(limits["credits"]["balance"], 1272.182153);
        assert_eq!(limits["reached"], Value::Null);

        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn limits_stored_before_credits_still_load() {
        let old: Limits = serde_json::from_str(
            r#"{"at":5,"windows":[{"key":"mo","minutes":43200,"used":100.0,"resets_at":0}],"plan":"go"}"#,
        )
        .unwrap();
        assert_eq!(old.windows.len(), 1);
        assert_eq!(old.credits, None);
        assert_eq!(old.reached, None);
    }

    #[test]
    fn gemini_json_session_is_replaced_on_rewrite() {
        let (d, mut idx) = home("gemini");
        let chats = d.join("gemini/tmp/repo/chats");
        std::fs::create_dir_all(&chats).unwrap();
        std::fs::write(d.join("gemini/tmp/repo/.project_root"), "/w/repo\n").unwrap();
        let s = chats.join("session-2026-10-29T10-00-abcd1234.json");
        let doc = |n: usize| {
            let msgs: Vec<Value> = (0..n)
                .map(|i| json!({"id": format!("g{i}"), "type": "gemini", "timestamp": "2026-10-29T10:00:00Z", "model": "gemini-2.5-pro", "tokens": {"input": 1000, "output": 50, "cached": 400, "thoughts": 10, "tool": 0, "total": 1060}}))
                .collect();
            json!({"sessionId": "abcd1234-0000-4000-8000-000000000001", "projectHash": "x", "startTime": "2026-10-29T10:00:00Z", "lastUpdated": "2026-10-29T10:00:00Z", "messages": msgs}).to_string()
        };
        std::fs::write(&s, doc(1)).unwrap();
        idx.sweep(now(), true, None).unwrap();
        let v = get(&idx, Range::Today, "project", 0);
        assert_eq!(v["totals"]["input"], 600);
        assert_eq!(v["totals"]["cacheRead"], 400);
        assert_eq!(v["totals"]["output"], 60);
        assert_eq!(v["groups"][0]["project"], "repo");
        std::fs::write(&s, doc(3)).unwrap();
        idx.sweep(now(), true, None).unwrap();
        assert_eq!(get(&idx, Range::Today, "agent", 0)["totals"]["input"], 1800);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn ranges_fold_days_in_the_client_timezone() {
        let (d, mut idx) = home("tz");
        let t = d.join("claude/projects/-w-app/s3.jsonl");
        let line = |id: &str, ts: &str| {
            format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": ts, "message": {"id": id, "model": "claude-opus-5-5", "usage": {"input_tokens": 0, "output_tokens": 100}}})
            )
        };
        append(&t, &line("a", "2026-10-29T17:00:00Z"));
        append(&t, &line("b", "2026-10-28T23:30:00Z"));
        append(&t, &line("c", "2026-10-25T12:00:00Z"));
        append(&t, &line("d", "2026-09-01T12:00:00Z"));
        idx.sweep(now(), true, None).unwrap();
        assert_eq!(get(&idx, Range::Today, "agent", 0)["totals"]["output"], 100);
        assert_eq!(
            get(&idx, Range::Today, "agent", 60)["totals"]["output"],
            200
        );
        let week = get(&idx, Range::Days(7), "agent", 0);
        assert_eq!(week["totals"]["output"], 300);
        let days = week["days"].as_array().unwrap();
        assert_eq!(days.len(), 7);
        assert_eq!(days[6]["day"], "2026-10-29");
        assert_eq!(days[5]["usage"]["output"], 100);
        assert_eq!(days[2]["usage"]["output"], 100);
        assert_eq!(
            get(&idx, Range::Days(30), "agent", 0)["totals"]["output"],
            300
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn claude_limit_hit_until_next_success() {
        let (d, mut idx) = home("hit");
        let t = d.join("claude/projects/-w-app/s4.jsonl");
        let at = |m: i64| format_rfc3339(real_now() - m * 60_000);
        append(
            &t,
            &format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": at(30), "message": {"id": "ok1", "model": "claude-opus-5-5", "usage": {"input_tokens": 1, "output_tokens": 1}}})
            ),
        );
        append(
            &t,
            &format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": at(10), "error": "rate_limit", "isApiErrorMessage": true, "message": {"id": "e1", "model": "<synthetic>", "content": [{"type": "text", "text": "You've hit your session limit · resets 7pm"}], "usage": {"input_tokens": 0, "output_tokens": 0}}})
            ),
        );
        idx.sweep(real_now(), true, None).unwrap();
        let q = |idx: &Index| {
            query(
                &idx.conn,
                &Query {
                    range: Range::Today,
                    by: "agent",
                    tz_offset_min: 0,
                    session: None,
                    now: real_now(),
                    oauth: false,
                    tap: None,
                },
                &HashMap::new(),
            )
            .unwrap()
        };
        let v = q(&idx);
        let hit = &agent_of(&v, "claude")["limits"]["hit"];
        assert_eq!(hit["message"], "You've hit your session limit · resets 7pm");
        assert!(agent_of(&v, "claude")["limits"]["windows"]
            .as_array()
            .unwrap()
            .is_empty());
        append(
            &t,
            &format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": at(1), "message": {"id": "ok2", "model": "claude-opus-5-5", "usage": {"input_tokens": 1, "output_tokens": 1}}})
            ),
        );
        idx.sweep(real_now(), false, None).unwrap();
        assert!(agent_of(&q(&idx), "claude")["limits"]["hit"].is_null());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn old_files_are_skipped() {
        let (d, mut idx) = home("old");
        let t = d.join("claude/projects/-w-app/old.jsonl");
        std::fs::copy(fixture("claude.jsonl"), &t).unwrap();
        let f = std::fs::File::options().append(true).open(&t).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + Duration::from_millis((now() - 40 * DAY_MS) as u64))
            .unwrap();
        idx.sweep(now(), true, None).unwrap();
        assert!(idx.files.is_empty());
        std::fs::remove_dir_all(&d).unwrap();
    }
    #[test]
    fn claude_plan_limits_from_oauth_fetch() {
        let (d, mut idx) = home("oauth");
        let t = d.join("claude/projects/-w-app/s1.jsonl");
        append(
            &t,
            &format!(
                "{}\n",
                json!({"type": "assistant", "timestamp": format_rfc3339(now() - 20 * 60_000), "message": {"id": "m1", "model": "claude-opus-5-5", "usage": {"input_tokens": 1, "output_tokens": 10}}})
            ),
        );
        idx.sweep(now(), true, None).unwrap();
        assert!(last_claude_activity(&idx.conn, now()).is_some_and(|t| now() - t < ACTIVE_MS));
        assert!(last_claude_activity(&idx.conn, now() + 2 * ACTIVE_MS).is_none());

        let claude = d.join("claude");
        let creds = |exp: i64| {
            std::fs::write(
                claude.join(".credentials.json"),
                json!({"claudeAiOauth": {"accessToken": "tok-secret", "expiresAt": exp, "subscriptionType": "max"}}).to_string(),
            )
            .unwrap()
        };
        let lim_tap = |idx: &Index, oauth: bool, tap: Option<Limits>| {
            query(
                &idx.conn,
                &Query {
                    range: Range::Today,
                    by: "agent",
                    tz_offset_min: 0,
                    session: None,
                    now: now(),
                    oauth,
                    tap,
                },
                &HashMap::new(),
            )
            .map(|v| agent_of(&v, "claude")["limits"].clone())
            .unwrap()
        };
        let lim = |idx: &Index, oauth: bool| {
            query(
                &idx.conn,
                &Query {
                    range: Range::Today,
                    by: "agent",
                    tz_offset_min: 0,
                    session: None,
                    now: now(),
                    oauth,
                    tap: None,
                },
                &HashMap::new(),
            )
            .map(|v| agent_of(&v, "claude")["limits"].clone())
            .unwrap()
        };

        assert!(!refresh_claude(&idx.conn, &claude, now(), |_| unreachable!()).unwrap());

        creds(now() + 3_600_000);
        let body = std::fs::read(fixture("claude-oauth-usage.json")).unwrap();
        assert!(refresh_claude(&idx.conn, &claude, now(), |_| Some((200, body))).unwrap());
        let v = lim(&idx, true);
        assert_eq!(v["source"], "claude");
        assert_eq!(v["plan"], "max");
        assert_eq!(v["auth"], "ok");
        let w: Vec<_> = v["windows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| {
                (
                    w["key"].as_str().unwrap().to_string(),
                    w["usedPercent"].as_f64().unwrap(),
                )
            })
            .collect();
        assert_eq!(w, [("5h".to_string(), 86.0), ("wk".to_string(), 48.0)]);
        assert_eq!(v["windows"][0]["resetsAt"], "2026-10-30T00:20:00Z");
        assert!(lim(&idx, false)["windows"].as_array().unwrap().is_empty());
        assert!(lim(&idx, false).get("auth").is_none());

        creds(now() - 1);
        assert!(refresh_claude(&idx.conn, &claude, now(), |_| unreachable!()).unwrap());
        let v = lim(&idx, true);
        assert_eq!(v["auth"], "expired");
        assert_eq!(v["windows"].as_array().unwrap().len(), 2);
        assert!(!v.to_string().contains("tok-secret"));

        let tapped = Limits {
            at: now(),
            windows: vec![Window {
                key: "5h".into(),
                minutes: 300,
                used: 97.0,
                resets_at: now() + 3_600_000,
            }],
            plan: None,
            ..Limits::default()
        };
        let v = lim_tap(&idx, true, Some(tapped.clone()));
        assert_eq!(v["via"], "statusline");
        assert_eq!(v["windows"][0]["usedPercent"], 97.0);
        assert!(v.get("auth").is_none());
        let v = lim_tap(&idx, false, Some(tapped.clone()));
        assert_eq!(v["source"], "claude");
        assert_eq!(v["windows"].as_array().unwrap().len(), 1);
        let old = Limits { at: 0, ..tapped };
        assert_eq!(lim_tap(&idx, true, Some(old))["via"], "claude");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn project_labels_are_the_repo_name_not_its_remote_or_path() {
        assert_eq!(repo_label("github.com/acme/app"), "app");
        assert_eq!(repo_label("local:/home/u/work/proj"), "proj");
        assert_eq!(repo_label("app-repo"), "app-repo");
        assert_eq!(repo_label(""), "");
    }

    #[test]
    fn api_validates_then_indexes_in_the_background() {
        let (d, _) = home("api");
        let t = d.join("claude/projects/-w-app/s1.jsonl");
        for (i, id) in ["m1", "m1", "m2"].iter().enumerate() {
            append(
                &t,
                &format!(
                    "{}\n",
                    json!({"type": "assistant", "timestamp": format_rfc3339(real_now() - 60_000 + i as i64 * 1000), "message": {"id": id, "model": "claude-opus-5-5", "usage": {"input_tokens": 1, "output_tokens": 10}}})
                ),
            );
        }
        init(
            &d,
            Dirs {
                claude: d.join("claude"),
                codex: d.join("codex"),
                gemini: d.join("gemini"),
                vibe: d.join("vibe"),
                copilot: d.join("copilot"),
                opencode: d.join("opencode"),
                agy: d.join("agy"),
            },
        );
        let req = |q: &[(&str, &str)]| crate::http::Request {
            method: "GET".into(),
            path: "/v1/usage".into(),
            query: q
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            headers: vec![],
            body: vec![],
        };
        assert_eq!(api(&req(&[("range", "1y")]), &[]).0, 400);
        assert_eq!(api(&req(&[("by", "repo")]), &[]).0, 400);
        assert_eq!(api(&req(&[("tzOffset", "9999")]), &[]).0, 400);
        assert_eq!(api(&req(&[("line", "nope")]), &[]).0, 404);
        let line = crate::Line {
            id: "L1".into(),
            agent: "claude",
            session_id: "s1".into(),
            repo: "app-repo".into(),
            ..Default::default()
        };
        let q = [("range", "30d"), ("by", "session"), ("line", "L1")];
        let deadline = real_now() + 10_000;
        let v = loop {
            let (code, v) = api(&req(&q), std::slice::from_ref(&line));
            assert_eq!(code, 200);
            if v["indexing"] == false && v["groups"].as_array().is_some_and(|g| !g.is_empty()) {
                break v;
            }
            assert!(real_now() < deadline, "{v}");
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(v["groups"][0]["lineId"], "L1");
        assert_eq!(v["groups"][0]["project"], "app-repo");
        assert_eq!(v["totals"]["messages"], 2);
        assert_eq!(v["range"], "30d");
        assert_eq!(v["days"].as_array().unwrap().len(), 30);
        let mode = std::fs::metadata(d.join("usage.db")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(mode.permissions().mode() & 0o777, 0o600);
        }
        let _ = mode;
    }
}
