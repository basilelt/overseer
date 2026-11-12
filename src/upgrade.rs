use crate::deliver::Job;
use crate::wire::PROTOCOL;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

pub const FORMAT: u32 = 1;
const OWN: &str = env!("CARGO_PKG_VERSION");

static EPOCH: AtomicU64 = AtomicU64::new(0);
static FROM: Mutex<Option<String>> = Mutex::new(None);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct State {
    pub format: u32,
    pub from: String,
    pub epoch: u64,
    pub lock_fd: i32,
    pub listener_fd: i32,
    pub terms: Vec<SavedTerm>,
    pub queue: Vec<Job>,
    pub theme: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct SavedTerm {
    pub id: String,
    pub pid: u32,
    pub fd: i32,
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
    pub total: u64,
    pub last_ms: i64,
    pub closed: bool,
    pub ring: String,
}

pub fn version_json() -> Value {
    let mut v = crate::daemon::version();
    v["epoch"] = json!(EPOCH.load(Ordering::SeqCst));
    v["pid"] = json!(std::process::id());
    if let Some(f) = FROM.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        v["upgradedFrom"] = json!(f);
    }
    v
}

pub fn hex(b: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(D[(x >> 4) as usize] as char);
        s.push(D[(x & 15) as usize] as char);
    }
    s
}

pub fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) {
        return Err("odd hex length".into());
    }
    s.as_bytes()
        .chunks(2)
        .map(|c| {
            let d = |x: u8| (x as char).to_digit(16);
            match (d(c[0]), d(c[1])) {
                (Some(a), Some(b)) => Ok((a * 16 + b) as u8),
                _ => Err("bad hex in ring".to_string()),
            }
        })
        .collect()
}

fn parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn newer(a: &str, b: &str) -> bool {
    let (mut x, mut y) = (parts(a), parts(b));
    let n = x.len().max(y.len());
    x.resize(n, 0);
    y.resize(n, 0);
    x > y
}

pub fn compatible(v: &Value, own: &str, force: bool) -> Result<String, String> {
    let to = v["version"]
        .as_str()
        .ok_or("the new binary did not report its version")?;
    if v["protocol"].as_u64() != Some(u64::from(PROTOCOL)) {
        return Err(format!(
            "the new binary speaks protocol {}, the running daemon {PROTOCOL}",
            v["protocol"]
        ));
    }
    if v["takeover"].as_u64() != Some(u64::from(FORMAT)) {
        return Err(format!(
            "{to} cannot take over this daemon (takeover format {FORMAT})"
        ));
    }
    if !force && !newer(to, own) {
        return Err(format!("{to} is not newer than the running {own}"));
    }
    Ok(to.to_string())
}

pub fn daemon_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = vec![];
    let mut it = args.into_iter().peekable();
    if it.peek().is_some_and(|a| a == "daemon") {
        it.next();
    }
    while let Some(a) = it.next() {
        match a.as_str() {
            "--takeover" => {
                it.next();
            }
            "--detach" | "--replace" | "--takeover-check" => {}
            _ => out.push(a),
        }
    }
    out
}

pub fn fds(st: &State) -> Vec<i32> {
    let mut v = vec![st.lock_fd, st.listener_fd];
    v.extend(st.terms.iter().filter(|t| t.fd >= 0).map(|t| t.fd));
    v
}

pub fn validate(st: &State) -> Result<(), String> {
    if st.format != FORMAT {
        return Err(format!(
            "takeover format {} is not supported (this binary reads {FORMAT})",
            st.format
        ));
    }
    if st.lock_fd < 0 || st.listener_fd < 0 {
        return Err("the takeover state has no lock or socket fd".into());
    }
    let mut all = fds(st);
    all.sort_unstable();
    if all.windows(2).any(|w| w[0] == w[1]) {
        return Err("the takeover state names the same fd twice".into());
    }
    for t in &st.terms {
        unhex(&t.ring).map_err(|e| format!("term {}: {e}", t.id))?;
    }
    Ok(())
}

pub fn load(path: &Path) -> Result<State, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let st: State = serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    validate(&st)?;
    Ok(st)
}

pub fn check(file: Option<&Path>) -> Result<Value, String> {
    let mut v = json!({"version": OWN, "protocol": PROTOCOL, "takeover": FORMAT});
    if let Some(f) = file {
        v["terms"] = json!(load(f)?.terms.len());
    }
    Ok(v)
}

fn remove_stale(state_dir: &Path) {
    let Ok(rd) = std::fs::read_dir(state_dir) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("takeover-") && name.ends_with(".json") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub fn resume(st: Option<State>, state_dir: &Path) -> Option<i32> {
    remove_stale(state_dir);
    let st = st?;
    EPOCH.store(st.epoch, Ordering::SeqCst);
    *FROM.lock().unwrap_or_else(|e| e.into_inner()) = Some(st.from.clone());
    let n = st.terms.len();
    crate::termtheme::restore(&st.theme);
    #[cfg(unix)]
    for t in st.terms {
        let id = t.id.clone();
        if let Err(e) = crate::term::adopt(t) {
            let msg = format!("overseer daemon: takeover: term {id} lost: {e}");
            crate::dlog::line("warn", &msg);
        }
    }
    crate::broadcast::restore(st.queue);
    let msg = format!(
        "overseer daemon: took over from {} (epoch {}, {n} terms)",
        st.from, st.epoch
    );
    crate::dlog::line("info", &msg);
    crate::board::publish(
        "daemon.upgraded",
        json!({"from": st.from, "to": OWN, "epoch": st.epoch}),
    );
    if cfg!(unix) {
        Some(st.listener_fd)
    } else {
        None
    }
}

#[cfg_attr(not(unix), allow(dead_code))]
fn target(body: &Value) -> Result<PathBuf, String> {
    let bin = if let Some(p) = body["path"].as_str().filter(|p| !p.is_empty()) {
        PathBuf::from(p)
    } else if let Some(v) = body["version"].as_str() {
        if v.is_empty()
            || !v
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".+-".contains(c))
        {
            return Err(format!("bad version {v:?}"));
        }
        crate::paths::server_dir().join(v).join("overseer")
    } else {
        return Err("path or version required".into());
    };
    if !bin.is_file() {
        return Err(format!("no server binary at {}", bin.display()));
    }
    Ok(bin)
}

#[cfg(unix)]
pub use unix::*;

#[cfg(unix)]
mod unix {
    use super::*;
    use crate::http::{respond, Request};
    use std::io::Write;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::net::UnixListener;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicI32};
    use std::time::Duration;

    static LOCK_FD: AtomicI32 = AtomicI32::new(-1);
    static LISTENER_FD: AtomicI32 = AtomicI32::new(-1);
    static UPGRADING: AtomicBool = AtomicBool::new(false);

    pub struct Plan {
        bin: PathBuf,
        file: PathBuf,
        epoch: u64,
        to: String,
    }

    pub fn set_lock(fd: i32) {
        LOCK_FD.store(fd, Ordering::SeqCst);
    }

    pub fn set_listener(fd: i32) {
        LISTENER_FD.store(fd, Ordering::SeqCst);
    }

    pub fn set_cloexec(fd: i32, on: bool) -> bool {
        unsafe {
            let f = libc::fcntl(fd, libc::F_GETFD);
            if f == -1 {
                return false;
            }
            let n = if on {
                f | libc::FD_CLOEXEC
            } else {
                f & !libc::FD_CLOEXEC
            };
            libc::fcntl(fd, libc::F_SETFD, n) != -1
        }
    }

    pub fn adopt_fd(fd: i32) -> Result<OwnedFd, String> {
        if fd < 0 || !set_cloexec(fd, true) {
            return Err(format!("inherited fd {fd} is not open"));
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    pub fn cloexec_all(st: &State) {
        for fd in fds(st) {
            set_cloexec(fd, true);
        }
    }

    pub fn adopt_lock(st: &State) -> Result<std::fs::File, String> {
        cloexec_all(st);
        Ok(std::fs::File::from(adopt_fd(st.lock_fd)?))
    }

    pub fn adopt_listener(fd: i32) -> Result<UnixListener, String> {
        Ok(UnixListener::from(adopt_fd(fd)?))
    }

    fn snapshot(epoch: u64) -> State {
        State {
            format: FORMAT,
            from: OWN.to_string(),
            epoch,
            lock_fd: LOCK_FD.load(Ordering::SeqCst),
            listener_fd: LISTENER_FD.load(Ordering::SeqCst),
            terms: crate::term::save_all(),
            queue: crate::broadcast::saved(),
            theme: crate::termtheme::saved(),
        }
    }

    fn write_state(path: &Path, st: &State) -> Result<(), String> {
        use crate::sys::OpenOptionsExt;
        let bytes = serde_json::to_vec(st).map_err(|e| e.to_string())?;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| format!("{}: {e}", path.display()))
    }

    fn preflight(bin: &Path, file: Option<&Path>) -> Result<Value, String> {
        let mut c = Command::new(bin);
        c.args(["daemon", "--takeover-check"]);
        if let Some(f) = file {
            c.arg(f);
        }
        let out =
            crate::exec::run_within(&mut c, Duration::from_secs(10)).map_err(|e| match e {
                crate::exec::RunError::TimedOut(_) => {
                    format!("{} --takeover-check timed out", bin.display())
                }
                e => format!("{}: {e}", bin.display()),
            })?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let parsed = stdout
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .and_then(|l| serde_json::from_str::<Value>(l).ok());
        match parsed {
            Some(v) if out.status.success() => Ok(v),
            _ => Err(format!(
                "{} cannot take over this daemon: {}",
                bin.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )),
        }
    }

    fn plan(body: &Value) -> Result<Plan, (u16, String)> {
        let bin = target(body).map_err(|e| (400, e))?;
        if LOCK_FD.load(Ordering::SeqCst) < 0 || LISTENER_FD.load(Ordering::SeqCst) < 0 {
            return Err((409, "this daemon has no socket to hand over".into()));
        }
        let v = preflight(&bin, None).map_err(|e| (409, e))?;
        let to = compatible(&v, OWN, body["force"] == true).map_err(|e| (409, e))?;
        let epoch = EPOCH.load(Ordering::SeqCst) + 1;
        let file = crate::default_state_dir().join(format!("takeover-{}.json", std::process::id()));
        write_state(&file, &snapshot(epoch)).map_err(|e| (500, e))?;
        if let Err(e) = preflight(&bin, Some(&file)) {
            let _ = std::fs::remove_file(&file);
            return Err((409, e));
        }
        Ok(Plan {
            bin,
            file,
            epoch,
            to,
        })
    }

    fn go(p: Plan) {
        if !crate::term::freeze(Duration::from_secs(2)) {
            crate::dlog::line(
                "warn",
                "overseer daemon: some terminal readers did not pause before the upgrade",
            );
        }
        let st = snapshot(p.epoch);
        let fds = fds(&st);
        for &fd in &fds {
            set_cloexec(fd, false);
        }
        let failed = match write_state(&p.file, &st) {
            Err(e) => e,
            Ok(()) => {
                crate::github::before_exec();
                Command::new(&p.bin)
                    .arg("daemon")
                    .args(daemon_args(std::env::args().skip(1)))
                    .arg("--takeover")
                    .arg(&p.file)
                    .exec()
                    .to_string()
            }
        };
        for &fd in &fds {
            set_cloexec(fd, true);
        }
        let _ = std::fs::remove_file(&p.file);
        crate::term::thaw();
        UPGRADING.store(false, Ordering::SeqCst);
        let msg = format!(
            "overseer daemon: hot upgrade to {} ({}) failed: {failed}; still serving {OWN}",
            p.to,
            p.bin.display()
        );
        crate::dlog::line("error", &msg);
    }

    pub fn serve(req: &Request, mut s: impl Write) {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        if UPGRADING.swap(true, Ordering::SeqCst) {
            return respond(
                &mut s,
                409,
                &json!({"error": "an upgrade is already in progress"}),
            );
        }
        match plan(&body) {
            Ok(p) => {
                respond(
                    &mut s,
                    202,
                    &json!({"from": OWN, "to": p.to, "epoch": p.epoch}),
                );
                let _ = s.flush();
                drop(s);
                let msg = format!(
                    "overseer daemon: hot upgrade {OWN} -> {} ({})",
                    p.to,
                    p.bin.display()
                );
                crate::dlog::line("info", &msg);
                go(p);
            }
            Err((code, e)) => {
                UPGRADING.store(false, Ordering::SeqCst);
                respond(&mut s, code, &json!({ "error": e }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> State {
        State {
            format: FORMAT,
            from: "0.8.0".into(),
            epoch: 2,
            lock_fd: 3,
            listener_fd: 4,
            terms: vec![
                SavedTerm {
                    id: "t1".into(),
                    pid: 42,
                    fd: 7,
                    cmd: vec!["/bin/sh".into()],
                    cols: 80,
                    rows: 24,
                    exit_code: None,
                    line_id: Some("l1".into()),
                    total: 5,
                    last_ms: 9,
                    ring: hex(b"hi\x1b[0m\xff"),
                    ..Default::default()
                },
                SavedTerm {
                    id: "t2".into(),
                    fd: -1,
                    exited: true,
                    exit_code: Some(1),
                    closed: true,
                    ..Default::default()
                },
            ],
            queue: vec![Job {
                id: "b1".into(),
                target: "line-1".into(),
                text: "go".into(),
                at_ms: 1,
            }],
            theme: json!({"dark": true, "fg": "#ededef", "bg": "#0b0b0c"}),
        }
    }

    fn chip() -> crate::profiles::Chip {
        crate::profiles::Chip {
            id: "p1".into(),
            name: "Fast".into(),
            model: "m".into(),
            effort: "high".into(),
        }
    }

    const FULL_TERM: &str = r#"{"id":"t","pid":4,"fd":5,"cwd":"/w","cmd":["a","b"],"title":"T","title_override":"O","cols":80,"rows":24,"created_ms":99,"exited":true,"exit_code":3,"line_id":"l","task_id":"k","agent":"claude","session":"s","flags":["--x"],"sized_by":"web","profile":{"id":"p1","name":"Fast","model":"m","effort":"high"},"workspace_id":"w1","total":7,"last_ms":8,"closed":true,"ring":"ab"}"#;

    #[test]
    fn saved_term_json_is_pinned() {
        let st = SavedTerm {
            id: "t".into(),
            pid: 4,
            fd: 5,
            cwd: "/w".into(),
            cmd: vec!["a".into(), "b".into()],
            title: "T".into(),
            title_override: Some("O".into()),
            cols: 80,
            rows: 24,
            created_ms: 99,
            exited: true,
            exit_code: Some(3),
            line_id: Some("l".into()),
            task_id: Some("k".into()),
            agent: Some("claude".into()),
            session: Some("s".into()),
            flags: vec!["--x".into()],
            sized_by: Some("web".into()),
            profile: Some(chip()),
            workspace_id: Some("w1".into()),
            total: 7,
            last_ms: 8,
            closed: true,
            ring: "ab".into(),
        };
        let want: Value = serde_json::from_str(FULL_TERM).unwrap();
        assert_eq!(serde_json::to_value(&st).unwrap(), want);
        assert_eq!(serde_json::from_str::<SavedTerm>(FULL_TERM).unwrap(), st);
    }

    #[test]
    fn saved_term_minimal_json_takes_defaults() {
        let st: SavedTerm = serde_json::from_str(r#"{"id":"t"}"#).unwrap();
        assert_eq!(
            st,
            SavedTerm {
                id: "t".into(),
                ..Default::default()
            }
        );
        assert_eq!(st.fd, 0);
        assert!(st.cmd.is_empty() && st.profile.is_none() && !st.closed);
    }

    #[test]
    fn state_roundtrips_through_json() {
        let st = sample();
        let back: State = serde_json::from_slice(&serde_json::to_vec(&st).unwrap()).unwrap();
        assert_eq!(back, st);
        assert_eq!(unhex(&back.terms[0].ring).unwrap(), b"hi\x1b[0m\xff");
        assert!(validate(&back).is_ok());
        assert_eq!(fds(&back), vec![3, 4, 7]);
        let dir = std::env::temp_dir().join(format!("sb-upgrade-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("takeover-1.json");
        std::fs::write(&f, serde_json::to_vec(&st).unwrap()).unwrap();
        assert_eq!(load(&f).unwrap(), st);
        assert_eq!(check(Some(&f)).unwrap()["terms"], 2);
        remove_stale(&dir);
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_states_are_refused() {
        let mut st = sample();
        st.format = FORMAT + 1;
        assert!(validate(&st).unwrap_err().contains("not supported"));
        let mut st = sample();
        st.terms[0].fd = 4;
        assert!(validate(&st).unwrap_err().contains("same fd"));
        let mut st = sample();
        st.terms[0].ring = "zz".into();
        assert!(validate(&st).is_err());
        let mut st = sample();
        st.listener_fd = -1;
        assert!(validate(&st).is_err());
        assert!(serde_json::from_str::<State>("not json").is_err());
    }

    #[test]
    fn hex_roundtrip() {
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(unhex(&hex(&all)).unwrap(), all);
        assert!(unhex("abc").is_err());
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "0.99"));
        assert!(newer("0.8.1", "0.8"));
        assert!(!newer("0.8.0", "0.8.0"));
        assert!(!newer("0.8.0-rc1", "0.8.0"));
        assert!(!newer("0.7.0", "0.8.0"));
        assert!(newer("v0.9.0", "0.8.5+abc"));
    }

    #[test]
    fn compatibility_needs_same_protocol_format_and_a_newer_version() {
        let v = |ver: &str, p: u32, f: u32| json!({"version": ver, "protocol": p, "takeover": f});
        assert_eq!(
            compatible(&v("0.9.0", PROTOCOL, FORMAT), "0.8.0", false).unwrap(),
            "0.9.0"
        );
        assert!(compatible(&v("0.8.0", PROTOCOL, FORMAT), "0.8.0", false).is_err());
        assert!(compatible(&v("0.8.0", PROTOCOL, FORMAT), "0.8.0", true).is_ok());
        assert!(compatible(&v("0.7.0", PROTOCOL, FORMAT), "0.8.0", false).is_err());
        assert!(
            compatible(&v("0.9.0", PROTOCOL + 1, FORMAT), "0.8.0", false)
                .unwrap_err()
                .contains("protocol")
        );
        assert!(compatible(&v("0.9.0", PROTOCOL, FORMAT + 1), "0.8.0", true).is_err());
        assert!(compatible(&json!({}), "0.8.0", true).is_err());
    }

    #[test]
    fn exec_args_keep_daemon_options_only() {
        let a = |v: &[&str]| daemon_args(v.iter().map(|s| s.to_string()));
        assert_eq!(
            a(&[
                "daemon",
                "--interval",
                "1",
                "--takeover",
                "/s/t.json",
                "--detach"
            ]),
            vec!["--interval", "1"]
        );
        assert_eq!(
            a(&["daemon", "--replace", "--http", "127.0.0.1:0"]),
            vec!["--http", "127.0.0.1:0"]
        );
        assert!(a(&["daemon"]).is_empty());
    }

    #[test]
    fn target_resolves_versions_and_rejects_bad_input() {
        assert!(target(&json!({})).unwrap_err().contains("required"));
        assert!(target(&json!({"version": "../x"}))
            .unwrap_err()
            .contains("bad version"));
        assert!(target(&json!({"version": "9.9.9-none"}))
            .unwrap_err()
            .contains(".overseer/server/9.9.9-none/overseer"));
        assert!(target(&json!({"path": "/nonexistent/overseer"})).is_err());
    }
}
