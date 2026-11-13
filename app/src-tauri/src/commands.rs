use crate::conn::{ConnManager, ConnState, Ctx, Target};
use crate::forward::{Forward, PortSettings, Ports, PortsState};
use crate::http;
use crate::sse::SseParser;
use crate::ws::{self, TermInput};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::async_runtime::{spawn, JoinHandle};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State, Window};
use tokio::sync::mpsc;

pub struct WindowConn {
    pub conn: Arc<ConnManager>,
    pub ports: Arc<Ports>,
    subs: Mutex<HashMap<u32, JoinHandle<()>>>,
    terms: Mutex<HashMap<u32, mpsc::Sender<TermInput>>>,
    closed: AtomicBool,
}

impl Default for WindowConn {
    fn default() -> Self {
        let conn = Arc::new(ConnManager::default());
        Self {
            ports: Ports::new(conn.clone()),
            conn,
            subs: Mutex::default(),
            terms: Mutex::default(),
            closed: AtomicBool::new(false),
        }
    }
}

impl WindowConn {
    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    fn add_sub(&self, id: u32, task: JoinHandle<()>) {
        let mut subs = self.subs.lock().unwrap();
        if self.is_closed() {
            task.abort();
        } else {
            subs.insert(id, task);
        }
    }

    fn add_term(&self, id: u32, tx: mpsc::Sender<TermInput>) -> bool {
        let mut terms = self.terms.lock().unwrap();
        if self.is_closed() {
            return false;
        }
        terms.insert(id, tx);
        true
    }

    fn drop_streams(&self) -> usize {
        let subs = std::mem::take(&mut *self.subs.lock().unwrap());
        let terms = std::mem::take(&mut *self.terms.lock().unwrap());
        for t in subs.values() {
            t.abort();
        }
        subs.len() + terms.len() + self.ports.unwatch_all()
    }

    pub fn shutdown(&self) {
        let subs = {
            let mut subs = self.subs.lock().unwrap();
            self.closed.store(true, Ordering::Relaxed);
            std::mem::take(&mut *subs)
        };
        for t in subs.into_values() {
            t.abort();
        }
        self.terms.lock().unwrap().clear();
        self.ports.stop();
        self.conn.disconnect();
    }
}

#[derive(Default)]
pub struct AppState {
    windows: Mutex<HashMap<String, Arc<WindowConn>>>,
    next: AtomicU32,
}

impl AppState {
    fn id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn window(&self, label: &str) -> Arc<WindowConn> {
        self.windows
            .lock()
            .unwrap()
            .entry(label.to_string())
            .or_default()
            .clone()
    }

    fn take_connections(&self, label: &str, with_own: bool) -> Vec<Arc<WindowConn>> {
        let prefix = format!("{label}{SLOT_SEP}");
        let mut windows = self.windows.lock().unwrap();
        let keys: Vec<String> = windows
            .keys()
            .filter(|k| (with_own && *k == label) || k.starts_with(&prefix))
            .cloned()
            .collect();
        keys.iter().filter_map(|k| windows.remove(k)).collect()
    }

    pub fn close(&self, label: &str) {
        let gone = self.take_connections(label, true);
        if !gone.is_empty() {
            crate::diag::info(
                "conn",
                format!(
                    "window {label} closed, dropping {} connection(s)",
                    gone.len()
                ),
            );
        }
        for w in gone {
            w.shutdown();
        }
    }

    /// A page that loads again has lost the callbacks of the previous one: stop what still feeds them.
    pub fn page_reloaded(&self, label: &str) {
        self.drop_slots(label);
        let own = self.windows.lock().unwrap().get(label).cloned();
        let dropped = own.map_or(0, |w| w.drop_streams());
        if dropped > 0 {
            crate::diag::info(
                "conn",
                format!(
                    "page of {label} loaded again, dropped {dropped} stream(s) of the previous one"
                ),
            );
        }
    }

    pub fn drop_slots(&self, label: &str) {
        let gone = self.take_connections(label, false);
        for w in gone {
            w.shutdown();
        }
    }

    pub fn drop_slot(&self, label: &str, slot: &str) {
        let w = self
            .windows
            .lock()
            .unwrap()
            .remove(&conn_key(label, Some(slot)));
        if let Some(w) = w {
            w.shutdown();
        }
    }
}

const SLOT_SEP: char = '\u{1f}';

/// One connection per window, plus one per extra host a window shows (its slot).
pub fn conn_key(label: &str, slot: Option<&str>) -> String {
    match slot.filter(|s| !s.is_empty()) {
        Some(s) => format!("{label}{SLOT_SEP}{s}"),
        None => label.to_string(),
    }
}

const RECONNECT_WAIT: Duration = Duration::from_secs(60);

fn win(state: &AppState, window: &Window, slot: &Option<String>) -> Arc<WindowConn> {
    state.window(&conn_key(window.label(), slot.as_deref()))
}

#[derive(Serialize)]
pub struct RawResponse {
    status: u16,
    body: Value,
}

#[tauri::command]
pub async fn daemon_request(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<RawResponse, String> {
    if !path.starts_with('/') {
        return Err("path must start with /".into());
    }
    let _t = crate::diag::track(
        format!(
            "daemon_request {} {}",
            method.to_ascii_uppercase(),
            crate::diag::strip_query(&path)
        ),
        Duration::MAX,
    );
    let bytes = body
        .filter(|b| !b.is_null())
        .map(|b| b.to_string().into_bytes());
    let res = win(&state, &window, &slot)
        .conn
        .request(
            &method.to_ascii_uppercase(),
            &path,
            bytes.as_deref(),
            RECONNECT_WAIT,
        )
        .await?;
    Ok(RawResponse {
        status: res.head.status,
        body: http::json_body(&res.body),
    })
}

#[derive(Serialize)]
pub struct BytesError {
    status: u16,
    error: String,
}

impl From<String> for BytesError {
    fn from(error: String) -> Self {
        Self { status: 0, error }
    }
}

impl From<&str> for BytesError {
    fn from(error: &str) -> Self {
        error.to_string().into()
    }
}

#[tauri::command]
pub async fn daemon_bytes(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    path: String,
    on_data: Channel<InvokeResponseBody>,
) -> Result<(), BytesError> {
    if !path.starts_with('/') {
        return Err("path must start with /".into());
    }
    let _t = crate::diag::track(
        format!("daemon_bytes GET {}", crate::diag::strip_query(&path)),
        Duration::MAX,
    );
    let conn = win(&state, &window, &slot)
        .conn
        .current_within(RECONNECT_WAIT)
        .await
        .ok_or("not connected to a daemon")?;
    let res = conn
        .request_within("GET", &path, None, Duration::from_secs(120))
        .await?;
    let status = res.head.status;
    if status != 200 {
        let error = http::error_field(&http::json_body(&res.body))
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(BytesError { status, error });
    }
    on_data
        .send(InvokeResponseBody::Raw(res.body))
        .map_err(|e| BytesError {
            status: 0,
            error: e.to_string(),
        })
}

const MAX_UPLOAD: u64 = 50 << 20;
const PROGRESS_STEP: u64 = 256 * 1024;

#[derive(Serialize, Clone)]
pub struct UploadProgress {
    sent: u64,
    total: u64,
}

#[tauri::command]
pub async fn upload_file(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    path: String,
    on_progress: Channel<UploadProgress>,
) -> Result<Value, String> {
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{path}: folders can't be uploaded"));
    }
    let total = meta.len();
    if total > MAX_UPLOAD {
        return Err(format!("{path}: larger than the 50 MiB upload limit"));
    }
    let name = std::path::Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let conn = win(&state, &window, &slot)
        .conn
        .current_within(RECONNECT_WAIT)
        .await
        .ok_or("not connected to a daemon")?;
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    let _ = on_progress.send(UploadProgress { sent: 0, total });
    let mut reported = 0u64;
    let res = conn
        .upload(
            &format!("/v1/uploads?name={}", http::percent_encode(&name)),
            total,
            file,
            |sent| {
                if sent == total || sent - reported >= PROGRESS_STEP {
                    reported = sent;
                    let _ = on_progress.send(UploadProgress { sent, total });
                }
            },
        )
        .await?;
    let body = http::json_body(&res.body);
    if !(200..300).contains(&res.head.status) {
        return Err(http::error_field(&body)
            .unwrap_or_else(|| format!("upload failed: HTTP {}", res.head.status)));
    }
    Ok(body)
}

#[tauri::command]
pub fn save_paste(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let bytes = raw_body(request.body())?;
    if bytes.len() as u64 > MAX_UPLOAD {
        return Err("larger than the 50 MiB upload limit".into());
    }
    let raw = request
        .headers()
        .get("x-name")
        .and_then(|v| v.to_str().ok())
        .map(http::percent_decode)
        .unwrap_or_default();
    let tmp = if cfg!(any(target_os = "android", target_os = "ios")) {
        app.path().app_cache_dir().map_err(|e| e.to_string())?
    } else {
        std::env::temp_dir()
    };
    let dir = tmp.join("overseer-paste");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let path = dir.join(format!("{millis}-{}", paste_name(&raw)));
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut file = opts
        .open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn save_download(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<String, String> {
    let bytes = raw_body(request.body())?;
    let raw = request
        .headers()
        .get("x-name")
        .and_then(|v| v.to_str().ok())
        .map(http::percent_decode)
        .unwrap_or_default();
    let paths = app.path();
    let home = paths.home_dir().ok();
    let dir = [
        paths.download_dir().ok(),
        home.as_ref().map(|h| h.join("Downloads")),
        paths.document_dir().ok(),
        home,
    ]
    .into_iter()
    .flatten()
    .find(|d| d.is_dir())
    .ok_or("no Downloads folder to save into")?;
    let (path, mut file) = create_unique(&dir, &paste_name(&raw))?;
    std::io::Write::write_all(&mut file, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn create_unique(
    dir: &std::path::Path,
    name: &str,
) -> Result<(std::path::PathBuf, std::fs::File), String> {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 0..1000 {
        let path = if n == 0 {
            dir.join(name)
        } else {
            dir.join(format!("{stem} ({n}){ext}"))
        };
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Err(format!("{}: too many copies of {name}", dir.display()))
}

fn raw_body(body: &tauri::ipc::InvokeBody) -> Result<std::borrow::Cow<'_, [u8]>, String> {
    match body {
        tauri::ipc::InvokeBody::Raw(b) => Ok(b.as_slice().into()),
        tauri::ipc::InvokeBody::Json(Value::Array(a)) => a
            .iter()
            .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
            .collect::<Option<Vec<u8>>>()
            .map(Into::into)
            .ok_or_else(|| "expected bytes".into()),
        _ => Err("expected bytes".into()),
    }
}

fn paste_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let clean = clean.trim_start_matches('.');
    if clean.is_empty() {
        "paste.png".into()
    } else {
        clean.to_string()
    }
}

#[tauri::command]
pub fn daemon_events(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    on_event: Channel<Value>,
) -> u32 {
    let id = state.id();
    let w = win(&state, &window, &slot);
    let mgr = w.conn.clone();
    let task = spawn(async move {
        let mut parser = SseParser::default();
        let mut failures = 0u32;
        loop {
            let conn = mgr.wait_current().await;
            let last = parser.last_id.clone();
            let got_data = stream_events(&conn, last, &mut parser, &on_event).await;
            failures = if got_data { 0 } else { failures + 1 };
            tokio::time::sleep(crate::conn::backoff(failures.min(4))).await;
        }
    });
    w.add_sub(id, task);
    id
}

async fn stream_events(
    conn: &crate::transport::DaemonConn,
    last: Option<String>,
    parser: &mut SseParser,
    ch: &Channel<Value>,
) -> bool {
    crate::sse::stream(conn, last, parser, |v| {
        let _ = ch.send(v);
    })
    .await
}

#[tauri::command]
pub fn daemon_events_stop(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    sub: u32,
) {
    if let Some(t) = win(&state, &window, &slot)
        .subs
        .lock()
        .unwrap()
        .remove(&sub)
    {
        t.abort();
    }
}

#[tauri::command]
pub async fn term_attach(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    term_id: String,
    on_data: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    let _t = crate::diag::track(format!("term_attach {term_id}"), crate::diag::SLOW_COMMAND);
    let id = state.id();
    let res = attach(win(&state, &window, &slot), id, &term_id, on_data).await;
    match &res {
        Ok(id) => crate::diag::info("term", format!("attached {term_id} as #{id}")),
        Err(e) => crate::diag::warn("term", format!("attach {term_id} failed: {e}")),
    }
    res
}

async fn attach(
    w: Arc<WindowConn>,
    id: u32,
    term_id: &str,
    on_data: Channel<InvokeResponseBody>,
) -> Result<u32, String> {
    let conn = w
        .conn
        .current_within(RECONNECT_WAIT)
        .await
        .ok_or("not connected to a daemon")?;
    let io = conn.open().await.map_err(|e| e.to_string())?;
    let path = format!("/v1/terms/{}/io", term_id.replace(['/', '?', '#'], ""));
    let socket = tokio::time::timeout(Duration::from_secs(15), ws::connect(io, &conn.path(&path)))
        .await
        .map_err(|_| "terminal attach timed out")??;
    let (tx, rx) = mpsc::channel(256);
    if !w.add_term(id, tx) {
        return Err("window closed".into());
    }
    spawn(async move {
        let out = on_data.clone();
        ws::pump(socket, rx, move |bytes| {
            let _ = out.send(InvokeResponseBody::Raw(bytes));
        })
        .await;
        let _ = on_data.send(InvokeResponseBody::Json("[]".into()));
        crate::diag::info("term", format!("stream #{id} closed"));
        w.terms.lock().unwrap().remove(&id);
    });
    Ok(id)
}

fn term_sender(
    state: &AppState,
    window: &Window,
    slot: &Option<String>,
    attach: u32,
) -> Result<mpsc::Sender<TermInput>, String> {
    win(state, window, slot)
        .terms
        .lock()
        .unwrap()
        .get(&attach)
        .cloned()
        .ok_or_else(|| format!("terminal {attach} is not attached"))
}

#[tauri::command]
pub async fn term_write(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    attach: u32,
    data: String,
) -> Result<(), String> {
    term_sender(&state, &window, &slot, attach)?
        .send(TermInput::Data(data))
        .await
        .map_err(|_| "terminal closed".into())
}

#[tauri::command]
pub async fn term_resize(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    attach: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    term_sender(&state, &window, &slot, attach)?
        .send(TermInput::Resize(cols, rows))
        .await
        .map_err(|_| "terminal closed".into())
}

#[tauri::command]
pub async fn term_background(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    attach: u32,
    background: bool,
) -> Result<(), String> {
    term_sender(&state, &window, &slot, attach)?
        .send(TermInput::Background(background))
        .await
        .map_err(|_| "terminal closed".into())
}

#[tauri::command]
pub fn term_detach(window: Window, state: State<'_, AppState>, slot: Option<String>, attach: u32) {
    crate::diag::info("term", format!("detach #{attach}"));
    win(&state, &window, &slot)
        .terms
        .lock()
        .unwrap()
        .remove(&attach);
}

#[tauri::command]
pub async fn conn_connect(
    app: AppHandle,
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    target: Target,
    on_state: Channel<ConnState>,
    server_bin_dir: Option<String>,
) -> Result<(), String> {
    let ctx = Ctx {
        resource_dir: app.path().resource_dir().ok(),
        bin_dir: server_bin_dir
            .filter(|s| !s.trim().is_empty())
            .map(|s| PathBuf::from(crate::sshconfig::expand_tilde(s.trim()))),
        exe_dir: std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from)),
        ssh: crate::sshg::SshCmd::from_env(),
        askpass: std::env::current_exe().ok(),
        data_dir: app.path().app_data_dir().ok(),
        cache_dir: app.path().app_cache_dir().ok(),
    };
    let emit = Arc::new(move |s: ConnState| {
        let _ = on_state.send(s);
    });
    #[cfg(desktop)]
    if slot.is_none() {
        let _ = window.set_title(&window_title(&target));
    }
    let w = win(&state, &window, &slot);
    w.ports.reset();
    w.ports.ensure_tracking();
    let _t = crate::diag::track(
        format!(
            "conn_connect {}",
            target.host.as_deref().unwrap_or(target.kind.as_str())
        ),
        Duration::MAX,
    );
    let res = w.conn.connect(target, emit, ctx).await;
    if w.is_closed() {
        w.conn.disconnect();
    }
    res
}

#[cfg(desktop)]
pub fn window_title(target: &Target) -> String {
    let host = if target.label.trim().is_empty() {
        target.host.as_deref().unwrap_or(target.kind.as_str())
    } else {
        target.label.trim()
    };
    format!("Overseer — {host}")
}

#[tauri::command]
pub fn conn_disconnect(window: Window, state: State<'_, AppState>, slot: Option<String>) {
    if let Some(s) = slot.as_deref().filter(|s| !s.is_empty()) {
        state.drop_slot(window.label(), s);
        return;
    }
    let w = win(&state, &window, &slot);
    w.conn.disconnect();
    w.ports.reset();
    w.terms.lock().unwrap().clear();
}

/// A reloaded page starts its slots again: the ones of the previous page are dropped first.
#[tauri::command]
pub fn conn_drop_slots(window: Window, state: State<'_, AppState>) {
    state.drop_slots(window.label());
}

#[tauri::command]
pub fn conn_wake(window: Window, state: State<'_, AppState>, slot: Option<String>) {
    win(&state, &window, &slot).conn.wake();
}

#[tauri::command]
pub async fn conn_restart_server(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
) -> Result<(), String> {
    crate::diag::info("conn", "server restart requested");
    let _t = crate::diag::track("conn_restart_server", Duration::MAX);
    let w = win(&state, &window, &slot);
    w.terms.lock().unwrap().clear();
    w.conn.restart_server().await
}

#[tauri::command]
pub fn conn_answer(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    id: u32,
    answer: Option<String>,
) {
    let cancelled = answer.is_none();
    crate::askpass::answer(id, answer);
    win(&state, &window, &slot).conn.clear_prompt(cancelled);
}

#[tauri::command]
pub fn ports_watch(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    on_state: Channel<PortsState>,
) -> u32 {
    let id = state.id();
    let w = win(&state, &window, &slot);
    w.ports.ensure_tracking();
    w.ports.watch(
        id,
        Box::new(move |s| {
            let _ = on_state.send(s.clone());
        }),
    );
    id
}

#[tauri::command]
pub fn ports_watch_stop(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    sub: u32,
) {
    win(&state, &window, &slot).ports.unwatch(sub);
}

#[tauri::command]
pub async fn ports_configure(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    settings: PortSettings,
) -> Result<(), String> {
    win(&state, &window, &slot).ports.configure(settings).await;
    Ok(())
}

#[tauri::command]
pub async fn ports_forward(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    remote_port: u16,
    local_port: Option<u16>,
) -> Result<Forward, String> {
    let _t = crate::diag::track(
        format!("ports_forward {remote_port}"),
        crate::diag::SLOW_COMMAND,
    );
    win(&state, &window, &slot)
        .ports
        .forward(remote_port, local_port, "manual")
        .await
}

#[tauri::command]
pub fn ports_unforward(
    window: Window,
    state: State<'_, AppState>,
    slot: Option<String>,
    remote_port: u16,
) {
    win(&state, &window, &slot).ports.unforward(remote_port);
}

#[cfg(desktop)]
#[tauri::command]
pub async fn ssh_hosts() -> Vec<crate::sshconfig::SshHost> {
    tokio::task::spawn_blocking(crate::ssh::list_hosts)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{create_unique, paste_name, raw_body, AppState};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn window_title_names_the_host() {
        use crate::conn::Target;
        assert_eq!(
            super::window_title(&Target {
                kind: "local".into(),
                host: None,
                label: "Local".into()
            }),
            "Overseer — Local"
        );
        assert_eq!(
            super::window_title(&Target {
                kind: "ssh".into(),
                host: Some("box".into()),
                label: String::new()
            }),
            "Overseer — box"
        );
    }

    #[test]
    fn each_window_gets_its_own_connection() {
        let s = AppState::default();
        let main = s.window("main");
        let other = s.window("w-2");
        assert!(Arc::ptr_eq(&main, &s.window("main")));
        assert!(!Arc::ptr_eq(&main, &other));
        assert!(!Arc::ptr_eq(&main.conn, &other.conn));
        assert!(!Arc::ptr_eq(&main.ports, &other.ports));
    }

    async fn finished(h: &tokio::task::AbortHandle) -> bool {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !h.is_finished() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .is_ok()
    }

    #[tokio::test]
    async fn closing_a_window_drops_only_its_connection() {
        let s = AppState::default();
        let main = s.window("main");
        let other = s.window("w-2");
        let (tx_main, _rx_main) = tokio::sync::mpsc::channel(1);
        let (tx_other, mut rx_other) = tokio::sync::mpsc::channel(1);
        assert!(main.add_term(1, tx_main));
        assert!(other.add_term(2, tx_other));
        let sub = tauri::async_runtime::spawn(std::future::pending::<()>());
        let abort = sub.inner().abort_handle();
        other.add_sub(3, sub);
        s.close("w-2");
        assert!(other.is_closed() && !main.is_closed());
        assert!(other.terms.lock().unwrap().is_empty());
        assert_eq!(main.terms.lock().unwrap().len(), 1);
        assert!(rx_other.recv().await.is_none());
        assert!(finished(&abort).await);
        assert_eq!(Arc::strong_count(&other), 1);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        assert!(!other.add_term(4, tx));
        let late = tauri::async_runtime::spawn(std::future::pending::<()>());
        let late_abort = late.inner().abort_handle();
        other.add_sub(5, late);
        assert!(finished(&late_abort).await);
        assert!(!Arc::ptr_eq(&other, &s.window("w-2")));
        s.close("missing");
    }

    #[tokio::test]
    async fn a_reloaded_page_stops_the_streams_of_the_previous_one() {
        use super::conn_key;
        let s = AppState::default();
        let main = s.window("main");
        let slot = s.window(&conn_key("main", Some("ssh:prod")));
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        assert!(main.add_term(1, tx));
        let sub = tauri::async_runtime::spawn(std::future::pending::<()>());
        let abort = sub.inner().abort_handle();
        main.add_sub(2, sub);
        let seen = Arc::new(AtomicU32::new(0));
        let count = seen.clone();
        main.ports.watch(
            3,
            Box::new(move |_| {
                count.fetch_add(1, Ordering::Relaxed);
            }),
        );
        s.page_reloaded("main");
        assert!(rx.recv().await.is_none());
        assert!(finished(&abort).await);
        assert!(slot.is_closed() && !main.is_closed());
        main.ports.reset();
        assert_eq!(seen.load(Ordering::Relaxed), 1);
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        assert!(main.add_term(4, tx));
        assert!(Arc::ptr_eq(&main, &s.window("main")));
        s.page_reloaded("browser-1");
        assert!(!s.windows.lock().unwrap().contains_key("browser-1"));
    }

    #[test]
    fn a_slot_is_its_own_connection_in_the_same_window() {
        use super::conn_key;
        assert_eq!(conn_key("main", None), "main");
        assert_eq!(conn_key("main", Some("")), "main");
        let s = AppState::default();
        let main = s.window(&conn_key("main", None));
        let prod = s.window(&conn_key("main", Some("ssh:prod")));
        assert!(!Arc::ptr_eq(&main, &prod));
        assert!(!Arc::ptr_eq(&main.conn, &prod.conn));
        assert!(Arc::ptr_eq(
            &prod,
            &s.window(&conn_key("main", Some("ssh:prod")))
        ));
    }

    #[test]
    fn dropping_a_slot_keeps_the_window_and_its_other_hosts() {
        use super::conn_key;
        let s = AppState::default();
        let main = s.window("main");
        let prod = s.window(&conn_key("main", Some("ssh:prod")));
        let gpu = s.window(&conn_key("main", Some("ssh:gpu")));
        s.drop_slot("main", "ssh:prod");
        assert!(prod.is_closed());
        assert!(!main.is_closed() && !gpu.is_closed());
        assert!(!Arc::ptr_eq(
            &prod,
            &s.window(&conn_key("main", Some("ssh:prod")))
        ));
        s.drop_slot("main", "missing");
    }

    #[test]
    fn dropping_the_slots_keeps_the_window_connection() {
        use super::conn_key;
        let s = AppState::default();
        let main = s.window("main");
        let prod = s.window(&conn_key("main", Some("h1")));
        let other = s.window(&conn_key("w-2", Some("h1")));
        s.drop_slots("main");
        assert!(prod.is_closed());
        assert!(!main.is_closed() && !other.is_closed());
        assert!(Arc::ptr_eq(&main, &s.window("main")));
    }

    #[test]
    fn closing_a_window_drops_every_slot_but_not_other_windows() {
        use super::conn_key;
        let s = AppState::default();
        let main = s.window("main");
        let prod = s.window(&conn_key("main", Some("ssh:prod")));
        let other = s.window("main-2");
        let other_slot = s.window(&conn_key("w-2", Some("ssh:prod")));
        s.close("main");
        assert!(main.is_closed() && prod.is_closed());
        assert!(!other.is_closed() && !other_slot.is_closed());
    }

    #[test]
    fn paste_body_is_raw_or_the_json_array_android_sends() {
        use tauri::ipc::InvokeBody;
        assert_eq!(
            &*raw_body(&InvokeBody::Raw(vec![1, 2, 255])).unwrap(),
            &[1, 2, 255]
        );
        assert_eq!(
            &*raw_body(&InvokeBody::Json(serde_json::json!([1, 2, 255]))).unwrap(),
            &[1, 2, 255]
        );
        assert!(raw_body(&InvokeBody::Json(serde_json::json!([256]))).is_err());
        assert!(raw_body(&InvokeBody::Json(serde_json::json!({"0": 1}))).is_err());
    }

    #[test]
    fn downloads_never_overwrite_an_existing_file() {
        let dir = std::env::temp_dir().join(format!("sb-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let names: Vec<String> = (0..3)
            .map(|_| {
                let (p, _) = create_unique(&dir, "icon.png").unwrap();
                p.file_name().unwrap().to_string_lossy().into_owned()
            })
            .collect();
        assert_eq!(names, ["icon.png", "icon (1).png", "icon (2).png"]);
        let (p, _) = create_unique(&dir, "README").unwrap();
        assert!(p.ends_with("README"));
        let (p, _) = create_unique(&dir, "README").unwrap();
        assert!(p.ends_with("README (1)"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn paste_names_are_flat_and_safe() {
        assert_eq!(paste_name("image.png"), "image.png");
        assert_eq!(paste_name("../../etc/passwd"), "passwd");
        assert_eq!(paste_name("C:\\x\\shot 1.png"), "shot_1.png");
        assert_eq!(paste_name(".."), "paste.png");
        assert_eq!(paste_name(""), "paste.png");
        assert_eq!(paste_name("a\nb.png"), "a_b.png");
    }
}
