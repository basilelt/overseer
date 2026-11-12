use crate::registry::Registry;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

const KEY: &str = "keepAwakeLidClosed";
const SAVED: &str = "keepAwakeLidSaved";
const ERROR: &str = "keepAwakeLidError";
pub const UNSUPPORTED: &str =
    "keeping agents running with the lid closed is not supported on this system";

pub type Run<'a> = &'a dyn Fn(&str, &[&str]) -> Result<String, String>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Linux,
    Mac,
    Windows,
    Other,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Other
        }
    }
}

struct Inhibitor {
    child: Child,
    _stdin: ChildStdin,
    _stdout: ChildStdout,
}

impl Drop for Inhibitor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static HELD: Mutex<Option<Inhibitor>> = Mutex::new(None);

pub fn enabled(reg: &Registry) -> bool {
    matches!(reg.get(KEY), Ok(Some(v)) if v == "true")
}

pub const INHIBIT_ARGS: [&str; 5] = [
    "--what=handle-lid-switch",
    "--no-ask-password",
    "--mode=block",
    "--who=Overseer",
    "--why=Keep agents running with the lid closed",
];

fn inhibit() -> Result<Inhibitor, String> {
    let mut child = Command::new("systemd-inhibit")
        .args(INHIBIT_ARGS)
        .arg("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("systemd-inhibit: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("systemd-inhibit: no stdin")?;
    let mut stdout = child.stdout.take().ok_or("systemd-inhibit: no stdout")?;
    let mut echo = [0u8; 1];
    let held = stdin.write_all(b"\n").and_then(|_| stdin.flush()).is_ok()
        && matches!(stdout.read(&mut echo), Ok(1));
    if held {
        child.stderr.take();
        return Ok(Inhibitor {
            child,
            _stdin: stdin,
            _stdout: stdout,
        });
    }
    let mut err = String::new();
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let err = err.trim();
    Err(if err.is_empty() {
        format!("systemd-inhibit exited with {status}")
    } else {
        err.to_string()
    })
}

fn linux_held() -> bool {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(h) = held.as_mut() {
        if matches!(h.child.try_wait(), Ok(None)) {
            return true;
        }
        *held = None;
    }
    false
}

fn linux_apply(on: bool) -> Result<(), String> {
    let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
    if !on {
        *held = None;
        return Ok(());
    }
    if let Some(h) = held.as_mut() {
        if matches!(h.child.try_wait(), Ok(None)) {
            return Ok(());
        }
    }
    *held = Some(inhibit()?);
    Ok(())
}

pub fn mac_active(pmset_g: &str) -> bool {
    pmset_g.lines().any(|l| {
        let mut w = l.split_whitespace();
        w.next() == Some("SleepDisabled") && w.next() == Some("1")
    })
}

pub fn mac_apply(run: Run, on: bool) -> Result<(), String> {
    let script = format!(
        "do shell script \"pmset -a disablesleep {}\" with administrator privileges",
        u8::from(on)
    );
    run("osascript", &["-e", &script]).map(|_| ())
}

pub fn win_lid(query: &str) -> Option<(u32, u32)> {
    let v: Vec<u32> = query
        .split_whitespace()
        .filter_map(|w| w.strip_prefix("0x"))
        .filter_map(|h| u32::from_str_radix(h, 16).ok())
        .collect();
    match v.as_slice() {
        [.., ac, dc] => Some((*ac, *dc)),
        _ => None,
    }
}

const WIN_LID: [&str; 3] = ["SCHEME_CURRENT", "SUB_BUTTONS", "LIDACTION"];

fn win_query(run: Run) -> Result<(u32, u32), String> {
    let out = run("powercfg", &["/query", WIN_LID[0], WIN_LID[1], WIN_LID[2]])?;
    win_lid(&out).ok_or_else(|| "powercfg: could not read the lid close action".into())
}

fn win_set(run: Run, ac: u32, dc: u32) -> Result<(), String> {
    let (ac, dc) = (ac.to_string(), dc.to_string());
    run(
        "powercfg",
        &["/setacvalueindex", WIN_LID[0], WIN_LID[1], WIN_LID[2], &ac],
    )?;
    run(
        "powercfg",
        &["/setdcvalueindex", WIN_LID[0], WIN_LID[1], WIN_LID[2], &dc],
    )?;
    run("powercfg", &["/setactive", "SCHEME_CURRENT"]).map(|_| ())
}

pub fn win_apply(run: Run, reg: &Registry, on: bool) -> Result<(), String> {
    let (ac, dc) = win_query(run)?;
    if on {
        if (ac, dc) != (0, 0) {
            reg.put(SAVED, &format!("{ac},{dc}"))?;
        }
        return win_set(run, 0, 0);
    }
    if (ac, dc) != (0, 0) {
        return Ok(());
    }
    let (ac, dc) = reg
        .get(SAVED)?
        .and_then(|s| {
            let (a, d) = s.split_once(',')?;
            Some((a.parse().ok()?, d.parse().ok()?))
        })
        .unwrap_or((1, 1));
    win_set(run, ac, dc)
}

pub fn active(os: Os, run: Run) -> bool {
    match os {
        Os::Linux => linux_held(),
        Os::Mac => run("pmset", &["-g"]).is_ok_and(|o| mac_active(&o)),
        Os::Windows => win_query(run).is_ok_and(|v| v == (0, 0)),
        Os::Other => false,
    }
}

pub fn apply(os: Os, run: Run, reg: &Registry, on: bool) -> Result<(), String> {
    match os {
        Os::Linux => linux_apply(on),
        Os::Mac => mac_apply(run, on),
        Os::Windows => win_apply(run, reg, on),
        Os::Other => Err(UNSUPPORTED.into()),
    }
}

fn set_error(reg: &Registry, e: Option<&str>) {
    if let Err(err) = reg.put(ERROR, e.unwrap_or("")) {
        crate::dlog::line("warn", &format!("keep awake with the lid closed: {err}"));
    }
}

fn last_error(reg: &Registry) -> Option<String> {
    reg.get(ERROR).ok().flatten().filter(|e| !e.is_empty())
}

pub fn status(os: Os, run: Run, reg: &Registry) -> Value {
    json!({
        "enabled": enabled(reg),
        "active": active(os, run),
        "supported": os != Os::Other,
        "error": last_error(reg),
    })
}

pub fn set(os: Os, run: Run, reg: &Registry, on: bool) -> Result<Value, (u16, String)> {
    if os == Os::Other {
        return Err((501, UNSUPPORTED.into()));
    }
    match apply(os, run, reg, on) {
        Ok(()) => {
            set_error(reg, None);
            reg.put(KEY, if on { "true" } else { "false" })
                .map_err(|e| (500, e))?;
            Ok(status(os, run, reg))
        }
        Err(e) => {
            set_error(reg, Some(&e));
            Err((500, e))
        }
    }
}

pub fn on_start(state_dir: &Path) {
    let Ok(reg) = Registry::open(state_dir) else {
        return;
    };
    set_error(&reg, None);
    if Os::current() != Os::Linux || !enabled(&reg) {
        return;
    }
    if let Err(e) = linux_apply(true) {
        crate::dlog::line("warn", &format!("keep awake with the lid closed: {e}"));
        set_error(&reg, Some(&e));
    }
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<String, String> {
    let mut c = Command::new(cmd);
    c.args(args).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let out = crate::git::output_within(&mut c, Duration::from_secs(120))
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("{cmd} exited with {}", out.status)
    } else {
        err
    })
}

pub fn routes(method: &str, segs: &[&str], body: &Value, state_dir: &Path) -> Option<(u16, Value)> {
    if segs != ["v1", "power", "lid"] {
        return None;
    }
    let reg = match Registry::open(state_dir) {
        Ok(r) => r,
        Err(e) => return Some((500, json!({"error": e}))),
    };
    let os = Os::current();
    Some(match method {
        "GET" => (200, status(os, &run_cmd, &reg)),
        "PUT" => match body["enabled"].as_bool() {
            None => (400, json!({"error": "enabled must be a boolean"})),
            Some(on) => match set(os, &run_cmd, &reg, on) {
                Ok(v) => (200, v),
                Err((code, e)) => (code, json!({"error": e})),
            },
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "sb-power-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn reg() -> Registry {
        Registry::open(&dir()).unwrap()
    }

    const WIN_QUERY: &str = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)
  Subgroup GUID: 4f971e89-eebd-4455-a8de-9e59040e7347  (Power buttons and lid)
    Power Setting GUID: 5ca83367-6e45-459f-a27b-476b1d01c936  (Lid close action)
      Possible Setting Index: 000
      Possible Setting Friendly Name: Do nothing
      Possible Setting Index: 001
      Possible Setting Friendly Name: Sleep
    Current AC Power Setting Index: 0x00000001
    Current DC Power Setting Index: 0x00000002
";

    #[test]
    fn mac_reads_sleep_disabled_from_pmset() {
        assert!(mac_active(
            "System-wide power settings:\n SleepDisabled\t\t1\n"
        ));
        assert!(!mac_active(
            "System-wide power settings:\n SleepDisabled\t\t0\n"
        ));
        assert!(!mac_active("Currently in use:\n sleep 1\n"));
    }

    #[test]
    fn mac_runs_pmset_through_an_admin_prompt() {
        let calls = RefCell::new(vec![]);
        let run = |c: &str, a: &[&str]| {
            calls.borrow_mut().push(format!("{c} {}", a.join(" ")));
            Ok(String::new())
        };
        mac_apply(&run, true).unwrap();
        mac_apply(&run, false).unwrap();
        let calls = calls.into_inner();
        assert!(calls[0].starts_with("osascript -e do shell script \"pmset -a disablesleep 1\""));
        assert!(calls[0].ends_with("with administrator privileges"));
        assert!(calls[1].contains("disablesleep 0"));
    }

    #[test]
    fn mac_reports_a_cancelled_prompt() {
        let run = |_: &str, _: &[&str]| Err("User canceled. (-128)".to_string());
        assert_eq!(mac_apply(&run, true).unwrap_err(), "User canceled. (-128)");
    }

    #[test]
    fn win_reads_ac_and_dc_lid_actions() {
        assert_eq!(win_lid(WIN_QUERY), Some((1, 2)));
        assert_eq!(win_lid("nothing here"), None);
        assert_eq!(win_lid("0x00000003"), None);
    }

    struct WinFake {
        lid: RefCell<(u32, u32)>,
        calls: RefCell<Vec<String>>,
    }

    impl WinFake {
        fn new(ac: u32, dc: u32) -> WinFake {
            WinFake {
                lid: RefCell::new((ac, dc)),
                calls: RefCell::new(vec![]),
            }
        }
        fn run(&self, c: &str, a: &[&str]) -> Result<String, String> {
            assert_eq!(c, "powercfg");
            self.calls.borrow_mut().push(a[0].to_string());
            let mut lid = self.lid.borrow_mut();
            match a[0] {
                "/query" => Ok(format!(
                    "Current AC Power Setting Index: 0x{:08x}\nCurrent DC Power Setting Index: 0x{:08x}\n",
                    lid.0, lid.1
                )),
                "/setacvalueindex" => {
                    lid.0 = a[4].parse().unwrap();
                    Ok(String::new())
                }
                "/setdcvalueindex" => {
                    lid.1 = a[4].parse().unwrap();
                    Ok(String::new())
                }
                "/setactive" => Ok(String::new()),
                _ => Err(format!("unexpected {a:?}")),
            }
        }
    }

    #[test]
    fn win_sets_do_nothing_and_restores_the_previous_actions() {
        let r = reg();
        let f = WinFake::new(1, 3);
        let run = |c: &str, a: &[&str]| f.run(c, a);
        let v = set(Os::Windows, &run, &r, true).unwrap();
        assert_eq!(*f.lid.borrow(), (0, 0));
        assert_eq!(v["enabled"], true);
        assert_eq!(v["active"], true);
        assert!(f.calls.borrow().contains(&"/setactive".to_string()));
        let v = set(Os::Windows, &run, &r, false).unwrap();
        assert_eq!(*f.lid.borrow(), (1, 3));
        assert_eq!(v["enabled"], false);
        assert_eq!(v["active"], false);
    }

    #[test]
    fn win_enabling_twice_keeps_the_original_actions() {
        let r = reg();
        let f = WinFake::new(2, 1);
        let run = |c: &str, a: &[&str]| f.run(c, a);
        set(Os::Windows, &run, &r, true).unwrap();
        set(Os::Windows, &run, &r, true).unwrap();
        set(Os::Windows, &run, &r, false).unwrap();
        assert_eq!(*f.lid.borrow(), (2, 1));
    }

    #[test]
    fn win_disable_leaves_a_lid_action_the_user_changed_since() {
        let r = reg();
        let f = WinFake::new(1, 1);
        let run = |c: &str, a: &[&str]| f.run(c, a);
        set(Os::Windows, &run, &r, true).unwrap();
        *f.lid.borrow_mut() = (3, 3);
        set(Os::Windows, &run, &r, false).unwrap();
        assert_eq!(*f.lid.borrow(), (3, 3));
    }

    #[test]
    fn win_disable_without_a_saved_action_falls_back_to_sleep() {
        let r = reg();
        let f = WinFake::new(0, 0);
        let run = |c: &str, a: &[&str]| f.run(c, a);
        set(Os::Windows, &run, &r, false).unwrap();
        assert_eq!(*f.lid.borrow(), (1, 1));
    }

    #[test]
    fn a_refused_change_keeps_the_option_off_and_reports_why() {
        let r = reg();
        let run = |_: &str, _: &[&str]| Err("Access is denied.".to_string());
        let e = set(Os::Windows, &run, &r, true).unwrap_err();
        assert_eq!(e, (500, "Access is denied.".to_string()));
        assert!(!enabled(&r));
        let s = status(Os::Windows, &run, &r);
        assert_eq!(s["error"], "Access is denied.");
        assert_eq!(s["active"], false);
    }

    #[test]
    fn unsupported_systems_answer_501() {
        let r = reg();
        let run = |_: &str, _: &[&str]| -> Result<String, String> { unreachable!() };
        assert_eq!(set(Os::Other, &run, &r, true).unwrap_err().0, 501);
        assert_eq!(status(Os::Other, &run, &r)["supported"], false);
    }

    #[test]
    fn mac_status_follows_pmset() {
        let r = reg();
        let run = |c: &str, _: &[&str]| match c {
            "pmset" => Ok(" SleepDisabled\t\t1\n".to_string()),
            _ => Ok(String::new()),
        };
        let v = set(Os::Mac, &run, &r, true).unwrap();
        assert_eq!(v["enabled"], true);
        assert_eq!(v["active"], true);
        assert_eq!(v["error"], Value::Null);
    }

    #[test]
    fn concurrent_states_each_see_their_own_error() {
        use std::sync::mpsc::channel;
        let (cleared, on_cleared) = channel::<()>();
        let (refused_set, on_refused) = channel::<()>();
        std::thread::scope(|s| {
            let mac = s.spawn(move || {
                let r = reg();
                let run = |c: &str, _: &[&str]| {
                    if c == "pmset" {
                        let _ = cleared.send(());
                        let _ = on_refused.recv();
                        return Ok(" SleepDisabled\t\t1\n".to_string());
                    }
                    Ok(String::new())
                };
                set(Os::Mac, &run, &r, true).unwrap()
            });
            let refused = s.spawn(move || {
                let r = reg();
                let run = |_: &str, _: &[&str]| Err("Access is denied.".to_string());
                on_cleared.recv().unwrap();
                set(Os::Windows, &run, &r, true).unwrap_err();
                refused_set.send(()).unwrap();
                status(Os::Windows, &run, &r)
            });
            let v = mac.join().unwrap();
            assert_eq!(v["active"], true);
            assert_eq!(v["error"], Value::Null);
            assert_eq!(refused.join().unwrap()["error"], "Access is denied.");
        });
    }

    #[test]
    fn routes_only_answer_their_path() {
        let d = dir();
        assert!(routes("GET", &["v1", "power"], &Value::Null, &d).is_none());
        assert!(routes("POST", &["v1", "power", "lid"], &Value::Null, &d).is_none());
        let (code, v) = routes(
            "PUT",
            &["v1", "power", "lid"],
            &json!({"enabled": "yes"}),
            &d,
        )
        .unwrap();
        assert_eq!(code, 400);
        assert_eq!(v["error"], "enabled must be a boolean");
    }
}
