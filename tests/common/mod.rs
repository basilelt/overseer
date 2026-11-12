#![allow(dead_code)]
use overseer::sync::{Transport, Ws};
use overseer::util::format_rfc3339;
use overseer_api::{router, App as ApiApp};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// Stubs and the system dirs, plus a dir holding only a symlink to the host's tmux when it
/// lives elsewhere (Homebrew on macOS); the rest of the host PATH stays hidden.
fn test_path(stubs: &Path) -> String {
    static TMUX_DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    let dir = TMUX_DIR.get_or_init(|| {
        let host = std::env::var_os("PATH")?;
        let tmux = std::env::split_paths(&host)
            .map(|d| d.join("tmux"))
            .find(|t| t.is_file())?;
        let parent = tmux.parent()?;
        if parent == Path::new("/usr/bin") || parent == Path::new("/bin") {
            return None;
        }
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tmux-bin");
        std::fs::create_dir_all(&dir).ok()?;
        let link = dir.join("tmux");
        if std::fs::read_link(&link).ok().as_deref() != Some(tmux.as_path()) {
            let tmp = dir.join(format!("tmux.{}", std::process::id()));
            let _ = std::fs::remove_file(&tmp);
            std::os::unix::fs::symlink(&tmux, &tmp).ok()?;
            std::fs::rename(&tmp, &link).ok()?;
        }
        Some(dir)
    });
    let mut p = format!("{}:/usr/bin:/bin", stubs.display());
    if cfg!(target_os = "macos") {
        // lsof lives in /usr/sbin on macOS.
        p.push_str(":/usr/sbin:/sbin");
    }
    if let Some(d) = dir {
        p.push(':');
        p.push_str(&d.display().to_string());
    }
    p
}
use std::sync::Arc;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

pub static N: AtomicUsize = AtomicUsize::new(0);
pub const SID: &str = "dddddddd-0000-4000-8000-000000000001";

pub struct T {
    pub sock: String,
    pub base: PathBuf,
    pub wt: PathBuf,
}

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_overseer")
}

impl T {
    pub fn new() -> T {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let base =
            Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("live-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let wt = base.join("repo");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::create_dir_all(base.join("home/.claude")).unwrap();
        let base = base.canonicalize().unwrap();
        let wt = wt.canonicalize().unwrap();
        for args in [
            vec!["init", "-q", "-b", "feature/AS-42-live"],
            vec!["config", "user.email", "dev@example.com"],
            vec!["remote", "add", "origin", "git@github.com:Acme/live.git"],
        ] {
            let ok = Command::new("git")
                .current_dir(&wt)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .args(&args)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        }
        let t = T {
            sock: format!("sb-test-{}-{n}", std::process::id()),
            base,
            wt,
        };
        let ok = t
            .tmux_cmd()
            .args([
                "new-session",
                "-d",
                "-s",
                "main",
                "-x",
                "160",
                "-y",
                "40",
                "sh",
            ])
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let path = t.tmux(&["display-message", "-p", "#{socket_path}"]);
        let watchdog = format!(
            "while kill -0 {} 2>/dev/null; do sleep 1; done; tmux -L {} kill-server; rm -f '{}'",
            std::process::id(),
            t.sock,
            path.trim()
        );
        t.tmux(&["run-shell", "-b", &watchdog]);
        t
    }

    pub fn env(&self, c: &mut Command) {
        let stubs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stubs");
        c.env_clear()
            .env("PATH", test_path(&stubs))
            .env("HOME", self.base.join("home"))
            .env("SHELL", "/bin/sh")
            .env("XDG_RUNTIME_DIR", self.base.join("run"))
            .env("TERM", "xterm-256color")
            .env("LANG", "C.UTF-8")
            .env("CLAUDE_CONFIG_DIR", self.base.join("home/.claude"))
            .env("CODEX_HOME", self.base.join("home/.codex"))
            .env("OVERSEER_STATE_DIR", self.base.join("state"))
            .env("OVERSEER_TMUX_SOCKET", &self.sock)
            .env("OVERSEER_TMUX_SNAPSHOT", self.base.join("snapshot.json"))
            .env("OVERSEER_HOLD_TIMEOUT_MS", "5000")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("SB_BIN", bin())
            .env("SB_STUB_LOG", self.base.join("stub.log"))
            .env("SB_SSH_LOG", self.base.join("ssh.log"))
            .env("OVERSEER_LOGIN_ENV", "0");
        die_with_parent(c);
    }

    pub fn tmux_cmd(&self) -> Command {
        let mut c = Command::new("tmux");
        self.env(&mut c);
        c.args(["-L", &self.sock, "-f", "/dev/null"]);
        c
    }

    pub fn tmux(&self, args: &[&str]) -> String {
        let out = self.tmux_cmd().args(args).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    pub fn sb(&self, args: &[&str]) -> Output {
        let mut c = Command::new(bin());
        self.env(&mut c);
        output(c.args(args), None)
    }

    pub fn json(&self, args: &[&str]) -> Value {
        let out = self.sb(args);
        serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "{args:?}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    }

    pub fn line(&self, sid: &str) -> Value {
        self.json(&["ls", "--json", "--all"])
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["line"]["agentSessionId"] == sid)
            .cloned()
            .unwrap_or(Value::Null)
    }

    pub fn start_agent(&self, name: &str, sid: &str, cmd: &str) -> String {
        let pane = self.tmux(&[
            "new-window",
            "-P",
            "-F",
            "#{pane_id}",
            "-t",
            "main",
            "-n",
            name,
            "-c",
            &self.wt.to_string_lossy(),
            "-e",
            &format!("SB_STUB_SESSION={sid}"),
            cmd,
        ]);
        let pane = pane.trim().to_string();
        self.wait(|| self.screen(&pane).contains('❯') && self.line(sid)["local"]["pid"].is_u64());
        pane
    }

    pub fn screen(&self, pane: &str) -> String {
        self.tmux(&["capture-pane", "-p", "-t", pane])
    }

    pub fn keys(&self, pane: &str, text: &str, enter: bool) {
        let mut args = vec!["send-keys", "-t", pane, "-l", text];
        self.tmux(&args);
        if enter {
            args = vec!["send-keys", "-t", pane, "Enter"];
            self.tmux(&args);
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    pub fn wait(&self, mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < end, "timed out");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn hold(&self, id: &str, yes: bool) -> (i32, Value) {
        let mut args = vec!["hold", id, "--json"];
        if yes {
            args.push("--yes");
        }
        let out = self.sb(&args);
        let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.code().unwrap_or(-1), v)
    }

    pub fn failed(v: &Value) -> Vec<String> {
        v["predicates"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, p)| p["ok"] == false)
            .map(|(k, _)| k.clone())
            .collect()
    }
}

impl Drop for T {
    fn drop(&mut self) {
        let path = self.tmux(&["display-message", "-p", "#{socket_path}"]);
        let _ = self.tmux_cmd().arg("kill-server").output();
        let path = path.trim();
        if path.ends_with(&self.sock) {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub struct Fx {
    pub base: PathBuf,
    pub wt: PathBuf,
    pub state: PathBuf,
    pub sock: PathBuf,
}

fn now_ms() -> i64 {
    overseer::util::now_ms()
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn remote_line(station: &str, i: usize, state: &str, at_ms: i64) -> Value {
    json!({"line": {
        "id": format!("{station}-{i:02}"), "owner": "dev@example.com", "stationId": station,
        "agent": "claude", "agentSessionId": format!("sess-{station}-{i}"),
        "ticketKey": format!("AS-{}", 100 + i % 7), "repo": "github.com/Acme/board",
        "branch": format!("feature/AS-{}-x", 100 + i % 7), "worktreePath": format!("/w/{station}/{i}"),
        "origin": "dev", "state": state, "lastActivityAt": format_rfc3339(at_ms),
        "envRef": "pr-101",
    }})
}

impl Fx {
    pub fn new(tag: &str, local: usize) -> Fx {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("board-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let wt = base.join("repo");
        let state = base.join("state");
        for d in [
            &wt,
            &state,
            &base.join("home/.claude/projects/p"),
            &base.join("gh"),
        ] {
            std::fs::create_dir_all(d).unwrap();
        }
        let base = base.canonicalize().unwrap();
        let wt = wt.canonicalize().unwrap();
        let state = state.canonicalize().unwrap();
        git(&wt, &["init", "-q", "-b", "feature/AS-42-board"]);
        git(&wt, &["config", "user.email", "dev@example.com"]);
        git(&wt, &["config", "user.name", "Dev"]);
        git(
            &wt,
            &["remote", "add", "origin", "git@github.com:Acme/board.git"],
        );
        std::fs::write(wt.join("a.txt"), "one\n").unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-q", "-m", "init"]);
        let ts = format_rfc3339(now_ms() - 60_000);
        for i in 0..local {
            let sid = format!("aaaaaaaa-0000-4000-8000-{i:012}");
            std::fs::write(
                base.join(format!("home/.claude/projects/p/{sid}.jsonl")),
                format!(
                    "{}\n",
                    json!({"type": "user", "sessionId": sid, "cwd": wt, "timestamp": ts})
                ),
            )
            .unwrap();
        }
        std::fs::write(
            state.join("config.json"),
            json!({"stationId": "vm-local", "yardUrl": "http://127.0.0.1:7070/"}).to_string(),
        )
        .unwrap();
        let sock =
            std::env::temp_dir().join(format!("sb-test-board-{tag}-{}.sock", std::process::id()));
        Fx {
            base,
            wt,
            state,
            sock,
        }
    }

    pub fn station(&self, id: &str, n: usize, last_seen_ms: i64, state: &str) {
        std::fs::create_dir_all(self.state.join("stations")).unwrap();
        let lines: Vec<Value> = (0..n)
            .map(|i| remote_line(id, i, state, now_ms() - 120_000))
            .collect();
        let v =
            json!({"stationId": id, "lastSeenAt": format_rfc3339(last_seen_ms), "lines": lines});
        let tmp = self.state.join("stations/.tmp");
        std::fs::write(&tmp, v.to_string()).unwrap();
        std::fs::rename(tmp, self.state.join(format!("stations/{id}.json"))).unwrap();
    }

    pub fn vars(&self) -> Vec<(&'static str, OsString)> {
        let stubs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/stubs");
        let p = |p: PathBuf| p.into_os_string();
        vec![
            ("PATH", test_path(&stubs).into()),
            ("HOME", p(self.base.join("home"))),
            ("LANG", "C.UTF-8".into()),
            ("XDG_RUNTIME_DIR", p(self.base.join("run"))),
            ("CLAUDE_CONFIG_DIR", p(self.base.join("home/.claude"))),
            ("CODEX_HOME", p(self.base.join("home/.codex"))),
            ("OVERSEER_STATE_DIR", p(self.state.clone())),
            ("OVERSEER_SOCKET", p(self.sock.clone())),
            (
                "OVERSEER_TMUX_SOCKET",
                format!("sb-test-board-none-{}", std::process::id()).into(),
            ),
            ("OVERSEER_TMUX_SNAPSHOT", p(self.base.join("none.json"))),
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("SB_GH_DIR", p(self.base.join("gh"))),
            ("SB_GH_LOG", p(self.base.join("gh.log"))),
            ("OVERSEER_LOGIN_ENV", "0".into()),
        ]
    }

    pub fn env(&self, c: &mut Command) {
        c.env_clear().envs(self.vars());
        die_with_parent(c);
    }

    pub fn sb(&self, args: &[&str]) -> Output {
        let mut c = Command::new(env!("CARGO_BIN_EXE_overseer"));
        self.env(&mut c);
        output(c.args(args), None)
    }

    pub fn daemon(&self, http: bool) -> Reap {
        let mut c = Command::new(env!("CARGO_BIN_EXE_overseer"));
        self.env(&mut c);
        c.args(["daemon", "--interval", "1"]);
        if http {
            c.args(["--http", "127.0.0.1:0"]);
        }
        let r = Reap::spawn(c.stderr(Stdio::null()));
        wait(|| {
            UnixStream::connect(&self.sock).is_ok()
                && (!http || self.state.join("board.json").exists())
        });
        r
    }

    pub fn req(&self, method: &str, target: &str, body: &str) -> (u16, String) {
        let s = UnixStream::connect(&self.sock).unwrap();
        exchange(s, method, target, body, "unix")
    }
}

pub struct Reap(pub Child);

impl Reap {
    pub fn spawn(c: &mut Command) -> Reap {
        Reap(c.spawn().unwrap())
    }
}

// PR_SET_PDEATHSIG fires when the spawning *thread* exits, which is the test thread here;
// it covers a harness killed by `timeout` or SIGKILL, where no Drop runs.
pub fn die_with_parent(c: &mut Command) {
    #[cfg(target_os = "linux")]
    unsafe {
        use std::os::unix::process::CommandExt;
        c.pre_exec(|| {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = c;
}

pub const RUN_LIMIT: Duration = Duration::from_secs(60);

pub fn output(c: &mut Command, input: Option<&str>) -> Output {
    let mut child = c
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let drain = |mut r: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut v = vec![];
            let _ = r.read_to_end(&mut v);
            v
        })
    };
    let stdout = drain(Box::new(child.stdout.take().unwrap()));
    let stderr = drain(Box::new(child.stderr.take().unwrap()));
    if let Some(i) = input {
        let _ = child.stdin.take().unwrap().write_all(i.as_bytes());
    }
    let end = Instant::now() + RUN_LIMIT;
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{c:?} still running after {RUN_LIMIT:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn wait(mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(20), "timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn exchange(
    mut s: impl Read + Write,
    method: &str,
    target: &str,
    body: &str,
    host: &str,
) -> (u16, String) {
    // One write: the daemon may reject on the head and close before a separate body write lands (EPIPE).
    s.write_all(
        format!(
            "{method} {target} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, out.split_once("\r\n\r\n").unwrap().1.to_string())
}

pub const ADMIN: &str = "admin-for-tests";

pub struct Api {
    pub app: Arc<ApiApp>,
    pub addr: String,
    pub offset: Arc<AtomicI64>,
    _rt: tokio::runtime::Runtime,
}

impl Api {
    pub fn start(base: &Path) -> Api {
        let offset = Arc::new(AtomicI64::new(0));
        let o = offset.clone();
        let clock = Arc::new(move || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
                + o.load(Ordering::SeqCst)
        });
        let app = ApiApp::open(&base.join("api.db"), Some(ADMIN), clock).unwrap();
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
        Api {
            app,
            addr,
            offset,
            _rt: rt,
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn http(&self, method: &str, path: &str, token: &str, body: Option<Value>) -> (u16, Value) {
        let body = body.map(|b| b.to_string()).unwrap_or_default();
        let mut s = TcpStream::connect(&self.addr).unwrap();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: t\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = out.split_once("\r\n\r\n").unwrap().1;
        (code, serde_json::from_str(body).unwrap_or(Value::Null))
    }

    pub fn enrol(&self, station: &str) -> String {
        let (code, v) = self.http(
            "POST",
            "/v1/stations",
            ADMIN,
            Some(json!({"stationId": station, "kind": "vm"})),
        );
        assert_eq!(code, 201, "{v}");
        v["token"].as_str().unwrap().to_string()
    }

    pub fn user(&self, name: &str) -> String {
        let (code, v) = self.http("POST", "/v1/users", ADMIN, Some(json!({ "name": name })));
        assert_eq!(code, 201, "{v}");
        v["token"].as_str().unwrap().to_string()
    }
}

pub fn recv(ws: &mut Ws, kind: &str) -> Value {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < end, "no {kind} frame");
        if let Some(v) = ws.recv(Duration::from_millis(200)).unwrap() {
            if v["kind"] == kind {
                return v;
            }
        }
    }
}

pub fn sb_stdin(t: &T, args: &[&str], input: &str) -> Output {
    let mut c = Command::new(bin());
    t.env(&mut c);
    output(c.args(args), Some(input))
}

pub fn config(t: &T, v: Value) {
    std::fs::create_dir_all(t.base.join("state")).unwrap();
    std::fs::write(t.base.join("state/config.json"), v.to_string()).unwrap();
}

pub fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

pub fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn wait_for<V>(mut f: impl FnMut() -> Option<V>) -> V {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn mcp(app: &App, own: Option<&str>, input: &[Value]) -> Vec<Value> {
    let mut c = Command::new(bin());
    app.fx.env(&mut c);
    if let Some(o) = own {
        c.env("OVERSEER_TASK_ID", o);
    }
    let mut child = c
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for m in input {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

pub fn read_events(s: &mut UnixStream, buf: &mut String, want: &[&str], within: Duration) {
    let end = Instant::now() + within;
    while !want.iter().all(|w| buf.contains(w)) {
        assert!(Instant::now() < end, "missing {want:?} in {buf}");
        let mut b = [0u8; 4096];
        if let Ok(n) = s.read(&mut b) {
            buf.push_str(&String::from_utf8_lossy(&b[..n]));
        }
    }
}

pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn js(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or(Value::Null)
}

pub fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_string())
        .collect()
}

pub fn line_json(app: &App, line: &str) -> Value {
    app.ok("GET", "/v1/lines?all=true", Value::Null)
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["line"]["id"] == line)
        .cloned()
        .unwrap()
}

pub struct App {
    pub fx: Fx,
    env: Vec<(String, String)>,
    _d: Option<Reap>,
}

impl App {
    pub fn new(tag: &str, config: Value) -> App {
        App::with_env(tag, config, &[])
    }

    pub fn with_env(tag: &str, config: Value, env: &[(&str, &str)]) -> App {
        let fx = Fx::new(tag, 0);
        let mut cfg = config;
        cfg["stationId"] = json!("vm-local");
        cfg["worktreeDir"] = json!(fx.base.join("worktrees"));
        cfg["worktreeFetch"] = json!(false);
        std::fs::write(fx.state.join("config.json"), cfg.to_string()).unwrap();
        let d = App::spawn_daemon(&fx, env);
        App {
            fx,
            env: env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            _d: Some(d),
        }
    }

    fn spawn_daemon(fx: &Fx, env: &[(&str, &str)]) -> Reap {
        let mut c = Command::new(bin());
        fx.env(&mut c);
        c.env("SB_BIN", bin())
            .env("SB_STUB_LOG", fx.base.join("stub.log"))
            .env("SHELL", "/bin/sh")
            .envs(env.iter().copied())
            .args(["daemon", "--interval", "1"]);
        let d = Reap::spawn(c.stderr(Stdio::null()));
        wait(|| UnixStream::connect(&fx.sock).is_ok());
        d
    }

    pub fn crash_and_restart(&mut self) {
        drop(self._d.take());
        let env: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self._d = Some(App::spawn_daemon(&self.fx, &env));
    }

    pub fn req(&self, method: &str, target: &str, body: Value) -> (u16, Value) {
        let b = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        let (code, out) = self.fx.req(method, target, &b);
        (code, serde_json::from_str(&out).unwrap_or(Value::Null))
    }

    pub fn ok(&self, method: &str, target: &str, body: Value) -> Value {
        let (code, v) = self.req(method, target, body);
        assert!((200..300).contains(&code), "{method} {target}: {code} {v}");
        v
    }

    pub fn events(&self, within: Duration) -> (UnixStream, String) {
        let mut s = UnixStream::connect(&self.fx.sock).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        s.write_all(b"GET /v1/events HTTP/1.1\r\nHost: unix\r\n\r\n")
            .unwrap();
        let mut buf = String::new();
        read_events(&mut s, &mut buf, &["retry:"], within);
        (s, buf)
    }

    pub fn ws(&self, id: &str) -> WebSocket<UnixStream> {
        let s = UnixStream::connect(&self.fx.sock).unwrap();
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let (ws, _) = tungstenite::client(format!("ws://localhost/v1/terms/{id}/io"), s).unwrap();
        ws
    }

    pub fn task(&self, id: &str) -> Value {
        self.ok("GET", &format!("/v1/tasks/{id}"), Value::Null)
    }
}

pub fn read_until(ws: &mut WebSocket<UnixStream>, needle: &str) -> String {
    let hard = Instant::now() + Duration::from_secs(120);
    let mut end = Instant::now() + Duration::from_secs(15);
    let n = needle.as_bytes();
    let mut got: Vec<u8> = vec![];
    let mut from = 0;
    loop {
        if got[from..].windows(n.len()).any(|w| w == n) {
            return String::from_utf8_lossy(&got).into_owned();
        }
        from = got.len().saturating_sub(n.len().saturating_sub(1));
        assert!(
            Instant::now() < end.min(hard),
            "no {needle:?} in {:?}",
            String::from_utf8_lossy(&got)
        );
        match ws.read() {
            Ok(Message::Binary(b)) => {
                got.extend_from_slice(&b);
                end = Instant::now() + Duration::from_secs(15);
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => panic!("{e}: {:?}", String::from_utf8_lossy(&got)),
        }
    }
}

pub fn send(ws: &mut WebSocket<UnixStream>, text: &str) {
    ws.send(Message::binary(text.as_bytes().to_vec())).unwrap();
}
