use crate::sys::OpenOptionsExt;
use crate::util::format_rfc3339;
use crate::{load_config, mkdir_private, procs::System, scan, Ctx};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::{HandshakeError, Message, WebSocket};

pub const HEARTBEAT: Duration = Duration::from_secs(15);
use crate::wire::*;

pub trait Transport {
    fn send(&mut self, v: &Value) -> Result<(), String>;
    fn recv(&mut self, wait: Duration) -> Result<Option<Value>, String>;
}

pub struct Ws(WebSocket<TcpStream>);

pub(crate) fn endpoint(api: &str) -> Result<(String, SocketAddr), String> {
    let base = api
        .strip_prefix("http://")
        .or_else(|| api.strip_prefix("ws://"))
        .ok_or("the api URL must start with http:// (no TLS in the daemon, see ADR 0009)")?
        .trim_end_matches('/');
    let host = base.split('/').next().unwrap_or(base);
    let addr = if host
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    let sa = addr
        .to_socket_addrs()
        .map_err(|e| format!("{addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("{addr}: no address"))?;
    Ok((base.to_string(), sa))
}

impl Ws {
    pub fn connect(api: &str, station: &str, token: &str) -> Result<Ws, String> {
        let (base, sa) = endpoint(api)?;
        let addr = sa.to_string();
        let sock = TcpStream::connect_timeout(&sa, Duration::from_secs(5))
            .map_err(|e| format!("{addr}: {e}"))?;
        sock.set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| e.to_string())?;
        let mut req = format!("ws://{base}/v1/stations/{station}/sync")
            .into_client_request()
            .map_err(|e| e.to_string())?;
        req.headers_mut().insert(
            "authorization",
            format!("Bearer {token}")
                .parse()
                .map_err(|_| "invalid token")?,
        );
        match tungstenite::client(req, sock) {
            Ok((ws, _)) => Ok(Ws(ws)),
            Err(HandshakeError::Failure(tungstenite::Error::Http(r))) => {
                Err(format!("api refused the station: HTTP {}", r.status()))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Transport for Ws {
    fn send(&mut self, v: &Value) -> Result<(), String> {
        self.0
            .send(Message::text(v.to_string()))
            .map_err(|e| e.to_string())
    }

    fn recv(&mut self, wait: Duration) -> Result<Option<Value>, String> {
        let _ = self
            .0
            .get_mut()
            .set_read_timeout(Some(wait.max(Duration::from_millis(10))));
        match self.0.read() {
            Ok(Message::Text(t)) => Ok(serde_json::from_str(t.as_str()).ok()),
            Ok(Message::Close(_)) => Err("api closed the connection".into()),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS sync_meta (k TEXT PRIMARY KEY, v INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS sync_outbox (seq INTEGER PRIMARY KEY, frame TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sync_pushed (id TEXT PRIMARY KEY, body TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sync_commands (
  id TEXT PRIMARY KEY,
  body TEXT NOT NULL,
  received_at TEXT NOT NULL,
  reported TEXT
);
";

pub struct Client {
    db: Connection,
    state_dir: PathBuf,
    station: String,
    conn: Option<Box<dyn Transport>>,
}

pub(crate) fn private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub(crate) fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
}

pub fn inbox(state_dir: &Path) -> PathBuf {
    state_dir.join("commands").join("inbox")
}

pub fn done(state_dir: &Path) -> PathBuf {
    state_dir.join("commands").join("done")
}

pub fn complete(
    state_dir: &Path,
    id: &str,
    status: &str,
    reason: Option<&str>,
    now_ms: i64,
) -> Result<(), String> {
    if !safe_id(id) || !STATUS_TERMINAL.contains(&status) {
        return Err("invalid command id or status".into());
    }
    mkdir_private(&done(state_dir)).map_err(|e| e.to_string())?;
    let mut v = json!({ "id": id, "status": status, "completedAt": format_rfc3339(now_ms) });
    if let Some(r) = reason {
        v["reason"] = json!(r);
    }
    private_file(
        &done(state_dir).join(format!("{id}.json")),
        v.to_string().as_bytes(),
    )
}

impl Client {
    pub fn open(state_dir: &Path, station: &str) -> Result<Client, String> {
        mkdir_private(state_dir).map_err(|e| e.to_string())?;
        let path = state_dir.join("sync.db");
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let db = Connection::open(&path).map_err(|e| e.to_string())?;
        db.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Client {
            db,
            state_dir: state_dir.into(),
            station: station.into(),
            conn: None,
        })
    }

    fn meta(&self, k: &str, default: i64) -> i64 {
        self.db
            .query_row("SELECT v FROM sync_meta WHERE k = ?1", [k], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or(default)
    }

    fn set_meta(&self, k: &str, v: i64) -> Result<(), String> {
        self.db
            .execute(
                "INSERT INTO sync_meta (k, v) VALUES (?1, ?2) ON CONFLICT (k) DO UPDATE SET v = excluded.v",
                params![k, v],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn acked(&self) -> i64 {
        self.meta("acked", 0)
    }

    pub fn unacked(&self) -> i64 {
        self.meta("next_seq", 1) - 1 - self.acked()
    }

    pub fn connected(&self) -> bool {
        self.conn.is_some()
    }

    pub fn stage(&self, lines: &[Value]) -> Result<Option<i64>, String> {
        let pushed: BTreeMap<String, String> = self
            .db
            .prepare("SELECT id, body FROM sync_pushed")
            .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
            .map_err(|e| e.to_string())?;
        let current: BTreeMap<String, &Value> = lines
            .iter()
            .filter_map(|l| Some((l["id"].as_str()?.to_string(), l)))
            .collect();
        let upsert: Vec<&Value> = current
            .iter()
            .filter(|(id, l)| pushed.get(*id) != Some(&l.to_string()))
            .map(|(_, l)| *l)
            .collect();
        if upsert.is_empty() {
            return Ok(None);
        }
        let seq = self.meta("next_seq", 1);
        let frame = json!({ "kind": FRAME_DELTA, "seq": seq, "upsert": upsert });
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let run = || -> rusqlite::Result<()> {
            tx.execute(
                "INSERT INTO sync_outbox (seq, frame) VALUES (?1, ?2)",
                params![seq, frame.to_string()],
            )?;
            for l in &upsert {
                tx.execute(
                    "INSERT INTO sync_pushed (id, body) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET body = excluded.body",
                    params![l["id"].as_str(), l.to_string()],
                )?;
            }
            Ok(())
        };
        run().map_err(|e| e.to_string())?;
        self.set_meta("next_seq", seq + 1)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(Some(seq))
    }

    pub fn frames_after(&self, api_seq: i64, resync: bool) -> Result<Vec<Value>, String> {
        let next = self.meta("next_seq", 1);
        let outbox: Vec<(i64, String)> = self
            .db
            .prepare("SELECT seq, frame FROM sync_outbox WHERE seq > ?1 ORDER BY seq")
            .and_then(|mut s| {
                s.query_map([api_seq], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .map_err(|e| e.to_string())?;
        let replayable = api_seq < next
            && match outbox.first() {
                Some((s, _)) => *s == api_seq + 1,
                None => next - 1 == api_seq,
            };
        if !resync && replayable {
            return Ok(outbox
                .into_iter()
                .filter_map(|(_, f)| serde_json::from_str(&f).ok())
                .collect());
        }
        let lines: Vec<Value> = self
            .db
            .prepare("SELECT body FROM sync_pushed ORDER BY id")
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(|e| e.to_string())?
            .iter()
            .filter_map(|b| serde_json::from_str(b).ok())
            .collect();
        self.db
            .execute("DELETE FROM sync_outbox", [])
            .map_err(|e| e.to_string())?;
        self.set_meta("next_seq", next + 1)?;
        Ok(vec![
            json!({ "kind": FRAME_SNAPSHOT, "seq": next, "lines": lines }),
        ])
    }

    fn on_ack(&self, seq: i64) -> Result<(), String> {
        self.set_meta("acked", seq)?;
        self.db
            .execute("DELETE FROM sync_outbox WHERE seq <= ?1", [seq])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn receive(&self, cmd: &Value, now_ms: i64) -> Result<Value, String> {
        let id = cmd["id"].as_str().unwrap_or_default();
        let mut echo = cmd.clone();
        if let Some(o) = echo.as_object_mut() {
            o.remove("args");
        }
        if !safe_id(id) {
            echo["status"] = json!(STATUS_REFUSED);
            echo["reason"] = json!("invalid command id");
            return Ok(json!({ "kind": FRAME_COMMAND_RESULT, "command": echo }));
        }
        let fresh = self
            .db
            .execute(
                "INSERT OR IGNORE INTO sync_commands (id, body, received_at) VALUES (?1, ?2, ?3)",
                params![id, echo.to_string(), format_rfc3339(now_ms)],
            )
            .map_err(|e| e.to_string())?
            == 1;
        if fresh {
            mkdir_private(&inbox(&self.state_dir)).map_err(|e| e.to_string())?;
            private_file(
                &inbox(&self.state_dir).join(format!("{id}.json")),
                cmd.to_string().as_bytes(),
            )?;
        }
        echo["status"] = json!(STATUS_DELIVERED);
        Ok(json!({ "kind": FRAME_COMMAND_RESULT, "command": echo }))
    }

    pub fn results(&self) -> Vec<(String, Value)> {
        let Ok(rd) = std::fs::read_dir(done(&self.state_dir)) else {
            return vec![];
        };
        let mut out = vec![];
        for path in rd.flatten().map(|e| e.path()) {
            let Some(r) = std::fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let Some(id) = r["id"].as_str() else { continue };
            let Ok((body, reported)) = self.db.query_row(
                "SELECT body, reported FROM sync_commands WHERE id = ?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            ) else {
                continue;
            };
            if reported.is_some() {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let mut c: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            for k in ["status", "reason", "completedAt"] {
                if !r[k].is_null() {
                    c[k] = r[k].clone();
                }
            }
            out.push((
                id.to_string(),
                json!({ "kind": FRAME_COMMAND_RESULT, "command": c }),
            ));
        }
        out
    }

    fn reported(&self, id: &str, now_ms: i64) {
        let _ = self.db.execute(
            "UPDATE sync_commands SET reported = ?2 WHERE id = ?1",
            params![id, format_rfc3339(now_ms)],
        );
        let _ = std::fs::remove_file(done(&self.state_dir).join(format!("{id}.json")));
    }

    pub fn handle(&self, v: &Value, now_ms: i64) -> Result<Vec<Value>, String> {
        match v["kind"].as_str() {
            Some(FRAME_ACK) => {
                let seq = v["seq"].as_i64().unwrap_or(0);
                if v[FRAME_RESYNC] == true {
                    self.frames_after(seq, true)
                } else {
                    self.on_ack(seq).map(|_| vec![])
                }
            }
            Some(FRAME_COMMAND) => Ok(vec![self.receive(&v["command"], now_ms)?]),
            Some(FRAME_STATIONS) => {
                write_stations(&self.state_dir, &self.station, v).map(|_| vec![])
            }
            _ => Ok(vec![]),
        }
    }

    fn io<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, String>) -> Result<T, String> {
        let r = f(self);
        if r.is_err() {
            self.conn = None;
        }
        r
    }

    fn send_all(&mut self, frames: &[Value]) -> Result<(), String> {
        let t = self.conn.as_mut().ok_or("not connected")?;
        frames.iter().try_for_each(|f| t.send(f))
    }

    pub fn connect(
        &mut self,
        mut t: Box<dyn Transport>,
        version: &str,
        now_ms: i64,
    ) -> Result<(), String> {
        t.send(&json!({
            "kind": FRAME_HELLO, "stationId": self.station, "daemonVersion": version,
            "lastAckedSeq": self.acked(),
        }))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        let ack = loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("no ack from the api".into());
            }
            match t.recv(left)? {
                Some(v) if v["kind"] == FRAME_ACK => break v,
                Some(v) => self
                    .handle(&v, now_ms)?
                    .iter()
                    .try_for_each(|f| t.send(f))?,
                None => {}
            }
        };
        let frames =
            self.frames_after(ack["seq"].as_i64().unwrap_or(0), ack[FRAME_RESYNC] == true)?;
        frames.iter().try_for_each(|f| t.send(f))?;
        self.conn = Some(t);
        Ok(())
    }

    pub fn push(&mut self, lines: &[Value]) -> Result<(), String> {
        let Some(seq) = self.stage(lines)? else {
            return Ok(());
        };
        if self.conn.is_none() {
            return Ok(());
        }
        let frame: Option<Value> = self
            .db
            .query_row("SELECT frame FROM sync_outbox WHERE seq = ?1", [seq], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|f| serde_json::from_str(&f).ok());
        self.io(|c| c.send_all(&frame.into_iter().collect::<Vec<_>>()))
    }

    pub fn pump(
        &mut self,
        wait: Duration,
        heartbeat: Option<Value>,
        now_ms: i64,
    ) -> Result<(), String> {
        self.io(|c| {
            if let Some(h) = heartbeat {
                c.send_all(&[h])?;
            }
            for (id, f) in c.results() {
                c.send_all(&[f])?;
                c.reported(&id, now_ms);
            }
            let deadline = Instant::now() + wait;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let t = c.conn.as_mut().ok_or("not connected")?;
                match t.recv(left)? {
                    Some(v) => {
                        let replies = c.handle(&v, now_ms)?;
                        c.send_all(&replies)?;
                        if v["kind"] == FRAME_COMMAND {
                            return Ok(());
                        }
                    }
                    None if Instant::now() >= deadline => return Ok(()),
                    None => {}
                }
            }
        })
    }

    pub fn heartbeat(&self, lines: &[Value]) -> Value {
        let mut h = json!({
            "kind": FRAME_HEARTBEAT,
            "seq": self.acked(),
            "activeCount": lines.iter().filter(|l| l["state"] == crate::wire::STATE_ACTIVE).count(),
        });
        if let Some((used, total)) = meminfo() {
            h["memUsedBytes"] = json!(used);
            h["memTotalBytes"] = json!(total);
        }
        h
    }
}

pub fn write_stations(state_dir: &Path, me: &str, v: &Value) -> Result<(), String> {
    let dir = state_dir.join("stations");
    mkdir_private(&dir).map_err(|e| e.to_string())?;
    let owner = load_config(state_dir).owner;
    let mut keep = vec![];
    for st in v["stations"].as_array().into_iter().flatten() {
        let Some(id) = st["stationId"].as_str().filter(|i| safe_id(i) && *i != me) else {
            continue;
        };
        let lines: Vec<&Value> = st["lines"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["owner"] == owner.as_str())
            .collect();
        let body = json!({ "stationId": id, "lastSeenAt": st["lastSeenAt"], "lines": lines });
        private_file(&dir.join(format!("{id}.json")), body.to_string().as_bytes())?;
        keep.push(format!("{id}.json"));
    }
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".json") && !keep.contains(&name) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

fn meminfo() -> Option<(u64, u64)> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb = |k: &str| -> Option<u64> {
        s.lines()
            .find_map(|l| l.strip_prefix(k))?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
            .map(|n| n * 1024)
    };
    let (total, avail) = (kb("MemTotal:")?, kb("MemAvailable:")?);
    Some((total.saturating_sub(avail), total))
}

fn creds(state_dir: &Path) -> Option<(String, String)> {
    let v: Value =
        serde_json::from_slice(&std::fs::read(state_dir.join("sync.json")).ok()?).ok()?;
    Some((
        v["api"].as_str()?.to_string(),
        v["token"].as_str()?.to_string(),
    ))
}

pub fn cli_enroll(args: &[String]) -> ExitCode {
    let api = match args {
        [flag, url]
            if flag == "--api" && (url.starts_with("http://") || url.starts_with("ws://")) =>
        {
            url
        }
        _ => {
            eprintln!("usage: overseer enroll --api http://HOST:PORT  (station token on stdin)");
            return ExitCode::from(2);
        }
    };
    let mut token = String::new();
    if std::io::stdin().read_line(&mut token).is_err() || token.trim().is_empty() {
        eprintln!("overseer: no station token on stdin");
        return ExitCode::from(2);
    }
    let state = crate::default_state_dir();
    let r = mkdir_private(&state)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            private_file(
                &state.join("sync.json"),
                json!({ "api": api, "token": token.trim() })
                    .to_string()
                    .as_bytes(),
            )
        });
    match r {
        Ok(()) => {
            println!(
                "station {} enrolled with {api}",
                load_config(&state).station_id
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("overseer: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn cli_sync(args: &[String]) -> ExitCode {
    let once = match args {
        [] => false,
        [a] if a == "--once" => true,
        _ => {
            eprintln!("usage: overseer sync [--once]");
            return ExitCode::from(2);
        }
    };
    let state = crate::default_state_dir();
    let Some((api, token)) = creds(&state) else {
        eprintln!("overseer: not enrolled, run `overseer enroll --api URL` first");
        return ExitCode::FAILURE;
    };
    let station = load_config(&state).station_id;
    let mut client = match Client::open(&state, &station) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("overseer: {e}");
            return ExitCode::FAILURE;
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    let mut backoff = Duration::from_secs(1);
    loop {
        let ctx = Ctx::from_env(&System);
        crate::inbox::process(&ctx);
        let scanned: Option<Vec<Value>> = match scan(&ctx) {
            Ok(l) => Some(l.iter().map(|l| l.to_json()["line"].clone()).collect()),
            Err(e) => {
                eprintln!("overseer: scan failed, nothing pushed: {e}");
                None
            }
        };
        if let Some(Err(e)) = scanned.as_ref().map(|l| client.push(l)) {
            eprintln!("overseer: sync: {e}");
        }
        let lines = scanned.as_deref().unwrap_or_default();
        if !client.connected() {
            let r = Ws::connect(&api, &station, &token)
                .and_then(|ws| client.connect(Box::new(ws), version, ctx.now_ms));
            if let Err(e) = r {
                eprintln!("overseer: sync: {e}");
            }
        }
        if client.connected() {
            backoff = Duration::from_secs(1);
            let hb = scanned.as_ref().map(|l| client.heartbeat(l));
            let wait = if once {
                Duration::from_millis(200)
            } else {
                HEARTBEAT
            };
            if let Err(e) = client.pump(wait, hb, ctx.now_ms) {
                eprintln!("overseer: sync: {e}");
            }
        }
        if once {
            if !crate::inbox::process(&Ctx::from_env(&System)).is_empty() {
                let _ = client.pump(Duration::from_millis(200), None, ctx.now_ms);
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while client.connected() && client.unacked() > 0 && Instant::now() < deadline {
                let _ = client.pump(Duration::from_millis(200), None, ctx.now_ms);
            }
            let ok = client.connected() && client.unacked() == 0;
            println!(
                "station {station}: {} lines, acked seq {}{}",
                lines.len(),
                client.acked(),
                if ok { "" } else { " (not fully synced)" }
            );
            return if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
        }
        if !client.connected() {
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Fake {
        incoming: Rc<RefCell<VecDeque<Value>>>,
        sent: Rc<RefCell<Vec<Value>>>,
    }

    impl Transport for Fake {
        fn send(&mut self, v: &Value) -> Result<(), String> {
            self.sent.borrow_mut().push(v.clone());
            Ok(())
        }
        fn recv(&mut self, _: Duration) -> Result<Option<Value>, String> {
            Ok(self.incoming.borrow_mut().pop_front())
        }
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-sync-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn line(id: &str, state: &str) -> Value {
        json!({ "id": id, "state": state, "stationId": "vm" })
    }

    fn kinds(f: &Fake) -> Vec<(String, i64)> {
        f.sent
            .borrow()
            .iter()
            .map(|v| {
                (
                    v["kind"].as_str().unwrap().to_string(),
                    v["seq"].as_i64().unwrap_or(-1),
                )
            })
            .collect()
    }

    #[test]
    fn stage_records_only_changes() {
        let d = dir("stage");
        let c = Client::open(&d, "vm").unwrap();
        assert_eq!(
            c.stage(&[line("a", "active"), line("b", "idle")]).unwrap(),
            Some(1)
        );
        assert_eq!(
            c.stage(&[line("a", "active"), line("b", "idle")]).unwrap(),
            None
        );
        assert_eq!(c.stage(&[line("a", "held")]).unwrap(), Some(2));
        let f = c.frames_after(1, false).unwrap();
        assert_eq!(f.len(), 1);
        assert_eq!(f[0]["upsert"][0]["state"], "held");
        assert!(f[0].get("remove").is_none());
        assert_eq!(c.stage(&[]).unwrap(), None);
        let snap = c.frames_after(0, true).unwrap();
        assert_eq!(snap[0]["lines"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn first_connect_sends_a_snapshot_then_replays_after_an_outage() {
        let d = dir("connect");
        let mut c = Client::open(&d, "vm").unwrap();
        c.stage(&[line("a", "active")]).unwrap();
        let f = Fake::default();
        f.incoming
            .borrow_mut()
            .push_back(json!({ "kind": "ack", "seq": 0, "resync": true }));
        c.connect(Box::new(f.clone()), "t", 0).unwrap();
        assert_eq!(kinds(&f), [("hello".into(), -1), ("snapshot".into(), 2)]);
        assert_eq!(f.sent.borrow()[1]["lines"][0]["id"], "a");
        f.incoming
            .borrow_mut()
            .push_back(json!({ "kind": "ack", "seq": 2 }));
        c.pump(Duration::ZERO, None, 0).unwrap();
        assert_eq!((c.acked(), c.unacked()), (2, 0));

        c.conn = None;
        c.stage(&[line("a", "idle")]).unwrap();
        c.stage(&[line("a", "held"), line("b", "active")]).unwrap();
        let f = Fake::default();
        f.incoming
            .borrow_mut()
            .push_back(json!({ "kind": "ack", "seq": 3, "resync": false }));
        c.connect(Box::new(f.clone()), "t", 0).unwrap();
        assert_eq!(f.sent.borrow()[0]["lastAckedSeq"], 2);
        assert_eq!(kinds(&f), [("hello".into(), -1), ("delta".into(), 4)]);

        let f = Fake::default();
        f.incoming
            .borrow_mut()
            .push_back(json!({ "kind": "ack", "seq": 1, "resync": false }));
        c.on_ack(4).unwrap();
        c.connect(Box::new(f.clone()), "t", 0).unwrap();
        assert_eq!(kinds(&f), [("hello".into(), -1), ("snapshot".into(), 5)]);
        assert_eq!(f.sent.borrow()[1]["lines"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn commands_are_spooled_once_and_results_reported_once() {
        let d = dir("commands");
        let mut c = Client::open(&d, "vm").unwrap();
        let cmd = json!({ "kind": "command", "command": {
            "id": "cmd-1", "lineId": "vm-1", "kind": "relaunch", "requestedBy": "bob",
            "requestedAt": "2026-10-28T10:00:00Z", "status": "pending", "args": { "message": "hi" },
        }});
        let r1 = c.handle(&cmd, 0).unwrap();
        let r2 = c.handle(&cmd, 0).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r1[0]["command"]["status"], "delivered");
        assert!(r1[0]["command"].get("args").is_none());
        let spooled: Value =
            serde_json::from_slice(&std::fs::read(inbox(&d).join("cmd-1.json")).unwrap()).unwrap();
        assert_eq!(spooled["args"]["message"], "hi");
        assert_eq!(std::fs::read_dir(inbox(&d)).unwrap().count(), 1);

        let evil = json!({ "kind": "command", "command": { "id": "../x", "lineId": "vm-1", "kind": "hold" } });
        assert_eq!(
            c.handle(&evil, 0).unwrap()[0]["command"]["status"],
            "refused"
        );
        assert!(!d.join("commands/x.json").exists());

        assert!(complete(&d, "../x", "succeeded", None, 0).is_err());
        complete(&d, "cmd-1", "refused", Some("agent is mid-turn"), 0).unwrap();
        let f = Fake::default();
        c.conn = Some(Box::new(f.clone()));
        c.pump(Duration::ZERO, None, 0).unwrap();
        c.pump(Duration::ZERO, None, 0).unwrap();
        let sent = f.sent.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["command"]["status"], "refused");
        assert_eq!(sent[0]["command"]["reason"], "agent is mid-turn");
        assert_eq!(sent[0]["command"]["lineId"], "vm-1");
        let _ = std::fs::remove_dir_all(d);
    }
}
