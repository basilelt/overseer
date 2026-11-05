use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

pub const LOG: &str = "app.log";
const MAX_BYTES: u64 = 5 << 20;
const KEEP: usize = 5;
const RING: usize = 200;
const SENTINEL: &str = "session";
const PREVIOUS: &str = "unexpected-exit.json";
const CRASHES_KEPT: usize = 10;
pub const PREVIOUS_LINES: usize = 500;
const PING: Duration = Duration::from_secs(2);
const FROZEN_AFTER: Duration = Duration::from_secs(5);
const FROZEN_REPEAT: Duration = Duration::from_secs(10);
pub const SLOW_COMMAND: Duration = Duration::from_secs(1);
pub const SLOW_DAEMON: Duration = Duration::from_secs(2);
const FRONTEND_BURST: u32 = 30;
const FRONTEND_WINDOW: Duration = Duration::from_secs(10);
const MAX_MESSAGE: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }

    pub fn parse(s: &str) -> Level {
        match s.to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warn" | "warning" => Level::Warn,
            "debug" | "trace" => Level::Debug,
            _ => Level::Info,
        }
    }
}

struct Sink {
    dir: Option<PathBuf>,
    file: Option<File>,
    size: u64,
    ring: VecDeque<String>,
    pending: Vec<String>,
}

static SINK: Mutex<Sink> = Mutex::new(Sink {
    dir: None,
    file: None,
    size: 0,
    ring: VecDeque::new(),
    pending: Vec::new(),
});
static SESSION: Mutex<Option<(PathBuf, Sentinel)>> = Mutex::new(None);
static PREVIOUS_SESSION: Mutex<Option<Previous>> = Mutex::new(None);
static VERSION: OnceLock<String> = OnceLock::new();

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn try_lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    for _ in 0..50 {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(std::sync::TryLockError::Poisoned(e)) => return Some(e.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(10))
            }
        }
    }
    None
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn rfc3339(ms: u64) -> String {
    let secs = ms / 1000;
    let (y, m, d) = civil((secs / 86_400) as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms % 1000
    )
}

pub fn format_line(ms: u64, level: Level, target: &str, msg: &str) -> String {
    let mut lines = msg.lines();
    let mut out = format!(
        "{} {:<5} {}: {}",
        rfc3339(ms),
        level.as_str(),
        target,
        lines.next().unwrap_or("")
    );
    for l in lines {
        out.push_str("\n    ");
        out.push_str(l);
    }
    out
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn run_until(b: &[u8], from: usize, keep: impl Fn(u8) -> bool) -> usize {
    let mut i = from;
    while i < b.len() && keep(b[i]) {
        i += 1;
    }
    i
}

const SECRET_KEYS: [&str; 11] = [
    "access_token",
    "refresh_token",
    "token",
    "password",
    "passwd",
    "passphrase",
    "secret",
    "api_key",
    "apikey",
    "authorization",
    "cookie",
];
const TOKEN_PREFIXES: [&str; 6] = ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"];

pub fn redact(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let b = s.as_bytes();
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for p in TOKEN_PREFIXES {
        for (i, _) in lower.match_indices(p) {
            if i > 0 && b[i - 1].is_ascii_alphanumeric() {
                continue;
            }
            let start = i + p.len();
            let end = run_until(b, start, is_word);
            if end - start >= 8 {
                spans.push((start, end));
            }
        }
    }
    for (i, _) in lower.match_indices("sk-") {
        if i > 0 && is_word(b[i - 1]) {
            continue;
        }
        let end = run_until(b, i + 3, |c| is_word(c) || c == b'-');
        if end - (i + 3) >= 16 {
            spans.push((i + 3, end));
        }
    }
    for (i, _) in lower.match_indices("bearer ") {
        let start = run_until(b, i + 7, |c| c == b' ');
        let end = run_until(b, start, |c| {
            !c.is_ascii_whitespace() && c != b'"' && c != b'\''
        });
        if end > start {
            spans.push((start, end));
        }
    }
    for key in SECRET_KEYS {
        for (i, _) in lower.match_indices(key) {
            if i > 0 && b[i - 1].is_ascii_alphanumeric() {
                continue;
            }
            let mut j = run_until(b, i + key.len(), |c| c == b'"' || c == b'\'');
            j = run_until(b, j, |c| c == b' ');
            if j >= b.len() || (b[j] != b'=' && b[j] != b':') {
                continue;
            }
            j = run_until(b, j + 1, |c| c == b' ');
            let (start, end) = match b.get(j) {
                Some(&q) if q == b'"' || q == b'\'' => (j + 1, run_until(b, j + 1, |c| c != q)),
                _ => (
                    j,
                    run_until(b, j, |c| {
                        !c.is_ascii_whitespace()
                            && !matches!(c, b'&' | b',' | b';' | b'}' | b'"' | b'\'')
                    }),
                ),
            };
            if end > start && &s[start..end] != "***" {
                spans.push((start, end));
            }
        }
    }
    for (i, _) in s.match_indices("://") {
        let start = i + 3;
        let end = run_until(b, start, |c| {
            !c.is_ascii_whitespace() && !matches!(c, b'/' | b'"' | b'\'')
        });
        let auth = &s[start..end];
        if let Some(at) = auth.rfind('@') {
            if let Some(colon) = auth[..at].find(':') {
                spans.push((start + colon + 1, start + at));
            }
        }
    }
    if spans.is_empty() {
        return s.to_string();
    }
    spans.sort();
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    for (start, end) in spans {
        if end <= pos {
            continue;
        }
        out.push_str(&s[pos..start.max(pos)]);
        out.push_str("***");
        pos = end;
    }
    out.push_str(&s[pos..]);
    out
}

impl Sink {
    fn push(&mut self, line: String) {
        for l in line.lines() {
            if self.ring.len() == RING {
                self.ring.pop_front();
            }
            self.ring.push_back(l.to_string());
        }
        if self.dir.is_none() {
            if self.pending.len() < 1000 {
                self.pending.push(line);
            }
            return;
        }
        self.write(&line);
    }

    fn write(&mut self, line: &str) {
        let len = line.len() as u64 + 1;
        if self.size + len > MAX_BYTES {
            if let Some(dir) = self.dir.clone() {
                self.file = None;
                rotate(&dir, LOG, KEEP);
                self.open(&dir);
            }
        }
        if let Some(f) = self.file.as_mut() {
            if f.write_all(line.as_bytes())
                .and_then(|_| f.write_all(b"\n"))
                .is_ok()
            {
                self.size += len;
            }
        }
    }

    fn open(&mut self, dir: &Path) {
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(LOG))
            .ok();
        self.size = self
            .file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len())
            .unwrap_or(0);
    }
}

pub fn rotate(dir: &Path, name: &str, keep: usize) {
    let path = |i: usize| {
        if i == 0 {
            dir.join(name)
        } else {
            dir.join(format!("{name}.{i}"))
        }
    };
    for i in (1..keep.max(1)).rev() {
        let from = path(i - 1);
        if from.exists() {
            let to = path(i);
            let _ = fs::remove_file(&to);
            let _ = fs::rename(&from, &to);
        }
    }
}

pub fn log(level: Level, target: &str, msg: &str) {
    let line = format_line(now_ms(), level, target, &redact(msg));
    if cfg!(debug_assertions) {
        eprintln!("{line}");
    }
    lock(&SINK).push(line);
}

pub fn info(target: &str, msg: impl AsRef<str>) {
    log(Level::Info, target, msg.as_ref());
}

pub fn warn(target: &str, msg: impl AsRef<str>) {
    log(Level::Warn, target, msg.as_ref());
}

pub fn error(target: &str, msg: impl AsRef<str>) {
    log(Level::Error, target, msg.as_ref());
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Sentinel {
    pub pid: u32,
    pub started: u64,
    pub version: String,
    pub state: String,
    pub crash: Option<String>,
    pub frozen: u64,
    pub view: String,
}

impl Sentinel {
    pub fn parse(s: &str) -> Option<Sentinel> {
        let mut out = Sentinel::default();
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            match k.trim() {
                "pid" => out.pid = v.trim().parse().unwrap_or(0),
                "started" => out.started = v.trim().parse().unwrap_or(0),
                "version" => out.version = v.trim().to_string(),
                "state" => out.state = v.trim().to_string(),
                "crash" => out.crash = Some(v.trim().to_string()).filter(|c| !c.is_empty()),
                "frozen" => out.frozen = v.trim().parse().unwrap_or(0),
                "view" => out.view = v.trim().to_string(),
                _ => {}
            }
        }
        (out.started > 0 || !out.state.is_empty()).then_some(out)
    }

    pub fn render(&self) -> String {
        let one = |s: &str| s.replace(['\n', '\r'], " ");
        format!(
            "pid={}\nstarted={}\nversion={}\nstate={}\ncrash={}\nfrozen={}\nview={}\n",
            self.pid,
            self.started,
            one(&self.version),
            one(&self.state),
            one(self.crash.as_deref().unwrap_or("")),
            self.frozen,
            one(&self.view)
        )
    }
}

pub fn classify(s: &Sentinel) -> Option<&'static str> {
    if s.crash.is_some() {
        return Some("crash");
    }
    if s.state != "active" {
        return None;
    }
    Some(if s.frozen > 0 { "freeze" } else { "killed" })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Previous {
    pub kind: String,
    pub started_at: u64,
    pub ended_at: Option<String>,
    pub detected_at: u64,
    pub version: String,
    pub frozen_secs: u64,
    pub view: String,
    pub crash_report: Option<String>,
    pub app_log: Vec<String>,
}

pub fn previous_from(
    s: &Sentinel,
    kind: &str,
    crash_report: Option<String>,
    app_log: Vec<String>,
    now: u64,
) -> Previous {
    let ended_at = app_log
        .iter()
        .rev()
        .filter_map(|l| l.split(' ').next())
        .find(|t| t.len() >= 20 && t.as_bytes()[0].is_ascii_digit())
        .map(String::from);
    Previous {
        kind: kind.into(),
        started_at: s.started,
        ended_at,
        detected_at: now,
        version: s.version.clone(),
        frozen_secs: s.frozen,
        view: s.view.clone(),
        crash_report,
        app_log,
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut f = File::create(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    let _ = fs::remove_file(path);
    fs::rename(&tmp, path)
}

fn tail_file(p: &Path, n: usize) -> Vec<String> {
    let Ok(mut f) = File::open(p) else {
        return Vec::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(2 << 20);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(n);
    lines[skip..].iter().map(|l| l.to_string()).collect()
}

pub fn tail(dir: &Path, name: &str, n: usize) -> Vec<String> {
    let mut out = tail_file(&dir.join(name), n);
    if out.len() < n {
        let mut older = tail_file(&dir.join(format!("{name}.1")), n - out.len());
        older.append(&mut out);
        out = older;
    }
    out
}

pub fn crash_report(
    at: u64,
    version: &str,
    thread: &str,
    location: &str,
    message: &str,
    backtrace: &str,
    recent: &[String],
) -> String {
    let mut out = String::new();
    out.push_str("Overseer crash report\n");
    out.push_str(&format!("Time: {}\n", rfc3339(at)));
    out.push_str(&format!(
        "Version: {version} ({} {})\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("Thread: {thread}\n"));
    out.push_str(&format!("Location: {location}\n"));
    out.push_str(&format!("Message: {}\n", redact(message)));
    out.push_str("\nBacktrace:\n");
    out.push_str(backtrace.trim_end());
    out.push_str(&format!("\n\nLast {} log lines:\n", recent.len()));
    for l in recent {
        out.push_str(l);
        out.push('\n');
    }
    out
}

fn update_session(f: impl FnOnce(&mut Sentinel)) {
    let mut g = lock(&SESSION);
    if let Some((path, s)) = g.as_mut() {
        f(s);
        let _ = write_atomic(path, s.render().as_bytes());
    }
}

fn prune_crashes(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut crashes: Vec<String> = rd
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("crash-") && n.ends_with(".txt"))
        .collect();
    crashes.sort();
    let extra = crashes.len().saturating_sub(CRASHES_KEPT);
    for n in &crashes[..extra] {
        let _ = fs::remove_file(dir.join(n));
    }
}

pub fn version() -> &'static str {
    VERSION.get().map(String::as_str).unwrap_or("?")
}

pub fn install_panic_hook(app_version: String) {
    let _ = VERSION.set(app_version);
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "non-string panic payload".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".into());
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        let now = now_ms();
        let line = format_line(
            now,
            Level::Error,
            "panic",
            &redact(&format!("thread '{thread}' panicked at {location}: {msg}")),
        );
        let ctx = try_lock(&SINK).map(|mut s| {
            s.push(line);
            (s.dir.clone(), s.ring.iter().cloned().collect::<Vec<_>>())
        });
        if let Some((Some(dir), recent)) = ctx {
            let name = format!("crash-{now}.txt");
            let report = crash_report(
                now,
                version(),
                &thread,
                &location,
                &msg,
                &backtrace,
                &recent,
            );
            if write_atomic(&dir.join(&name), report.as_bytes()).is_ok() {
                if let Some(mut g) = try_lock(&SESSION) {
                    if let Some((path, s)) = g.as_mut() {
                        s.crash = Some(name);
                        let _ = write_atomic(path, s.render().as_bytes());
                    }
                }
            }
        }
        prev(info);
    }));
}

pub fn init(dir: PathBuf, version: &str) {
    if let Err(e) = fs::create_dir_all(&dir) {
        eprintln!("overseer: log folder {}: {e}", dir.display());
        return;
    }
    let now = now_ms();
    let mut previous: Option<Previous> = fs::read(dir.join(PREVIOUS))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let old = fs::read_to_string(dir.join(SENTINEL))
        .ok()
        .and_then(|s| Sentinel::parse(&s));
    if let Some((s, kind)) = old.as_ref().and_then(|s| classify(s).map(|k| (s, k))) {
        let crash = s
            .crash
            .as_ref()
            .and_then(|n| fs::read_to_string(dir.join(n)).ok());
        let p = previous_from(s, kind, crash, tail(&dir, LOG, PREVIOUS_LINES), now);
        if let Ok(json) = serde_json::to_vec(&p) {
            let _ = write_atomic(&dir.join(PREVIOUS), &json);
        }
        previous = Some(p);
    }
    let sentinel = Sentinel {
        pid: std::process::id(),
        started: now,
        version: version.into(),
        state: "active".into(),
        ..Default::default()
    };
    let _ = write_atomic(&dir.join(SENTINEL), sentinel.render().as_bytes());
    *lock(&SESSION) = Some((dir.join(SENTINEL), sentinel));
    {
        let mut s = lock(&SINK);
        s.dir = Some(dir.clone());
        s.open(&dir);
        for line in std::mem::take(&mut s.pending) {
            s.write(&line);
        }
    }
    prune_crashes(&dir);
    if let Some(p) = &previous {
        warn(
            "session",
            format!(
                "previous session (started {}, version {}) ended unexpectedly: {}",
                rfc3339(p.started_at),
                p.version,
                p.kind
            ),
        );
    }
    *lock(&PREVIOUS_SESSION) = previous;
}

pub fn session_end() {
    let mut g = lock(&SESSION);
    if let Some((path, _)) = g.take() {
        info("session", "clean exit");
        let _ = fs::remove_file(path);
    }
}

pub fn log_dir() -> Option<PathBuf> {
    lock(&SINK).dir.clone()
}

pub fn resolve_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_log_dir()
        .ok()
        .or_else(|| app.path().app_data_dir().ok().map(|d| d.join("logs")))
}

pub fn log_start(app: &AppHandle) {
    let v = app.package_info().version.to_string();
    info(
        "app",
        format!(
            "Overseer {v} starting (server {}, {} {}, {} build, pid {}, log folder {})",
            crate::install::SERVER_VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            std::process::id(),
            log_dir()
                .map(|d| d.display().to_string())
                .unwrap_or_default()
        ),
    );
    #[cfg(target_os = "macos")]
    std::thread::spawn(|| info("app", format!("code signature: {}", signing_identity())));
}

#[cfg(target_os = "macos")]
fn signing_identity() -> String {
    let Ok(exe) = std::env::current_exe() else {
        return "unknown".into();
    };
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf);
    let target = bundle.unwrap_or(exe);
    match std::process::Command::new("/usr/bin/codesign")
        .arg("-dvv")
        .arg(&target)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stderr);
            let auth: Vec<&str> = text
                .lines()
                .filter_map(|l| l.strip_prefix("Authority="))
                .collect();
            let team = text
                .lines()
                .find_map(|l| l.strip_prefix("TeamIdentifier="))
                .unwrap_or("none");
            if auth.is_empty() {
                let sig = text
                    .lines()
                    .find_map(|l| l.strip_prefix("Signature="))
                    .unwrap_or("unsigned or ad-hoc");
                format!("{sig} (team {team})")
            } else {
                format!("{} (team {team})", auth.join(" < "))
            }
        }
        Err(e) => format!("codesign failed: {e}"),
    }
}

struct Watch {
    last_pong: Option<Instant>,
    visible: bool,
    focused: bool,
    view: String,
    frozen_at: Option<Instant>,
    reported: Option<Instant>,
}

impl Watch {
    const fn new() -> Self {
        Watch {
            last_pong: None,
            visible: true,
            focused: true,
            view: String::new(),
            frozen_at: None,
            reported: None,
        }
    }

    fn watching(&self) -> bool {
        self.visible && self.focused
    }

    fn set(&mut self, visible: bool, focused: bool, now: Instant) -> bool {
        let was = self.watching();
        self.visible = visible;
        self.focused = focused;
        if was == self.watching() {
            return false;
        }
        self.frozen_at = None;
        self.reported = None;
        if self.last_pong.is_some() {
            self.last_pong = Some(now);
        }
        self.watching()
    }
}

static WATCH: Mutex<Watch> = Mutex::new(Watch::new());
static WATCHER: OnceLock<std::thread::Thread> = OnceLock::new();

struct Inflight {
    id: u64,
    name: String,
    start: Instant,
}

static INFLIGHT: Mutex<Vec<Inflight>> = Mutex::new(Vec::new());
static LAST_DONE: Mutex<String> = Mutex::new(String::new());
static NEXT: AtomicU64 = AtomicU64::new(1);

pub struct Track {
    id: u64,
    slow: Duration,
}

pub fn track(name: impl Into<String>, slow: Duration) -> Track {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    lock(&INFLIGHT).push(Inflight {
        id,
        name: name.into(),
        start: Instant::now(),
    });
    Track { id, slow }
}

impl Drop for Track {
    fn drop(&mut self) {
        let done = {
            let mut g = lock(&INFLIGHT);
            g.iter().position(|i| i.id == self.id).map(|p| g.remove(p))
        };
        if let Some(d) = done {
            let took = d.start.elapsed();
            if took >= self.slow {
                warn(
                    "perf",
                    format!("{} took {:.1} s", d.name, took.as_secs_f64()),
                );
            }
            *lock(&LAST_DONE) = d.name;
        }
    }
}

pub fn strip_query(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

pub fn activity() -> String {
    let inflight: Vec<String> = lock(&INFLIGHT)
        .iter()
        .map(|i| format!("{} ({:.1} s)", i.name, i.start.elapsed().as_secs_f64()))
        .collect();
    let last = lock(&LAST_DONE).clone();
    format!(
        "in flight: {}; last completed: {}",
        if inflight.is_empty() {
            "nothing".into()
        } else {
            inflight.join(", ")
        },
        if last.is_empty() {
            "nothing"
        } else {
            last.as_str()
        }
    )
}

pub fn start_watchdog(app: AppHandle) {
    let spawned = std::thread::Builder::new()
        .name("diag-watchdog".into())
        .spawn(move || {
            let _ = WATCHER.set(std::thread::current());
            let mut seq = 0u64;
            loop {
                if !lock(&WATCH).watching() {
                    std::thread::park();
                    continue;
                }
                std::thread::sleep(PING);
                if !lock(&WATCH).watching() {
                    continue;
                }
                seq += 1;
                let _ = app.emit_to("main", "diag:ping", seq);
                tick(Instant::now());
            }
        });
    if let Err(e) = spawned {
        warn("ui", format!("watchdog not started: {e}"));
    }
}

fn tick(now: Instant) {
    let mut w = lock(&WATCH);
    let Some(last) = w.last_pong.filter(|_| w.watching()) else {
        return;
    };
    let gap = now.saturating_duration_since(last);
    if gap < FROZEN_AFTER {
        return;
    }
    if w.reported
        .is_some_and(|r| now.saturating_duration_since(r) < FROZEN_REPEAT)
    {
        return;
    }
    if w.frozen_at.is_none() {
        w.frozen_at = Some(last);
    }
    w.reported = Some(now);
    let secs = gap.as_secs();
    let view = w.view.clone();
    drop(w);
    warn(
        "ui",
        format!(
            "UI unresponsive for {secs} s (view: {view}; {})",
            activity()
        ),
    );
    update_session(|s| {
        s.frozen = secs;
        s.view = view;
    });
}

pub fn pong(view: String) {
    let mut w = lock(&WATCH);
    let now = Instant::now();
    let recovered = w
        .frozen_at
        .take()
        .map(|at| now.saturating_duration_since(at));
    w.reported = None;
    w.last_pong = Some(now);
    w.view = view.chars().take(200).collect();
    drop(w);
    if let Some(d) = recovered {
        info(
            "ui",
            format!("UI responsive again after {:.1} s", d.as_secs_f64()),
        );
        update_session(|s| s.frozen = 0);
    }
}

fn wake_watchdog(wake: bool) {
    if let Some(t) = WATCHER.get().filter(|_| wake) {
        t.unpark();
    }
}

pub fn focus(focused: bool) {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return;
    }
    let mut w = lock(&WATCH);
    let visible = w.visible || focused;
    let wake = w.set(visible, focused, Instant::now());
    drop(w);
    wake_watchdog(wake);
}

pub fn visibility(visible: bool) {
    let mut w = lock(&WATCH);
    let focused = w.focused;
    let wake = w.set(visible, focused, Instant::now());
    drop(w);
    wake_watchdog(wake);
    if cfg!(any(target_os = "android", target_os = "ios")) {
        update_session(|s| {
            s.state = if visible { "active" } else { "background" }.into();
            s.frozen = 0;
        });
    }
}

struct Bucket {
    window: Option<Instant>,
    used: u32,
    dropped: u32,
}

static BUCKET: Mutex<Bucket> = Mutex::new(Bucket {
    window: None,
    used: 0,
    dropped: 0,
});

pub fn allow_frontend(now: Instant) -> (bool, u32) {
    let mut b = lock(&BUCKET);
    let mut reported = 0;
    if b.window
        .is_none_or(|w| now.saturating_duration_since(w) >= FRONTEND_WINDOW)
    {
        reported = b.dropped;
        *b = Bucket {
            window: Some(now),
            used: 0,
            dropped: 0,
        };
    }
    if b.used >= FRONTEND_BURST {
        b.dropped += 1;
        return (false, reported);
    }
    b.used += 1;
    (true, reported)
}

fn clip(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tail {
    path: String,
    lines: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    version: String,
    server_version: &'static str,
    os: &'static str,
    arch: &'static str,
    log_dir: String,
}

#[tauri::command]
pub fn diag_log(level: String, message: String, stack: Option<String>) {
    let (ok, dropped) = allow_frontend(Instant::now());
    if dropped > 0 {
        warn(
            "webview",
            format!("{dropped} frontend log lines dropped (rate limit)"),
        );
    }
    if !ok {
        return;
    }
    let mut msg = clip(&message, MAX_MESSAGE).to_string();
    if let Some(st) = stack.filter(|s| !s.trim().is_empty()) {
        msg.push('\n');
        msg.push_str(clip(&st, MAX_MESSAGE));
    }
    log(Level::parse(&level), "webview", &msg);
}

#[tauri::command]
pub fn diag_pong(window: tauri::Window, view: String) {
    if window.label() == "main" {
        pong(view);
    }
}

#[tauri::command]
pub fn diag_visibility(window: tauri::Window, visible: bool) {
    if window.label() == "main" {
        visibility(visible);
    }
}

#[tauri::command]
pub fn diag_session_end(reason: String) {
    info("session", format!("ending: {}", clip(&reason, 200)));
    session_end();
}

#[tauri::command]
pub fn diag_previous() -> Option<Previous> {
    lock(&PREVIOUS_SESSION).clone()
}

#[tauri::command]
pub fn diag_dismiss() {
    lock(&PREVIOUS_SESSION).take();
    if let Some(dir) = log_dir() {
        let _ = fs::remove_file(dir.join(PREVIOUS));
    }
}

#[tauri::command]
pub fn diag_tail(lines: Option<usize>) -> Tail {
    let n = lines.unwrap_or(1000).clamp(1, 20_000);
    let dir = log_dir();
    Tail {
        path: dir
            .as_ref()
            .map(|d| d.join(LOG).display().to_string())
            .unwrap_or_default(),
        lines: dir
            .map(|d| tail(&d, LOG, n))
            .unwrap_or_else(|| lock(&SINK).ring.iter().cloned().collect()),
    }
}

#[tauri::command]
pub fn diag_info(app: AppHandle) -> Info {
    Info {
        version: app.package_info().version.to_string(),
        server_version: crate::install::SERVER_VERSION,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        log_dir: log_dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default(),
    }
}

#[tauri::command]
pub fn diag_save(app: AppHandle, text: String) -> Result<String, String> {
    let dir = app
        .path()
        .download_dir()
        .ok()
        .filter(|d| d.is_dir())
        .or_else(log_dir)
        .ok_or("no folder to save into")?;
    let stamp = rfc3339(now_ms()).replace([':', '.'], "-");
    let path = dir.join(format!("overseer-logs-{stamp}.txt"));
    fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    info(
        "diag",
        format!("diagnostic bundle saved to {}", path.display()),
    );
    Ok(path.display().to_string())
}

#[cfg(desktop)]
#[tauri::command]
pub fn diag_open_dir(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = log_dir().ok_or("no log folder")?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "sb-diag-{name}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn timestamps_are_utc_rfc3339() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_709_164_800_123), "2024-02-29T00:00:00.123Z");
        assert_eq!(rfc3339(1_735_689_599_999), "2024-12-31T23:59:59.999Z");
    }

    #[test]
    fn lines_have_level_target_and_indented_continuations() {
        assert_eq!(
            format_line(0, Level::Warn, "ui", "frozen"),
            "1970-01-01T00:00:00.000Z WARN  ui: frozen"
        );
        assert_eq!(
            format_line(0, Level::Error, "webview", "boom\nat f\nat g"),
            "1970-01-01T00:00:00.000Z ERROR webview: boom\n    at f\n    at g"
        );
    }

    #[test]
    fn redaction_masks_secrets_and_keeps_the_rest() {
        let plain = "GET /v1/terms took 2.3 s (attempt 2, host w)";
        assert_eq!(redact(plain), plain);
        assert_eq!(
            redact("Authorization: Bearer abc.def-ghi"),
            "Authorization: *** ***"
        );
        assert_eq!(redact("curl -H 'bearer xyz123'"), "curl -H 'bearer ***'");
        assert_eq!(redact("url?token=s3cr3t&x=1"), "url?token=***&x=1");
        assert_eq!(
            redact(r#"{"token":"abc","name":"n"}"#),
            r#"{"token":"***","name":"n"}"#
        );
        assert_eq!(
            redact("GITHUB_TOKEN=ghp_ABCDEFGHijklmnop1234"),
            "GITHUB_TOKEN=***"
        );
        assert_eq!(
            redact("using ghp_ABCDEFGHijklmnop1234 now"),
            "using ghp_*** now"
        );
        assert_eq!(
            redact("key github_pat_11AAAAAAA0123456789_abc"),
            "key github_pat_***"
        );
        assert_eq!(
            redact("OPENAI sk-proj-abcdefghijklmnopqrstuv"),
            "OPENAI sk-***"
        );
        assert_eq!(
            redact("password = hunter2 and more"),
            "password = *** and more"
        );
        assert_eq!(redact("PASSWD:'x y'"), "PASSWD:'***'");
        assert_eq!(
            redact("ssh://bob:pa55@host:22/x"),
            "ssh://bob:***@host:22/x"
        );
        assert_eq!(redact("https://host/x?y=1"), "https://host/x?y=1");
        assert_eq!(redact("tokens: 5"), "tokens: 5");
        assert_eq!(redact("task-sk-1 done"), "task-sk-1 done");
    }

    #[test]
    fn sentinel_round_trips_and_classifies() {
        let s = Sentinel {
            pid: 7,
            started: 42,
            version: "0.3.0".into(),
            state: "active".into(),
            crash: None,
            frozen: 0,
            view: "agents".into(),
        };
        assert_eq!(Sentinel::parse(&s.render()), Some(s.clone()));
        assert_eq!(classify(&s), Some("killed"));
        assert_eq!(
            classify(&Sentinel {
                frozen: 12,
                ..s.clone()
            }),
            Some("freeze")
        );
        assert_eq!(
            classify(&Sentinel {
                crash: Some("crash-1.txt".into()),
                ..s.clone()
            }),
            Some("crash")
        );
        assert_eq!(
            classify(&Sentinel {
                state: "background".into(),
                ..s.clone()
            }),
            None
        );
        assert_eq!(
            classify(&Sentinel {
                state: "background".into(),
                crash: Some("c".into()),
                ..s.clone()
            }),
            Some("crash")
        );
        assert_eq!(Sentinel::parse(""), None);
        assert_eq!(Sentinel::parse("garbage"), None);
    }

    #[test]
    fn previous_takes_the_last_timestamp_from_the_tail() {
        let s = Sentinel {
            started: 5,
            version: "1.0.0".into(),
            state: "active".into(),
            ..Default::default()
        };
        let log = vec![
            "1970-01-01T00:00:00.005Z INFO  app: start".to_string(),
            "    continuation".to_string(),
        ];
        let p = previous_from(&s, "killed", None, log, 9);
        assert_eq!(p.ended_at.as_deref(), Some("1970-01-01T00:00:00.005Z"));
        assert_eq!(
            (p.kind.as_str(), p.started_at, p.detected_at),
            ("killed", 5, 9)
        );
    }

    #[test]
    fn crash_report_has_headline_backtrace_and_recent_lines() {
        let r = crash_report(
            0,
            "1.2.3",
            "main",
            "src/x.rs:1:2",
            "boom token=abc",
            "0: frame",
            &["a".into(), "b".into()],
        );
        assert!(r.starts_with(
            "Overseer crash report\nTime: 1970-01-01T00:00:00.000Z\nVersion: 1.2.3 ("
        ));
        assert!(r.contains("Thread: main\nLocation: src/x.rs:1:2\nMessage: boom token=***\n"));
        assert!(r.contains("\nBacktrace:\n0: frame\n\nLast 2 log lines:\na\nb\n"));
    }

    #[test]
    fn rotation_shifts_and_keeps_the_newest_files() {
        let d = tmp("rotate");
        for i in 0..7 {
            fs::write(d.join(LOG), format!("gen {i}")).unwrap();
            rotate(&d, LOG, 5);
        }
        assert!(!d.join(LOG).exists());
        assert_eq!(fs::read_to_string(d.join("app.log.1")).unwrap(), "gen 6");
        assert_eq!(fs::read_to_string(d.join("app.log.4")).unwrap(), "gen 3");
        assert!(!d.join("app.log.5").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn tail_spans_the_rotated_file() {
        let d = tmp("tail");
        fs::write(d.join("app.log.1"), "a\nb\nc\n").unwrap();
        fs::write(d.join(LOG), "d\ne\n").unwrap();
        assert_eq!(tail(&d, LOG, 2), vec!["d", "e"]);
        assert_eq!(tail(&d, LOG, 4), vec!["b", "c", "d", "e"]);
        assert_eq!(tail(&d, LOG, 50).len(), 5);
        assert!(tail(&d, "missing.log", 5).is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn frontend_logging_is_rate_limited() {
        let t0 = Instant::now();
        let mut allowed = 0;
        for _ in 0..40 {
            if allow_frontend(t0).0 {
                allowed += 1;
            }
        }
        assert!(allowed <= FRONTEND_BURST);
        let (ok, dropped) = allow_frontend(t0 + FRONTEND_WINDOW);
        assert!(ok);
        assert!(dropped >= 10);
    }

    #[test]
    fn watchdog_sleeps_while_hidden_or_unfocused() {
        let t0 = Instant::now();
        let mut w = Watch::new();
        w.last_pong = Some(t0);
        assert!(w.watching());
        assert!(!w.set(true, false, t0));
        assert!(!w.watching());
        assert!(!w.set(false, false, t0));
        let later = t0 + Duration::from_secs(60);
        assert!(!w.set(false, true, later));
        assert!(
            w.set(true, true, later),
            "becoming visible and focused wakes the watchdog"
        );
        assert_eq!(
            w.last_pong,
            Some(later),
            "the time spent asleep is not a freeze"
        );
        assert!(!w.set(true, true, later));
    }

    #[test]
    fn slow_paths_drop_the_query() {
        assert_eq!(strip_query("/v1/logs?tail=3"), "/v1/logs");
        assert_eq!(strip_query("/v1/terms"), "/v1/terms");
    }
}
