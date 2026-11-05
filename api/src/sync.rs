use crate::{bearer, fail, lock, App, Out};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::mpsc;

const TERMINAL: [&str; 3] = ["succeeded", "refused", "failed"];

pub async fn upgrade(
    State(app): State<Arc<App>>,
    Path(station): Path<String>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    match bearer(&h) {
        Some(t) if app.station_token_ok(&station, t) => ws
            .on_upgrade(move |s| session(app, station, s))
            .into_response(),
        _ => fail(StatusCode::UNAUTHORIZED, "unknown or revoked station token").into_response(),
    }
}

async fn session(app: Arc<App>, station: String, mut ws: WebSocket) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let generation = app.attach(&station, tx);
    'conn: loop {
        tokio::select! {
            m = ws.recv() => match m {
                Some(Ok(Message::Text(t))) => {
                    for f in app.frame(&station, t.as_str()) {
                        if ws.send(Message::Text(f.into())).await.is_err() {
                            break 'conn;
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            o = rx.recv() => match o {
                Some(Out::Frame(f)) => {
                    if ws.send(Message::Text(f.into())).await.is_err() {
                        break;
                    }
                }
                Some(Out::Close) | None => {
                    let _ = ws.send(Message::Close(None)).await;
                    break;
                }
            },
        }
    }
    app.detach(&station, generation);
}

impl App {
    pub fn frame(&self, station: &str, text: &str) -> Vec<String> {
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return vec![];
        };
        let now = self.now();
        let db = self.db();
        let Ok((last, online)) = db.query_row(
            "SELECT last_seq, online FROM stations WHERE id = ?1 AND revoked_at IS NULL",
            [station],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?)),
        ) else {
            return vec![];
        };
        let _ = db.execute(
            "UPDATE stations SET last_seen_ms = ?2, online = 1 WHERE id = ?1",
            params![station, now],
        );
        if !online {
            self.emit(
                &db,
                "station.online",
                json!({ "stationId": station, "lastSeenAt": crate::rfc3339(now) }),
            );
        }
        let ack = |seq: i64, resync: bool| {
            json!({ "kind": "ack", "seq": seq, "resync": resync }).to_string()
        };
        match v["kind"].as_str() {
            Some("hello") => {
                let theirs = v["lastAckedSeq"].as_i64().unwrap_or(0);
                let mut out = vec![ack(last, last == 0 || theirs > last)];
                out.extend(
                    lock(&self.inflight)
                        .values()
                        .filter(|(s, _)| s == station)
                        .map(|(_, f)| f.clone()),
                );
                out
            }
            Some("snapshot") => match v["seq"].as_i64().filter(|s| *s >= 1) {
                Some(seq) => {
                    self.apply(&db, station, seq, &v["lines"], &Value::Null);
                    vec![ack(seq, false)]
                }
                None => vec![],
            },
            Some("delta") => match v["seq"].as_i64() {
                Some(seq) if seq <= last => vec![ack(last, false)],
                Some(seq) if seq == last + 1 => {
                    self.apply(&db, station, seq, &v["upsert"], &v["remove"]);
                    vec![ack(seq, false)]
                }
                Some(_) => vec![ack(last, true)],
                None => vec![],
            },
            Some("heartbeat") => {
                let _ = db.execute(
                    "UPDATE stations SET active_count = ?2, mem_used = ?3, mem_total = ?4 WHERE id = ?1",
                    params![
                        station,
                        v["activeCount"].as_i64(),
                        v["memUsedBytes"].as_i64(),
                        v["memTotalBytes"].as_i64()
                    ],
                );
                vec![self.stations_frame(&db, station)]
            }
            Some("command_result") => {
                self.complete(&db, station, &v["command"]);
                vec![]
            }
            _ => vec![],
        }
    }

    fn stations_frame(&self, db: &Connection, station: &str) -> String {
        let mut by: Vec<(String, Option<i64>, Vec<Value>)> = db
            .prepare("SELECT id, last_seen_ms FROM stations WHERE id != ?1 AND revoked_at IS NULL ORDER BY id")
            .and_then(|mut s| {
                s.query_map([station], |r| Ok((r.get(0)?, r.get(1)?, vec![])))?
                    .collect()
            })
            .unwrap_or_default();
        let lines: Vec<(String, String)> = db
            .prepare(
                "SELECT station_id, body FROM lines WHERE station_id != ?1 AND state != 'hung-up'",
            )
            .and_then(|mut s| {
                s.query_map([station], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .unwrap_or_default();
        for (sid, body) in lines {
            if let (Some(e), Ok(l)) = (
                by.iter_mut().find(|e| e.0 == sid),
                serde_json::from_str::<Value>(&body),
            ) {
                e.2.push(l);
            }
        }
        let stations: Vec<Value> = by
            .into_iter()
            .map(|(id, seen, lines)| {
                let mut v = json!({ "stationId": id, "lines": lines });
                if let Some(ms) = seen {
                    v["lastSeenAt"] = json!(crate::rfc3339(ms));
                }
                v
            })
            .collect();
        json!({ "kind": "stations", "stations": stations }).to_string()
    }

    fn apply(&self, db: &Connection, station: &str, seq: i64, upsert: &Value, remove: &Value) {
        let Ok(tx) = db.unchecked_transaction() else {
            return;
        };
        for l in upsert.as_array().into_iter().flatten() {
            if !self.upsert_line(&tx, station, l) {
                eprintln!("overseer-api: {station} sent an invalid line, skipped");
            }
        }
        for id in remove
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let _ = tx.execute(
                "DELETE FROM lines WHERE id = ?1 AND station_id = ?2",
                params![id, station],
            );
        }
        let _ = tx.execute(
            "UPDATE stations SET last_seq = ?2 WHERE id = ?1",
            params![station, seq],
        );
        if let Err(e) = tx.commit() {
            eprintln!("overseer-api: cannot apply seq {seq} from {station}: {e}");
        }
    }

    fn complete(&self, db: &Connection, station: &str, c: &Value) {
        let (Some(id), Some(status)) = (c["id"].as_str(), c["status"].as_str()) else {
            return;
        };
        if status != "delivered" && !TERMINAL.contains(&status) {
            return;
        }
        let Ok((owner, body)) = db.query_row(
            "SELECT station_id, body FROM commands WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ) else {
            return;
        };
        if owner != station {
            return;
        }
        lock(&self.inflight).remove(id);
        let Ok(mut cmd) = serde_json::from_str::<Value>(&body) else {
            return;
        };
        let old = cmd["status"].as_str().unwrap_or_default();
        if TERMINAL.contains(&old) || old == status {
            return;
        }
        cmd["status"] = json!(status);
        if let Some(r) = c["reason"].as_str() {
            cmd["reason"] = json!(r);
        }
        if TERMINAL.contains(&status) {
            cmd["completedAt"] = c["completedAt"]
                .as_str()
                .map(|s| json!(s))
                .unwrap_or_else(|| json!(crate::rfc3339(self.now())));
        }
        let _ = db.execute(
            "UPDATE commands SET status = ?2, body = ?3 WHERE id = ?1",
            params![id, status, cmd.to_string()],
        );
        if TERMINAL.contains(&status) {
            self.emit(db, "command.completed", cmd);
        }
    }
}
