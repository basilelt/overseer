mod common;

use common::{bin, die_with_parent, exchange, output, wait, Reap};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Sbx {
    base: PathBuf,
    sock: PathBuf,
}

impl Sbx {
    fn new(tag: &str) -> Sbx {
        let base = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("hooks-owner-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home/.claude")).unwrap();
        std::fs::create_dir_all(base.join("run")).unwrap();
        Sbx {
            base: base.canonicalize().unwrap(),
            sock: std::env::temp_dir().join(format!("sb-ho-{tag}-{}.sock", std::process::id())),
        }
    }

    fn home(&self) -> PathBuf {
        self.base.join("home")
    }

    fn default_state(&self) -> PathBuf {
        if cfg!(target_os = "macos") {
            self.home().join("Library/Application Support/overseer")
        } else {
            self.home().join(".local/state/overseer")
        }
    }

    fn other_state(&self) -> PathBuf {
        self.base.join("dev-state")
    }

    fn settings(&self) -> String {
        std::fs::read_to_string(self.home().join(".claude/settings.json")).unwrap_or_default()
    }

    fn shim(&self) -> PathBuf {
        self.home().join(".overseer/bin/overseer")
    }

    fn cmd(&self, state: &Path) -> Command {
        let mut c = Command::new(bin());
        c.env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", self.home())
            .env("LANG", "C.UTF-8")
            .env("XDG_RUNTIME_DIR", self.base.join("run"))
            .env("CLAUDE_CONFIG_DIR", self.home().join(".claude"))
            .env("CODEX_HOME", self.home().join(".codex"))
            .env("OVERSEER_STATE_DIR", state)
            .env("OVERSEER_SOCKET", &self.sock)
            .env(
                "OVERSEER_TMUX_SOCKET",
                format!("sb-hooks-owner-{}", std::process::id()),
            )
            .env("OVERSEER_LOGIN_ENV", "0")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        die_with_parent(&mut c);
        c
    }

    fn install_hooks(&self, state: &Path) {
        std::fs::create_dir_all(state).unwrap();
        let o = output(
            self.cmd(state)
                .args(["hooks", "install", "claude", "--apply"]),
            None,
        );
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }

    fn daemon(&self, state: &Path) -> (Reap, PathBuf) {
        std::fs::create_dir_all(state).unwrap();
        let log = self.base.join(format!(
            "daemon-{}.log",
            state.file_name().unwrap().to_string_lossy()
        ));
        let f = std::fs::File::create(&log).unwrap();
        let _ = std::fs::remove_file(&self.sock);
        let mut c = self.cmd(state);
        c.args(["daemon", "--interval", "1"]).stderr(Stdio::from(f));
        let r = Reap::spawn(&mut c);
        wait(|| UnixStream::connect(&self.sock).is_ok());
        (r, log)
    }
}

fn names(text: &str, state: &Path) -> bool {
    text.contains(&format!("OVERSEER_STATE_DIR='{}'", state.display()))
        || text.contains(&format!("OVERSEER_STATE_DIR={} ", state.display()))
}

#[test]
fn dev_daemon_leaves_hooks_of_the_default_state_dir_alone() {
    let b = Sbx::new("keep");
    b.install_hooks(&b.default_state());
    let before = b.settings();
    assert!(names(&before, &b.default_state()));
    let (_d, log) = b.daemon(&b.other_state());
    wait(|| {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("not repairing hooks")
    });
    assert_eq!(b.settings(), before);
    assert!(!b.settings().contains("dev-state"));
}

#[test]
fn dev_daemon_does_not_repoint_the_cli_shim() {
    let b = Sbx::new("shim");
    b.install_hooks(&b.default_state());
    let shim = b.shim();
    let installed = b.base.join("installed-overseer");
    std::fs::write(&installed, "x").unwrap();
    std::fs::remove_file(&shim).unwrap();
    std::os::unix::fs::symlink(&installed, &shim).unwrap();
    let (_d, log) = b.daemon(&b.other_state());
    wait(|| {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("not repairing hooks")
    });
    assert_eq!(std::fs::read_link(&shim).unwrap(), installed);
}

#[test]
fn default_state_daemon_heals_hooks_hijacked_by_another_state_dir() {
    let b = Sbx::new("heal");
    b.install_hooks(&b.other_state());
    assert!(names(&b.settings(), &b.other_state()));
    let (_d, _log) = b.daemon(&b.default_state());
    wait(|| names(&b.settings(), &b.default_state()));
    assert!(!b.settings().contains("dev-state"));
}

#[test]
fn default_state_daemon_repoints_the_cli_shim() {
    let b = Sbx::new("healshim");
    b.install_hooks(&b.other_state());
    let shim = b.shim();
    let stale = b.base.join("stale-overseer");
    std::fs::write(&stale, "x").unwrap();
    std::fs::remove_file(&shim).unwrap();
    std::os::unix::fs::symlink(&stale, &shim).unwrap();
    let (_d, _log) = b.daemon(&b.default_state());
    wait(|| std::fs::read_link(&shim).is_ok_and(|p| p != stale));
}

#[test]
fn sandbox_with_its_own_home_installs_and_keeps_its_hooks() {
    let b = Sbx::new("sandbox");
    let state = b.other_state();
    assert!(b.settings().is_empty());
    let (d, log) = b.daemon(&state);
    wait(|| b.shim().exists());
    let (code, body) = exchange(
        UnixStream::connect(&b.sock).unwrap(),
        "POST",
        "/v1/hooks/install",
        r#"{"agents":["claude"],"dryRun":false}"#,
        "unix",
    );
    assert_eq!(code, 200, "{body}");
    assert!(names(&b.settings(), &state));
    drop(d);
    let _ = std::fs::remove_file(&b.sock);
    let (_d2, log2) = b.daemon(&state);
    let _ = wait_quiet(&log2);
    assert!(names(&b.settings(), &state));
    let text = std::fs::read_to_string(&log).unwrap() + &std::fs::read_to_string(&log2).unwrap();
    assert!(!text.contains("not repairing hooks"), "{text}");
}

fn wait_quiet(_log: &Path) -> bool {
    std::thread::sleep(std::time::Duration::from_millis(500));
    true
}
