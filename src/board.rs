use crate::actions::{find_line, log_action};
use crate::daemon::{dry_run, q, respond_raw, Request};
use crate::git::git_out;
use crate::sys::OpenOptionsExt;
use crate::util::{format_rfc3339, parse_rfc3339};
use crate::{scan, Ctx, Line, RECENT_MS};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const INDEX: &str = include_str!("../board/index.html");
const JS: &str = include_str!("../board/board.js");
const CSS: &str = include_str!("../board/board.css");
const OFFLINE_AFTER_MS: i64 = 120_000;
const DIFF_CAP: usize = 512 * 1024;
const REPLAY: usize = 256;

#[derive(Default)]
struct Bus {
    next: u64,
    subs: Vec<Sender<Arc<String>>>,
    recent: VecDeque<(u64, Arc<String>)>,
}

static BUS: Mutex<Option<Bus>> = Mutex::new(None);
type Seen = (HashMap<String, (String, String)>, HashMap<String, bool>);
static PREV: Mutex<Option<Seen>> = Mutex::new(None);

pub fn subscribe(after: Option<u64>) -> (Receiver<Arc<String>>, Vec<Arc<String>>) {
    let (tx, rx) = channel();
    let mut g = BUS.lock().unwrap_or_else(|e| e.into_inner());
    let bus = g.get_or_insert_with(Bus::default);
    let backlog = after
        .map(|a| {
            bus.recent
                .iter()
                .filter(|(id, _)| *id > a)
                .map(|(_, e)| e.clone())
                .collect()
        })
        .unwrap_or_default();
    bus.subs.push(tx);
    (rx, backlog)
}

pub const SSE_PING: &[u8] = b": ping\n\n";

pub fn sse_frame(id: u64, kind: &str, env: &Value) -> String {
    format!("id: {id}\nevent: {kind}\ndata: {env}\n\n")
}

pub fn publish(kind: &str, data: Value) {
    let mut g = BUS.lock().unwrap_or_else(|e| e.into_inner());
    let bus = g.get_or_insert_with(Bus::default);
    bus.next += 1;
    let id = bus.next;
    let env = json!({"id": id.to_string(), "type": kind, "at": format_rfc3339(crate::procs::now_ms()), "data": data});
    let frame = Arc::new(sse_frame(id, kind, &env));
    bus.subs.retain(|s| s.send(frame.clone()).is_ok());
    bus.recent.push_back((id, frame));
    if bus.recent.len() > REPLAY {
        bus.recent.pop_front();
    }
}

pub fn subscribers() -> usize {
    BUS.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or(0, |b| b.subs.len())
}

struct Station {
    id: String,
    last_seen_ms: i64,
    lines: Vec<Value>,
}

fn stations(ctx: &Ctx) -> Vec<Station> {
    let Ok(dir) = std::fs::read_dir(ctx.state_dir.join(crate::paths::STATIONS_DIR)) else {
        return vec![];
    };
    let mut out: Vec<Station> = dir
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice::<Value>(&std::fs::read(e.path()).ok()?).ok())
        .filter_map(|v| {
            let id = v["stationId"].as_str()?.to_string();
            (id != ctx.config.station_id).then(|| Station {
                last_seen_ms: v["lastSeenAt"]
                    .as_str()
                    .and_then(parse_rfc3339)
                    .unwrap_or(0),
                lines: v["lines"].as_array().cloned().unwrap_or_default(),
                id,
            })
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

pub fn remote_station(ctx: &Ctx, line_id: &str) -> Option<(String, bool, i64)> {
    stations(ctx).into_iter().find_map(|s| {
        s.lines
            .iter()
            .any(|v| v.get("line").unwrap_or(v)["id"] == line_id)
            .then(|| {
                (
                    s.id.clone(),
                    ctx.now_ms - s.last_seen_ms <= OFFLINE_AFTER_MS,
                    s.last_seen_ms,
                )
            })
    })
}

fn yard_url(ctx: &Ctx) -> Option<String> {
    let v: Value =
        serde_json::from_slice(&std::fs::read(ctx.state_dir.join("config.json")).ok()?).ok()?;
    v["yardUrl"]
        .as_str()
        .map(|s| s.trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
}

fn entries(ctx: &Ctx, lines: &[Line], all: bool) -> (Vec<Value>, Vec<Value>) {
    let now = ctx.now_ms;
    let local: Vec<&Line> = lines.iter().filter(|l| all || l.recent(now)).collect();
    let count =
        |it: &mut dyn Iterator<Item = &str>| it.filter(|s| *s == crate::wire::STATE_ACTIVE).count();
    let mut sts = vec![json!({
        "stationId": ctx.config.station_id, "local": true, "online": true,
        "lastSeenAt": format_rfc3339(now),
        "activeCount": count(&mut lines.iter().map(|l| l.state)),
    })];
    let relay = crate::remote::user(&ctx.state_dir).is_some();
    let mut out: Vec<Value> = local
        .iter()
        .map(|l| {
            let mut v = l.to_json();
            v["online"] = json!(true);
            v["actionable"] = json!(true);
            v
        })
        .collect();
    for s in stations(ctx) {
        let online = now - s.last_seen_ms <= OFFLINE_AFTER_MS;
        let seen = format_rfc3339(s.last_seen_ms);
        let actionable = online && relay;
        let reason = if actionable {
            String::new()
        } else if online {
            format!(
                "line runs on station {}; run `overseer login` here to act through the api",
                s.id
            )
        } else {
            format!("station {} offline since {seen}", s.id)
        };
        let ls: Vec<Value> = s
            .lines
            .into_iter()
            .map(|v| {
                if v.get("line").is_some() {
                    v["line"].clone()
                } else {
                    v
                }
            })
            .filter(|l| l["id"].is_string())
            .collect();
        sts.push(json!({
            "stationId": s.id, "local": false, "online": online, "lastSeenAt": seen,
            "activeCount": count(&mut ls.iter().map(|l| l["state"].as_str().unwrap_or(""))),
        }));
        for mut l in ls {
            let at = l["lastActivityAt"]
                .as_str()
                .and_then(parse_rfc3339)
                .unwrap_or(0);
            if !all && l["state"] != crate::wire::STATE_ACTIVE && now - at > RECENT_MS {
                continue;
            }
            l["stationId"] = json!(s.id);
            let mut e =
                json!({"line": l, "online": online, "actionable": actionable, "remote": true});
            if !actionable {
                e["reason"] = json!(reason);
            }
            out.push(e);
        }
    }
    (sts, out)
}

pub fn board(ctx: &Ctx, all: bool) -> Result<Value, String> {
    let lines = crate::current_lines(ctx)?;
    let (stations, lines) = entries(ctx, &lines, all);
    Ok(json!({
        "generatedAt": format_rfc3339(ctx.now_ms),
        "stationId": ctx.config.station_id,
        "yardUrl": yard_url(ctx),
        "stations": stations,
        "lines": lines,
    }))
}

pub fn observe(ctx: &Ctx, lines: &[Line]) {
    let (sts, entries) = entries(ctx, lines, false);
    let mut g = PREV.lock().unwrap_or_else(|e| e.into_inner());
    let first = g.is_none();
    let (prev, prev_st) = g.get_or_insert_with(Default::default);
    let mut seen = HashMap::new();
    for e in &entries {
        let l = &e["line"];
        let id = l["id"].as_str().unwrap_or_default().to_string();
        let state = l["state"].as_str().unwrap_or_default().to_string();
        let body = change_key(e);
        let local = event_local(e);
        if !first {
            match prev.get(&id) {
                None => publish("line.created", json!({"line": l, "local": local})),
                Some((s, _)) if *s != state => {
                    publish(
                        "line.state_changed",
                        json!({"line": l, "local": local, "from": s, "to": state}),
                    );
                    if state == crate::wire::STATE_HUNG_UP {
                        publish("line.hung_up", json!({"line": l}));
                    }
                }
                Some((_, b)) if *b != body => {
                    publish("line.updated", json!({"line": l, "local": local}))
                }
                _ => {}
            }
        }
        seen.insert(id, (state, body));
    }
    for s in &sts {
        let id = s["stationId"].as_str().unwrap_or_default().to_string();
        let on = s["online"] == true;
        if !first && prev_st.get(&id).is_some_and(|p| *p != on) {
            let kind = if on {
                "station.online"
            } else {
                "station.offline"
            };
            publish(
                kind,
                json!({"stationId": id, "lastSeenAt": s["lastSeenAt"]}),
            );
        }
        prev_st.insert(id, on);
    }
    *prev = seen;
}

fn event_local(entry: &Value) -> Value {
    let mut local = match entry.get("local") {
        Some(l) if l.is_object() => l.clone(),
        _ => return json!({}),
    };
    for k in [
        "holdPending",
        "heldReason",
        "heldAt",
        "title",
        "termId",
        "pid",
    ] {
        if local.get(k).is_none() {
            local[k] = Value::Null;
        }
    }
    local
}

fn change_key(entry: &Value) -> String {
    let mut v = entry.clone();
    if let Some(local) = v.get_mut("local").and_then(Value::as_object_mut) {
        local.remove("rssBytes");
    }
    v.to_string()
}

fn worktree_line(ctx: &Ctx, id: &str, fresh: bool) -> Result<Line, (u16, String)> {
    let lines = if fresh {
        scan(ctx)
    } else {
        crate::daemon::cached_lines(ctx, Some(id))
    }
    .map_err(|e| (500, e))?;
    let mut l = find_line(&lines, id).map_err(|e| (404, e))?.clone();
    let Some(top) = crate::review::line_dir(&l)
        .and_then(|d| crate::git::git(d, &["rev-parse", "--show-toplevel"]))
        .filter(|t| !t.is_empty())
    else {
        return Err((
            409,
            format!(
                "worktree {} does not exist on this station",
                l.worktree_path
            ),
        ));
    };
    l.worktree_path = top;
    Ok(l)
}

type FileStamp = Option<(u64, i128)>;

#[derive(PartialEq)]
struct DiffStamp {
    status: String,
    refs: Option<crate::git::RefStamp>,
    index: FileStamp,
    files: Vec<FileStamp>,
}

type Diffs = HashMap<String, (DiffStamp, String, String, bool)>;

static DIFFS: Mutex<Option<Diffs>> = Mutex::new(None);

fn stat_of(p: &Path) -> FileStamp {
    let m = std::fs::symlink_metadata(p).ok()?;
    let t = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i128);
    Some((m.len(), t))
}

fn diff_stamp(wt: &str, status: &str) -> DiffStamp {
    let files = status
        .lines()
        .filter_map(|l| l.get(3..))
        .map(|p| p.rsplit(" -> ").next().unwrap_or(p).trim_matches('"'))
        .map(|p| stat_of(&Path::new(wt).join(p)))
        .collect();
    DiffStamp {
        status: status.to_string(),
        refs: crate::git::ref_stamp(wt, None),
        index: crate::git::git_dirs(wt).and_then(|(gd, _)| stat_of(&gd.join("index"))),
        files,
    }
}

fn diff(ctx: &Ctx, id: &str) -> Result<Value, (u16, String)> {
    let l = worktree_line(ctx, id, false)?;
    let wt = l.worktree_path.as_str();
    let g = |a: &[&str]| git_out(wt, a).map_err(|e| (409, e));
    let status = g(&["status", "--porcelain"])?;
    let stamp = diff_stamp(wt, &status);
    let hit = DIFFS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(wt))
        .filter(|c| c.0 == stamp && stamp.refs.is_some())
        .map(|c| (c.1.clone(), c.2.clone(), c.3));
    let (stat, d, truncated) = match hit {
        Some(h) => h,
        None => {
            let base = if git_out(wt, &["rev-parse", "--verify", "-q", "HEAD"]).is_ok() {
                vec!["HEAD"]
            } else {
                vec![]
            };
            let with = |a: &[&'static str]| [a, base.as_slice()].concat();
            let mut d = g(&with(&["diff", "--no-ext-diff"]))?;
            let truncated = d.len() > DIFF_CAP;
            if truncated {
                let mut cut = DIFF_CAP;
                while !d.is_char_boundary(cut) {
                    cut -= 1;
                }
                d.truncate(cut);
            }
            let stat = g(&with(&["diff", "--no-ext-diff", "--stat"]))?;
            let mut c = DIFFS.lock().unwrap_or_else(|e| e.into_inner());
            let m = c.get_or_insert_with(HashMap::new);
            if m.len() >= 8 && !m.contains_key(wt) {
                m.clear();
            }
            m.insert(wt.to_string(), (stamp, stat.clone(), d.clone(), truncated));
            (stat, d, truncated)
        }
    };
    Ok(json!({
        "lineId": l.id, "worktree": wt,
        "status": status,
        "stat": stat,
        "diff": d, "truncated": truncated,
    }))
}

fn change(ctx: &Ctx, r: &Request, id: &str, kind: &str) -> Result<Value, (u16, String)> {
    let l = worktree_line(ctx, id, true)?;
    let wt = l.worktree_path.as_str();
    let message = serde_json::from_slice::<Value>(&r.body)
        .ok()
        .and_then(|b| b["message"].as_str().map(|m| m.trim().to_string()))
        .unwrap_or_default();
    if kind == "commit" && message.is_empty() {
        return Err((400, "message required".into()));
    }
    if l.busy || l.state == crate::wire::STATE_ACTIVE {
        return Err((
            409,
            format!(
                "line {} is mid-turn; wait until the agent is at its prompt",
                l.id
            ),
        ));
    }
    let files: Vec<String> = git_out(wt, &["status", "--porcelain"])
        .map_err(|e| (409, e))?
        .lines()
        .map(str::to_string)
        .collect();
    if files.is_empty() {
        return Err((
            409,
            "nothing to commit or stash: the worktree is clean".into(),
        ));
    }
    let dry = dry_run(r);
    let mut v = json!({"lineId": l.id, "dryRun": dry, "files": files});
    if !dry {
        let res = if kind == "commit" {
            git_out(wt, &["add", "-A"])
                .and_then(|_| git_out(wt, &["commit", "-q", "-m", &message]))
                .and_then(|_| git_out(wt, &["rev-parse", "HEAD"]))
                .map(|sha| v["commit"] = json!(sha.trim()))
        } else {
            git_out(
                wt,
                &["stash", "push", "-u", "-m", &format!("overseer {}", l.id)],
            )
            .and_then(|_| git_out(wt, &["stash", "list", "-1", "--format=%gd %s"]))
            .map(|s| v["stash"] = json!(s.trim()))
        };
        log_action(
            ctx,
            &json!({"action": kind, "dryRun": false, "lineId": l.id, "result": res.as_ref().map(|_| v.clone()).unwrap_or_else(|e| json!({"error": e}))}),
        );
        res.map_err(|e| (409, e))?;
    }
    Ok(v)
}

pub fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 3 <= b.len() => match s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                Some(x) => {
                    out.push(x);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn sse(r: &Request, s: &mut dyn Write) {
    let after = r
        .headers
        .iter()
        .find(|(k, _)| k == "last-event-id")
        .and_then(|(_, v)| v.parse().ok());
    let (rx, backlog) = subscribe(after);
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\nretry: 2000\n\n";
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    for f in backlog {
        if s.write_all(f.as_bytes()).is_err() {
            return;
        }
    }
    let _ = s.flush();
    loop {
        let out = match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(f) => s.write_all(f.as_bytes()),
            Err(RecvTimeoutError::Timeout) => s.write_all(SSE_PING),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if out.and_then(|_| s.flush()).is_err() {
            return;
        }
    }
}

pub fn serve(ctx: &Ctx, r: &Request, s: &mut dyn Write) -> bool {
    let segs: Vec<&str> = r.path.trim_matches('/').split('/').collect();
    let json = |s: &mut dyn Write, res: Result<Value, (u16, String)>| {
        let (code, v) = res
            .map(|v| (200, v))
            .unwrap_or_else(|(c, e)| (c, json!({"error": e})));
        respond_raw(s, code, "application/json", v.to_string().as_bytes());
    };
    match (r.method.as_str(), segs.as_slice()) {
        ("GET", ["board"]) => respond_raw(s, 200, "text/html; charset=utf-8", INDEX.as_bytes()),
        ("GET", ["board", "board.js"]) => {
            respond_raw(s, 200, "text/javascript; charset=utf-8", JS.as_bytes())
        }
        ("GET", ["board", "board.css"]) => {
            respond_raw(s, 200, "text/css; charset=utf-8", CSS.as_bytes())
        }
        ("GET", ["v1", "events"]) => sse(r, s),
        ("GET", ["v1", "board"]) => json(s, board(ctx, r.is_true("all")).map_err(|e| (500, e))),
        ("GET", ["v1", "pr"]) => {
            let (repo, branch) = (r.param("repo"), r.param("branch"));
            match (repo, branch) {
                (Some(repo), Some(branch)) => json(s, Ok(crate::gh::pr_info(&repo, &branch))),
                _ => json(s, Err((400, "repo and branch required".into()))),
            }
        }
        ("GET", ["v1", "lines", id, "diff"]) => json(s, diff(ctx, id)),
        ("POST", ["v1", "lines", id, kind @ ("commit" | "stash")]) => {
            json(s, change(ctx, r, id, kind))
        }
        _ => return false,
    }
    true
}

pub fn url_file(state_dir: &Path) -> PathBuf {
    state_dir.join(crate::paths::BOARD_FILE)
}

pub fn token() -> Result<String, String> {
    let mut b = [0u8; 16];
    crate::sys::random(&mut b).map_err(|e| format!("random: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

pub fn authorized(r: &Request, token: &str) -> bool {
    let host_ok = r
        .headers
        .iter()
        .find(|(k, _)| k == "host")
        .map(|(_, h)| h.rsplit_once(':').map_or(h.as_str(), |x| x.0))
        .is_some_and(|h| matches!(h, "127.0.0.1" | "localhost" | "[::1]"));
    let public =
        r.method == "GET" && matches!(r.path.as_str(), "/board/board.js" | "/board/board.css");
    let bearer = r
        .headers
        .iter()
        .find(|(k, _)| k == "authorization")
        .and_then(|(_, v)| v.strip_prefix("Bearer "));
    let given = q(r, "token").or(bearer).unwrap_or_default().as_bytes();
    let same = given.len() == token.len()
        && given
            .iter()
            .zip(token.as_bytes())
            .fold(0, |a, (x, y)| a | (x ^ y))
            == 0;
    host_ok && (public || same)
}

pub fn serve_http(addr: &str, state_dir: &Path) -> Result<String, String> {
    let sa: SocketAddr = addr
        .parse()
        .map_err(|_| format!("--http wants 127.0.0.1:<port>, got {addr}"))?;
    if !sa.ip().is_loopback() {
        return Err(format!("--http must be a loopback address, got {addr}"));
    }
    let l = TcpListener::bind(sa).map_err(|e| format!("bind {addr}: {e}"))?;
    let port = l.local_addr().map_err(|e| e.to_string())?.port();
    let tok = token()?;
    let url = format!("http://{}:{port}/board?token={tok}", sa.ip());
    let f = url_file(state_dir);
    let _ = std::fs::remove_file(&f);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&f)
        .and_then(|mut w| w.write_all(json!({"url": url}).to_string().as_bytes()))
        .map_err(|e| format!("{}: {e}", f.display()))?;
    crate::util::supervise("board", move || {
        for st in l.incoming() {
            match st {
                Ok(st) => {
                    let _ = st.set_read_timeout(Some(crate::daemon::HEAD_TIMEOUT));
                    let _ = st.set_write_timeout(Some(Duration::from_secs(5)));
                    crate::daemon::accept(st, Some(tok.clone()));
                }
                Err(e) => crate::daemon::accept_failed(&e),
            }
        }
    });
    Ok(url)
}

pub fn cli(state_dir: &Path) -> Result<String, String> {
    let sock = crate::daemon::socket_path(state_dir);
    let (code, _) = crate::daemon::request(&sock, "GET", "/v1/status", None).map_err(|e| {
        format!("daemon not reachable ({e}); start `overseer daemon --http 127.0.0.1:0`")
    })?;
    if code != 200 {
        return Err(format!("daemon answered {code}"));
    }
    let url = std::fs::read(url_file(state_dir))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["url"].as_str().map(str::to_string));
    let mut out = format!(
        "board on the daemon socket: GET /board via {}\n",
        sock.display()
    );
    match url {
        Some(u) => out.push_str(&format!("{u}\n")),
        None => out.push_str(
            "no loopback board: restart the daemon with `--http 127.0.0.1:0` for a browser URL\n",
        ),
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_frames_are_id_event_data_and_a_blank_line() {
        let f = sse_frame(7, "x.y", &json!({"a": 1}));
        assert_eq!(f, "id: 7\nevent: x.y\ndata: {\"a\":1}\n\n");
        assert_eq!(SSE_PING, b": ping\n\n");
    }

    #[test]
    fn diff_stamp_changes_with_edits_commits_and_staging() {
        let d = std::env::temp_dir().join(format!("sb-diffstamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let wt = d.canonicalize().unwrap().to_string_lossy().into_owned();
        let run = |a: &[&str]| {
            git_out(
                &wt,
                &[
                    &["-c", "user.email=t@e.x", "-c", "user.name=t"][..],
                    &[
                        "-c",
                        "commit.gpgsign=false",
                        "-c",
                        "core.hooksPath=/dev/null",
                    ][..],
                    a,
                ]
                .concat(),
            )
            .unwrap()
        };
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(d.join("a.txt"), "one\n").unwrap();
        run(&["add", "a.txt"]);
        run(&["commit", "-q", "-m", "a"]);
        std::fs::write(d.join("a.txt"), "two\n").unwrap();
        let st = run(&["status", "--porcelain"]);
        let s1 = diff_stamp(&wt, &st);
        assert!(s1.refs.is_some());
        assert!(s1 == diff_stamp(&wt, &st));
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(d.join("a.txt"), "three\n").unwrap();
        let s2 = diff_stamp(&wt, &run(&["status", "--porcelain"]));
        assert!(s1 != s2);
        run(&["add", "a.txt"]);
        let s3 = diff_stamp(&wt, &run(&["status", "--porcelain"]));
        assert!(s2 != s3);
        run(&["commit", "-q", "-m", "b"]);
        let s4 = diff_stamp(&wt, &run(&["status", "--porcelain"]));
        assert!(s3 != s4);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn memory_drift_is_not_a_line_change() {
        let a = json!({"line": {"id": "l1", "state": "active"}, "local": {"busy": false, "rssBytes": 1000}});
        let b = json!({"line": {"id": "l1", "state": "active"}, "local": {"busy": false, "rssBytes": 2000}});
        let c = json!({"line": {"id": "l1", "state": "active"}, "local": {"busy": true, "rssBytes": 2000}});
        assert_eq!(change_key(&a), change_key(&b));
        assert_ne!(change_key(&b), change_key(&c));
    }
}
