use overseer_api::util::{sha256, sign};
use overseer_api::{router, App};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

const ADMIN: &str = "test-admin-token";
const T0: i64 = 1_793_000_000_000;

struct Srv {
    app: Arc<App>,
    addr: String,
    clock: Arc<AtomicI64>,
    dir: PathBuf,
    _rt: tokio::runtime::Runtime,
}

impl Drop for Srv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Station(WebSocket<TcpStream>);

impl Station {
    fn send(&mut self, v: Value) {
        self.0.send(Message::text(v.to_string())).unwrap();
    }

    fn recv(&mut self) -> Value {
        loop {
            match self.0.read().expect("frame from api") {
                Message::Text(t) => return serde_json::from_str(t.as_str()).unwrap(),
                Message::Close(_) => panic!("closed"),
                _ => {}
            }
        }
    }

    fn hello(&mut self, station: &str, last: i64) -> Value {
        self.send(json!({ "kind": "hello", "stationId": station, "daemonVersion": "test", "lastAckedSeq": last }));
        self.recv()
    }
}

fn line(id: &str, station: &str, owner: &str, state: &str, ticket: Option<&str>) -> Value {
    let mut l = json!({
        "id": id, "owner": owner, "stationId": station, "agent": "claude",
        "agentSessionId": format!("sess-{id}"), "repo": "github.com/acme/webshop",
        "branch": "main", "worktreePath": "/w", "origin": "dev", "state": state,
        "lastActivityAt": "2026-10-28T10:00:00Z",
    });
    if let Some(t) = ticket {
        l["ticketKey"] = json!(t);
    }
    l
}

fn ids(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["line"]["id"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

impl Srv {
    fn start() -> Srv {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sb-api-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let clock = Arc::new(AtomicI64::new(T0));
        let c = clock.clone();
        let app = App::open(
            &dir.join("api.db"),
            Some(ADMIN),
            Arc::new(move || c.load(Ordering::SeqCst)),
        )
        .unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let listener = rt
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let a = app.clone();
        rt.spawn(async move {
            a.start(None);
            axum::serve(listener, router(a)).await.unwrap();
        });
        Srv {
            app,
            addr,
            clock,
            dir,
            _rt: rt,
        }
    }

    fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    fn raw(&self, method: &str, path: &str, headers: &str, body: &str) -> TcpStream {
        let mut s = TcpStream::connect(&self.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: test\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        s
    }

    fn http(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value) {
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let mut s = self.raw(method, path, &auth, &body);
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let status = out[9..12].parse().unwrap();
        let rest = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
        (status, serde_json::from_str(rest).unwrap_or(Value::Null))
    }

    fn enrol(&self, station: &str, kind: &str) -> String {
        let (st, v) = self.http(
            "POST",
            "/v1/stations",
            Some(ADMIN),
            Some(json!({ "stationId": station, "kind": kind })),
        );
        assert_eq!(st, 201, "{v}");
        v["token"].as_str().unwrap().to_string()
    }

    fn user(&self, name: &str) -> String {
        let (st, v) = self.http(
            "POST",
            "/v1/users",
            Some(ADMIN),
            Some(json!({ "name": name })),
        );
        assert_eq!(st, 201, "{v}");
        v["token"].as_str().unwrap().to_string()
    }

    fn station(&self, id: &str, token: &str) -> Result<Station, tungstenite::Error> {
        let s = TcpStream::connect(&self.addr).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut req = format!("ws://{}/v1/stations/{id}/sync", self.addr)
            .into_client_request()
            .unwrap();
        req.headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        match tungstenite::client(req, s) {
            Ok((ws, _)) => Ok(Station(ws)),
            Err(tungstenite::HandshakeError::Failure(e)) => Err(e),
            Err(_) => panic!("handshake interrupted"),
        }
    }

    fn db(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.dir.join("api.db")).unwrap()
    }

    fn count(&self, sql: &str) -> i64 {
        self.db().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn stored_bytes(&self) -> Vec<u8> {
        ["api.db", "api.db-wal"]
            .iter()
            .flat_map(|f| std::fs::read(self.dir.join(f)).unwrap_or_default())
            .collect()
    }

    fn sse(&self, token: &str, query: &str, last: Option<i64>) -> Sse {
        let mut h = format!("Authorization: Bearer {token}\r\nAccept: text/event-stream\r\n");
        if let Some(l) = last {
            h.push_str(&format!("Last-Event-ID: {l}\r\n"));
        }
        let s = self.raw("GET", &format!("/v1/events{query}"), &h, "");
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        Sse(s, String::new())
    }
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

struct Sse(TcpStream, String);

impl Sse {
    fn next(&mut self) -> Option<(i64, String, Value)> {
        loop {
            while let Some(i) = self.1.find("\n\n") {
                let block: String = self.1.drain(..i + 2).collect();
                let field = |k: &str| {
                    block
                        .lines()
                        .find_map(|l| l.strip_prefix(k))
                        .map(|v| v.trim().to_string())
                };
                if let (Some(id), Some(ev), Some(data)) =
                    (field("id:"), field("event:"), field("data:"))
                {
                    return Some((
                        id.parse().unwrap(),
                        ev,
                        serde_json::from_str(&data).unwrap(),
                    ));
                }
            }
            let mut buf = [0u8; 4096];
            match self.0.read(&mut buf) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self
                    .1
                    .push_str(&String::from_utf8_lossy(&buf[..n]).replace("\r\n", "\n")),
            }
        }
    }
}

fn wait_for(mut f: impl FnMut() -> bool) {
    for _ in 0..100 {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("condition not met in time");
}

#[test]
fn two_stations_lines_appear_in_one_read_and_tokens_are_hashed() {
    let srv = Srv::start();
    let ta = srv.enrol("mac-bob", "mac");
    let tb = srv.enrol("vm-node2-dev1", "vm");
    let u = srv.user("bob");
    let again = json!({ "stationId": "mac-bob", "kind": "mac" });
    assert_eq!(
        srv.http("POST", "/v1/stations", Some(ADMIN), Some(again.clone()))
            .0,
        409
    );
    assert_eq!(
        srv.http("POST", "/v1/stations", Some(&u), Some(again)).0,
        403
    );
    assert_eq!(srv.http("GET", "/v1/lines", None, None).0, 401);
    assert!(srv.station("mac-bob", &tb).is_err());

    let mut a = srv.station("mac-bob", &ta).unwrap();
    assert_eq!(
        a.hello("mac-bob", 0),
        json!({ "kind": "ack", "seq": 0, "resync": true })
    );
    a.send(json!({ "kind": "snapshot", "seq": 1, "lines": [
        line("mac-bob-1", "mac-bob", "alice", "active", Some("AS-1")),
        line("mac-bob-2", "mac-bob", "bob", "idle", None),
    ]}));
    assert_eq!(a.recv()["seq"], 1);
    let mut b = srv.station("vm-node2-dev1", &tb).unwrap();
    b.hello("vm-node2-dev1", 0);
    b.send(json!({ "kind": "snapshot", "seq": 1, "lines": [
        line("vm-node2-dev1-1", "vm-node2-dev1", "alice", "held", Some("AS-1")),
    ]}));
    assert_eq!(b.recv()["seq"], 1);
    b.send(json!({ "kind": "delta", "seq": 2, "upsert": [line("mac-bob-9", "mac-bob", "alice", "active", None)] }));
    assert_eq!(b.recv()["seq"], 2);

    let (st, v) = srv.http("GET", "/v1/lines?owner=alice", Some(&u), None);
    assert_eq!(st, 200);
    assert_eq!(ids(&v), ["mac-bob-1", "vm-node2-dev1-1"]);
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["stationOnline"] == true));
    assert_eq!(
        ids(&srv
            .http("GET", "/v1/lines?ticketKey=AS-1", Some(&u), None)
            .1)
        .len(),
        2
    );
    assert_eq!(
        ids(&srv
            .http("GET", "/v1/lines?stationId=vm-node2-dev1", Some(&u), None)
            .1),
        ["vm-node2-dev1-1"]
    );
    let (st, one) = srv.http("GET", "/v1/lines/mac-bob-2", Some(&u), None);
    assert_eq!((st, one["line"]["owner"].as_str()), (200, Some("bob")));
    assert_eq!(srv.http("GET", "/v1/lines/nope", Some(&u), None).0, 404);

    let stored = srv.stored_bytes();
    for t in [&ta, &tb, &u] {
        assert!(!contains(&stored, t), "token stored in clear");
    }
    assert!(contains(&stored, &sha256(&ta)));
}

#[test]
fn station_offline_ten_minutes_resyncs_without_duplicates() {
    let srv = Srv::start();
    let t = srv.enrol("vm", "vm");
    let u = srv.user("alice");
    let mut s = srv.station("vm", &t).unwrap();
    s.hello("vm", 0);
    s.send(json!({ "kind": "snapshot", "seq": 1, "lines": [
        line("vm-1", "vm", "alice", "active", None),
        line("vm-2", "vm", "alice", "active", None),
    ]}));
    assert_eq!(s.recv()["seq"], 1);
    s.send(
        json!({ "kind": "delta", "seq": 2, "upsert": [line("vm-1", "vm", "alice", "idle", None)] }),
    );
    assert_eq!(s.recv()["seq"], 2);
    s.send(json!({ "kind": "heartbeat", "activeCount": 1, "memUsedBytes": 5, "memTotalBytes": 9 }));
    let st = s.recv();
    assert_eq!(
        (st["kind"].as_str(), st["stations"].as_array().map(Vec::len)),
        (Some("stations"), Some(0))
    );
    s.send(json!({ "kind": "delta", "seq": 2, "upsert": [] }));
    assert_eq!(s.recv()["seq"], 2);
    drop(s);

    srv.advance(10 * 60_000);
    srv.app.sweep();
    let (_, stations) = srv.http("GET", "/v1/stations", Some(&u), None);
    assert_eq!(stations[0]["online"], false);
    assert_eq!(stations[0]["activeCount"], 1);
    let (_, v) = srv.http("GET", "/v1/lines", Some(&u), None);
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["stationOnline"] == false));
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'station.offline'"),
        1
    );

    let mut s = srv.station("vm", &t).unwrap();
    assert_eq!(
        s.hello("vm", 1),
        json!({ "kind": "ack", "seq": 2, "resync": false })
    );
    let replay = [
        json!({ "kind": "delta", "seq": 2, "upsert": [line("vm-1", "vm", "alice", "idle", None)] }),
        json!({ "kind": "delta", "seq": 3, "upsert": [line("vm-3", "vm", "alice", "active", None)] }),
        json!({ "kind": "delta", "seq": 4, "upsert": [line("vm-1", "vm", "alice", "held", None)] }),
        json!({ "kind": "delta", "seq": 3, "upsert": [line("vm-3", "vm", "alice", "active", None)] }),
    ];
    let acks: Vec<Value> = replay
        .into_iter()
        .map(|f| {
            s.send(f);
            s.recv()["seq"].clone()
        })
        .collect();
    assert_eq!(acks, [json!(2), json!(3), json!(4), json!(4)]);

    let (_, v) = srv.http("GET", "/v1/lines?includeHungUp=true", Some(&u), None);
    assert_eq!(ids(&v), ["vm-1", "vm-2", "vm-3"]);
    assert!(v
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["stationOnline"] == true));
    let held = srv.http("GET", "/v1/lines/vm-1", Some(&u), None).1;
    assert_eq!(held["line"]["state"], "held");
    assert_eq!(srv.count("SELECT count(*) FROM lines"), 3);
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'line.created'"),
        3
    );
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'line.state_changed'"),
        2
    );
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'station.online'"),
        2
    );

    s.send(json!({ "kind": "delta", "seq": 6, "upsert": [] }));
    assert_eq!(s.recv(), json!({ "kind": "ack", "seq": 4, "resync": true }));
    s.send(json!({ "kind": "snapshot", "seq": 7, "lines": [line("vm-1", "vm", "alice", "held", None)] }));
    assert_eq!(s.recv()["seq"], 7);
    assert_eq!(srv.count("SELECT count(*) FROM lines"), 3);
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'line.created'"),
        3
    );
}

#[test]
fn relayed_hold_is_audited_and_content_is_not_stored() {
    let srv = Srv::start();
    let t = srv.enrol("vm", "vm");
    let u = srv.user("bob");
    let mut s = srv.station("vm", &t).unwrap();
    s.hello("vm", 0);
    s.send(json!({ "kind": "snapshot", "seq": 1, "lines": [line("vm-1", "vm", "alice", "idle", None)] }));
    s.recv();

    let post = |body: Value| srv.http("POST", "/v1/lines/vm-1/commands", Some(&u), Some(body));
    let (st, cmd) = post(json!({ "id": "cmd-1", "kind": "hold" }));
    assert_eq!(st, 202, "{cmd}");
    assert_eq!(
        (cmd["status"].as_str(), cmd["requestedBy"].as_str()),
        (Some("pending"), Some("bob"))
    );
    let frame = s.recv();
    assert_eq!(
        (frame["kind"].as_str(), frame["command"]["id"].as_str()),
        (Some("command"), Some("cmd-1"))
    );
    s.send(
        json!({ "kind": "command_result", "command": { "id": "cmd-1", "status": "delivered" } }),
    );
    s.send(json!({ "kind": "command_result", "command": { "id": "cmd-1", "status": "refused", "reason": "agent is mid-turn" } }));
    s.send(
        json!({ "kind": "command_result", "command": { "id": "cmd-1", "status": "succeeded" } }),
    );
    let audit = || srv.http("GET", "/v1/lines/vm-1/commands", Some(&u), None).1;
    wait_for(|| audit()[0]["status"] == "refused");
    let a = audit();
    assert_eq!(a.as_array().unwrap().len(), 1);
    assert_eq!(a[0]["reason"], "agent is mid-turn");
    assert_eq!(a[0]["requestedBy"], "bob");
    assert!(a[0]["completedAt"].is_string());

    assert_eq!(
        post(json!({ "id": "cmd-1", "kind": "hold" })).1["status"],
        "refused"
    );
    assert_eq!(audit().as_array().unwrap().len(), 1);
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'command.requested'"),
        1
    );
    assert_eq!(
        srv.count("SELECT count(*) FROM events WHERE type = 'command.completed'"),
        1
    );
    assert_eq!(post(json!({ "id": "x", "kind": "explode" })).0, 400);
    assert_eq!(post(json!({ "id": "y", "kind": "link" })).0, 400);
    assert_eq!(
        srv.http(
            "POST",
            "/v1/lines/nope/commands",
            Some(&u),
            Some(json!({ "id": "z", "kind": "hold" }))
        )
        .0,
        404
    );

    let secret = "PLEASE-FIX-THE-LOGIN-BUG-7731";
    let (st, c) = post(json!({ "id": "cmd-2", "kind": "relaunch", "args": { "message": secret } }));
    assert_eq!(st, 202);
    assert!(c.get("args").is_none());
    assert_eq!(s.recv()["command"]["args"]["message"], secret);
    assert!(!contains(&srv.stored_bytes(), secret));

    drop(s);
    srv.advance(60_000);
    srv.app.sweep();
    assert_eq!(post(json!({ "id": "cmd-3", "kind": "close" })).0, 409);
    assert_eq!(audit().as_array().unwrap().len(), 2);
}

#[test]
fn revoked_tokens_are_refused_and_live_socket_closed() {
    let srv = Srv::start();
    let t = srv.enrol("vm", "vm");
    let u = srv.user("bob");
    let mut s = srv.station("vm", &t).unwrap();
    s.hello("vm", 0);
    assert_eq!(
        srv.http("DELETE", "/v1/stations/vm/token", Some(&u), None)
            .0,
        403
    );
    assert_eq!(
        srv.http("DELETE", "/v1/stations/vm/token", Some(ADMIN), None)
            .0,
        204
    );
    let closed = loop {
        match s.0.read() {
            Ok(Message::Close(_)) | Err(_) => break true,
            Ok(_) => {}
        }
    };
    assert!(closed);
    match srv.station("vm", &t) {
        Err(tungstenite::Error::Http(r)) => assert_eq!(r.status(), 401),
        _ => panic!("revoked token accepted"),
    }
    let (_, st) = srv.http("GET", "/v1/stations", Some(&u), None);
    assert_eq!(st[0]["revoked"], true);
    let t2 = srv.enrol("vm", "vm");
    assert!(srv.station("vm", &t2).is_ok());
    assert!(srv.station("vm", &t).is_err());

    assert_eq!(
        srv.http("DELETE", "/v1/users/bob/token", Some(ADMIN), None)
            .0,
        204
    );
    assert_eq!(srv.http("GET", "/v1/lines", Some(&u), None).0, 401);
}

#[test]
fn sse_replays_from_last_event_id_then_streams_live() {
    let srv = Srv::start();
    let t = srv.enrol("vm", "vm");
    let u = srv.user("bob");
    let mut s = srv.station("vm", &t).unwrap();
    s.hello("vm", 0);
    s.send(json!({ "kind": "snapshot", "seq": 1, "lines": [
        line("vm-1", "vm", "alice", "active", None),
        line("vm-2", "vm", "alice", "active", None),
    ]}));
    s.recv();

    let mut all = srv.sse(&u, "", Some(0));
    let got: Vec<(i64, String)> = (0..3)
        .map(|_| all.next().map(|e| (e.0, e.1)).unwrap())
        .collect();
    assert_eq!(
        got,
        [
            (1, "station.online".into()),
            (2, "line.created".into()),
            (3, "line.created".into())
        ]
    );

    let mut tail = srv.sse(&u, "", Some(2));
    let (id, _, ev) = tail.next().unwrap();
    assert_eq!((id, ev["data"]["line"]["id"].as_str()), (3, Some("vm-2")));
    s.send(json!({ "kind": "delta", "seq": 2, "upsert": [line("vm-1", "vm", "alice", "hung-up", None)] }));
    s.recv();
    let (id, ty, ev) = tail.next().unwrap();
    assert_eq!((id, ty.as_str()), (4, "line.state_changed"));
    assert_eq!(
        (ev["data"]["from"].as_str(), ev["data"]["to"].as_str()),
        (Some("active"), Some("hung-up"))
    );
    assert_eq!(tail.next().unwrap().1, "line.hung_up");

    let mut created = srv.sse(&u, "?types=line.created", Some(0));
    assert_eq!(created.next().unwrap().0, 2);
    assert_eq!(created.next().unwrap().0, 3);
    assert!(created.next().is_none());
    assert!(srv.http("GET", "/v1/events", None, None).0 == 401);
}

#[test]
fn webhooks_are_hmac_signed() {
    let srv = Srv::start();
    let hook = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/hook", hook.local_addr().unwrap());
    let u = srv.user("bob");
    let body = json!({ "url": url, "types": ["line.created"] });
    assert_eq!(
        srv.http("POST", "/v1/webhooks", Some(&u), Some(body.clone()))
            .0,
        403
    );
    let https = json!({ "url": "https://x/y", "types": ["line.created"] });
    assert_eq!(
        srv.http("POST", "/v1/webhooks", Some(ADMIN), Some(https)).0,
        400
    );
    let (st, w) = srv.http("POST", "/v1/webhooks", Some(ADMIN), Some(body));
    assert_eq!(st, 201);
    let secret = w["secret"].as_str().unwrap().to_string();

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut c, _) = hook.accept().unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = c.read(&mut buf).unwrap();
            req.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&req).to_string();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let len: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                if body.len() >= len {
                    c.write_all(b"HTTP/1.1 204 No Content\r\n\r\n").unwrap();
                    tx.send((head.to_string(), body.to_string())).unwrap();
                    return;
                }
            }
        }
    });

    let t = srv.enrol("vm", "vm");
    let mut s = srv.station("vm", &t).unwrap();
    s.hello("vm", 0);
    s.send(json!({ "kind": "snapshot", "seq": 1, "lines": [line("vm-1", "vm", "alice", "active", None)] }));
    s.recv();

    let (head, body) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let header = |k: &str| {
        head.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}: ")))
            .unwrap()
            .to_string()
    };
    assert_eq!(header("X-Overseer-Event"), "line.created");
    let sig = header("X-Overseer-Signature");
    let ts: i64 = sig[2..sig.find(',').unwrap()].parse().unwrap();
    assert_eq!(ts, T0 / 1000);
    assert_eq!(sig, sign(&secret, ts, &body));
    assert_ne!(sig, sign("wrong", ts, &body));
    let ev: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(ev["data"]["line"]["id"], "vm-1");
    assert_eq!(ev["id"], header("X-Overseer-Delivery"));
}

#[test]
fn post_lines_starts_on_a_runner_without_storing_the_prompt() {
    let srv = Srv::start();
    let u = srv.user("feedback-hub");
    let prompt = "USER-FEEDBACK-CONTENT-4242";
    let body = json!({
        "owner": "feedback-bot", "repo": "github.com/acme/webshop", "branch": "feedback/AS-9",
        "agent": "claude", "origin": "feedback", "ticketKey": "AS-9", "prompt": prompt,
        "idempotencyKey": "fb-1",
    });
    assert_eq!(
        srv.http("POST", "/v1/lines", Some(&u), Some(body.clone()))
            .0,
        503
    );

    let t = srv.enrol("runner-3", "runner");
    let mut s = srv.station("runner-3", &t).unwrap();
    s.hello("runner-3", 0);
    s.send(json!({ "kind": "snapshot", "seq": 1, "lines": [] }));
    s.recv();

    let (addr, b, tok) = (srv.addr.clone(), body.clone(), u.clone());
    let req = std::thread::spawn(move || {
        let mut c = TcpStream::connect(addr).unwrap();
        let b = b.to_string();
        write!(
            c,
            "POST /v1/lines HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {tok}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        let v: Value = serde_json::from_str(out.split_once("\r\n\r\n").unwrap().1).unwrap();
        (out[9..12].to_string(), v)
    });
    let frame = s.recv();
    let cmd = &frame["command"];
    assert_eq!(
        (cmd["kind"].as_str(), cmd["id"].as_str()),
        (Some("start"), Some("fb-1"))
    );
    assert_eq!(cmd["args"]["prompt"], prompt);
    let line_id = cmd["lineId"].as_str().unwrap().to_string();
    assert!(line_id.starts_with("runner-3-"));
    s.send(json!({ "kind": "command_result", "command": { "id": "fb-1", "status": "delivered" } }));
    let (st, line) = req.join().unwrap();
    assert_eq!(st, "202");
    assert_eq!(line["id"], line_id.as_str());
    assert_eq!(
        (line["origin"].as_str(), line["ticketKey"].as_str()),
        (Some("feedback"), Some("AS-9"))
    );

    let (st, again) = srv.http("POST", "/v1/lines", Some(&u), Some(body));
    assert_eq!((st, again["id"].as_str()), (202, Some(line_id.as_str())));
    assert_eq!(
        ids(&srv
            .http("GET", "/v1/lines?owner=feedback-bot", Some(&u), None)
            .1)
        .len(),
        1
    );
    let audit = srv
        .http(
            "GET",
            &format!("/v1/lines/{line_id}/commands"),
            Some(&u),
            None,
        )
        .1;
    assert_eq!(audit[0]["args"]["ticketKey"], "AS-9");
    assert!(audit[0]["args"].get("prompt").is_none());
    assert!(!contains(&srv.stored_bytes(), prompt));
}
