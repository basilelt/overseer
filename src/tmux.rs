use crate::procs::Probe;
use std::process::{Command, Stdio};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
pub struct Tmux {
    pub socket: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Pane {
    pub id: String,
    pub pid: u32,
    pub session: String,
    pub attached: bool,
    pub held: Option<String>,
    pub claude_session: Option<String>,
    pub dead: bool,
    pub command: String,
}

const FMT: &str = "#{pane_id}\t#{pane_pid}\t#{session_name}\t#{session_attached}\t#{@sb_held}\t#{@claude_session}\t#{pane_dead}\t#{pane_current_command}";

impl Tmux {
    pub fn from_env() -> Tmux {
        Tmux {
            socket: std::env::var("OVERSEER_TMUX_SOCKET")
                .ok()
                .filter(|s| !s.is_empty()),
        }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new("tmux");
        if let Some(s) = &self.socket {
            c.args(["-L", s]);
        }
        c.stdin(Stdio::null()).stderr(Stdio::null());
        c
    }

    pub fn run(&self, args: &[&str]) -> Option<String> {
        crate::exec::stdout_ok(self.cmd().args(args), TIMEOUT)
    }

    pub fn ok(&self, args: &[&str]) -> Result<(), String> {
        self.run(args)
            .map(|_| ())
            .ok_or_else(|| format!("tmux {} failed", args.first().unwrap_or(&"")))
    }

    pub fn load_buffer(&self, name: &str, text: &str) -> Result<(), String> {
        use std::io::Write;
        let mut c = self.cmd();
        let mut child = crate::exec::Watchdog::guard(c.args(["load-buffer", "-b", name, "-"]))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| format!("tmux load-buffer: {e}"))?;
        let dog = crate::exec::Watchdog::start(child.id(), TIMEOUT);
        let wrote = child
            .stdin
            .take()
            .map_or(Ok(()), |mut w| w.write_all(text.as_bytes()));
        let status = child.wait();
        if dog.finish() {
            return Err("tmux load-buffer timed out".into());
        }
        wrote.map_err(|e| format!("tmux load-buffer: {e}"))?;
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => Err("tmux load-buffer failed".into()),
        }
    }

    pub fn version(&self) -> Option<String> {
        self.run(&["-V"]).map(|s| s.trim().to_string())
    }

    #[cfg(unix)]
    fn no_server(&self) -> bool {
        if self.socket.is_none() && std::env::var_os("TMUX").is_some_and(|v| !v.is_empty()) {
            return false;
        }
        let dir = std::env::var_os("TMUX_TMPDIR")
            .filter(|d| !d.is_empty())
            .map_or_else(
                || std::path::PathBuf::from("/tmp"),
                std::path::PathBuf::from,
            );
        let uid = unsafe { libc::getuid() };
        let name = self.socket.as_deref().unwrap_or("default");
        !dir.join(format!("tmux-{uid}")).join(name).exists()
    }

    #[cfg(not(unix))]
    fn no_server(&self) -> bool {
        false
    }

    pub fn panes(&self) -> Vec<Pane> {
        if self.no_server() {
            return vec![];
        }
        self.run(&["list-panes", "-a", "-F", FMT])
            .unwrap_or_default()
            .lines()
            .filter_map(parse_pane)
            .collect()
    }

    pub fn capture(&self, pane: &str) -> Option<String> {
        self.run(&["capture-pane", "-p", "-e", "-J", "-t", pane])
    }

    pub fn pane_option(&self, pane: &str, name: &str) -> Option<String> {
        self.run(&["show-options", "-p", "-v", "-t", pane, name])
            .map(|s| s.trim_end().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn set_pane_option(
        &self,
        pane: &str,
        name: &str,
        value: Option<&str>,
    ) -> Result<(), String> {
        match value {
            Some(v) => self.ok(&["set-option", "-p", "-t", pane, name, v]),
            None => self.ok(&["set-option", "-p", "-u", "-t", pane, name]),
        }
    }

    pub fn session_of(&self, pane: &str) -> Option<String> {
        self.run(&["display-message", "-p", "-t", pane, "#{session_name}"])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn attach(&self, session: &str) -> std::io::Error {
        use crate::sys::CommandExt;
        let mut c = Command::new("tmux");
        if let Some(s) = &self.socket {
            c.args(["-L", s]);
        }
        c.args(["attach-session", "-t", session]).exec()
    }
}

fn parse_pane(l: &str) -> Option<Pane> {
    let f: Vec<&str> = l.split('\t').collect();
    if f.len() < 8 {
        return None;
    }
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Some(Pane {
        id: f[0].to_string(),
        pid: f[1].parse().ok()?,
        session: f[2].to_string(),
        attached: f[3].parse::<u32>().unwrap_or(0) > 0,
        held: opt(f[4]),
        claude_session: opt(f[5]),
        dead: f[6] == "1",
        command: f[7].to_string(),
    })
}

pub fn pane_of<'a>(pid: u32, panes: &'a [Pane], probe: &dyn Probe) -> Option<&'a Pane> {
    let mut p = pid;
    for _ in 0..16 {
        if let Some(pane) = panes.iter().find(|x| x.pid == p && !x.dead) {
            return Some(pane);
        }
        p = probe.ppid(p).filter(|n| *n > 1)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn no_server_means_no_fork() {
        let t = Tmux {
            socket: Some(format!("sb-absent-{}", std::process::id())),
        };
        assert!(t.no_server());
        assert!(t.panes().is_empty());
    }

    #[test]
    fn parses_pane_rows() {
        let p = parse_pane("%3\t123\twork-AB\t0\tvm-01X\t\t0\tclaude").unwrap();
        assert_eq!((p.id.as_str(), p.pid, p.attached), ("%3", 123, false));
        assert_eq!(p.held.as_deref(), Some("vm-01X"));
        assert!(p.claude_session.is_none() && !p.dead);
        assert!(parse_pane("bad").is_none());
    }
}
