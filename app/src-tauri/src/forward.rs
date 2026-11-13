use crate::conn::ConnManager;
use crate::http;
use crate::sse::SseParser;
use crate::transport::DaemonConn;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PortSettings {
    pub auto_forward: bool,
    pub ignore: Vec<u16>,
    pub localhost_only: bool,
}

impl Default for PortSettings {
    fn default() -> Self {
        Self {
            auto_forward: true,
            ignore: vec![22],
            localhost_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Forward {
    pub remote_port: u16,
    pub local_port: u16,
    pub local_addr: String,
    pub url: String,
    pub source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct PortsState {
    pub remote: bool,
    pub detected: Vec<Value>,
    pub forwards: Vec<Forward>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<Forward>,
}

pub type Watcher = Box<dyn Fn(&PortsState) + Send + Sync>;

struct Active {
    info: Forward,
    task: JoinHandle<()>,
}

#[derive(Default)]
struct Inner {
    settings: PortSettings,
    remote: bool,
    detected: BTreeMap<u16, Value>,
    forwards: BTreeMap<u16, Active>,
    watchers: HashMap<u32, Watcher>,
    tracker: Option<JoinHandle<()>>,
    generation: u64,
    stopped: std::collections::HashSet<u16>,
    closed: bool,
}

pub struct Ports {
    conn: Arc<ConnManager>,
    inner: Mutex<Inner>,
}

pub fn candidates(preferred: u16) -> Vec<u16> {
    let mut out: Vec<u16> = (0..=20u16)
        .filter_map(|i| preferred.checked_add(i))
        .filter(|p| *p != 0)
        .collect();
    out.push(0);
    out
}

pub async fn bind_free(addr: &str, preferred: u16) -> std::io::Result<TcpListener> {
    let mut last = None;
    for p in candidates(preferred) {
        match TcpListener::bind((addr, p)).await {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("no free port")))
}

fn port_of(v: &Value) -> Option<u16> {
    v.get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
}

fn wants_auto(s: &PortSettings, port: u16) -> bool {
    s.auto_forward && !s.ignore.contains(&port)
}

impl Ports {
    pub fn new(conn: Arc<ConnManager>) -> Arc<Self> {
        Arc::new(Self {
            conn,
            inner: Mutex::new(Inner::default()),
        })
    }

    pub fn snapshot(&self) -> PortsState {
        let g = self.inner.lock().unwrap();
        PortsState {
            remote: g.remote,
            detected: g.detected.values().cloned().collect(),
            forwards: g.forwards.values().map(|a| a.info.clone()).collect(),
            added: vec![],
        }
    }

    fn notify(&self, added: Vec<Forward>) {
        let mut s = self.snapshot();
        s.added = added;
        let g = self.inner.lock().unwrap();
        for w in g.watchers.values() {
            w(&s);
        }
    }

    pub fn watch(&self, id: u32, w: Watcher) {
        let s = self.snapshot();
        w(&s);
        self.inner.lock().unwrap().watchers.insert(id, w);
    }

    pub fn unwatch(&self, id: u32) {
        self.inner.lock().unwrap().watchers.remove(&id);
    }

    pub fn unwatch_all(&self) -> usize {
        let mut g = self.inner.lock().unwrap();
        let n = g.watchers.len();
        g.watchers.clear();
        n
    }

    pub fn reset(&self) {
        let mut g = self.inner.lock().unwrap();
        g.generation += 1;
        for (_, a) in std::mem::take(&mut g.forwards) {
            a.task.abort();
        }
        g.detected.clear();
        g.stopped.clear();
        g.remote = false;
        drop(g);
        self.notify(vec![]);
    }

    pub async fn configure(self: &Arc<Self>, settings: PortSettings) {
        let rebind = {
            let mut g = self.inner.lock().unwrap();
            let rebind = g.settings.localhost_only != settings.localhost_only;
            g.settings = settings;
            rebind
        };
        if rebind {
            let olds: Vec<(u16, u16, &'static str)> = {
                let g = self.inner.lock().unwrap();
                g.forwards
                    .values()
                    .map(|a| (a.info.remote_port, a.info.local_port, a.info.source))
                    .collect()
            };
            for (remote, local, source) in olds {
                self.drop_forward(remote);
                let _ = self.forward(remote, Some(local), source).await;
            }
        }
        let added = self.sync_auto().await;
        self.notify(added);
    }

    pub fn stop(&self) {
        {
            let mut g = self.inner.lock().unwrap();
            g.closed = true;
            g.watchers.clear();
            if let Some(t) = g.tracker.take() {
                t.abort();
            }
        }
        self.reset();
    }

    pub fn ensure_tracking(self: &Arc<Self>) {
        let mut g = self.inner.lock().unwrap();
        if g.closed || g.tracker.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        let me = self.clone();
        g.tracker = Some(tokio::spawn(async move { me.track().await }));
    }

    async fn track(self: Arc<Self>) {
        loop {
            let conn = self.conn.wait_current().await;
            let remote = matches!(*conn, DaemonConn::Tunnel(_));
            let generation = {
                let mut g = self.inner.lock().unwrap();
                g.remote = remote;
                g.generation
            };
            if let Ok(res) = conn.request("GET", "/v1/ports", None).await {
                if res.head.status == 200 {
                    let list = http::json_body(&res.body);
                    let ports: Vec<Value> = list.as_array().cloned().unwrap_or_default();
                    self.replace_detected(ports, generation).await;
                }
            }
            let mut parser = SseParser::default();
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
            let me = self.clone();
            let applier = tokio::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    me.apply_event(&ev, generation).await;
                }
            });
            crate::sse::stream(&conn, None, &mut parser, |v| {
                if v["type"].as_str().is_some_and(|t| t.starts_with("port.")) {
                    let _ = tx.send(v);
                }
            })
            .await;
            drop(tx);
            let _ = applier.await;
            drop(conn);
            let mut rx = self.conn.subscribe();
            let _ =
                tokio::time::timeout(Duration::from_secs(2), rx.wait_for(|c| c.is_none())).await;
        }
    }

    async fn replace_detected(self: &Arc<Self>, ports: Vec<Value>, generation: u64) {
        {
            let mut g = self.inner.lock().unwrap();
            if g.generation != generation {
                return;
            }
            g.detected = ports
                .into_iter()
                .filter_map(|p| Some((port_of(&p)?, p)))
                .collect();
            let gone: Vec<u16> = g
                .forwards
                .iter()
                .filter(|(p, a)| a.info.source == "auto" && !g.detected.contains_key(p))
                .map(|(p, _)| *p)
                .collect();
            for p in gone {
                if let Some(a) = g.forwards.remove(&p) {
                    a.task.abort();
                }
            }
        }
        let added = self.sync_auto().await;
        self.notify(added);
    }

    async fn apply_event(self: &Arc<Self>, ev: &Value, generation: u64) {
        let data = &ev["data"];
        let Some(port) = port_of(data) else { return };
        let mut added = vec![];
        match ev["type"].as_str() {
            Some("port.opened") => {
                {
                    let mut g = self.inner.lock().unwrap();
                    if g.generation != generation {
                        return;
                    }
                    g.detected.insert(port, data.clone());
                }
                added = self.sync_auto().await;
            }
            Some("port.closed") => {
                let mut g = self.inner.lock().unwrap();
                g.detected.remove(&port);
                if g.forwards
                    .get(&port)
                    .is_some_and(|a| a.info.source == "auto")
                {
                    if let Some(a) = g.forwards.remove(&port) {
                        a.task.abort();
                    }
                }
            }
            _ => return,
        }
        self.notify(added);
    }

    async fn sync_auto(self: &Arc<Self>) -> Vec<Forward> {
        let todo: Vec<u16> = {
            let g = self.inner.lock().unwrap();
            if !g.remote {
                return vec![];
            }
            g.detected
                .keys()
                .copied()
                .filter(|p| {
                    wants_auto(&g.settings, *p)
                        && !g.forwards.contains_key(p)
                        && !g.stopped.contains(p)
                })
                .collect()
        };
        let mut added = vec![];
        for p in todo {
            if let Ok(f) = self.forward(p, None, "auto").await {
                added.push(f);
            }
        }
        added
    }

    pub async fn forward(
        self: &Arc<Self>,
        remote_port: u16,
        local_port: Option<u16>,
        source: &'static str,
    ) -> Result<Forward, String> {
        let (addr, detected, remote) = {
            let mut g = self.inner.lock().unwrap();
            if let Some(a) = g.forwards.get(&remote_port) {
                if source == "manual" {
                    return Err(format!(
                        "port {remote_port} is already forwarded to {}",
                        a.info.local_addr
                    ));
                }
                return Ok(a.info.clone());
            }
            if source == "manual" {
                g.stopped.remove(&remote_port);
            }
            (
                if g.settings.localhost_only {
                    "127.0.0.1"
                } else {
                    "0.0.0.0"
                },
                g.detected.get(&remote_port).cloned(),
                g.remote,
            )
        };
        if !remote {
            return Err(
                "ports of the local daemon are reachable directly; forwarding is for remote hosts"
                    .into(),
            );
        }
        let listener = bind_free(addr, local_port.unwrap_or(remote_port))
            .await
            .map_err(|e| format!("cannot listen on {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| e.to_string())?.port();
        let d = detected.unwrap_or(Value::Null);
        let info = Forward {
            remote_port,
            local_port: local,
            local_addr: format!("{addr}:{local}"),
            url: format!("http://localhost:{local}"),
            source,
            process: d["process"].as_str().map(String::from),
            pid: d["pid"].as_u64(),
            term_id: d["termId"].as_str().map(String::from),
            project: d["project"].as_str().map(String::from),
            error: None,
        };
        let conn = self.conn.clone();
        let task = tokio::spawn(serve(listener, conn, remote_port));
        let mut g = self.inner.lock().unwrap();
        if let Some(old) = g.forwards.insert(
            remote_port,
            Active {
                info: info.clone(),
                task,
            },
        ) {
            old.task.abort();
        }
        drop(g);
        if source == "manual" {
            self.notify(vec![]);
        }
        Ok(info)
    }

    fn drop_forward(&self, remote_port: u16) -> bool {
        let removed = self.inner.lock().unwrap().forwards.remove(&remote_port);
        removed.map(|a| a.task.abort()).is_some()
    }

    pub fn unforward(&self, remote_port: u16) {
        self.inner.lock().unwrap().stopped.insert(remote_port);
        if self.drop_forward(remote_port) {
            self.notify(vec![]);
        }
    }
}

async fn serve(listener: TcpListener, conn: Arc<ConnManager>, remote_port: u16) {
    while let Ok((mut sock, _)) = listener.accept().await {
        let conn = conn.clone();
        tokio::spawn(async move {
            let Ok(c) = tokio::time::timeout(Duration::from_secs(30), conn.wait_current()).await
            else {
                return;
            };
            let DaemonConn::Tunnel(t) = &*c else { return };
            let Ok(mut stream) = t.mux.open(&format!("tcp:{remote_port}")).await else {
                return;
            };
            drop(c);
            let _ = tokio::io::copy_bidirectional(&mut sock, &mut stream).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stopped_ports_never_track_again() {
        let p = Ports::new(Arc::new(ConnManager::default()));
        p.ensure_tracking();
        assert!(p.inner.lock().unwrap().tracker.is_some());
        p.watch(1, Box::new(|_| {}));
        p.stop();
        p.ensure_tracking();
        let g = p.inner.lock().unwrap();
        assert!(g.tracker.is_none() && g.watchers.is_empty());
    }

    #[test]
    fn candidate_ports() {
        let c = candidates(5173);
        assert_eq!(&c[..3], &[5173, 5174, 5175]);
        assert_eq!(c.len(), 22);
        assert_eq!(*c.last().unwrap(), 0);
        assert_eq!(candidates(65535), vec![65535, 0]);
    }

    #[tokio::test]
    async fn binds_the_next_free_port_when_taken() {
        let taken = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = taken.local_addr().unwrap().port();
        let l = bind_free("127.0.0.1", port).await.unwrap();
        let got = l.local_addr().unwrap().port();
        assert_ne!(got, port);
        assert!(got > port && got <= port.saturating_add(20) || port > 65515);
    }

    #[test]
    fn auto_forward_respects_settings() {
        let s = PortSettings::default();
        assert!(wants_auto(&s, 5173));
        assert!(!wants_auto(&s, 22));
        assert!(!wants_auto(
            &PortSettings {
                auto_forward: false,
                ..s.clone()
            },
            5173
        ));
        let v: PortSettings = serde_json::from_value(
            serde_json::json!({"autoForward":true,"ignore":[22,9000],"localhostOnly":false}),
        )
        .unwrap();
        assert!(!wants_auto(&v, 9000) && !v.localhost_only);
    }

    #[tokio::test]
    async fn local_mode_lists_ports_without_forwarding() {
        let p = Ports::new(Arc::new(ConnManager::default()));
        let seen = Arc::new(Mutex::new(Vec::<PortsState>::new()));
        let s2 = seen.clone();
        p.watch(1, Box::new(move |s| s2.lock().unwrap().push(s.clone())));
        let gen = p.inner.lock().unwrap().generation;
        p.replace_detected(
            vec![serde_json::json!({"port": 5173, "process": "node"})],
            gen,
        )
        .await;
        assert!(p
            .forward(5173, None, "manual")
            .await
            .unwrap_err()
            .contains("local"));
        let last = seen.lock().unwrap().last().cloned().unwrap();
        assert!(!last.remote && last.forwards.is_empty() && last.added.is_empty());
        assert_eq!(port_of(&last.detected[0]), Some(5173));
        p.reset();
        assert!(seen.lock().unwrap().last().unwrap().detected.is_empty());
    }

    #[tokio::test]
    async fn remote_ports_are_auto_forwarded_and_dropped_on_close() {
        let p = Ports::new(Arc::new(ConnManager::default()));
        p.inner.lock().unwrap().remote = true;
        let seen = Arc::new(Mutex::new(Vec::<PortsState>::new()));
        let s2 = seen.clone();
        p.watch(1, Box::new(move |s| s2.lock().unwrap().push(s.clone())));
        let gen = p.inner.lock().unwrap().generation;
        let free = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let ev = serde_json::json!({"type": "port.opened", "data": {"port": free, "process": "python3", "pid": 7, "termId": "t1"}});
        p.apply_event(&ev, gen).await;
        let last = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.added.len(), 1);
        let f = &last.added[0];
        assert_eq!(
            (
                f.remote_port,
                f.source,
                f.process.as_deref(),
                f.term_id.as_deref()
            ),
            (free, "auto", Some("python3"), Some("t1"))
        );
        assert_eq!(f.url, format!("http://localhost:{}", f.local_port));
        let ignored = serde_json::json!({"type": "port.opened", "data": {"port": 22}});
        p.apply_event(&ignored, gen).await;
        assert_eq!(p.snapshot().forwards.len(), 1);
        p.apply_event(
            &serde_json::json!({"type": "port.closed", "data": {"port": free}}),
            gen,
        )
        .await;
        let s = p.snapshot();
        assert!(s.forwards.is_empty());
        assert_eq!(s.detected.len(), 1);
        let m = p.forward(free, None, "manual").await.unwrap();
        p.apply_event(
            &serde_json::json!({"type": "port.closed", "data": {"port": free}}),
            gen,
        )
        .await;
        assert_eq!(p.snapshot().forwards, vec![m]);
        assert!(p
            .forward(free, None, "manual")
            .await
            .unwrap_err()
            .contains("already forwarded"));
        p.unforward(free);
        assert!(p.snapshot().forwards.is_empty());
        p.apply_event(&ev, gen).await;
        assert!(
            p.snapshot().forwards.is_empty(),
            "a stopped port is not auto-forwarded again"
        );
        p.configure(PortSettings::default()).await;
        assert!(p.snapshot().forwards.is_empty());
        p.forward(free, None, "manual").await.unwrap();
        p.unforward(free);
    }
}
