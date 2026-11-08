use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const UNIT: &str = "overseer-daemon.service";
pub const LABEL: &str = "dev.overseer.daemon";
pub const SUPERVISED: &str = "OVERSEER_SUPERVISED";
pub const LINGER_FLAG: &str = "linger-enabled-by-overseer";

pub type Run<'a> = &'a dyn Fn(&str, &[&str]) -> Result<String, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Systemd,
    Launchd,
    Unsupported,
}

impl Kind {
    pub fn detect() -> Kind {
        if cfg!(target_os = "macos") {
            Kind::Launchd
        } else if cfg!(target_os = "linux") && Path::new("/run/systemd/system").is_dir() {
            Kind::Systemd
        } else {
            Kind::Unsupported
        }
    }

    fn name(self) -> &'static str {
        match self {
            Kind::Systemd => "systemd",
            Kind::Launchd => "launchd",
            Kind::Unsupported => "none",
        }
    }
}

pub struct Env {
    pub kind: Kind,
    pub home: PathBuf,
    pub config_home: PathBuf,
    pub shim: PathBuf,
    pub log: PathBuf,
    pub user: String,
}

impl Env {
    pub fn current(state_dir: &Path) -> Env {
        let home = crate::home();
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Env {
            kind: Kind::detect(),
            config_home,
            shim: crate::install::shim_path(),
            log: state_dir.join("daemon.log"),
            user: user_name(),
            home,
        }
    }

    pub fn path(&self) -> Option<PathBuf> {
        match self.kind {
            Kind::Systemd => Some(self.config_home.join("systemd/user").join(UNIT)),
            Kind::Launchd => Some(
                self.home
                    .join("Library/LaunchAgents")
                    .join(format!("{LABEL}.plist")),
            ),
            Kind::Unsupported => None,
        }
    }

    pub fn text(&self) -> Option<String> {
        match self.kind {
            Kind::Systemd => Some(unit_text(&self.shim)),
            Kind::Launchd => Some(plist_text(&self.shim, &self.log)),
            Kind::Unsupported => None,
        }
    }
}

fn user_name() -> String {
    ["USER", "LOGNAME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .or_else(|| {
            run_cmd("id", &["-un"])
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default()
}

fn systemd_quote(p: &Path) -> String {
    let s = p.to_string_lossy().replace('%', "%%");
    if s.chars()
        .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s
    }
}

pub fn unit_text(shim: &Path) -> String {
    format!(
        "[Unit]\n\
         Description=Overseer server\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} daemon\n\
         Environment={SUPERVISED}=1\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         OOMPolicy=continue\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        systemd_quote(shim)
    )
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn plist_text(shim: &Path, log: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>daemon</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>{SUPERVISED}</key>
    <string>1</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml(&shim.to_string_lossy()),
        xml(&log.to_string_lossy())
    )
}

pub fn linger_command(user: &str) -> String {
    if user.is_empty() {
        "loginctl enable-linger".into()
    } else {
        format!("loginctl enable-linger {user}")
    }
}

pub fn linger(env: &Env, run: Run) -> Option<bool> {
    if env.kind != Kind::Systemd {
        return None;
    }
    if !env.user.is_empty()
        && Path::new("/var/lib/systemd/linger")
            .join(&env.user)
            .exists()
    {
        return Some(true);
    }
    if env.user.is_empty() {
        return None;
    }
    match run(
        "loginctl",
        &["show-user", &env.user, "--property=Linger", "--value"],
    ) {
        Ok(v) => Some(v.trim() == "yes"),
        Err(_) => None,
    }
}

pub fn status(env: &Env, run: Run) -> Value {
    let path = env.path();
    let current = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok());
    let installed = current.is_some();
    let enabled = match env.kind {
        Kind::Systemd if installed => {
            run("systemctl", &["--user", "is-enabled", UNIT]).is_ok_and(|s| s.trim() == "enabled")
        }
        Kind::Launchd => installed,
        _ => false,
    };
    let linger = linger(env, run);
    json!({
        "platform": env.kind.name(),
        "supported": env.kind != Kind::Unsupported,
        "installed": installed,
        "enabled": enabled,
        "current": installed && current == env.text(),
        "supervised": std::env::var(SUPERVISED).is_ok_and(|v| v == "1"),
        "path": path.map(|p| p.to_string_lossy().into_owned()),
        "linger": linger,
        "lingerCommand": (env.kind == Kind::Systemd).then(|| linger_command(&env.user)),
    })
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("sb-tmp");
    std::fs::write(&tmp, text)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn enable(env: &Env, run: Run) -> Result<(), String> {
    let (Some(path), Some(text)) = (env.path(), env.text()) else {
        return Err("starting the server at boot is not supported on this system".into());
    };
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        write_atomic(&path, &text)?;
    }
    if env.kind == Kind::Systemd {
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", UNIT])?;
    }
    Ok(())
}

pub fn refresh(env: &Env, run: Run) -> Result<bool, String> {
    let (Some(path), Some(text)) = (env.path(), env.text()) else {
        return Ok(false);
    };
    match std::fs::read_to_string(&path) {
        Ok(current) if current != text => {}
        _ => return Ok(false),
    }
    write_atomic(&path, &text)?;
    if env.kind == Kind::Systemd {
        run("systemctl", &["--user", "daemon-reload"])?;
    }
    Ok(true)
}

pub fn disable(env: &Env, run: Run) -> Result<(), String> {
    let Some(path) = env.path() else {
        return Ok(());
    };
    if env.kind == Kind::Systemd && path.exists() {
        let _ = run("systemctl", &["--user", "disable", UNIT]);
    }
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{}: {e}", path.display())),
    }
    if env.kind == Kind::Systemd {
        let _ = run("systemctl", &["--user", "daemon-reload"]);
    }
    Ok(())
}

pub fn linger_flag(state_dir: &Path) -> PathBuf {
    state_dir.join(LINGER_FLAG)
}

pub fn enable_linger(env: &Env, run: Run, state_dir: &Path) -> Result<(), String> {
    if env.kind != Kind::Systemd {
        return Ok(());
    }
    let was_off = linger(env, run) == Some(false);
    run("loginctl", &["enable-linger", "--no-ask-password"]).map(|_| ())?;
    match linger(env, run) {
        Some(true) => {
            if was_off {
                let _ = crate::mkdir_private(state_dir)
                    .and_then(|_| std::fs::write(linger_flag(state_dir), ""));
            }
            Ok(())
        }
        _ => Err(format!(
            "linger is still off; run {} on the server",
            linger_command(&env.user)
        )),
    }
}

pub fn disable_linger_if_ours(env: &Env, run: Run, state_dir: &Path) -> Result<bool, String> {
    if env.kind != Kind::Systemd || !linger_flag(state_dir).exists() {
        return Ok(false);
    }
    run("loginctl", &["disable-linger", "--no-ask-password"])?;
    let _ = std::fs::remove_file(linger_flag(state_dir));
    Ok(true)
}

pub fn unload(env: &Env, run: Run) -> Result<(), String> {
    if env.kind != Kind::Launchd {
        return Ok(());
    }
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = 0;
    match run("launchctl", &["bootout", &format!("gui/{uid}/{LABEL}")]) {
        Err(e) if !not_loaded(&e) => Err(e),
        _ => Ok(()),
    }
}

fn not_loaded(e: &str) -> bool {
    let e = e.to_lowercase();
    [
        "could not find",
        "no such process",
        "not found",
        "not loaded",
    ]
    .iter()
    .any(|m| e.contains(m))
        || e.contains("failed: 3:")
        || e.contains("failed: 113:")
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<String, String> {
    let mut c = std::process::Command::new(cmd);
    c.args(args)
        .stdin(std::process::Stdio::null())
        .env("SYSTEMD_PAGER", "");
    #[cfg(unix)]
    if std::env::var_os("XDG_RUNTIME_DIR").is_none() {
        c.env(
            "XDG_RUNTIME_DIR",
            format!("/run/user/{}", unsafe { libc::getuid() }),
        );
    }
    let out = crate::git::output_within(&mut c, std::time::Duration::from_secs(60))
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let out_text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Err(format!(
            "{cmd} {}: {}",
            args.join(" "),
            if err.is_empty() { out_text } else { err }
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn env(kind: Kind, tag: &str) -> Env {
        let home = std::env::temp_dir().join(format!(
            "sb-autostart-{tag}-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&home);
        Env {
            kind,
            config_home: home.join(".config"),
            shim: home.join(".overseer/bin/overseer"),
            log: home.join("state/daemon.log"),
            user: "nobody-sb-test".into(),
            home,
        }
    }

    #[test]
    fn unit_points_at_the_shim_and_restarts_only_on_failure() {
        let u = unit_text(Path::new("/home/u/.overseer/bin/overseer"));
        assert!(u.contains("ExecStart=/home/u/.overseer/bin/overseer daemon\n"));
        assert!(u.contains("Restart=on-failure\n"));
        assert!(u.contains("OOMPolicy=continue\n"));
        assert!(u.contains("Environment=OVERSEER_SUPERVISED=1\n"));
        assert!(u.contains("WantedBy=default.target\n"));
        let spaced = unit_text(Path::new("/home/a b/.overseer/bin/overseer"));
        assert!(spaced.contains("ExecStart=\"/home/a b/.overseer/bin/overseer\" daemon"));
        let p = plist_text(
            Path::new("/Users/u/.overseer/bin/overseer"),
            Path::new("/x/&.log"),
        );
        assert!(p.contains("<string>/Users/u/.overseer/bin/overseer</string>"));
        assert!(p.contains("<key>SuccessfulExit</key>\n    <false/>"));
        assert!(p.contains("/x/&amp;.log"));
    }

    #[test]
    fn enable_is_idempotent_and_never_starts_the_unit() {
        let e = env(Kind::Systemd, "enable");
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| -> Result<String, String> {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            Ok(if a.contains(&"is-enabled") {
                "enabled\n"
            } else {
                "no\n"
            }
            .into())
        };
        enable(&e, &run).unwrap();
        enable(&e, &run).unwrap();
        let path = e.path().unwrap();
        assert_eq!(
            path,
            e.home.join(".config/systemd/user/overseer-daemon.service")
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), unit_text(&e.shim));
        let c = calls.borrow().clone();
        assert!(
            c.iter()
                .all(|x| !x.contains(" start") && !x.contains("--now")),
            "{c:?}"
        );
        assert_eq!(
            c.iter()
                .filter(|x| x.ends_with("enable overseer-daemon.service"))
                .count(),
            2
        );
        let s = status(&e, &run);
        assert_eq!(s["installed"], true);
        assert_eq!(s["enabled"], true);
        assert_eq!(s["current"], true);
        assert_eq!(s["linger"], false);
        assert_eq!(s["lingerCommand"], "loginctl enable-linger nobody-sb-test");
        disable(&e, &run).unwrap();
        assert!(!path.exists());
        disable(&e, &run).unwrap();
        let _ = std::fs::remove_dir_all(&e.home);
    }

    #[test]
    fn refresh_rewrites_an_outdated_unit_and_leaves_a_missing_or_current_one_alone() {
        let e = env(Kind::Systemd, "refresh");
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| -> Result<String, String> {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            Ok(String::new())
        };
        let path = e.path().unwrap();
        assert_eq!(refresh(&e, &run), Ok(false));
        assert!(
            !path.exists(),
            "never installs a unit the user did not enable"
        );
        let old = unit_text(&e.shim).replace("OOMPolicy=continue\n", "");
        write_atomic(&path, &old).unwrap();
        assert_eq!(refresh(&e, &run), Ok(true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), unit_text(&e.shim));
        assert_eq!(*calls.borrow(), ["systemctl --user daemon-reload"]);
        assert_eq!(refresh(&e, &run), Ok(false));
        assert_eq!(calls.borrow().len(), 1);
        let _ = std::fs::remove_dir_all(&e.home);
    }

    #[test]
    fn linger_failure_reports_the_command_instead_of_escalating() {
        let e = env(Kind::Systemd, "linger");
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| -> Result<String, String> {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            if a.first() == Some(&"enable-linger") {
                Err("Interactive authentication required.".into())
            } else {
                Ok("no".into())
            }
        };
        let err = enable_linger(&e, &run, &e.home.join("state")).unwrap_err();
        assert!(err.contains("Interactive authentication required"), "{err}");
        let c = calls.borrow().clone();
        assert!(c.iter().all(|x| !x.contains("sudo")), "{c:?}");
        assert!(c
            .iter()
            .any(|x| x.contains("enable-linger") && x.contains("--no-ask-password")));
    }

    #[test]
    fn linger_is_flagged_only_when_overseer_turned_it_on() {
        let e = env(Kind::Systemd, "linger-flag");
        let state = e.home.join("state");
        let off = |on: bool| {
            let calls = RefCell::new(vec![]);
            let state_on = std::cell::Cell::new(on);
            let run = move |c: &str, a: &[&str]| -> Result<String, String> {
                calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
                if a.first() == Some(&"enable-linger") {
                    state_on.set(true);
                    return Ok(String::new());
                }
                Ok(if state_on.get() { "yes" } else { "no" }.into())
            };
            run
        };
        enable_linger(&e, &off(true), &state).unwrap();
        assert!(!linger_flag(&state).exists());
        enable_linger(&e, &off(false), &state).unwrap();
        assert!(linger_flag(&state).exists());
        let _ = std::fs::remove_dir_all(&e.home);
    }

    #[test]
    fn disable_linger_runs_only_when_the_flag_exists() {
        let e = env(Kind::Systemd, "linger-undo");
        let state = e.home.join("state");
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| -> Result<String, String> {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            Ok(String::new())
        };
        assert_eq!(disable_linger_if_ours(&e, &run, &state), Ok(false));
        assert!(calls.borrow().is_empty());
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(linger_flag(&state), "").unwrap();
        assert_eq!(disable_linger_if_ours(&e, &run, &state), Ok(true));
        assert_eq!(
            *calls.borrow(),
            ["loginctl disable-linger --no-ask-password"]
        );
        assert!(!linger_flag(&state).exists());
        let failing = |_: &str, _: &[&str]| -> Result<String, String> { Err("denied".into()) };
        std::fs::write(linger_flag(&state), "").unwrap();
        assert!(disable_linger_if_ours(&e, &failing, &state).is_err());
        assert!(linger_flag(&state).exists());
        let launchd = env(Kind::Launchd, "linger-launchd");
        assert_eq!(disable_linger_if_ours(&launchd, &run, &state), Ok(false));
        let _ = std::fs::remove_dir_all(&e.home);
    }

    #[test]
    fn unload_boots_the_launch_agent_out_and_ignores_one_that_is_not_loaded() {
        let e = env(Kind::Launchd, "unload");
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| -> Result<String, String> {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            Ok(String::new())
        };
        unload(&e, &run).unwrap();
        let c = calls.borrow()[0].clone();
        assert!(c.starts_with("launchctl bootout gui/") && c.ends_with("/dev.overseer.daemon"));
        let missing = |_: &str, _: &[&str]| -> Result<String, String> {
            Err("Boot-out failed: 3: No such process".into())
        };
        assert!(unload(&e, &missing).is_ok());
        let broken = |_: &str, _: &[&str]| -> Result<String, String> { Err("denied".into()) };
        assert!(unload(&e, &broken).is_err());
        let sd = env(Kind::Systemd, "unload-sd");
        assert!(unload(&sd, &broken).is_ok());
        let _ = std::fs::remove_dir_all(&e.home);
    }

    #[test]
    fn launchd_writes_a_login_agent_and_unsupported_refuses() {
        let e = env(Kind::Launchd, "launchd");
        let run = |_: &str, _: &[&str]| -> Result<String, String> { Err("no".into()) };
        enable(&e, &run).unwrap();
        let path = e.path().unwrap();
        assert!(path.ends_with("Library/LaunchAgents/dev.overseer.daemon.plist"));
        assert_eq!(status(&e, &run)["enabled"], true);
        assert_eq!(status(&e, &run)["linger"], Value::Null);
        disable(&e, &run).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&e.home);
        let none = env(Kind::Unsupported, "none");
        assert!(enable(&none, &run).is_err());
        assert_eq!(status(&none, &run)["supported"], false);
    }
}
