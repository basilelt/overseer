use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static N: AtomicUsize = AtomicUsize::new(0);

struct Sandbox {
    root: PathBuf,
    state: PathBuf,
}

impl Sandbox {
    fn new() -> Sandbox {
        let n = N.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("ovs-cli-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = root.join("state");
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("run")).unwrap();
        Sandbox { root, state }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_overseer"));
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.root.join("home"))
            .env("XDG_RUNTIME_DIR", self.root.join("run"))
            .env("OVERSEER_STATE_DIR", &self.state)
            .env(
                "OVERSEER_TMUX_SOCKET",
                format!("ovs-cli-none-{}", std::process::id()),
            )
            .env("OVERSEER_LOGIN_ENV", "0")
            .args(args)
            .stdin(Stdio::null());
        c
    }

    fn run(&self, args: &[&str]) -> (Output, Duration) {
        let start = Instant::now();
        let mut child = self
            .cmd(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let end = start + Duration::from_secs(10);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() > end {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{args:?} did not exit; it started something");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let t = start.elapsed();
        (child.wait_with_output().unwrap(), t)
    }

    fn daemon(&self) -> Child {
        self.cmd(&["daemon", "--interval", "1"])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn sock(&self) -> PathBuf {
        self.state.join("daemon.sock")
    }

    fn untouched(&self) -> bool {
        !self.state.exists()
    }

    fn version(&self) -> Option<String> {
        let mut s = UnixStream::connect(self.sock()).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
        s.write_all(b"GET /v1/version HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
            .ok()?;
        let mut out = String::new();
        s.read_to_string(&mut out).ok()?;
        out.contains("200").then_some(out)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Reap(Child);

impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !f() {
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn daemon_help_prints_usage_and_starts_nothing() {
    for flag in ["--help", "-h"] {
        let sb = Sandbox::new();
        let (out, took) = sb.run(&["daemon", flag]);
        assert!(out.status.success(), "{flag}");
        assert!(String::from_utf8_lossy(&out.stdout).contains("overseer daemon [--interval"));
        assert!(took < Duration::from_secs(5));
        assert!(sb.untouched(), "{flag} touched the state dir");
    }
}

#[test]
fn help_works_for_every_command_and_touches_nothing() {
    let sb = Sandbox::new();
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["help"],
        vec!["ls", "--help"],
        vec!["mcp", "--help"],
        vec!["proxy", "-h"],
        vec!["board", "--help"],
        vec!["daemon", "--once", "--help"],
    ] {
        let (out, _) = sb.run(&args);
        assert!(out.status.success(), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("usage:"));
    }
    assert!(sb.untouched());
}

#[test]
fn unknown_flags_and_extra_arguments_fail_without_starting_anything() {
    let sb = Sandbox::new();
    for args in [
        vec!["daemon", "--bogus"],
        vec!["daemon", "extra"],
        vec!["daemon", "--interval"],
        vec!["daemon", "--interval", "soon"],
        vec!["daemon", "--detach", "--bogus"],
        vec!["status", "--jsno"],
        vec!["board", "x"],
        vec!["doctor", "--fix"],
        vec!["resume"],
        vec!["resume", "a", "b"],
        vec!["hold", "a", "--force"],
    ] {
        let (out, took) = sb.run(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("usage:"), "{args:?}: {err}");
        assert!(took < Duration::from_secs(5));
    }
    assert!(sb.untouched());
}

#[test]
fn a_second_daemon_on_a_locked_state_dir_exits_and_leaves_the_first_alone() {
    let sb = Sandbox::new();
    let a = Reap(sb.daemon());
    wait_for(|| sb.version().is_some());
    let pid = std::fs::read_to_string(sb.state.join("daemon.pid")).unwrap();
    assert_eq!(pid.trim(), a.0.id().to_string());
    let before = names(&sb.state);
    let db = std::fs::metadata(sb.state.join("overseer.db")).unwrap();

    for _ in 0..3 {
        let (out, _) = sb.run(&["daemon", "--interval", "1"]);
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("a daemon is already running"), "{err}");
    }

    assert_eq!(
        std::fs::read_to_string(sb.state.join("daemon.pid"))
            .unwrap()
            .trim(),
        a.0.id().to_string()
    );
    assert!(sb.version().is_some(), "the first daemon stopped answering");
    assert_eq!(names(&sb.state), before);
    assert_eq!(
        std::fs::metadata(sb.state.join("overseer.db"))
            .unwrap()
            .len(),
        db.len()
    );
    let mut a = a;
    assert!(a.0.try_wait().unwrap().is_none(), "the first daemon exited");
}

#[test]
fn a_second_daemon_does_not_replace_the_pid_file_or_socket_when_the_lock_is_free() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(&sb.state).unwrap();
    let mut live = Command::new("sleep");
    std::os::unix::process::CommandExt::arg0(&mut live, "overseer daemon");
    let mut live = Reap(live.arg("60").spawn().unwrap());
    std::fs::write(sb.state.join("daemon.pid"), live.0.id().to_string()).unwrap();
    let (out, _) = sb.run(&["daemon", "--interval", "1"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("a daemon is already running"));
    assert_eq!(
        std::fs::read_to_string(sb.state.join("daemon.pid")).unwrap(),
        live.0.id().to_string()
    );
    assert!(!sb.sock().exists());
    assert!(!sb.state.join("overseer.db").exists());
    assert!(live.0.try_wait().unwrap().is_none());
}

#[test]
fn a_stale_pid_file_does_not_block_a_daemon() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(&sb.state).unwrap();
    std::fs::write(sb.state.join("daemon.pid"), "999999").unwrap();
    let _d = Reap(sb.daemon());
    wait_for(|| sb.version().is_some());
}
