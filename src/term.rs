use crate::board::publish;
use crate::daemon::{header, respond_raw, Request, Stream};
use crate::procs::{now_ms, Probe};
use crate::util::{format_rfc3339, ulid};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tungstenite::protocol::Role;
use tungstenite::{Message, WebSocket};

const RING: usize = 2 << 20;
const EXITED_RING: usize = 64 << 10;
const EXITED_TRIM_MS: i64 = 10 * 60_000;
const CHUNK: usize = 64 << 10;
const MAX_BACKLOG: usize = 16 << 20;
const LAGGED: usize = usize::MAX / 2;
const FLUSH_BATCH: usize = 256 << 10;

type Chunks = Receiver<Arc<[u8]>>;
type Waker = Sender<Arc<[u8]>>;

#[derive(Clone, Default)]
pub struct Info {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub cmd: Vec<String>,
    pub title: String,
    pub title_override: Option<String>,
    pub cols: u16,
    pub rows: u16,
    pub created_ms: i64,
    pub exited: bool,
    pub exit_code: Option<u32>,
    pub line_id: Option<String>,
    pub task_id: Option<String>,
    pub agent: Option<String>,
    pub session: Option<String>,
    pub flags: Vec<String>,
    pub sized_by: Option<String>,
    pub profile: Option<crate::profiles::Chip>,
    pub workspace_id: Option<String>,
    pub resume: Option<(String, String)>,
}

struct Sub {
    tx: Sender<Arc<[u8]>>,
    backlog: Option<Arc<AtomicUsize>>,
}

impl Sub {
    fn feed(&self, chunk: &Arc<[u8]>) -> bool {
        if let Some(b) = &self.backlog {
            if b.fetch_add(chunk.len(), Ordering::SeqCst) + chunk.len() > MAX_BACKLOG {
                b.fetch_add(LAGGED, Ordering::SeqCst);
                let _ = self.tx.send(Arc::from(&[][..]));
                return false;
            }
        }
        self.tx.send(chunk.clone()).is_ok()
    }
}

impl Out {
    fn hang_up(&mut self) {
        for s in self.subs.drain(..) {
            let _ = s.tx.send(Arc::from(&[][..]));
        }
    }
}

#[derive(Default)]
struct Out {
    ring: VecDeque<u8>,
    subs: Vec<Sub>,
    closed: bool,
    total: u64,
    last_ms: i64,
    burst_ms: i64,
    submit_ms: i64,
    scan: crate::termtheme::Scanner,
}

pub const TURN_GRACE_MS: i64 = 1_500;
pub const TURN_QUIET_MS: i64 = 4_000;

pub fn turn_busy(submit_ms: i64, burst_ms: i64, last_ms: i64, now_ms: i64) -> bool {
    submit_ms > 0
        && last_ms - submit_ms >= TURN_GRACE_MS
        && burst_ms <= submit_ms + TURN_QUIET_MS
        && now_ms - last_ms < TURN_QUIET_MS
}

pub fn effective_busy(
    hook_busy: bool,
    hook_at: i64,
    submit_ms: i64,
    detector: Option<bool>,
) -> bool {
    hook_busy || (submit_ms > hook_at && detector == Some(true))
}

enum Master {
    Pty(Box<dyn MasterPty + Send>),
    #[cfg(unix)]
    Fd(std::fs::File),
    #[cfg(unix)]
    Gone,
}

impl Master {
    fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        match self {
            Master::Pty(m) => m
                .resize(PtySize {
                    rows,
                    cols,
                    ..Default::default()
                })
                .map_err(|e| e.to_string()),
            #[cfg(unix)]
            Master::Fd(f) => {
                use std::os::fd::AsRawFd;
                let ws = libc::winsize {
                    ws_row: rows,
                    ws_col: cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                if unsafe {
                    libc::ioctl(
                        f.as_raw_fd(),
                        libc::TIOCSWINSZ as _,
                        &ws as *const libc::winsize,
                    )
                } != 0
                {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok(())
            }
            #[cfg(unix)]
            Master::Gone => Ok(()),
        }
    }

    #[cfg(unix)]
    fn raw_fd(&self) -> Option<i32> {
        use std::os::fd::AsRawFd;
        match self {
            Master::Pty(m) => MasterPty::as_raw_fd(&**m),
            Master::Fd(f) => Some(f.as_raw_fd()),
            Master::Gone => None,
        }
    }
}

pub struct Term {
    info: Mutex<Info>,
    out: Mutex<Out>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Master>,
    killer: Mutex<Option<Box<dyn ChildKiller + Send + Sync>>>,
    gen: AtomicU64,
    holding: AtomicBool,
    killed: AtomicBool,
}

#[derive(Default)]
pub struct Spec {
    pub cwd: String,
    pub cmd: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    pub line_id: Option<String>,
    pub task_id: Option<String>,
    pub shown: Option<Vec<String>>,
    pub session: Option<String>,
    pub flags: Vec<String>,
    pub profile: Option<crate::profiles::Chip>,
    pub id: Option<String>,
    pub workspace_id: Option<String>,
}

static TERMS: Mutex<BTreeMap<String, Arc<Term>>> = Mutex::new(BTreeMap::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn default_shell() -> String {
    let var = if cfg!(windows) { "COMSPEC" } else { "SHELL" };
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into())
}

pub fn client_kind(v: &Value) -> Option<String> {
    v.as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 16
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        .map(str::to_string)
}

pub fn may_resize(i: &Info, by: Option<&str>) -> bool {
    by != Some("phone") || (i.line_id.is_none() && i.agent.is_none())
}

pub fn size(v: &Value) -> Result<Option<(u16, u16)>, String> {
    let (c, r) = (&v["cols"], &v["rows"]);
    if c.is_null() && r.is_null() {
        return Ok(None);
    }
    let n = |x: &Value| {
        x.as_u64()
            .filter(|n| (1..=1000).contains(n))
            .map(|n| n as u16)
    };
    match (n(c), n(r)) {
        (Some(c), Some(r)) => Ok(Some((c, r))),
        _ => Err("cols and rows must be integers in 1..=1000".into()),
    }
}

type Launched = (
    Box<dyn portable_pty::Child + Send + Sync>,
    Reader,
    Box<dyn Write + Send>,
    Box<dyn MasterPty + Send>,
);

struct Reader {
    r: Box<dyn Read + Send>,
    fd: Option<i32>,
}

#[cfg(unix)]
impl From<std::fs::File> for Reader {
    fn from(f: std::fs::File) -> Reader {
        use std::os::fd::AsRawFd;
        Reader {
            fd: Some(f.as_raw_fd()),
            r: Box::new(f),
        }
    }
}

fn clone_reader(m: &dyn MasterPty) -> Result<Reader, String> {
    #[cfg(unix)]
    if let Some(fd) = m.as_raw_fd() {
        let d = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
        if d >= 0 {
            use std::os::fd::FromRawFd;
            return Ok(unsafe { std::fs::File::from_raw_fd(d) }.into());
        }
    }
    Ok(Reader {
        r: m.try_clone_reader().map_err(|e| e.to_string())?,
        fd: None,
    })
}

#[cfg(unix)]
mod gate {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    static FROZEN: AtomicBool = AtomicBool::new(false);
    static COUNT: Mutex<(usize, usize)> = Mutex::new((0, 0));
    static CHANGED: Condvar = Condvar::new();

    pub enum Ready {
        Data,
        Wake,
    }

    fn wake() -> Option<(i32, i32)> {
        static PIPE: OnceLock<Option<(i32, i32)>> = OnceLock::new();
        *PIPE.get_or_init(|| {
            let mut fds = [0i32; 2];
            if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
                return None;
            }
            for fd in fds {
                unsafe {
                    libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
                    let fl = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, fl | libc::O_NONBLOCK);
                }
            }
            Some((fds[0], fds[1]))
        })
    }

    fn count() -> std::sync::MutexGuard<'static, (usize, usize)> {
        COUNT.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub struct Pump(bool);

    impl Pump {
        pub fn enter(gated: bool) -> Pump {
            let gated = gated && wake().is_some();
            if gated {
                count().0 += 1;
            }
            Pump(gated)
        }

        pub fn gated(&self) -> bool {
            self.0
        }
    }

    impl Drop for Pump {
        fn drop(&mut self) {
            if self.0 {
                count().0 -= 1;
                CHANGED.notify_all();
            }
        }
    }

    pub fn frozen() -> bool {
        FROZEN.load(Ordering::SeqCst)
    }

    pub fn park() {
        let mut c = count();
        c.1 += 1;
        CHANGED.notify_all();
        while FROZEN.load(Ordering::SeqCst) {
            c = CHANGED.wait(c).unwrap_or_else(|e| e.into_inner());
        }
        c.1 -= 1;
    }

    pub fn ready(fd: i32) -> Ready {
        let Some((w, _)) = wake() else {
            return Ready::Data;
        };
        let mut p = [
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: w,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            let n = unsafe { libc::poll(p.as_mut_ptr(), 2, -1) };
            if n >= 0 {
                break;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return Ready::Data;
            }
        }
        if p[1].revents != 0 && FROZEN.load(Ordering::SeqCst) {
            Ready::Wake
        } else if p[0].revents != 0 {
            Ready::Data
        } else {
            Ready::Wake
        }
    }

    pub fn freeze(limit: Duration) -> bool {
        let Some((_, w)) = wake() else {
            return false;
        };
        FROZEN.store(true, Ordering::SeqCst);
        unsafe { libc::write(w, [1u8].as_ptr().cast(), 1) };
        let end = Instant::now() + limit;
        let mut c = count();
        while c.1 < c.0 {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            c = CHANGED
                .wait_timeout(c, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }

    pub fn thaw() {
        if let Some((r, _)) = wake() {
            let mut b = [0u8; 64];
            while unsafe { libc::read(r, b.as_mut_ptr().cast(), b.len()) } > 0 {}
        }
        let _c = count();
        FROZEN.store(false, Ordering::SeqCst);
        CHANGED.notify_all();
    }
}

#[cfg(unix)]
pub fn freeze(limit: Duration) -> bool {
    gate::freeze(limit)
}

#[cfg(unix)]
pub fn thaw() {
    gate::thaw()
}

fn launch(
    cwd: &str,
    argv: &[String],
    env: &[(String, String)],
    id: &str,
    cols: u16,
    rows: u16,
) -> Result<Launched, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            ..Default::default()
        })
        .map_err(|e| format!("openpty: {e}"))?;
    let mut cb = match argv.split_first() {
        Some((prog, args)) => {
            let mut cb = CommandBuilder::new(prog);
            cb.args(args);
            cb
        }
        None => CommandBuilder::new_default_prog(),
    };
    cb.cwd(cwd);
    for (k, v) in std::env::vars_os() {
        if let (Some(k), Some(v)) = (k.to_str(), v.to_str()) {
            if crate::shellenv::session_bound(k, v) {
                cb.env_remove(k);
            }
        }
    }
    for (k, v) in crate::shellenv::term_env() {
        cb.env(k, v);
    }
    if let Some(v) = crate::termtheme::colorfgbg() {
        cb.env("COLORFGBG", v);
    }
    for (k, v) in env {
        cb.env(k, v);
    }
    cb.env("OVERSEER_TERM_ID", id);
    let child = pair.slave.spawn_command(cb).map_err(|e| {
        format!(
            "spawn {}: {e}",
            argv.first().cloned().unwrap_or_else(default_shell)
        )
    })?;
    drop(pair.slave);
    let reader = clone_reader(&*pair.master)?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    Ok((child, reader, writer, pair.master))
}

fn watch(term: &Arc<Term>, mut child: Box<dyn portable_pty::Child + Send + Sync>, reader: Reader) {
    let gen = term.gen.load(Ordering::SeqCst);
    let (done_tx, done_rx) = channel::<()>();
    let t = term.clone();
    std::thread::spawn(move || {
        pump(&t, reader, gen);
        let _ = done_tx.send(());
    });
    let t = term.clone();
    std::thread::spawn(move || {
        let status = child.wait().ok();
        let code = status.as_ref().map(|s| s.exit_code());
        let _ = done_rx.recv_timeout(Duration::from_secs(1));
        if t.stale(gen) {
            return;
        }
        if status.is_some_and(|s| s.signal().is_none()) && t.fall_back_to_shell() {
            return;
        }
        {
            let mut o = lock(&t.out);
            if !o.closed {
                o.closed = true;
                o.hang_up();
            }
        }
        {
            let mut i = lock(&t.info);
            i.exited = true;
            i.exit_code = code;
        }
        publish("term.exited", json!({"term": t.to_json()}));
    });
}

pub fn spawn(s: Spec) -> Result<Arc<Term>, String> {
    if !std::path::Path::new(&s.cwd).is_dir() {
        return Err(format!("cwd {} is not a directory", s.cwd));
    }
    let id =
        s.id.clone()
            .filter(|i| !lock(&TERMS).contains_key(i))
            .unwrap_or_else(|| ulid(now_ms()));
    let cmd = if s.cmd.is_empty() {
        vec![default_shell()]
    } else {
        s.shown.clone().unwrap_or_else(|| s.cmd.clone())
    };
    let (child, reader, writer, master) = launch(&s.cwd, &s.cmd, &s.env, &id, s.cols, s.rows)?;
    let title = s
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            std::path::Path::new(&cmd[0])
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| cmd[0].clone())
        });
    let term = Arc::new(Term {
        info: Mutex::new(Info {
            id: id.clone(),
            pid: child.process_id().unwrap_or(0),
            cwd: s.cwd,
            cmd,
            title,
            title_override: None,
            cols: s.cols,
            rows: s.rows,
            created_ms: now_ms(),
            exited: false,
            exit_code: None,
            agent: crate::agents::of_argv(&s.cmd).map(|a| a.id.to_string()),
            line_id: s.line_id,
            task_id: s.task_id,
            session: s.session,
            flags: s.flags,
            sized_by: None,
            profile: s.profile,
            workspace_id: s.workspace_id,
            resume: None,
        }),
        out: Mutex::default(),
        writer: Mutex::new(writer),
        master: Mutex::new(Master::Pty(master)),
        killer: Mutex::new(Some(child.clone_killer())),
        gen: AtomicU64::new(0),
        holding: AtomicBool::new(false),
        killed: AtomicBool::new(false),
    });
    lock(&TERMS).insert(id, term.clone());
    publish("term.created", json!({"term": term.to_json()}));
    watch(&term, child, reader);
    Ok(term)
}

fn pump(t: &Term, reader: Reader, gen: u64) {
    let Reader { mut r, fd } = reader;
    #[cfg(unix)]
    let pump = gate::Pump::enter(fd.is_some());
    #[cfg(not(unix))]
    let _ = fd;
    let mut buf = vec![0u8; 16 << 10];
    loop {
        #[cfg(unix)]
        if let Some(fd) = fd.filter(|_| pump.gated()) {
            if gate::frozen() {
                gate::park();
                continue;
            }
            if let gate::Ready::Wake = gate::ready(fd) {
                continue;
            }
        }
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buf[..n];
                if let Err(p) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| t.push(chunk)))
                {
                    crate::dlog::line(
                        "error",
                        &format!(
                            "term {}: output handling panicked ({}); still streaming",
                            t.info().id,
                            crate::util::panic_text(p.as_ref())
                        ),
                    );
                }
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    if t.stale(gen) || t.may_fall_back() {
        return;
    }
    let mut o = lock(&t.out);
    o.closed = true;
    o.hang_up();
}

impl Term {
    fn stale(&self, gen: u64) -> bool {
        self.holding.load(Ordering::SeqCst) || self.gen.load(Ordering::SeqCst) != gen
    }

    pub fn hold(&self, on: bool) {
        self.holding.store(on, Ordering::SeqCst);
    }

    // An agent that quits on its own (Ctrl+C, /quit) leaves a shell in its tab, with the
    // session it ran and how to resume it, instead of closing the tab.
    fn may_fall_back(&self) -> bool {
        let i = lock(&self.info);
        cfg!(unix)
            && i.agent.as_deref().is_some_and(|a| a != "shell")
            && i.task_id.is_none()
            && !self.killed.load(Ordering::SeqCst)
    }

    #[cfg(unix)]
    fn fall_back_to_shell(self: &Arc<Self>) -> bool {
        self.may_fall_back() && self.to_shell("exited").is_ok()
    }

    #[cfg(not(unix))]
    fn fall_back_to_shell(self: &Arc<Self>) -> bool {
        false
    }

    #[cfg(unix)]
    pub fn to_shell(self: &Arc<Self>, verb: &str) -> Result<(), String> {
        let i = self.info();
        let agent = i.agent.clone().unwrap_or_default();
        let note = match i.resume.clone() {
            Some((session, cmd)) => {
                format!("{agent} {verb}. Session {session}, resume with: {cmd}")
            }
            None => format!("{agent} {verb}."),
        };
        self.detach();
        self.respawn(vec![
            "/bin/sh".into(),
            "-c".into(),
            "printf '\\r\\n%s\\r\\n\\r\\n' \"$1\"; exec \"${SHELL:-/bin/sh}\"".into(),
            "sh".into(),
            note,
        ])
    }

    #[cfg(not(unix))]
    pub fn to_shell(self: &Arc<Self>, _verb: &str) -> Result<(), String> {
        Err("no shell fallback on this platform".into())
    }

    pub fn detach(&self) {
        {
            let mut i = lock(&self.info);
            i.agent = None;
            i.line_id = None;
            i.session = None;
            i.flags = vec![];
            i.resume = None;
        }
        publish("term.updated", json!({"term": self.to_json()}));
    }

    pub fn respawn(self: &Arc<Self>, argv: Vec<String>) -> Result<(), String> {
        let i = self.info();
        let launched = launch(&i.cwd, &argv, &[], &i.id, i.cols, i.rows);
        let (child, reader, writer, master) = match launched {
            Ok(l) => l,
            Err(e) => {
                self.finish(None);
                return Err(e);
            }
        };
        self.gen.fetch_add(1, Ordering::SeqCst);
        *lock(&self.writer) = writer;
        *lock(&self.master) = Master::Pty(master);
        *lock(&self.killer) = Some(child.clone_killer());
        {
            let mut i = lock(&self.info);
            i.pid = child.process_id().unwrap_or(0);
            i.cmd = argv;
            i.exited = false;
            i.exit_code = None;
        }
        {
            let mut o = lock(&self.out);
            o.closed = false;
            o.scan = Default::default();
        }
        self.hold(false);
        watch(self, child, reader);
        publish("term.updated", json!({"term": self.to_json()}));
        Ok(())
    }

    pub fn finish(&self, code: Option<u32>) {
        {
            let mut o = lock(&self.out);
            o.closed = true;
            o.hang_up();
        }
        {
            let mut i = lock(&self.info);
            if i.exited {
                return;
            }
            i.exited = true;
            i.exit_code = code;
        }
        publish("term.exited", json!({"term": self.to_json()}));
    }

    pub fn grid(&self) -> crate::screen::Grid {
        let (cols, rows) = {
            let i = lock(&self.info);
            (i.cols, i.rows)
        };
        let o = lock(&self.out);
        let from = o.ring.len().saturating_sub(crate::screen::TAIL);
        let bytes: Vec<u8> = o.ring.range(from..).copied().collect();
        drop(o);
        crate::screen::render(&bytes, cols, rows)
    }

    fn push(&self, b: &[u8]) {
        let mut o = lock(&self.out);
        o.total += b.len() as u64;
        let now = now_ms();
        if now - o.last_ms >= TURN_QUIET_MS {
            o.burst_ms = now;
        }
        o.last_ms = now;
        o.ring.extend(b);
        let over = o.ring.len().saturating_sub(RING);
        o.ring.drain(..over);
        let chunk: Arc<[u8]> = b.into();
        o.subs.retain(|s| s.feed(&chunk));
        let reply = o.scan.feed(b, crate::termtheme::current());
        drop(o);
        if !reply.is_empty() {
            let mut w = lock(&self.writer);
            let _ = w.write_all(&reply).and_then(|_| w.flush());
        }
    }

    fn notify_theme(&self, dark: bool) {
        if self.exited() || !lock(&self.out).scan.notify {
            return;
        }
        let mut w = lock(&self.writer);
        let _ = w
            .write_all(crate::termtheme::report(dark))
            .and_then(|_| w.flush());
    }

    #[cfg(test)]
    fn subscribe(&self) -> (Vec<u8>, Option<Chunks>) {
        let mut o = lock(&self.out);
        let replay = o.ring.iter().copied().collect();
        if o.closed {
            return (replay, None);
        }
        let (tx, rx) = channel();
        o.subs.push(Sub { tx, backlog: None });
        (replay, Some(rx))
    }

    fn subscribe_waking(&self, backlog: &Arc<AtomicUsize>) -> (Vec<u8>, Option<(Waker, Chunks)>) {
        let mut o = lock(&self.out);
        let replay = o.ring.iter().copied().collect();
        if o.closed {
            return (replay, None);
        }
        let (tx, rx) = channel();
        o.subs.push(Sub {
            tx: tx.clone(),
            backlog: Some(backlog.clone()),
        });
        (replay, Some((tx, rx)))
    }

    fn out_closed(&self) -> bool {
        lock(&self.out).closed
    }

    pub fn info(&self) -> Info {
        lock(&self.info).clone()
    }

    pub fn exited(&self) -> bool {
        lock(&self.info).exited
    }

    pub fn written(&self) -> u64 {
        lock(&self.out).total
    }

    pub fn last_output_ms(&self) -> i64 {
        lock(&self.out).last_ms
    }

    pub fn turn_busy(&self, now_ms: i64) -> Option<bool> {
        let o = lock(&self.out);
        (o.submit_ms > 0).then(|| turn_busy(o.submit_ms, o.burst_ms, o.last_ms, now_ms))
    }

    pub fn submit_ms(&self) -> i64 {
        lock(&self.out).submit_ms
    }

    pub fn screen(&self) -> Vec<u8> {
        lock(&self.out).ring.iter().copied().collect()
    }

    pub fn input(&self, b: &[u8]) -> std::io::Result<()> {
        if b.contains(&b'\r') {
            lock(&self.out).submit_ms = now_ms();
        }
        let mut w = lock(&self.writer);
        w.write_all(b)?;
        w.flush()
    }

    pub fn resize(&self, cols: u16, rows: u16, by: Option<String>) -> Result<(), String> {
        if !may_resize(&self.info(), by.as_deref()) {
            return Ok(());
        }
        lock(&self.master).resize(cols, rows)?;
        {
            let mut i = lock(&self.info);
            if (i.cols, i.rows) == (cols, rows) && (by.is_none() || i.sized_by == by) {
                return Ok(());
            }
            i.cols = cols;
            i.rows = rows;
            if by.is_some() {
                i.sized_by = by;
            }
        }
        publish("term.updated", json!({"term": self.to_json()}));
        Ok(())
    }

    pub fn set_title(&self, title: Option<String>) {
        {
            let mut i = lock(&self.info);
            if i.title_override == title {
                return;
            }
            i.title_override = title;
        }
        publish("term.updated", json!({"term": self.to_json()}));
    }

    pub fn set_workspace(&self, id: Option<String>) {
        {
            let mut i = lock(&self.info);
            if i.workspace_id == id {
                return;
            }
            i.workspace_id = id;
        }
        publish("term.updated", json!({"term": self.to_json()}));
    }

    pub fn set_line(&self, line_id: &str, resume: Option<(String, String)>) {
        {
            let mut i = lock(&self.info);
            if resume.is_some() {
                i.resume = resume;
            }
            if i.line_id.as_deref() == Some(line_id) {
                return;
            }
            i.line_id = Some(line_id.to_string());
        }
        publish("term.updated", json!({"term": self.to_json()}));
    }

    #[cfg(unix)]
    pub fn kill(self: &Arc<Self>) -> Result<(), String> {
        self.killed.store(true, Ordering::SeqCst);
        let group = -(self.info().pid as i32);
        if group == 0 {
            return self.kill_direct();
        }
        if unsafe { libc::kill(group, libc::SIGHUP) } != 0 {
            self.kill_direct()?;
        }
        let t = self.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            if !t.exited() {
                unsafe { libc::kill(group, libc::SIGKILL) };
            }
        });
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn kill(self: &Arc<Self>) -> Result<(), String> {
        self.killed.store(true, Ordering::SeqCst);
        self.kill_direct()
    }

    fn kill_direct(&self) -> Result<(), String> {
        if let Some(k) = lock(&self.killer).as_mut() {
            return k.kill().map_err(|e| e.to_string());
        }
        #[cfg(unix)]
        {
            let pid = self.info().pid as i32;
            if pid > 0 && unsafe { libc::kill(pid, libc::SIGKILL) } == 0 {
                return Ok(());
            }
        }
        Err("term has no process to kill".into())
    }

    #[cfg(unix)]
    fn save(&self) -> crate::upgrade::SavedTerm {
        let i = self.info();
        let fd = if i.exited {
            -1
        } else {
            lock(&self.master).raw_fd().unwrap_or(-1)
        };
        let o = lock(&self.out);
        let (a, b) = o.ring.as_slices();
        let mut ring = crate::upgrade::hex(a);
        ring.push_str(&crate::upgrade::hex(b));
        crate::upgrade::SavedTerm {
            id: i.id,
            pid: i.pid,
            fd,
            cwd: i.cwd,
            cmd: i.cmd,
            title: i.title,
            title_override: i.title_override,
            cols: i.cols,
            rows: i.rows,
            created_ms: i.created_ms,
            exited: i.exited,
            exit_code: i.exit_code,
            line_id: i.line_id,
            task_id: i.task_id,
            agent: i.agent,
            session: i.session,
            flags: i.flags,
            sized_by: i.sized_by,
            profile: i.profile,
            workspace_id: i.workspace_id,
            total: o.total,
            last_ms: o.last_ms,
            closed: o.closed,
            ring,
        }
    }

    pub fn to_json(&self) -> Value {
        let i = lock(&self.info);
        let mut v = json!({
            "id": i.id, "pid": i.pid, "cwd": i.cwd, "cmd": i.cmd, "title": i.title, "titleOverride": i.title_override,
            "cols": i.cols, "rows": i.rows, "createdAt": format_rfc3339(i.created_ms),
            "exited": i.exited,
        });
        if let Some(c) = i.exit_code {
            v["exitCode"] = json!(c);
        }
        if let Some(l) = &i.line_id {
            v["lineId"] = json!(l);
        }
        if let Some(t) = &i.task_id {
            v["taskId"] = json!(t);
        }
        if let Some(a) = &i.agent {
            v["agent"] = json!(a);
        }
        if let Some(b) = &i.sized_by {
            v["sizedBy"] = json!(b);
        }
        if let Some(p) = &i.profile {
            v["profile"] = json!(p);
        }
        if let Some(w) = &i.workspace_id {
            v["workspaceId"] = json!(w);
        }
        v
    }
}

#[cfg(unix)]
pub fn save_all() -> Vec<crate::upgrade::SavedTerm> {
    all().iter().map(|t| t.save()).collect()
}

#[cfg(unix)]
fn wait_pid(pid: u32) -> Option<u32> {
    let mut status = 0;
    while unsafe { libc::waitpid(pid as libc::pid_t, &mut status, 0) } == -1 {
        if std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
            return None;
        }
    }
    if libc::WIFEXITED(status) {
        Some(libc::WEXITSTATUS(status) as u32)
    } else {
        libc::WIFSIGNALED(status).then_some(1)
    }
}

#[cfg(unix)]
pub fn adopt(s: crate::upgrade::SavedTerm) -> Result<Arc<Term>, String> {
    let ring = crate::upgrade::unhex(&s.ring)?;
    let live = !s.exited && s.fd >= 0 && s.pid > 0;
    let (master, writer, reader): (Master, Box<dyn Write + Send>, Option<std::fs::File>) = if live {
        let f = std::fs::File::from(crate::upgrade::adopt_fd(s.fd)?);
        let r = f.try_clone().map_err(|e| e.to_string())?;
        let w = f.try_clone().map_err(|e| e.to_string())?;
        (Master::Fd(f), Box::new(w), (!s.closed).then_some(r))
    } else {
        (Master::Gone, Box::new(std::io::sink()), None)
    };
    let pid = s.pid;
    let mut scan = crate::termtheme::Scanner::default();
    scan.feed(&ring, None);
    let term = Arc::new(Term {
        info: Mutex::new(Info {
            id: s.id.clone(),
            pid,
            cwd: s.cwd,
            cmd: s.cmd,
            title: s.title,
            title_override: s.title_override,
            cols: s.cols,
            rows: s.rows,
            created_ms: s.created_ms,
            exited: !live,
            exit_code: s.exit_code,
            line_id: s.line_id,
            task_id: s.task_id,
            agent: s.agent,
            session: s.session,
            flags: s.flags,
            sized_by: s.sized_by,
            profile: s.profile,
            workspace_id: s.workspace_id,
            resume: None,
        }),
        out: Mutex::new(Out {
            ring: ring.into(),
            subs: vec![],
            closed: !live || s.closed,
            total: s.total,
            last_ms: s.last_ms,
            scan,
            ..Default::default()
        }),
        writer: Mutex::new(writer),
        master: Mutex::new(master),
        killer: Mutex::new(None),
        gen: AtomicU64::new(0),
        holding: AtomicBool::new(false),
        killed: AtomicBool::new(false),
    });
    lock(&TERMS).insert(s.id, term.clone());
    if live {
        let (done_tx, done_rx) = channel::<()>();
        if let Some(r) = reader {
            let t = term.clone();
            std::thread::spawn(move || {
                pump(&t, r.into(), 0);
                let _ = done_tx.send(());
            });
        } else {
            drop(done_tx);
        }
        let t = term.clone();
        std::thread::spawn(move || {
            let code = wait_pid(pid);
            let _ = done_rx.recv_timeout(Duration::from_secs(1));
            if t.stale(0) {
                return;
            }
            {
                let mut i = lock(&t.info);
                i.exited = true;
                i.exit_code = code;
            }
            publish("term.exited", json!({"term": t.to_json()}));
        });
    }
    Ok(term)
}

pub fn theme_changed(dark: bool) {
    for t in all() {
        t.notify_theme(dark);
    }
}

pub fn get(id: &str) -> Option<Arc<Term>> {
    lock(&TERMS).get(id).cloned()
}

pub fn all() -> Vec<Arc<Term>> {
    lock(&TERMS).values().cloned().collect()
}

pub fn remove(id: &str) {
    lock(&TERMS).remove(id);
}

pub fn owner_of(pid: u32, probe: &dyn Probe) -> Option<String> {
    let live: Vec<(u32, String)> = lock(&TERMS)
        .values()
        .map(|t| t.info())
        .filter(|i| !i.exited && i.pid > 0)
        .map(|i| (i.pid, i.id))
        .collect();
    if live.is_empty() {
        return None;
    }
    let mut p = pid;
    for _ in 0..16 {
        if let Some((_, id)) = live.iter().find(|(tp, _)| *tp == p) {
            return Some(id.clone());
        }
        p = probe.ppid(p).filter(|&q| q > 1 && q != p)?;
    }
    None
}

pub fn trim_exited(now: i64) -> usize {
    let mut freed = 0;
    for t in all() {
        if !t.exited() {
            continue;
        }
        let mut o = lock(&t.out);
        if o.subs.is_empty() && now - o.last_ms >= EXITED_TRIM_MS && o.ring.len() > EXITED_RING {
            let over = o.ring.len() - EXITED_RING;
            o.ring.drain(..over);
            o.ring.shrink_to_fit();
            freed += over;
        }
    }
    freed
}

pub fn link_lines(lines: &[crate::Line]) {
    for l in lines {
        if let Some(t) = l.term_id.as_deref().and_then(get) {
            t.set_line(
                &l.id,
                Some((l.session_id.clone(), l.resume_command.clone())),
            );
        }
    }
}

pub fn insert(t: &Term, text: &str) -> std::io::Result<()> {
    let screen = t.screen();
    let bracketed = screen.windows(8).any(|w| w == b"\x1b[?2004h");
    let body = if bracketed {
        format!("\x1b[200~{text}\x1b[201~")
    } else {
        text.replace(['\r', '\n'], " ")
    };
    t.input(body.as_bytes())
}

pub fn paste(t: &Term, text: &str) -> std::io::Result<()> {
    insert(t, text)?;
    std::thread::sleep(Duration::from_millis(300));
    t.input(b"\r")
}

pub fn ws(id: &str, req: &Request, mut s: impl Stream) {
    let Some(t) = get(id) else {
        return respond_raw(
            &mut s,
            404,
            "application/json",
            br#"{"error":"no such term"}"#,
        );
    };
    let upgrade = header(req, "upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
    let Some(key) = header(req, "sec-websocket-key").filter(|_| upgrade) else {
        return respond_raw(
            &mut s,
            400,
            "application/json",
            br#"{"error":"websocket upgrade required"}"#,
        );
    };
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let head = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n");
    if s.write_all(head.as_bytes()).is_err() {
        return;
    }
    let _ = s.set_read_timeout(None);
    let ctl = Arc::new(Ctl::default());
    let (replay, sub) = t.subscribe_waking(&ctl.backlog);
    let (tx, rx) = sub.unzip();
    let input = match (tx, s.reader()) {
        (Some(tx), Some(r)) => {
            let (t, ctl) = (t.clone(), ctl.clone());
            std::thread::Builder::new()
                .stack_size(256 << 10)
                .spawn(move || ws_input(r, &t, &ctl, &tx))
                .is_ok()
        }
        _ => false,
    };
    let mut ws = WebSocket::from_raw_socket(s, Role::Server, None);
    let wrote = replay
        .chunks(CHUNK)
        .try_for_each(|c| ws.write(Message::binary(c.to_vec())))
        .and_then(|_| ws.flush());
    if let (Ok(()), true, Some(rx)) = (wrote, input, rx) {
        ws_output(&mut ws, &t, &ctl, &rx);
    }
    let _ = ws.close(None);
    let _ = ws.flush();
    ws.get_ref().hang_up();
}

#[derive(Default)]
struct Ctl {
    gone: AtomicBool,
    backlog: Arc<AtomicUsize>,
    pongs: Mutex<Vec<tungstenite::Bytes>>,
}

struct ReadOnly(Box<dyn Read + Send>);

impl Read for ReadOnly {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(b)
    }
}

impl Write for ReadOnly {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn ws_input(r: Box<dyn Read + Send>, t: &Term, ctl: &Ctl, wake: &Waker) {
    let mut ws = WebSocket::from_raw_socket(ReadOnly(r), Role::Server, None);
    loop {
        match ws.read() {
            Ok(Message::Binary(b)) => {
                let _ = t.input(&b);
            }
            Ok(Message::Text(m)) => {
                let v = serde_json::from_str::<Value>(m.as_str()).unwrap_or_default();
                if let Ok(Some((c, r))) = size(&v["resize"]) {
                    let _ = t.resize(c, r, client_kind(&v["client"]));
                }
            }
            Ok(Message::Ping(p)) => {
                lock(&ctl.pongs).push(p);
                let _ = wake.send(Arc::from(&[][..]));
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    ctl.gone.store(true, Ordering::SeqCst);
    let _ = wake.send(Arc::from(&[][..]));
}

fn ws_output<S: Stream>(ws: &mut WebSocket<S>, t: &Term, ctl: &Ctl, rx: &Chunks) {
    while let Ok(first) = rx.recv() {
        let mut next = Some(first);
        let mut batch = 0;
        while let Some(c) = next.take() {
            if c.is_empty() {
                for p in std::mem::take(&mut *lock(&ctl.pongs)) {
                    if ws.write(Message::Pong(p)).is_err() {
                        return;
                    }
                }
                if ctl.gone.load(Ordering::SeqCst)
                    || t.out_closed()
                    || ctl.backlog.load(Ordering::SeqCst) >= LAGGED
                {
                    return;
                }
            } else {
                if ws.write(Message::binary(c.to_vec())).is_err() {
                    return;
                }
                ctl.backlog.fetch_sub(c.len(), Ordering::SeqCst);
                batch += c.len();
                if batch >= FLUSH_BATCH {
                    break;
                }
            }
            next = rx.try_recv().ok();
        }
        if ws.flush().is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lagging_socket_is_dropped_instead_of_buffering_forever() {
        let backlog = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = channel();
        let sub = Sub {
            tx,
            backlog: Some(backlog.clone()),
        };
        let chunk: Arc<[u8]> = vec![b'x'; 1 << 20].into();
        let fed = (0..64).take_while(|_| sub.feed(&chunk)).count();
        assert_eq!(fed, MAX_BACKLOG >> 20);
        assert!(backlog.load(Ordering::SeqCst) >= LAGGED);
        let got: Vec<Arc<[u8]>> = rx.try_iter().collect();
        assert_eq!(got.len(), fed + 1);
        assert!(got.last().unwrap().is_empty());
        let free = Sub {
            tx: channel().0,
            backlog: None,
        };
        assert!(!free.feed(&chunk));
    }

    #[test]
    fn phones_never_resize_agent_terms() {
        let mut i = Info {
            id: "t".into(),
            pid: 0,
            cwd: "/".into(),
            cmd: vec![],
            title: String::new(),
            title_override: None,
            cols: 120,
            rows: 40,
            created_ms: 0,
            exited: false,
            exit_code: None,
            line_id: None,
            task_id: None,
            agent: None,
            session: None,
            flags: vec![],
            sized_by: None,
            profile: None,
            workspace_id: None,
            resume: None,
        };
        assert!(may_resize(&i, Some("phone")));
        assert!(may_resize(&i, Some("desktop")));
        i.agent = Some("claude".into());
        assert!(!may_resize(&i, Some("phone")));
        assert!(may_resize(&i, Some("desktop")));
        assert!(may_resize(&i, None));
        i.agent = None;
        i.line_id = Some("ln_1".into());
        assert!(!may_resize(&i, Some("phone")));
    }

    #[test]
    fn quiet_output_ends_a_turn() {
        let submit = 10_000;
        assert!(!turn_busy(0, 0, 20_000, 20_500));
        assert!(!turn_busy(submit, 9_000, submit + 200, submit + 300));
        assert!(turn_busy(submit, 9_000, submit + 3_000, submit + 3_500));
        assert!(turn_busy(
            submit,
            submit + 100,
            submit + 9_000,
            submit + 9_100
        ));
        assert!(!turn_busy(
            submit,
            9_000,
            submit + 9_000,
            submit + 9_000 + TURN_QUIET_MS
        ));
        assert!(effective_busy(true, 5, 0, None));
        assert!(effective_busy(false, 5, 9, Some(true)));
        assert!(!effective_busy(false, 9, 5, Some(true)));
        assert!(!effective_busy(false, 5, 9, Some(false)));
        assert!(!turn_busy(
            submit,
            submit + 20_000,
            submit + 25_000,
            submit + 25_100
        ));
    }

    #[test]
    fn resize_clients_are_short_names() {
        assert_eq!(client_kind(&json!("phone")).as_deref(), Some("phone"));
        assert_eq!(client_kind(&json!("desktop")).as_deref(), Some("desktop"));
        assert_eq!(client_kind(&Value::Null), None);
        assert_eq!(client_kind(&json!("")), None);
        assert_eq!(client_kind(&json!("a b")), None);
        assert_eq!(client_kind(&json!("x".repeat(17))), None);
        assert_eq!(
            size(&json!({"cols": 40, "rows": 20})).unwrap(),
            Some((40, 20))
        );
    }

    #[cfg(unix)]
    #[test]
    fn answers_color_queries_and_pushes_theme_changes() {
        let set = |dark: bool, bg: &str| {
            let body = json!({"dark": dark, "fg": "#18181b", "bg": bg});
            crate::termtheme::routes("POST", &["v1", "theme"], &body).unwrap()
        };
        assert_eq!(set(false, "#ffffff").0, 200);
        let script = r#"stty -echo -icanon min 1; echo "fgbg=$COLORFGBG"; printf '\033[?2031h\033]11;?\033\\'; head -c 25 | od -An -c | tr -d ' \n'; echo; echo replied; head -c 9 | od -An -c | tr -d ' \n'; echo; echo notified; sleep 30"#;
        let t = spawn(Spec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cmd: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
        let (replay, rx) = t.subscribe();
        let rx = rx.unwrap();
        let mut seen = replay;
        let mut wait_for = |word: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !String::from_utf8_lossy(&seen).contains(word) {
                assert!(std::time::Instant::now() < deadline, "no {word}");
                if let Ok(c) = rx.recv_timeout(Duration::from_millis(100)) {
                    seen.extend_from_slice(&c);
                }
            }
            String::from_utf8_lossy(&seen).into_owned()
        };
        let out = wait_for("replied");
        assert!(out.contains("fgbg=0;15"), "{out}");
        assert!(out.contains("033]11;rgb:ffff/ffff/ffff033\\"), "{out}");
        assert_eq!(set(true, "#0b0b0c").0, 200);
        let out = wait_for("notified");
        assert!(out.contains("033[?997;1n"), "{out}");
        t.kill().unwrap();
        remove(&t.info().id);
    }

    #[cfg(unix)]
    #[test]
    fn exited_terms_keep_only_a_short_tail() {
        let t = spawn(Spec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cmd: vec![
                "/bin/sh".into(),
                "-c".into(),
                "head -c 300000 /dev/zero | tr '\\0' x; echo; echo the-end".into(),
            ],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
        let id = t.info().id;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !(t.exited() && t.out_closed()) {
            assert!(std::time::Instant::now() < deadline, "term did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
        let full = lock(&t.out).ring.len();
        assert!(full > EXITED_RING, "{full}");
        let last = lock(&t.out).last_ms;
        assert_eq!(trim_exited(last + 1000), 0);
        assert!(trim_exited(last + EXITED_TRIM_MS) >= full - EXITED_RING);
        let (replay, rx) = t.subscribe();
        assert!(rx.is_none());
        assert_eq!(replay.len(), EXITED_RING);
        assert!(String::from_utf8_lossy(&replay).contains("the-end"));
        remove(&id);
    }

    #[cfg(unix)]
    #[test]
    fn respawn_keeps_id_and_subscribers() {
        let t = spawn(Spec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cmd: vec!["/bin/sh".into(), "-c".into(), "sleep 30".into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap();
        let (id, old) = (t.info().id, t.info().pid);
        let (_, rx) = t.subscribe();
        let rx = rx.unwrap();
        t.hold(true);
        unsafe { libc::kill(old as i32, libc::SIGKILL) };
        std::thread::sleep(Duration::from_millis(300));
        assert!(!t.exited());
        t.respawn(vec![
            "/bin/sh".into(),
            "-c".into(),
            "echo again; sleep 30".into(),
        ])
        .unwrap();
        let i = t.info();
        assert_eq!(i.id, id);
        assert_ne!(i.pid, old);
        assert!(!i.exited);
        let mut seen = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !String::from_utf8_lossy(&seen).contains("again") {
            assert!(
                std::time::Instant::now() < deadline,
                "no output after respawn"
            );
            if let Ok(c) = rx.recv_timeout(Duration::from_millis(100)) {
                seen.extend_from_slice(&c);
            }
        }
        assert!(!t.exited());
        t.kill().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !t.exited() {
            assert!(std::time::Instant::now() < deadline, "term did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
        remove(&id);
    }
}
