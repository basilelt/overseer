use crate::util::{rfc3339, sign, token, ulid};
use crate::{dberr, fail, reply, ApiError, App, Reply};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use futures_util::Stream;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};

pub const TYPES: [&str; 8] = [
    "line.created",
    "line.updated",
    "line.state_changed",
    "line.hung_up",
    "station.online",
    "station.offline",
    "command.requested",
    "command.completed",
];

fn event_id(e: &Value) -> i64 {
    e["id"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
}

impl App {
    fn events_after(&self, after: i64) -> VecDeque<Value> {
        let db = self.db();
        db.prepare("SELECT id, type, at_ms, data FROM events WHERE id > ?1 ORDER BY id")
            .and_then(|mut s| {
                s.query_map([after], |r| {
                    Ok(json!({
                        "id": r.get::<_, i64>(0)?.to_string(),
                        "type": r.get::<_, String>(1)?,
                        "at": rfc3339(r.get(2)?),
                        "data": serde_json::from_str::<Value>(&r.get::<_, String>(3)?).unwrap_or(Value::Null),
                    }))
                })?
                .collect()
            })
            .unwrap_or_default()
    }
}

type Feed = (
    VecDeque<Value>,
    broadcast::Receiver<Value>,
    i64,
    Option<HashSet<String>>,
);

pub async fn sse(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    app.who(&h)?;
    let types: Option<HashSet<String>> = q.get("types").map(|t| {
        t.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    });
    let rx = app.events.subscribe();
    let last = h
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<i64>().ok());
    let backlog = last.map(|n| app.events_after(n)).unwrap_or_default();
    let feed: Feed = (backlog, rx, last.unwrap_or(0), types);
    let stream =
        futures_util::stream::unfold(feed, |(mut backlog, mut rx, mut seen, types)| async move {
            loop {
                let ev = match backlog.pop_front() {
                    Some(e) => e,
                    None => rx.recv().await.ok()?,
                };
                let id = event_id(&ev);
                if id <= seen {
                    continue;
                }
                seen = id;
                let ty = ev["type"].as_str().unwrap_or_default().to_string();
                if types.as_ref().is_some_and(|t| !t.contains(&ty)) {
                    continue;
                }
                let out = Event::default()
                    .id(id.to_string())
                    .event(ty)
                    .data(ev.to_string());
                return Some((Ok(out), (backlog, rx, seen, types)));
            }
        });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub async fn create_webhook(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Json(b): Json<Value>,
) -> Reply {
    app.admin(&h)?;
    let url = b["url"]
        .as_str()
        .filter(|u| u.starts_with("http://") && u.len() > 7)
        .ok_or_else(|| fail(StatusCode::BAD_REQUEST, "url: http:// only (see ADR 0009)"))?;
    let types: Vec<&str> = b["types"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if types.is_empty() || types.iter().any(|t| !TYPES.contains(t)) {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "types: non-empty list of event types",
        ));
    }
    let id = ulid(app.now());
    let secret = token("sbw");
    app.db()
        .execute(
            "INSERT INTO webhooks (id, url, types, secret, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id,
                url,
                json!(types).to_string(),
                secret,
                rfc3339(app.now())
            ],
        )
        .map_err(dberr)?;
    reply(
        StatusCode::CREATED,
        json!({ "id": id, "url": url, "types": types, "secret": secret }),
    )
}

pub async fn delete_webhook(
    State(app): State<Arc<App>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Reply {
    app.admin(&h)?;
    match app
        .db()
        .execute("DELETE FROM webhooks WHERE id = ?1", [&id])
    {
        Ok(0) => Err(fail(StatusCode::NOT_FOUND, "not found")),
        Ok(_) => Ok(StatusCode::NO_CONTENT.into_response()),
        Err(e) => Err(dberr(e)),
    }
}

pub async fn dispatch(app: Arc<App>, mut rx: mpsc::UnboundedReceiver<Value>) {
    while let Some(ev) = rx.recv().await {
        let ty = ev["type"].as_str().unwrap_or_default().to_string();
        let hooks: Vec<(String, String, String)> = app
            .db()
            .prepare("SELECT id, url, secret, types FROM webhooks")
            .and_then(|mut s| {
                s.query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, String>(3)?))
                })?
                .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, _, _, types)| {
                serde_json::from_str::<Vec<String>>(types).is_ok_and(|t| t.contains(&ty))
            })
            .map(|(id, url, secret, _)| (id, url, secret))
            .collect();
        for (id, url, secret) in hooks {
            tokio::spawn(deliver(app.clone(), id, url, secret, ev.clone()));
        }
    }
}

async fn deliver(app: Arc<App>, hook: String, url: String, secret: String, ev: Value) {
    let body = ev.to_string();
    for attempt in 0..3u32 {
        let signature = sign(&secret, app.now() / 1000, &body);
        match post(&url, &ev, &signature, &body).await {
            Ok(()) => return,
            Err(e) if attempt == 2 => {
                eprintln!(
                    "overseer-api: webhook {hook} event {} failed: {e}",
                    ev["id"]
                );
            }
            Err(_) => tokio::time::sleep(Duration::from_secs(2u64.pow(attempt * 2))).await,
        }
    }
}

async fn post(url: &str, ev: &Value, signature: &str, body: &str) -> Result<(), String> {
    let rest = url.strip_prefix("http://").ok_or("not http")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = if host
        .rsplit_once(':')
        .is_some_and(|(_, p)| p.parse::<u16>().is_ok())
    {
        host.to_string()
    } else {
        format!("{host}:80")
    };
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         X-Overseer-Event: {}\r\nX-Overseer-Delivery: {}\r\nX-Overseer-Signature: {signature}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len(),
        ev["type"].as_str().unwrap_or_default(),
        ev["id"].as_str().unwrap_or_default(),
    );
    let exchange = async {
        let mut s = tokio::net::TcpStream::connect(&addr)
            .await
            .map_err(|e| e.to_string())?;
        s.write_all(req.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut head = [0u8; 12];
        s.read_exact(&mut head).await.map_err(|e| e.to_string())?;
        let status = String::from_utf8_lossy(&head[9..12]).to_string();
        if status.starts_with('2') {
            Ok(())
        } else {
            Err(format!("status {status}"))
        }
    };
    tokio::time::timeout(Duration::from_secs(5), exchange)
        .await
        .map_err(|_| "timeout".to_string())?
}
