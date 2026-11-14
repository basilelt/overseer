use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};

#[cfg(unix)]
use std::path::{Path, PathBuf};

static OUTCOME: OnceLock<(&'static str, String)> = OnceLock::new();
#[cfg(unix)]
static AT_START: OnceLock<Vec<PathBuf>> = OnceLock::new();
static EXTRA: Mutex<Vec<String>> = Mutex::new(vec![]);

// Install dirs created after start join the PATH of detection and new terms. Kept apart from
// the process env: set_var is not thread-safe once the daemon runs.
pub fn extended() -> Option<String> {
    let extra = EXTRA.lock().unwrap_or_else(|e| e.into_inner());
    (!extra.is_empty()).then(|| merge_path(&path_var(), &extra.join(":")))
}

pub fn path() -> String {
    extended().unwrap_or_else(path_var)
}

#[cfg(not(unix))]
pub fn refresh() {}

#[cfg(unix)]
pub fn refresh() {
    let home = crate::home();
    let at_start = AT_START.get().map(Vec::as_slice).unwrap_or_default();
    let new = new_dirs(&home, &path(), at_start);
    if !new.is_empty() {
        EXTRA
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .extend(new.iter().map(|p| p.to_string_lossy().into_owned()));
    }
}

#[cfg(unix)]
pub fn new_dirs(home: &Path, path: &str, at_start: &[PathBuf]) -> Vec<PathBuf> {
    fallback_dirs(home, path)
        .into_iter()
        .filter(|d| !at_start.contains(d))
        .collect()
}

fn path_var() -> String {
    std::env::var("PATH").unwrap_or_default()
}

// Markers a Claude Code or Codex session sets on its own children. A daemon started from one
// (then kept by takeovers) would hand them to every terminal: a resumed claude stops saving.
const AGENT_SESSION: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_PLUGIN_DATA",
    "CLAUDE_PLUGIN_ROOT",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_ENV_FILE",
    "CODEX_COMPANION_SESSION_ID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
];

pub fn session_bound(k: &str, v: &str) -> bool {
    match k {
        "SSH_CONNECTION" | "SSH_CLIENT" | "SSH_TTY" | "TMUX" | "TMUX_PANE" | "STY" => true,
        _ if AGENT_SESSION.contains(&k) => true,
        "SSH_AUTH_SOCK" => v
            .strip_prefix("/tmp/ssh-")
            .and_then(|rest| rest.split_once('/'))
            .is_some_and(|(dir, file)| !dir.is_empty() && file.starts_with("agent.")),
        _ => false,
    }
}

pub fn term_env() -> [(&'static str, &'static str); 5] {
    [
        ("TERM", "xterm-256color"),
        ("COLORTERM", "truecolor"),
        ("TERM_PROGRAM", "overseer"),
        ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION")),
        ("OVERSEER_TERM", "1"),
    ]
}

fn scrub() {
    for (k, v) in std::env::vars_os() {
        let (Some(k), Some(v)) = (k.to_str(), v.to_str()) else {
            continue;
        };
        if session_bound(k, v) {
            std::env::remove_var(k);
        }
    }
}

pub fn init() {
    scrub();
    #[cfg(unix)]
    let _ = AT_START.set(fallback_dirs(&crate::home(), ""));
    let source = load();
    let _ = OUTCOME.set((source, path_var()));
}

pub fn to_json() -> Value {
    let (source, path) = OUTCOME
        .get()
        .cloned()
        .unwrap_or_else(|| ("inherited", path_var()));
    let path = extended().unwrap_or(path);
    json!({"path": path, "source": source})
}

#[cfg(not(unix))]
fn load() -> &'static str {
    "inherited"
}

#[cfg(unix)]
fn load() -> &'static str {
    if std::env::var("OVERSEER_LOGIN_ENV").is_ok_and(|v| v == "0") {
        return "inherited";
    }
    let original = path_var();
    let vars = probe().filter(|v| v.iter().any(|(k, _)| k == "PATH"));
    if let Some(vars) = vars {
        for (k, v) in &vars {
            if k == "PATH" {
                std::env::set_var("PATH", merge_path(v, &original));
            } else if keep(k) && !session_bound(k, v) {
                std::env::set_var(k, v);
            }
        }
        return "login-shell";
    }
    let extra: Vec<String> = fallback_dirs(&crate::home(), &original)
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    std::env::set_var("PATH", merge_path(&original, &extra.join(":")));
    "fallback"
}

#[cfg(unix)]
pub fn keep(k: &str) -> bool {
    !k.is_empty()
        && !matches!(
            k,
            "TMUX" | "TMUX_PANE" | "TERM" | "SHLVL" | "PWD" | "OLDPWD" | "_" | "SB_MARK"
        )
        && !k.starts_with("CMUX_")
        && !k.starts_with("OVERSEER_")
        && !k.starts_with("BASH_FUNC_")
}

pub fn merge_path(primary: &str, extra: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for p in primary.split(':').chain(extra.split(':')) {
        if !p.is_empty() && !out.contains(&p) {
            out.push(p);
        }
    }
    out.join(":")
}

#[cfg(unix)]
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(unix)]
pub fn parse_env(output: &[u8], marker: &str) -> Option<Vec<(String, String)>> {
    let start = find(output, marker.as_bytes())? + marker.len();
    let rest = &output[start..];
    let end = rest.iter().rposition(|b| *b == 0)?;
    Some(
        rest[..end]
            .split(|b| *b == 0)
            .filter_map(|e| {
                let s = String::from_utf8_lossy(e);
                let (k, v) = s.split_once('=')?;
                (!k.is_empty()).then(|| (k.to_string(), v.to_string()))
            })
            .collect(),
    )
}

#[cfg(unix)]
fn version(name: &str) -> Option<Vec<u64>> {
    name.strip_prefix('v')
        .unwrap_or(name)
        .split('.')
        .map(|p| p.parse().ok())
        .collect()
}

#[cfg(unix)]
fn newest_node(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let v = version(e.file_name().to_str()?)?;
            let bin = e.path().join("bin");
            bin.is_dir().then_some((v, bin))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p)
}

#[cfg(unix)]
pub fn fallback_dirs(home: &Path, existing_path: &str) -> Vec<PathBuf> {
    let have: Vec<&str> = existing_path.split(':').collect();
    let mut dirs = vec![
        home.join(crate::paths::USER_BIN),
        home.join(".cargo/bin"),
        home.join(".bun/bin"),
        home.join(".deno/bin"),
    ];
    dirs.extend(newest_node(&home.join(".nvm/versions/node")));
    dirs.extend([
        home.join(".volta/bin"),
        home.join(".asdf/shims"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    let mut out: Vec<PathBuf> = vec![];
    for d in dirs {
        let known = d.to_str().is_some_and(|s| have.contains(&s));
        if d.is_dir() && !known && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[cfg(unix)]
fn probe() -> Option<Vec<(String, String)>> {
    use crate::sys::CommandExt;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let marker = format!("__SB_ENV_{}_{nanos}__", std::process::id());
    let done = format!("{marker}END__");
    let mut c = Command::new(&shell);
    c.args([
        "-l",
        "-i",
        "-c",
        "printf %s \"$SB_MARK\"; env -0; printf %sEND__ \"$SB_MARK\"",
    ])
    .env("SB_MARK", &marker)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = c.spawn().ok()?;
    let mut out = child.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut b = [0u8; 8192];
        while let Ok(n) = out.read(&mut b) {
            if n == 0 || tx.send(b[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut buf = vec![];
    let ok = loop {
        match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(chunk) => {
                buf.extend_from_slice(&chunk);
                if find(&buf, done.as_bytes()).is_some() {
                    break true;
                }
            }
            Err(_) => break false,
        }
    };
    if !ok {
        unsafe {
            libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
        }
    }
    let grace = Instant::now() + Duration::from_secs(1);
    while matches!(child.try_wait(), Ok(None)) && Instant::now() < grace {
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    if !ok {
        return None;
    }
    parse_env(&buf, &marker)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn parses_the_dump_after_the_marker() {
        let m = "__SB_ENV_1_2__";
        let mut out = b"welcome to zsh\nPATH=/noise\0\n".to_vec();
        out.extend_from_slice(m.as_bytes());
        out.extend_from_slice(
            b"PATH=/a:/b\0SB_MARK=__SB_ENV_1_2__\0EQ=x=y\0ML=line1\nline2\0=bad\0noeq\0",
        );
        out.extend_from_slice(format!("{m}END__bye=1").as_bytes());
        let v = parse_env(&out, m).unwrap();
        let get = |k: &str| v.iter().find(|(a, _)| a == k).map(|(_, b)| b.as_str());
        assert_eq!(get("PATH"), Some("/a:/b"));
        assert_eq!(get("EQ"), Some("x=y"));
        assert_eq!(get("ML"), Some("line1\nline2"));
        assert_eq!(get("SB_MARK"), Some(m));
        assert_eq!(v.len(), 4, "{v:?}");
        assert!(!keep("SB_MARK"));
        assert!(parse_env(b"PATH=/x\0", m).is_none());
    }

    #[test]
    fn keeps_only_safe_keys() {
        for k in ["PATH", "NVM_DIR", "SSH_AUTH_SOCK", "LANG"] {
            assert!(keep(k), "{k}");
        }
        for k in [
            "TMUX",
            "TMUX_PANE",
            "TERM",
            "SHLVL",
            "PWD",
            "OLDPWD",
            "_",
            "CMUX_SOCKET",
            "OVERSEER_STATE_DIR",
            "",
        ] {
            assert!(!keep(k), "{k}");
        }
    }

    #[test]
    fn merges_paths_in_order() {
        assert_eq!(merge_path("/a:/b", "/b:/c::/a:/d"), "/a:/b:/c:/d");
        assert_eq!(merge_path("", "/x"), "/x");
    }

    #[test]
    fn fallback_dirs_on_a_fake_home() {
        let home = std::env::temp_dir().join(format!(
            "sb-shellenv-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&home);
        for d in [
            ".local/bin",
            ".cargo/bin",
            ".nvm/versions/node/v9.11.2/bin",
            ".nvm/versions/node/v24.1.0/bin",
            ".nvm/versions/node/v24.10.0",
            ".nvm/versions/node/default",
        ] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        let mine = |v: Vec<PathBuf>| -> Vec<PathBuf> {
            v.into_iter().filter(|p| p.starts_with(&home)).collect()
        };
        assert_eq!(
            mine(fallback_dirs(&home, "/usr/bin:/bin")),
            vec![
                home.join(".local/bin"),
                home.join(".cargo/bin"),
                home.join(".nvm/versions/node/v24.1.0/bin"),
            ]
        );
        let existing = format!("/usr/bin:{}", home.join(".cargo/bin").display());
        assert_eq!(
            mine(fallback_dirs(&home, &existing)),
            vec![
                home.join(".local/bin"),
                home.join(".nvm/versions/node/v24.1.0/bin"),
            ]
        );
        assert_eq!(version("v24.10.0").unwrap(), vec![24, 10, 0]);
        assert!(version("v24.10.0") > version("v24.9.1"));
        assert!(version("v24.9.1") > version("v8.0.0"));
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn new_dirs_are_only_those_created_after_start() {
        let home = std::env::temp_dir().join(format!(
            "sb-shellenv-new-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".cargo/bin")).unwrap();
        let at_start = fallback_dirs(&home, "");
        assert!(new_dirs(&home, "/usr/bin", &at_start).is_empty());
        for d in [".local/bin", ".bun/bin"] {
            std::fs::create_dir_all(home.join(d)).unwrap();
        }
        let path = format!("/usr/bin:{}", home.join(".bun/bin").display());
        assert_eq!(
            new_dirs(&home, &path, &at_start),
            vec![home.join(".local/bin")]
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[test]
    fn drops_ssh_session_and_multiplexer_vars() {
        for k in [
            "SSH_CONNECTION",
            "SSH_CLIENT",
            "SSH_TTY",
            "TMUX",
            "TMUX_PANE",
            "STY",
        ] {
            assert!(session_bound(k, "x"), "{k}");
        }
        assert!(session_bound(
            "SSH_AUTH_SOCK",
            "/tmp/ssh-XXXXabcd/agent.1234"
        ));
        assert!(!session_bound(
            "SSH_AUTH_SOCK",
            "/run/user/1000/gnupg/S.gpg-agent.ssh"
        ));
        assert!(!session_bound("SSH_AUTH_SOCK", "/home/u/.ssh/agent.sock"));
        assert!(!session_bound("SSH_AUTH_SOCK", "/tmp/ssh-/agent.1"));
        assert!(!session_bound("SSH_AUTH_SOCK", "/tmp/ssh-a/b/socket"));
        assert!(!session_bound("PATH", "/usr/bin"));
        assert!(!session_bound("TERM", "screen"));
    }

    #[test]
    fn drops_inherited_agent_session_markers() {
        for k in [
            "CLAUDECODE",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_EXECPATH",
            "CLAUDE_CODE_MESSAGING_SOCKET",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CLAUDE_CODE_SESSION_ATTENDED",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_SSE_PORT",
            "CLAUDE_PID",
            "CLAUDE_EFFORT",
            "CLAUDE_PLUGIN_DATA",
            "CLAUDE_PLUGIN_ROOT",
            "CLAUDE_PROJECT_DIR",
            "CLAUDE_ENV_FILE",
            "CODEX_COMPANION_SESSION_ID",
            "CODEX_SANDBOX",
            "CODEX_SANDBOX_NETWORK_DISABLED",
        ] {
            assert!(session_bound(k, "x"), "{k}");
        }
    }

    #[test]
    fn keeps_agent_user_configuration() {
        for k in [
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "CLAUDE_CODE_SUBAGENT_MODEL",
            "CLAUDE_CONFIG_DIR",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_MODEL",
            "CODEX_HOME",
        ] {
            assert!(!session_bound(k, "x"), "{k}");
        }
    }

    #[test]
    fn marks_overseer_terms() {
        let env = term_env();
        let get = |k: &str| env.iter().find(|(a, _)| *a == k).map(|(_, v)| *v);
        assert_eq!(get("TERM_PROGRAM"), Some("overseer"));
        assert_eq!(get("OVERSEER_TERM"), Some("1"));
        assert_eq!(get("TERM_PROGRAM_VERSION"), Some(env!("CARGO_PKG_VERSION")));
        assert_eq!(get("TERM"), Some("xterm-256color"));
    }
}
