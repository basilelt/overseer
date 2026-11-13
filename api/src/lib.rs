pub mod events;
pub mod sync;
pub mod util;
pub mod wire {
    include!("../../contract/wire.rs");
}

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use util::{rfc3339, sha256, token, ulid, valid_id};
use wire::*;

pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub const OFFLINE_AFTER_MS: i64 = 45_000;
pub const RETENTION_MS: i64 = 7 * 86_400_000;
const KINDS: [&str; 3] = ["mac", "vm", "runner"];
const CONTENT_ARGS: [&str; 2] = ["message", "prompt"];

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS stations (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  owner TEXT,
  token_hash TEXT UNIQUE,
  created_at TEXT NOT NULL,
  revoked_at TEXT,
  last_seq INTEGER NOT NULL DEFAULT 0,
  last_seen_ms INTEGER,
  online INTEGER NOT NULL DEFAULT 0,
  active_count INTEGER,
  mem_used INTEGER,
  mem_total INTEGER
);
CREATE TABLE IF NOT EXISTS users (
  name TEXT PRIMARY KEY,
  token_hash TEXT UNIQUE,
  created_at TEXT NOT NULL,
  revoked_at TEXT
);
CREATE TABLE IF NOT EXISTS lines (
  id TEXT PRIMARY KEY,
  station_id TEXT NOT NULL,
  owner TEXT NOT NULL,
  ticket_key TEXT,
  state TEXT NOT NULL,
  body TEXT NOT NULL,
  synced_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS lines_owner ON lines (owner);
CREATE INDEX IF NOT EXISTS lines_ticket ON lines (ticket_key);
CREATE INDEX IF NOT EXISTS lines_station ON lines (station_id);
CREATE TABLE IF NOT EXISTS commands (
  id TEXT PRIMARY KEY,
  line_id TEXT NOT NULL,
  station_id TEXT NOT NULL,
  status TEXT NOT NULL,
  requested_ms INTEGER NOT NULL,
  body TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS commands_line ON commands (line_id);
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  type TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS webhooks (
  id TEXT PRIMARY KEY,
  url TEXT NOT NULL,
  types TEXT NOT NULL,
  secret TEXT NOT NULL,
  created_at TEXT NOT NULL
);
";

pub enum Out {
    Frame(String),
    Close,
}

struct Conn {
    generation: u64,
    tx: mpsc::UnboundedSender<Out>,
}

pub struct App {
    db: Mutex<Connection>,
    clock: Clock,
    admin: Option<String>,
    events: broadcast::Sender<Value>,
    hooks: mpsc::UnboundedSender<Value>,
    hooks_rx: Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
    conns: Mutex<HashMap<String, Conn>>,
    inflight: Mutex<HashMap<String, (String, String)>>,
    generation: AtomicU64,
}

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

pub type Reply = Result<Response, ApiError>;

pub fn fail(code: StatusCode, msg: &str) -> ApiError {
    ApiError(code, msg.into())
}

fn reply(code: StatusCode, v: Value) -> Reply {
    Ok((code, Json(v)).into_response())
}

fn dberr(e: rusqlite::Error) -> ApiError {
    eprintln!("overseer-api: database error: {e}");
    fail(StatusCode::INTERNAL_SERVER_ERROR, "database error")
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn bearer(h: &HeaderMap) -> Option<&str> {
    h.get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

pub enum Who {
    Admin,
    User(String),
}

impl Who {
    fn name(&self) -> String {
        match self {
            Who::Admin => "admin".into(),
            Who::User(n) => n.clone(),
        }
    }
}

fn str_field<'a>(b: &'a Value, k: &str) -> Option<&'a str> {
    b[k].as_str().filter(|s| !s.is_empty())
}

fn bad(msg: &str) -> ApiError {
    fail(StatusCode::BAD_REQUEST, msg)
}

impl App {
    pub fn open(
        path: &std::path::Path,
        admin_token: Option<&str>,
        clock: Clock,
    ) -> Result<Arc<App>, String> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")
            .and_then(|_| db.execute_batch(SCHEMA))
            .map_err(|e| e.to_string())?;
        let (hooks, hooks_rx) = mpsc::unbounded_channel();
        Ok(Arc::new(App {
            db: Mutex::new(db),
            clock,
            admin: admin_token.filter(|t| !t.is_empty()).map(sha256),
            events: broadcast::channel(1024).0,
            hooks,
            hooks_rx: Mutex::new(Some(hooks_rx)),
            conns: Mutex::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
        }))
    }

    pub fn start(self: &Arc<Self>, sweep_every: Option<Duration>) {
        if let Some(rx) = lock(&self.hooks_rx).take() {
            tokio::spawn(events::dispatch(self.clone(), rx));
        }
        if let Some(every) = sweep_every {
            let app = self.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(every);
                loop {
                    tick.tick().await;
                    app.sweep();
                }
            });
        }
    }

    pub fn now(&self) -> i64 {
        (self.clock)()
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.db)
    }

    fn emit(&self, db: &Connection, ty: &str, data: Value) {
        let at = self.now();
        let r = db.execute(
            "INSERT INTO events (type, at_ms, data) VALUES (?1, ?2, ?3)",
            params![ty, at, data.to_string()],
        );
        if let Err(e) = r {
            eprintln!("overseer-api: cannot record event {ty}: {e}");
            return;
        }
        let ev = json!({ "id": db.last_insert_rowid().to_string(), "type": ty, "at": rfc3339(at), "data": data });
        let _ = self.events.send(ev.clone());
        let _ = self.hooks.send(ev);
    }

    pub fn sweep(&self) {
        let now = self.now();
        let db = self.db();
        let stale: Vec<(String, i64)> = db
            .prepare("SELECT id, last_seen_ms FROM stations WHERE online = 1 AND last_seen_ms < ?1")
            .and_then(|mut s| {
                s.query_map([now - OFFLINE_AFTER_MS], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .unwrap_or_default();
        for (id, seen) in stale {
            if db
                .execute("UPDATE stations SET online = 0 WHERE id = ?1", [&id])
                .is_ok()
            {
                self.emit(
                    &db,
                    "station.offline",
                    json!({ "stationId": id, "lastSeenAt": rfc3339(seen) }),
                );
            }
        }
        let _ = db.execute("DELETE FROM events WHERE at_ms < ?1", [now - RETENTION_MS]);
    }

    pub fn who(&self, h: &HeaderMap) -> Result<Who, ApiError> {
        let deny = || fail(StatusCode::UNAUTHORIZED, "missing or invalid token");
        let hash = sha256(bearer(h).ok_or_else(deny)?);
        if self.admin.as_deref() == Some(hash.as_str()) {
            return Ok(Who::Admin);
        }
        self.db()
            .query_row(
                "SELECT name FROM users WHERE token_hash = ?1 AND revoked_at IS NULL",
                [&hash],
                |r| r.get(0),
            )
            .optional()
            .map_err(dberr)?
            .map(Who::User)
            .ok_or_else(deny)
    }

    fn admin(&self, h: &HeaderMap) -> Result<(), ApiError> {
        match self.who(h)? {
            Who::Admin => Ok(()),
            Who::User(_) => Err(fail(StatusCode::FORBIDDEN, "admin token required")),
        }
    }

    pub fn station_token_ok(&self, station: &str, token: &str) -> bool {
        self.db()
            .query_row(
                "SELECT 1 FROM stations WHERE id = ?1 AND token_hash = ?2 AND revoked_at IS NULL",
                params![station, sha256(token)],
                |_| Ok(()),
            )
            .is_ok()
    }

    fn attach(&self, station: &str, tx: mpsc::UnboundedSender<Out>) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::Relaxed);
        if let Some(old) = lock(&self.conns).insert(station.into(), Conn { generation, tx }) {
            let _ = old.tx.send(Out::Close);
        }
        generation
    }

    fn detach(&self, station: &str, generation: u64) {
        let mut c = lock(&self.conns);
        if c.get(station).is_some_and(|c| c.generation == generation) {
            c.remove(station);
        }
    }

    fn kick(&self, station: &str) {
        if let Some(old) = lock(&self.conns).remove(station) {
            let _ = old.tx.send(Out::Close);
        }
    }

    fn send(&self, station: &str, frame: String) -> bool {
        lock(&self.conns)
            .get(station)
            .is_some_and(|c| c.tx.send(Out::Frame(frame)).is_ok())
    }

    fn connected(&self, db: &Connection, station: &str) -> bool {
        lock(&self.conns).contains_key(station)
            && db
                .query_row(
                    "SELECT online FROM stations WHERE id = ?1 AND revoked_at IS NULL",
                    [station],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(false)
    }

    fn upsert_line(&self, db: &Connection, station: &str, line: &Value) -> rusqlite::Result<bool> {
        let (Some(id), Some(owner), Some(state)) = (
            str_field(line, "id"),
            str_field(line, "owner"),
            line["state"].as_str().filter(|s| LINE_STATES.contains(s)),
        ) else {
            return Ok(false);
        };
        if line["stationId"].as_str() != Some(station) {
            return Ok(false);
        }
        let old: Option<(String, String)> = db
            .query_row(
                "SELECT station_id, body FROM lines WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        if old.as_ref().is_some_and(|(s, _)| s != station) {
            return Ok(false);
        }
        db.execute(
            "INSERT INTO lines (id, station_id, owner, ticket_key, state, body, synced_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET owner = excluded.owner, ticket_key = excluded.ticket_key,
               state = excluded.state, body = excluded.body, synced_ms = excluded.synced_ms",
            params![id, station, owner, line["ticketKey"].as_str(), state, line.to_string(), self.now()],
        )?;
        let old: Option<Value> = old.and_then(|(_, b)| serde_json::from_str(&b).ok());
        match old {
            None => self.emit(db, "line.created", json!({ "line": line })),
            Some(o) if o["state"] != line["state"] => {
                self.emit(
                    db,
                    "line.state_changed",
                    json!({ "line": line, "from": o["state"], "to": state }),
                );
                if state == STATE_HUNG_UP {
                    self.emit(db, "line.hung_up", json!({ "line": line }));
                }
            }
            Some(o) => {
                let strip = |v: &Value| {
                    let mut v = v.clone();
                    if let Some(m) = v.as_object_mut() {
                        m.remove("lastActivityAt");
                    }
                    v
                };
                if strip(&o) != strip(line) {
                    self.emit(db, "line.updated", json!({ "line": line }));
                }
            }
        }
        Ok(true)
    }

    fn relay(
        &self,
        id: &str,
        line_id: &str,
        kind: &str,
        args: Map<String, Value>,
        by: &str,
    ) -> Result<(Value, bool), ApiError> {
        let db = self.db();
        let existing: Option<(String, String)> = db
            .query_row(
                "SELECT line_id, body FROM commands WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(dberr)?;
        if let Some((l, body)) = existing {
            if l != line_id {
                return Err(fail(StatusCode::CONFLICT, "command id already used"));
            }
            return Ok((serde_json::from_str(&body).unwrap_or(Value::Null), false));
        }
        let station: String = db
            .query_row(
                "SELECT station_id FROM lines WHERE id = ?1",
                [line_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(dberr)?
            .ok_or_else(|| fail(StatusCode::NOT_FOUND, "line not found"))?;
        if !self.connected(&db, &station) {
            return Err(fail(StatusCode::CONFLICT, "station offline; not queued"));
        }
        let now = self.now();
        let mut stored = args.clone();
        stored.retain(|k, _| !CONTENT_ARGS.contains(&k.as_str()));
        let mut cmd = json!({
            "id": id, "lineId": line_id, "kind": kind, "requestedBy": by,
            "requestedAt": rfc3339(now), "status": STATUS_PENDING,
        });
        if !stored.is_empty() {
            cmd["args"] = Value::Object(stored);
        }
        db.execute(
            "INSERT INTO commands (id, line_id, station_id, status, requested_ms, body) VALUES (?1, ?2, ?3, 'pending', ?4, ?5)",
            params![id, line_id, station, now, cmd.to_string()],
        )
        .map_err(dberr)?;
        self.emit(&db, "command.requested", cmd.clone());
        let mut full = cmd.clone();
        if !args.is_empty() {
            full["args"] = Value::Object(args);
        }
        let frame = json!({ "kind": FRAME_COMMAND, "command": full }).to_string();
        lock(&self.inflight).insert(id.into(), (station.clone(), frame.clone()));
        self.send(&station, frame);
        Ok((cmd, true))
    }

    fn command_status(&self, id: &str) -> Option<Value> {
        self.db()
            .query_row("SELECT body FROM commands WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|b| serde_json::from_str(&b).ok())
    }
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/stations", post(enrol).get(list_stations))
        .route("/v1/stations/{id}/token", delete(revoke_station))
        .route("/v1/stations/{id}/sync", get(sync::upgrade))
        .route("/v1/users", post(create_user))
        .route("/v1/users/{name}/token", delete(revoke_user))
        .route("/v1/lines", get(list_lines).post(create_line))
        .route("/v1/lines/{id}", get(get_line))
        .route("/v1/lines/{id}/commands", post(relay).get(audit))
        .route("/v1/events", get(events::sse))
        .route("/v1/webhooks", post(events::create_webhook))
        .route("/v1/webhooks/{id}", delete(events::delete_webhook))
        .with_state(app)
}

fn issue(app: &App, table: &str, key: &str, id: &str, extra: Option<(&str, &str)>) -> Reply {
    let db = app.db();
    let revoked: Option<Option<String>> = db
        .query_row(
            &format!("SELECT revoked_at FROM {table} WHERE {key} = ?1"),
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(dberr)?;
    if matches!(revoked, Some(None)) {
        return Err(fail(StatusCode::CONFLICT, "already enrolled"));
    }
    let prefix = if table == "stations" { "sbs" } else { "sbu" };
    let t = token(prefix);
    let now = rfc3339(app.now());
    let r = match (revoked, extra) {
        (Some(_), Some((kind, owner))) => db.execute(
            "UPDATE stations SET token_hash = ?2, revoked_at = NULL, kind = ?3, owner = NULLIF(?4, '') WHERE id = ?1",
            params![id, sha256(&t), kind, owner],
        ),
        (None, Some((kind, owner))) => db.execute(
            "INSERT INTO stations (id, kind, owner, token_hash, created_at) VALUES (?1, ?2, NULLIF(?3, ''), ?4, ?5)",
            params![id, kind, owner, sha256(&t), now],
        ),
        (Some(_), None) => db.execute(
            "UPDATE users SET token_hash = ?2, revoked_at = NULL WHERE name = ?1",
            params![id, sha256(&t)],
        ),
        (None, None) => db.execute(
            "INSERT INTO users (name, token_hash, created_at) VALUES (?1, ?2, ?3)",
            params![id, sha256(&t), now],
        ),
    };
    r.map_err(dberr)?;
    reply(
        StatusCode::CREATED,
        json!({ key_name(table): id, "token": t }),
    )
}

fn key_name(table: &str) -> &'static str {
    if table == "stations" {
        "stationId"
    } else {
        "name"
    }
}

fn revoke(app: &App, table: &str, key: &str, id: &str) -> Reply {
    let n = app
        .db()
        .execute(
            &format!("UPDATE {table} SET token_hash = NULL, revoked_at = ?2 WHERE {key} = ?1"),
            params![id, rfc3339(app.now())],
        )
        .map_err(dberr)?;
    if n == 0 {
        return Err(fail(StatusCode::NOT_FOUND, "not found"));
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn enrol(State(app): State<Arc<App>>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    app.admin(&h)?;
    let id = str_field(&b, "stationId")
        .filter(|s| valid_id(s))
        .ok_or_else(|| bad("stationId: letters, digits, . _ - @ only"))?;
    let kind = str_field(&b, "kind")
        .filter(|k| KINDS.contains(k))
        .ok_or_else(|| bad("kind: mac, vm or runner"))?;
    issue(
        &app,
        "stations",
        "id",
        id,
        Some((kind, b["owner"].as_str().unwrap_or(""))),
    )
}

async fn revoke_station(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Reply {
    app.admin(&h)?;
    let r = revoke(&app, "stations", "id", &id);
    app.kick(&id);
    r
}

async fn create_user(State(app): State<Arc<App>>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    app.admin(&h)?;
    let name = str_field(&b, "name")
        .filter(|s| valid_id(s) && *s != "admin")
        .ok_or_else(|| bad("name: letters, digits, . _ - @ only"))?;
    issue(&app, "users", "name", name, None)
}

async fn revoke_user(State(app): State<Arc<App>>, h: HeaderMap, Path(name): Path<String>) -> Reply {
    app.admin(&h)?;
    revoke(&app, "users", "name", &name)
}

async fn list_stations(State(app): State<Arc<App>>, h: HeaderMap) -> Reply {
    app.who(&h)?;
    let db = app.db();
    let mut s = db
        .prepare(
            "SELECT id, kind, online, last_seen_ms, active_count, mem_used, mem_total, revoked_at IS NOT NULL
             FROM stations ORDER BY id",
        )
        .map_err(dberr)?;
    let rows: Vec<Value> = s
        .query_map([], |r| {
            let mut v = json!({
                "stationId": r.get::<_, String>(0)?,
                "kind": r.get::<_, String>(1)?,
                "online": r.get::<_, bool>(2)?,
                "revoked": r.get::<_, bool>(7)?,
            });
            if let Some(ms) = r.get::<_, Option<i64>>(3)? {
                v["lastSeenAt"] = json!(rfc3339(ms));
            }
            for (i, k) in [
                (4, "activeCount"),
                (5, "memUsedBytes"),
                (6, "memTotalBytes"),
            ] {
                if let Some(n) = r.get::<_, Option<i64>>(i)? {
                    v[k] = json!(n);
                }
            }
            Ok(v)
        })
        .and_then(|m| m.collect())
        .map_err(dberr)?;
    reply(StatusCode::OK, json!(rows))
}

fn line_view(body: &str, synced_ms: i64, online: bool) -> Value {
    json!({
        "line": serde_json::from_str::<Value>(body).unwrap_or(Value::Null),
        "stationOnline": online,
        "syncedAt": rfc3339(synced_ms),
    })
}

const LINE_SELECT: &str =
    "SELECT l.body, l.synced_ms, s.online FROM lines l JOIN stations s ON s.id = l.station_id";

async fn list_lines(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Reply {
    app.who(&h)?;
    if q.get("state")
        .is_some_and(|s| !LINE_STATES.contains(&s.as_str()))
    {
        return Err(bad("state: active, idle, held or hung-up"));
    }
    let mut sql = format!("{LINE_SELECT} WHERE 1 = 1");
    let mut args: Vec<&String> = vec![];
    for (k, col) in [
        ("owner", "l.owner"),
        ("ticketKey", "l.ticket_key"),
        ("ticket", "l.ticket_key"),
        ("stationId", "l.station_id"),
        ("state", "l.state"),
    ] {
        if let Some(v) = q.get(k) {
            args.push(v);
            sql.push_str(&format!(" AND {col} = ?{}", args.len()));
        }
    }
    if !q.contains_key("state") && q.get("includeHungUp").map(String::as_str) != Some("true") {
        sql.push_str(" AND l.state != 'hung-up'");
    }
    sql.push_str(" ORDER BY json_extract(l.body, '$.lastActivityAt') DESC, l.id");
    let db = app.db();
    let mut s = db.prepare(&sql).map_err(dberr)?;
    let rows: Vec<Value> = s
        .query_map(rusqlite::params_from_iter(args), |r| {
            Ok(line_view(&r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
        })
        .and_then(|m| m.collect())
        .map_err(dberr)?;
    reply(StatusCode::OK, json!(rows))
}

async fn get_line(State(app): State<Arc<App>>, h: HeaderMap, Path(id): Path<String>) -> Reply {
    app.who(&h)?;
    let v = app
        .db()
        .query_row(&format!("{LINE_SELECT} WHERE l.id = ?1"), [&id], |r| {
            Ok(line_view(&r.get::<_, String>(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()
        .map_err(dberr)?
        .ok_or_else(|| fail(StatusCode::NOT_FOUND, "line not found"))?;
    reply(StatusCode::OK, v)
}

fn command_args(kind: &str, args: &Value) -> Result<Map<String, Value>, ApiError> {
    let args = match args {
        Value::Null => Map::new(),
        Value::Object(m) => m.clone(),
        _ => return Err(bad("args must be an object")),
    };
    for (k, v) in &args {
        let ok = match k.as_str() {
            "ticketKey" => v.as_str().is_some_and(util::ticket_ok),
            "message" => v.is_string(),
            "dryRun" => v.is_boolean(),
            _ => false,
        };
        if !ok {
            return Err(bad(&format!("invalid args.{k}")));
        }
    }
    if kind == CMD_LINK && !args.contains_key("ticketKey") {
        return Err(bad("link needs args.ticketKey"));
    }
    if kind == CMD_RELAUNCH && !args.contains_key("message") {
        return Err(bad("relaunch needs args.message"));
    }
    Ok(args)
}

async fn relay(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Path(line_id): Path<String>,
    Json(b): Json<Value>,
) -> Reply {
    let who = app.who(&h)?;
    let id = str_field(&b, "id")
        .filter(|s| valid_id(s))
        .ok_or_else(|| bad("id: letters, digits, . _ - @ only"))?;
    let kind = str_field(&b, "kind")
        .filter(|k| RELAY_COMMANDS.contains(k))
        .ok_or_else(|| bad("kind: resume, hold, close, link or relaunch"))?;
    let args = command_args(kind, &b["args"])?;
    let (cmd, _) = app.relay(id, &line_id, kind, args, &who.name())?;
    reply(StatusCode::ACCEPTED, cmd)
}

async fn audit(State(app): State<Arc<App>>, h: HeaderMap, Path(line_id): Path<String>) -> Reply {
    app.who(&h)?;
    let db = app.db();
    let mut s = db
        .prepare("SELECT body FROM commands WHERE line_id = ?1 ORDER BY requested_ms, rowid")
        .map_err(dberr)?;
    let rows: Vec<Value> = s
        .query_map([&line_id], |r| r.get::<_, String>(0))
        .and_then(|m| m.collect::<Result<Vec<_>, _>>())
        .map_err(dberr)?
        .iter()
        .filter_map(|b| serde_json::from_str(b).ok())
        .collect();
    reply(StatusCode::OK, json!(rows))
}

async fn create_line(State(app): State<Arc<App>>, h: HeaderMap, Json(b): Json<Value>) -> Reply {
    let who = app.who(&h)?;
    let field = |k: &str| str_field(&b, k).ok_or_else(|| bad(&format!("{k} is required")));
    let (owner, repo) = (field("owner")?, field("repo")?);
    let branch = match (str_field(&b, "branch"), b["ticketKey"].as_str()) {
        (Some(br), _) => br.to_string(),
        (None, Some(t)) if util::ticket_ok(t) => format!("feedback/{t}"),
        _ => return Err(bad("branch is required without a ticketKey")),
    };
    let branch = branch.as_str();
    let agent = field("agent").and_then(|a| {
        ["claude", "codex"]
            .contains(&a)
            .then_some(a)
            .ok_or_else(|| bad("agent: claude or codex"))
    })?;
    let origin = field("origin").and_then(|o| {
        ["dev", "feedback"]
            .contains(&o)
            .then_some(o)
            .ok_or_else(|| bad("origin: dev or feedback"))
    })?;
    let prompt = b["prompt"]
        .as_str()
        .ok_or_else(|| bad("prompt is required"))?;
    let ticket = b["ticketKey"].as_str();
    if ticket.is_some_and(|t| !util::ticket_ok(t)) {
        return Err(bad("ticketKey: PROJECT-<digits>"));
    }
    let dry = match b.get("dryRun") {
        None => None,
        Some(Value::Bool(d)) => Some(*d),
        Some(_) => return Err(bad("dryRun must be a boolean")),
    };
    let key = match b["idempotencyKey"].as_str() {
        Some(k) if valid_id(k) => k.to_string(),
        Some(_) => return Err(bad("idempotencyKey: letters, digits, . _ - @ only")),
        None => ulid(app.now()),
    };
    let line_id = match app.command_status(&key) {
        Some(c) => c["lineId"].as_str().unwrap_or_default().to_string(),
        None => {
            let db = app.db();
            let station: Option<String> = match b["stationId"].as_str() {
                Some(s) => app.connected(&db, s).then(|| s.to_string()),
                None => db
                    .prepare(
                        "SELECT id FROM stations WHERE kind = 'runner' AND online = 1 AND revoked_at IS NULL
                         ORDER BY last_seen_ms DESC",
                    )
                    .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>())
                    .map_err(dberr)?
                    .into_iter()
                    .find(|s: &String| app.connected(&db, s)),
            };
            let station =
                station.ok_or_else(|| fail(StatusCode::SERVICE_UNAVAILABLE, "no runner online"))?;
            let now = app.now();
            let line_id = format!("{station}-{}", ulid(now));
            let mut line = json!({
                "id": line_id, "owner": owner, "stationId": station, "agent": agent,
                "agentSessionId": "pending", "repo": repo, "branch": branch,
                "worktreePath": "pending", "origin": origin, "state": STATE_ACTIVE,
                "lastActivityAt": rfc3339(now),
            });
            if let Some(t) = ticket {
                line["ticketKey"] = json!(t);
            }
            let _ = app.upsert_line(&db, &station, &line);
            drop(db);
            let mut args = Map::new();
            for (k, v) in [
                ("owner", owner),
                ("repo", repo),
                ("branch", branch),
                ("agent", agent),
                ("origin", origin),
                ("prompt", prompt),
            ] {
                args.insert(k.into(), json!(v));
            }
            if let Some(t) = ticket {
                args.insert("ticketKey".into(), json!(t));
            }
            if let Some(d) = dry {
                args.insert("dryRun".into(), json!(d));
            }
            app.relay(&key, &line_id, CMD_START, args, &who.name())?;
            line_id
        }
    };
    for _ in 0..100 {
        let status = app.command_status(&key).map(|c| c["status"].clone());
        match status.as_ref().and_then(Value::as_str) {
            Some(STATUS_PENDING) => tokio::time::sleep(Duration::from_millis(100)).await,
            Some(STATUS_REFUSED | STATUS_FAILED) => {
                return Err(fail(StatusCode::CONFLICT, "runner refused the line"))
            }
            _ => break,
        }
    }
    if app
        .command_status(&key)
        .is_some_and(|c| c["status"] == STATUS_PENDING)
    {
        return Err(fail(
            StatusCode::GATEWAY_TIMEOUT,
            "runner did not accept in time",
        ));
    }
    let v = app
        .db()
        .query_row("SELECT body FROM lines WHERE id = ?1", [&line_id], |r| {
            r.get::<_, String>(0)
        })
        .map_err(dberr)?;
    reply(
        StatusCode::ACCEPTED,
        serde_json::from_str(&v).unwrap_or(Value::Null),
    )
}
