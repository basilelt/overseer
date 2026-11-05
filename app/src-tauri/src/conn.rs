use crate::askpass::{Handler, Prompt};
use crate::http;
use crate::install::{self, Fallback, Platform, ServerUpdate, SERVER_VERSION};
use crate::ssh;
use crate::sshg::SshCmd;
use crate::transport::{self, local_socket, DaemonConn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Target {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ConnState {
    pub phase: &'static str,
    pub target: Target,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerUpdate>,
}

impl ConnState {
    pub fn new(phase: &'static str, target: &Target) -> Self {
        Self {
            phase,
            target: target.clone(),
            ..Default::default()
        }
    }
}

pub type Emit = Arc<dyn Fn(ConnState) + Send + Sync>;

pub struct Reporter {
    emit: Emit,
    last: Mutex<ConnState>,
}

impl Reporter {
    pub fn new(emit: Emit, target: &Target) -> Arc<Self> {
        Arc::new(Self {
            emit,
            last: Mutex::new(ConnState::new("idle", target)),
        })
    }

    pub fn set(&self, mut s: ConnState) {
        let mut last = self.last.lock().unwrap();
        if s.resolved.is_none() && s.target == last.target {
            s.resolved.clone_from(&last.resolved);
        }
        let changed = describe_change(&last, &s);
        *last = s.clone();
        drop(last);
        if let Some(line) = changed {
            if s.phase == "error" {
                crate::diag::error("conn", line);
            } else if s.phase == "reconnecting" {
                crate::diag::warn("conn", line);
            } else {
                crate::diag::info("conn", line);
            }
        }
        (self.emit)(s);
    }

    fn remember_resolved(&self, r: &str) {
        let mut last = self.last.lock().unwrap();
        if last.resolved.as_deref() != Some(r) {
            crate::diag::info(
                "conn",
                format!("{} resolved: {r}", target_label(&last.target)),
            );
        }
        last.resolved = Some(r.to_string());
    }

    pub fn prompt(&self, p: Option<Prompt>) {
        let mut s = self.last.lock().unwrap().clone();
        match &p {
            Some(p) => crate::diag::info(
                "conn",
                format!("{} prompt shown ({})", target_label(&s.target), p.kind),
            ),
            None if s.prompt.is_some() => crate::diag::info("conn", "prompt closed"),
            None => {}
        }
        s.prompt = p;
        (self.emit)(s);
    }
}

#[derive(Clone, Default)]
pub struct Ctx {
    pub resource_dir: Option<PathBuf>,
    pub bin_dir: Option<PathBuf>,
    pub exe_dir: Option<PathBuf>,
    pub ssh: SshCmd,
    pub askpass: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub cache_dir: Option<PathBuf>,
}

pub fn backoff(attempt: u32) -> Duration {
    Duration::from_millis((500u64 << attempt.min(6)).min(30_000))
}

#[derive(Debug, PartialEq)]
pub enum Next {
    Retry(Duration),
    GiveUp,
}

#[derive(Default)]
pub struct Reconnect {
    attempt: u32,
}

impl Reconnect {
    pub fn attempt(&self) -> u32 {
        self.attempt + 1
    }

    pub fn woke(&mut self) {
        self.attempt = 0;
    }

    pub fn failed(&mut self, err: &str) -> Next {
        if ssh::permanent(err) {
            return Next::GiveUp;
        }
        let d = backoff(self.attempt);
        self.attempt += 1;
        Next::Retry(d)
    }
}

type Session = (Target, Arc<Reporter>, Ctx);

pub struct ConnManager {
    tx: watch::Sender<Option<Arc<DaemonConn>>>,
    monitor: Mutex<Option<JoinHandle<()>>>,
    reporter: Mutex<Option<Arc<Reporter>>>,
    session: Mutex<Option<Session>>,
    cancelled: std::sync::atomic::AtomicBool,
    wake: tokio::sync::Notify,
}

impl Default for ConnManager {
    fn default() -> Self {
        Self {
            tx: watch::channel(None).0,
            monitor: Mutex::new(None),
            reporter: Mutex::new(None),
            session: Mutex::new(None),
            cancelled: Default::default(),
            wake: Default::default(),
        }
    }
}

impl ConnManager {
    pub fn current(&self) -> Option<Arc<DaemonConn>> {
        self.tx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<DaemonConn>>> {
        self.tx.subscribe()
    }

    pub async fn wait_current(&self) -> Arc<DaemonConn> {
        let mut rx = self.tx.subscribe();
        let c = rx
            .wait_for(|c| c.is_some())
            .await
            .ok()
            .and_then(|c| c.clone());
        match c {
            Some(c) => c,
            None => std::future::pending().await,
        }
    }

    pub fn clear_prompt(&self, cancelled: bool) {
        if cancelled {
            self.cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(r) = self.reporter.lock().unwrap().as_ref() {
            r.prompt(None);
        }
    }

    fn with_cancel(&self, e: String) -> String {
        if self
            .cancelled
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            return format!("{e}\n(prompt cancelled)");
        }
        e
    }

    pub fn wake(&self) {
        crate::diag::info("conn", "wake: checking the connection after resume");
        self.wake.notify_waiters();
        let Some(conn) = self.current() else { return };
        tauri::async_runtime::spawn(async move {
            let alive = tokio::time::timeout(
                Duration::from_secs(5),
                conn.request("GET", "/v1/version", None),
            )
            .await
            .is_ok_and(|r| r.is_ok());
            if let (false, DaemonConn::Tunnel(t)) = (alive, &*conn) {
                crate::diag::warn("conn", "no answer after resume, dropping the tunnel");
                t.mux.kill("no answer after the app resumed");
            }
        });
    }

    pub fn disconnect(&self) {
        if let Some((t, _, _)) = self.session.lock().unwrap().as_ref() {
            crate::diag::info("conn", format!("{} disconnect", target_label(t)));
        }
        if let Some(h) = self.monitor.lock().unwrap().take() {
            h.abort();
        }
        self.reporter.lock().unwrap().take();
        self.session.lock().unwrap().take();
        self.tx.send_replace(None);
    }

    pub async fn connect(
        self: &Arc<Self>,
        target: Target,
        emit: Emit,
        ctx: Ctx,
    ) -> Result<(), String> {
        self.disconnect();
        let rep = Reporter::new(emit, &target);
        *self.reporter.lock().unwrap() = Some(rep.clone());
        *self.session.lock().unwrap() = Some((target.clone(), rep.clone(), ctx.clone()));
        self.start(target, rep, ctx, true, false).await
    }

    pub async fn restart_server(self: &Arc<Self>) -> Result<(), String> {
        let (target, rep, ctx) = self
            .session
            .lock()
            .unwrap()
            .clone()
            .ok_or("not connected")?;
        if let Some(h) = self.monitor.lock().unwrap().take() {
            h.abort();
        }
        self.tx.send_replace(None);
        rep.set(ConnState {
            attempt: Some(1),
            detail: Some("restarting the server".into()),
            ..ConnState::new("reconnecting", &target)
        });
        self.start(target, rep, ctx, false, true).await
    }

    async fn start(
        self: &Arc<Self>,
        target: Target,
        rep: Arc<Reporter>,
        ctx: Ctx,
        steps: bool,
        replace: bool,
    ) -> Result<(), String> {
        match establish(&target, &rep, &ctx, steps, replace).await {
            Ok(e) => {
                self.tx.send_replace(Some(Arc::new(e.conn)));
                rep.set(ConnState {
                    version: Some(e.version),
                    server: e.server,
                    ..ConnState::new("connected", &target)
                });
                let me = self.clone();
                *self.monitor.lock().unwrap() =
                    Some(tokio::spawn(
                        async move { me.monitor(target, rep, ctx).await },
                    ));
                Ok(())
            }
            Err(e) => {
                rep.set(ConnState {
                    error: Some(e.clone()),
                    ..ConnState::new("error", &target)
                });
                Err(e)
            }
        }
    }

    async fn monitor(self: Arc<Self>, target: Target, rep: Arc<Reporter>, ctx: Ctx) {
        loop {
            let Some(conn) = self.current() else { return };
            let reason = match &*conn {
                DaemonConn::Tunnel(t) => t.mux.dead().await,
                _ => loop {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    if tokio::time::timeout(
                        Duration::from_secs(5),
                        conn.request("GET", "/v1/version", None),
                    )
                    .await
                    .is_ok_and(|r| r.is_ok())
                    {
                        continue;
                    }
                    break "the daemon stopped answering".to_string();
                },
            };
            drop(conn);
            crate::diag::warn(
                "conn",
                format!("{} connection lost: {reason}", target_label(&target)),
            );
            self.tx.send_replace(None);
            let mut rc = Reconnect::default();
            let mut last = reason;
            loop {
                rep.set(ConnState {
                    attempt: Some(rc.attempt()),
                    detail: Some(last.clone()),
                    ..ConnState::new("reconnecting", &target)
                });
                self.cancelled
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                match establish(&target, &rep, &ctx, false, false).await {
                    Ok(e) => {
                        self.tx.send_replace(Some(Arc::new(e.conn)));
                        rep.set(ConnState {
                            version: Some(e.version),
                            server: e.server,
                            ..ConnState::new("connected", &target)
                        });
                        break;
                    }
                    Err(e) => match rc.failed(&self.with_cancel(e.clone())) {
                        Next::Retry(d) => {
                            last = e;
                            tokio::select! {
                                _ = tokio::time::sleep(d) => {}
                                _ = self.wake.notified() => rc.woke(),
                            }
                        }
                        Next::GiveUp => {
                            rep.set(ConnState {
                                error: Some(e),
                                ..ConnState::new("error", &target)
                            });
                            return;
                        }
                    },
                }
            }
        }
    }
}

fn target_label(t: &Target) -> String {
    match &t.host {
        Some(h) => format!("{} {h}", t.kind),
        None => t.kind.clone(),
    }
}

fn describe_change(last: &ConnState, s: &ConnState) -> Option<String> {
    if last.phase == s.phase
        && last.detail == s.detail
        && last.error == s.error
        && last.attempt == s.attempt
        && last.version == s.version
        && last.target == s.target
    {
        return None;
    }
    let mut out = format!("{} {}", target_label(&s.target), s.phase);
    if let Some(a) = s.attempt {
        out.push_str(&format!(" (attempt {a})"));
    }
    if let Some(v) = &s.version {
        out.push_str(&format!(", server {v}"));
    }
    if let Some(d) = &s.detail {
        out.push_str(&format!(": {d}"));
    }
    if let Some(e) = &s.error {
        out.push_str(&format!(": {e}"));
    }
    if let Some(u) = &s.server {
        out.push_str(&format!(
            "; server update pending: running {}, installed {}, {} live terminals ({})",
            u.running, u.installed, u.terms, u.reason
        ));
    }
    Some(out)
}

fn step(rep: &Reporter, target: &Target, phase: &'static str, detail: Option<String>) {
    rep.set(ConnState {
        detail,
        ..ConnState::new(phase, target)
    });
}

pub struct Established {
    pub conn: DaemonConn,
    pub version: String,
    pub server: Option<ServerUpdate>,
}

struct Opened {
    conn: DaemonConn,
    version: String,
    installed: Option<String>,
    upgrade: Value,
}

impl Opened {
    fn remote(conn: DaemonConn, version: String, installed: Option<String>) -> Self {
        Self {
            conn,
            version,
            installed,
            upgrade: json!({ "version": SERVER_VERSION }),
        }
    }
}

async fn establish(
    target: &Target,
    rep: &Arc<Reporter>,
    ctx: &Ctx,
    steps: bool,
    mut replace: bool,
) -> Result<Established, String> {
    loop {
        let o = open(target, rep, ctx, steps, replace).await?;
        if !install::should_upgrade(&o.version, SERVER_VERSION, o.installed.as_deref()) {
            return Ok(Established {
                conn: o.conn,
                version: o.version,
                server: None,
            });
        }
        if steps {
            step(
                rep,
                target,
                "starting",
                Some(format!("{} → {SERVER_VERSION}", o.version)),
            );
        }
        crate::diag::info(
            "conn",
            format!(
                "{} runs server {}, upgrading to {SERVER_VERSION} in place",
                target_label(target),
                o.version
            ),
        );
        let reason = match hot_upgrade(&o.conn, &o.upgrade).await {
            Ok(v) => {
                crate::diag::info("conn", format!("hot upgrade done, server {v}"));
                return Ok(Established {
                    conn: o.conn,
                    version: v,
                    server: None,
                });
            }
            Err(e) => e,
        };
        let terms = live_terms(&o.conn).await;
        crate::diag::warn(
            "conn",
            format!(
                "hot upgrade unavailable ({reason}), {} live terminals",
                terms.map(|t| t.to_string()).unwrap_or_else(|| "?".into())
            ),
        );
        match install::fallback(&o.version, SERVER_VERSION, terms, &reason, replace) {
            Fallback::Replace => {
                crate::diag::info("conn", "restarting the server with --replace");
                drop(o);
                replace = true;
            }
            Fallback::Offer(u) => {
                return Ok(Established {
                    conn: o.conn,
                    version: o.version,
                    server: Some(u),
                })
            }
        }
    }
}

async fn open(
    target: &Target,
    rep: &Arc<Reporter>,
    ctx: &Ctx,
    steps: bool,
    replace: bool,
) -> Result<Opened, String> {
    if steps {
        step(rep, target, "connecting", None);
    }
    match (target.kind.as_str(), &target.host) {
        ("ssh", Some(host)) => establish_ssh(host, target, rep, ctx, steps, replace).await,
        ("ssh", None) => Err("ssh target without a host".into()),
        ("direct", Some(id)) => establish_direct(id, target, rep, ctx, steps, replace).await,
        ("direct", None) => Err("direct target without a saved host".into()),
        _ => establish_local(target, rep, ctx, steps, replace).await,
    }
}

fn error_of(res: &http::Response) -> String {
    http::json_body(&res.body)
        .get("error")
        .and_then(|e| e.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("HTTP {}", res.head.status))
}

async fn hot_upgrade(conn: &DaemonConn, body: &Value) -> Result<String, String> {
    if body.is_null() {
        return Err("no server binary to upgrade to".into());
    }
    let res = conn
        .request("POST", "/v1/upgrade", Some(body.to_string().as_bytes()))
        .await
        .map_err(|e| format!("hot upgrade: {e}"))?;
    if res.head.status != 202 {
        return Err(format!("hot upgrade: {}", error_of(&res)));
    }
    let end = tokio::time::Instant::now() + Duration::from_secs(20);
    let mut last = String::new();
    while tokio::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(300)).await;
        match conn.version().await {
            Ok(v) if !install::older(&v, SERVER_VERSION) => return Ok(v),
            Ok(v) => last = format!("still {v}"),
            Err(e) => last = e,
        }
    }
    Err(format!(
        "hot upgrade: the daemon did not come back as {SERVER_VERSION} ({last})"
    ))
}

async fn live_terms(conn: &DaemonConn) -> Option<usize> {
    let res = conn.request("GET", "/v1/terms", None).await.ok()?;
    (res.head.status == 200)
        .then(|| install::live_terms(&http::json_body(&res.body)))
        .flatten()
}

async fn wait_version(conn: &DaemonConn, tries: u32) -> Result<String, String> {
    let mut last = String::new();
    for i in 0..tries {
        match conn.version().await {
            Ok(v) => return Ok(v),
            Err(e) => last = e,
        }
        if i + 1 < tries {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    Err(last)
}

fn local_binary(ctx: &Ctx) -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    if let Some(e) = &ctx.exe_dir {
        c.push(e.join(if cfg!(windows) {
            "overseer.exe"
        } else {
            "overseer"
        }));
    }
    if let Some(p) = Platform::local() {
        c.extend(install::binary_candidates(
            p,
            ctx.resource_dir.as_deref(),
            ctx.bin_dir.as_deref(),
            None,
        ));
    }
    if let Some(path) = std::env::var_os("PATH") {
        c.extend(std::env::split_paths(&path).map(|d| d.join("overseer")));
    }
    c.into_iter().find(|p| p.is_file())
}

async fn run(bin: &Path, args: &[&str]) -> Result<String, String> {
    let out = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::process::Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .map_err(|_| format!("{} {}: timed out", bin.display(), args.join(" ")))?
    .map_err(|e| format!("{}: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!(
            "{} {} failed: {}",
            bin.display(),
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

async fn local_opened(conn: DaemonConn, version: String, ctx: &Ctx) -> Opened {
    let mut o = Opened {
        conn,
        version,
        installed: None,
        upgrade: Value::Null,
    };
    if install::older(&o.version, SERVER_VERSION) {
        if let Some(bin) = local_binary(ctx) {
            if let Ok(out) = run(&bin, &["version"]).await {
                o.installed = out.split_whitespace().nth(1).map(String::from);
                o.upgrade = json!({ "path": bin.display().to_string() });
            }
        }
    }
    o
}

async fn establish_local(
    target: &Target,
    rep: &Reporter,
    ctx: &Ctx,
    steps: bool,
    replace: bool,
) -> Result<Opened, String> {
    if let (false, Ok(conn)) = (replace, transport::local()) {
        match wait_version(&conn, 1).await {
            Ok(v) => return Ok(local_opened(conn, v, ctx).await),
            Err(e) if e.contains("protocol") => return Err(e),
            Err(_) => {}
        }
    }
    if steps {
        step(rep, target, "starting", None);
    }
    let bin = local_binary(ctx).ok_or_else(|| {
        format!(
            "the overseer daemon is not running at {} and no overseer binary was found (app folder, bundled resources, server binaries folder, PATH)",
            local_socket().display()
        )
    })?;
    let args: &[&str] = if replace {
        &["daemon", "--detach", "--replace"]
    } else {
        &["daemon", "--detach"]
    };
    let started = run(&bin, args).await;
    let conn = if cfg!(windows) {
        transport::local()?
    } else {
        let printed = started
            .as_ref()
            .ok()
            .and_then(|s| s.lines().last())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        DaemonConn::Local(printed.unwrap_or_else(local_socket))
    };
    match wait_version(&conn, 20).await {
        Ok(v) => Ok(local_opened(conn, v, ctx).await),
        Err(e) => Err(started
            .err()
            .unwrap_or_else(|| format!("daemon did not answer after start: {e}"))),
    }
}

pub fn ssh_options(ctx: &Ctx, replace: bool) -> ssh::Options {
    let (res, bin, exe) = (
        ctx.resource_dir.clone(),
        ctx.bin_dir.clone(),
        ctx.exe_dir.clone(),
    );
    ssh::Options {
        ssh: ctx.ssh.clone(),
        askpass_program: ctx.askpass.clone(),
        binaries: Box::new(move |p| {
            install::binary_candidates(p, res.as_deref(), bin.as_deref(), exe.as_deref())
        }),
        replace,
    }
}

async fn establish_ssh(
    host: &str,
    target: &Target,
    rep: &Arc<Reporter>,
    ctx: &Ctx,
    steps: bool,
    replace: bool,
) -> Result<Opened, String> {
    let r = rep.clone();
    let on_prompt: Handler = Arc::new(move |p| {
        r.prompt(Some(p));
        Box::pin(std::future::pending())
    });
    let on_step = |phase: &'static str, detail: Option<String>, resolved: &str| {
        rep.remember_resolved(resolved);
        if steps {
            step(rep, target, phase, detail);
        }
    };
    let tunnel =
        Arc::new(ssh::connect(host, &ssh_options(ctx, replace), on_prompt, &on_step).await?);
    ssh::watch_exit(&tunnel);
    let installed = tunnel.installed.clone();
    let conn = DaemonConn::Tunnel(tunnel);
    let v = wait_version(&conn, 20)
        .await
        .map_err(|e| format!("remote daemon did not answer: {e}"))?;
    Ok(Opened::remote(conn, v, installed))
}

async fn establish_direct(
    id: &str,
    target: &Target,
    rep: &Arc<Reporter>,
    ctx: &Ctx,
    steps: bool,
    replace: bool,
) -> Result<Opened, String> {
    let dir = ctx.data_dir.clone().ok_or("no app data folder")?;
    let on_step = |phase: &'static str, detail: Option<String>| {
        if steps {
            step(rep, target, phase, detail);
        }
    };
    let (tunnel, described) = crate::direct::connect(
        &dir,
        ctx.cache_dir.as_deref(),
        id,
        rep.clone(),
        replace,
        &on_step,
    )
    .await?;
    rep.remember_resolved(&described);
    let tunnel = Arc::new(tunnel);
    ssh::watch_exit(&tunnel);
    let installed = tunnel.installed.clone();
    let conn = DaemonConn::Tunnel(tunnel);
    let v = wait_version(&conn, 20)
        .await
        .map_err(|e| format!("remote daemon did not answer: {e}"))?;
    Ok(Opened::remote(conn, v, installed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(0), Duration::from_millis(500));
        assert_eq!(backoff(1), Duration::from_secs(1));
        assert_eq!(backoff(5), Duration::from_secs(16));
        assert_eq!(backoff(6), Duration::from_secs(30));
        assert_eq!(backoff(40), Duration::from_secs(30));
    }

    #[test]
    fn reconnect_retries_transient_errors_and_stops_on_permanent_ones() {
        let mut rc = Reconnect::default();
        assert_eq!(rc.attempt(), 1);
        assert_eq!(
            rc.failed("ssh: connect to host w port 22: No route to host"),
            Next::Retry(Duration::from_millis(500))
        );
        assert_eq!(
            rc.failed("Connection timed out during banner exchange"),
            Next::Retry(Duration::from_secs(1))
        );
        assert_eq!(
            rc.failed("Permission denied (publickey)."),
            Next::Retry(Duration::from_secs(2))
        );
        assert_eq!(rc.attempt(), 4);
        for _ in 0..10 {
            rc.failed("network is unreachable");
        }
        assert_eq!(
            rc.failed("network is unreachable"),
            Next::Retry(Duration::from_secs(30))
        );
        assert_eq!(rc.failed("Host key verification failed."), Next::GiveUp);
        rc.woke();
        assert_eq!(rc.attempt(), 1);
        assert_eq!(
            rc.failed("ssh: connect to host w port 22: Network is unreachable"),
            Next::Retry(Duration::from_millis(500))
        );
        assert_eq!(
            rc.failed(&crate::direct::client::host_key_changed("w", 22)),
            Next::GiveUp
        );
        assert_eq!(
            rc.failed(&crate::direct::client::auth_failed("u@w:22")),
            Next::GiveUp
        );
        assert_eq!(
            rc.failed("Host key of w not trusted (cancelled)"),
            Next::GiveUp
        );
        assert_eq!(rc.failed(crate::ssh::NOT_INSTALLED), Next::GiveUp);
        assert_eq!(
            Reconnect::default().failed("prompt cancelled"),
            Next::GiveUp
        );
        let m = ConnManager::default();
        assert_eq!(
            m.with_cancel("Permission denied (publickey).".into()),
            "Permission denied (publickey)."
        );
        m.clear_prompt(true);
        let e = m.with_cancel("Permission denied (publickey).".into());
        assert_eq!(Reconnect::default().failed(&e), Next::GiveUp);
        assert_eq!(m.with_cancel("x".into()), "x");
    }

    #[test]
    fn reporter_keeps_the_resolution_and_overlays_prompts() {
        let seen = Arc::new(Mutex::new(Vec::<ConnState>::new()));
        let s2 = seen.clone();
        let t = Target {
            kind: "ssh".into(),
            host: Some("ws".into()),
            label: "ws".into(),
        };
        let rep = Reporter::new(Arc::new(move |s| s2.lock().unwrap().push(s)), &t);
        rep.remember_resolved("ws → w:22");
        rep.set(ConnState::new("connecting", &t));
        let (_, p, _) = crate::askpass::ask("pw: ".into(), "");
        rep.prompt(Some(p));
        rep.prompt(None);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].resolved.as_deref(), Some("ws → w:22"));
        assert_eq!(
            (seen[1].phase, seen[1].prompt.as_ref().map(|p| p.kind)),
            ("connecting", Some("secret"))
        );
        assert!(seen[2].prompt.is_none() && seen[2].resolved.is_some());
    }

    #[test]
    fn conn_state_serializes_like_the_frontend_type() {
        let s = ConnState {
            phase: "installing",
            target: Target {
                kind: "ssh".into(),
                host: Some("devbox".into()),
                label: "devbox".into(),
            },
            version: Some("0.1.0".into()),
            detail: Some("linux-x86_64 (8.0 MB)".into()),
            resolved: Some("devbox → d:22".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!({"phase":"installing","target":{"kind":"ssh","host":"devbox","label":"devbox"},"version":"0.1.0","detail":"linux-x86_64 (8.0 MB)","resolved":"devbox → d:22"})
        );
        let t: Target =
            serde_json::from_value(serde_json::json!({"kind":"local","label":"Local"})).unwrap();
        assert_eq!(t.host, None);
    }
}
