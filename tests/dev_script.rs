#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Dev {
    base: PathBuf,
}

impl Dev {
    fn new(name: &str) -> Dev {
        let base = std::env::temp_dir().join(format!("dsh-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("home/.overseer/bin")).unwrap();
        std::fs::write(base.join("home/.overseer/bin/overseer"), "installed").unwrap();
        std::fs::write(base.join("home/.gitconfig"), "[user]\n\tname = dev\n").unwrap();
        Dev { base }
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/dev.sh"))
            .args(args)
            .env("HOME", self.base.join("home"))
            .env("OVERSEER_DEV_DIR", self.base.join("dev"))
            .env("OVERSEER_DEV_BIN", env!("CARGO_BIN_EXE_overseer"))
            .env("OVERSEER_DEV_TMUX", self.base.file_name().unwrap())
            .env("XDG_STATE_HOME", self.base.join("home/.local/state"))
            .env("OVERSEER_STATE_DIR", self.base.join("real-state"))
            .output()
            .unwrap()
    }

    fn pid(&self) -> Option<String> {
        std::fs::read_to_string(self.base.join("dev/state/daemon.pid")).ok()
    }
}

impl Drop for Dev {
    fn drop(&mut self) {
        let _ = self.run(&["reset"]);
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn ok(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success()
}

#[test]
fn up_starts_an_isolated_daemon_and_down_stops_it() {
    let d = Dev::new("updown");
    let o = d.run(&["up"]);
    let log = std::fs::read_to_string(d.base.join("dev/state/daemon.log")).unwrap_or_default();
    assert!(
        o.status.success(),
        "{}\n{log}",
        String::from_utf8_lossy(&o.stderr)
    );
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains("dev/state/daemon.sock"), "{out}");
    let sock = d.base.join("dev/state/daemon.sock");
    assert!(std::os::unix::net::UnixStream::connect(&sock).is_ok());
    let pid = d.pid().expect("daemon.pid");
    assert!(alive(&pid));
    assert!(ok(&d.run(&["status"])).contains("running"));

    assert_eq!(
        std::fs::read_to_string(d.base.join("home/.overseer/bin/overseer")).unwrap(),
        "installed"
    );
    assert!(!d.base.join("real-state").exists());
    assert!(!d.base.join("home/.local/state/overseer").exists());
    assert!(d.base.join("dev/home/.gitconfig").is_file());

    ok(&d.run(&["up"]));
    assert_eq!(d.pid().as_deref(), Some(pid.as_str()));

    ok(&d.run(&["down"]));
    assert!(!alive(&pid));
    assert!(!d.run(&["status"]).status.success());
}

#[test]
fn reset_removes_the_dev_dir() {
    let d = Dev::new("reset");
    ok(&d.run(&["up"]));
    let pid = d.pid().unwrap();
    ok(&d.run(&["reset"]));
    assert!(!alive(&pid));
    assert!(!d.base.join("dev").exists());
    assert!(d.base.join("home/.overseer/bin/overseer").is_file());
}

#[test]
fn app_refuses_without_the_dev_daemon() {
    let d = Dev::new("app");
    let o = d.run(&["app"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("scripts/dev.sh up"));
}

#[test]
fn unknown_command_prints_usage_and_fails() {
    let d = Dev::new("usage");
    let o = d.run(&["nope"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stdout).contains("usage: scripts/dev.sh"));
}
