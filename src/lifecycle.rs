use crate::autostart;
use crate::registry::Registry;
use crate::term::Info;
use crate::Line;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Mutex;

const BOOT_KEY: &str = "boot_id";
const TERMS_KEY: &str = "saved_terms";
const AFTER_KEY: &str = "after_reboot";

static LAST: Mutex<Option<String>> = Mutex::new(None);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AfterReboot {
    Agents,
    Shells,
    Nothing,
}

impl AfterReboot {
    pub fn parse(s: &str) -> Option<AfterReboot> {
        match s {
            "agents" => Some(AfterReboot::Agents),
            "shells" => Some(AfterReboot::Shells),
            "nothing" => Some(AfterReboot::Nothing),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AfterReboot::Agents => "agents",
            AfterReboot::Shells => "shells",
            AfterReboot::Nothing => "nothing",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Saved {
    pub id: String,
    pub cwd: String,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
    pub agent: Option<String>,
    pub session: Option<String>,
    pub flags: Vec<String>,
    pub line_id: Option<String>,
    pub task_id: Option<String>,
    pub workspace_id: Option<String>,
    pub title_override: Option<String>,
    pub profile: Option<crate::profiles::Chip>,
}

impl From<&Info> for Saved {
    fn from(i: &Info) -> Self {
        Self {
            id: i.id.clone(),
            cwd: i.cwd.clone(),
            title: i.title.clone(),
            cols: i.cols,
            rows: i.rows,
            agent: i.agent.clone(),
            session: i.session.clone(),
            flags: i.flags.clone(),
            line_id: i.line_id.clone(),
            task_id: i.task_id.clone(),
            workspace_id: i.workspace_id.clone(),
            title_override: i.title_override.clone(),
            profile: i.profile.clone(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Restore {
    pub saved: Saved,
    pub cmd: Vec<String>,
}

pub fn boot_id() -> Option<String> {
    if cfg!(target_os = "linux") {
        return std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }
    if cfg!(target_os = "macos") {
        let out = crate::git::output_within(
            std::process::Command::new("sysctl").args(["-n", "kern.boottime"]),
            crate::git::PROBE_TIMEOUT,
        )
        .ok()?;
        return parse_boottime(&String::from_utf8_lossy(&out.stdout));
    }
    None
}

pub fn parse_boottime(s: &str) -> Option<String> {
    let rest = &s[s.find("sec =")? + 5..];
    let n: String = rest
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!n.is_empty()).then_some(n)
}

pub fn is_reboot(stored: Option<&str>, current: Option<&str>) -> bool {
    matches!((stored, current), (Some(a), Some(b)) if a != b)
}

pub fn is_stub_wrapper(cmd: &[String]) -> bool {
    cmd.last().is_some_and(|c| c.contains(" stub "))
}

pub fn snapshot(terms: &[Info], lines: &[Line]) -> Vec<Saved> {
    terms
        .iter()
        .filter_map(|i| {
            if i.exited {
                return None;
            }
            let line = lines
                .iter()
                .find(|l| l.term_id.as_deref() == Some(i.id.as_str()) && l.alive());
            if line.is_none() && is_stub_wrapper(&i.cmd) {
                return None;
            }
            Some(Saved {
                agent: line.map(|l| l.agent.to_string()),
                session: line.map(|l| l.session_id.clone()),
                flags: line.map(|l| l.launch_args.clone()).unwrap_or_default(),
                line_id: line.map(|l| l.id.clone()).or_else(|| i.line_id.clone()),
                profile: line.and(i.profile.clone()),
                ..Saved::from(i)
            })
        })
        .collect()
}

pub fn plan(saved: Vec<Saved>, mode: AfterReboot, exists: impl Fn(&str) -> bool) -> Vec<Restore> {
    if mode == AfterReboot::Nothing {
        return vec![];
    }
    let mut out = vec![];
    for s in saved {
        if s.id.is_empty() || !exists(&s.cwd) {
            continue;
        }
        let resume = match (&s.agent, &s.session) {
            (Some(a), Some(sid)) if !sid.is_empty() => {
                if mode != AfterReboot::Agents {
                    continue;
                }
                crate::agents::get(a)
                    .and_then(|ag| ag.resume_argv(sid, &crate::resume_flags(a, &s.flags)))
            }
            _ => None,
        };
        let mut saved = s;
        if resume.is_none() {
            saved.agent = None;
            saved.session = None;
            saved.flags = vec![];
            saved.line_id = None;
            saved.task_id = None;
            saved.profile = None;
        }
        out.push(Restore {
            cmd: resume.unwrap_or_default(),
            saved,
        });
    }
    out
}

pub fn after_reboot(reg: &Registry) -> AfterReboot {
    reg.get(AFTER_KEY)
        .ok()
        .flatten()
        .and_then(|v| AfterReboot::parse(&v))
        .unwrap_or(AfterReboot::Agents)
}

pub fn on_start(state_dir: &Path, takeover: bool) {
    let reg = match Registry::open(state_dir) {
        Ok(r) => r,
        Err(e) => return crate::dlog::line("warn", &format!("overseer daemon: boot check: {e}")),
    };
    let current = boot_id();
    let stored = reg.get(BOOT_KEY).ok().flatten();
    if let Some(c) = &current {
        let _ = reg.put(BOOT_KEY, c);
    }
    if takeover {
        return;
    }
    let cause = if is_reboot(stored.as_deref(), current.as_deref()) {
        "machine rebooted"
    } else {
        "daemon restarted"
    };
    let mode = after_reboot(&reg);
    let saved: Vec<Saved> = reg
        .get(TERMS_KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let todo = plan(saved, mode, |p| Path::new(p).is_dir());
    let (mut agents, mut shells) = (0, 0);
    for r in todo {
        let s = r.saved;
        let agent = s.agent.as_deref().and_then(crate::agents::get);
        let applied = agent
            .map(|a| crate::profiles::for_restore(state_dir, a, &s.cwd, s.profile.as_ref()))
            .unwrap_or_default();
        let spawned = crate::term::spawn(crate::term::Spec {
            id: Some(s.id.clone()),
            cwd: s.cwd.clone(),
            cmd: r.cmd,
            env: applied.env,
            cols: s.cols.max(20),
            rows: s.rows.max(5),
            title: Some(s.title.clone()).filter(|t| !t.is_empty()),
            line_id: s.line_id.clone(),
            task_id: s.task_id.clone(),
            session: s.session.clone(),
            flags: s.flags.clone(),
            profile: applied.chip,
            ..Default::default()
        });
        if let Ok(t) = &spawned {
            t.set_workspace(s.workspace_id.clone());
            t.set_title(s.title_override.clone());
        }
        match spawned {
            Ok(_) if agent.is_some() => agents += 1,
            Ok(_) => shells += 1,
            Err(e) => crate::dlog::line(
                "warn",
                &format!(
                    "overseer daemon: after reboot: term {} not restored: {e}",
                    s.id
                ),
            ),
        }
    }
    crate::dlog::line(
        "info",
        &format!(
            "overseer daemon: {cause}; restored {agents} agent sessions and {shells} shells ({})",
            mode.as_str()
        ),
    );
}

pub fn persist(state_dir: &Path, lines: &[Line]) {
    let infos: Vec<Info> = crate::term::all().iter().map(|t| t.info()).collect();
    let text = serde_json::to_string(&snapshot(&infos, lines)).unwrap_or_default();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() == Some(text.as_str()) {
        return;
    }
    if Registry::open(state_dir)
        .and_then(|r| r.put(TERMS_KEY, &text))
        .is_ok()
    {
        *last = Some(text);
    }
}

fn payload(state_dir: &Path, reg: &Registry) -> Value {
    let env = autostart::Env::current(state_dir);
    json!({
        "afterReboot": after_reboot(reg).as_str(),
        "autostart": autostart::status(&env, &autostart::run_cmd),
    })
}

pub fn routes(method: &str, segs: &[&str], body: &Value, state_dir: &Path) -> Option<(u16, Value)> {
    if segs != ["v1", "lifecycle"] {
        return None;
    }
    let reg = match Registry::open(state_dir) {
        Ok(r) => r,
        Err(e) => return Some((500, json!({"error": e}))),
    };
    match method {
        "GET" => Some((200, payload(state_dir, &reg))),
        "PATCH" => {
            if let Some(v) = body.get("afterReboot") {
                let Some(m) = v.as_str().and_then(AfterReboot::parse) else {
                    return Some((
                        400,
                        json!({"error": "afterReboot must be agents, shells or nothing"}),
                    ));
                };
                if let Err(e) = reg.put(AFTER_KEY, m.as_str()) {
                    return Some((500, json!({"error": e})));
                }
            }
            let env = autostart::Env::current(state_dir);
            let mut errors = vec![];
            match body.get("autostart").and_then(Value::as_bool) {
                Some(true) => errors.extend(autostart::enable(&env, &autostart::run_cmd).err()),
                Some(false) => errors.extend(autostart::disable(&env, &autostart::run_cmd).err()),
                None => {}
            }
            if body.get("linger").and_then(Value::as_bool) == Some(true) {
                errors.extend(autostart::enable_linger(&env, &autostart::run_cmd, state_dir).err());
            }
            let mut out = payload(state_dir, &reg);
            if !errors.is_empty() {
                out["error"] = json!(errors.join("; "));
            }
            Some((200, out))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_SAVED: &str = r#"{"id":"t","cwd":"/w","title":"T","cols":80,"rows":24,"agent":"claude","session":"s","flags":["--x"],"lineId":"l","taskId":"k","workspaceId":"w1","titleOverride":"O","profile":{"id":"p1","name":"Fast","model":"m","effort":"high"}}"#;

    #[test]
    fn saved_json_is_pinned() {
        let sv = Saved {
            id: "t".into(),
            cwd: "/w".into(),
            title: "T".into(),
            cols: 80,
            rows: 24,
            agent: Some("claude".into()),
            session: Some("s".into()),
            flags: vec!["--x".into()],
            line_id: Some("l".into()),
            task_id: Some("k".into()),
            workspace_id: Some("w1".into()),
            title_override: Some("O".into()),
            profile: Some(crate::profiles::Chip {
                id: "p1".into(),
                name: "Fast".into(),
                model: "m".into(),
                effort: "high".into(),
            }),
        };
        let want: serde_json::Value = serde_json::from_str(FULL_SAVED).unwrap();
        assert_eq!(serde_json::to_value(&sv).unwrap(), want);
        assert_eq!(serde_json::from_str::<Saved>(FULL_SAVED).unwrap(), sv);
    }

    #[test]
    fn saved_minimal_json_takes_defaults() {
        let sv: Saved = serde_json::from_str(r#"{"id":"t","cwd":"/w"}"#).unwrap();
        assert_eq!(
            sv,
            Saved {
                id: "t".into(),
                cwd: "/w".into(),
                ..Default::default()
            }
        );
        assert!(sv.flags.is_empty() && sv.agent.is_none() && sv.cols == 0);
    }

    fn info(id: &str, cmd: &[&str], exited: bool) -> Info {
        Info {
            id: id.into(),
            pid: 1,
            cwd: format!("/w/{id}"),
            cmd: cmd.iter().map(|s| s.to_string()).collect(),
            title: id.into(),
            title_override: None,
            cols: 100,
            rows: 30,
            created_ms: 0,
            exited,
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
        }
    }

    fn line(id: &str, term: &str, sid: &str, alive: bool) -> Line {
        Line {
            id: id.into(),
            agent: "claude",
            session_id: sid.into(),
            term_id: Some(term.into()),
            pid: alive.then_some(42),
            state: if alive {
                crate::wire::STATE_ACTIVE
            } else {
                crate::wire::STATE_IDLE
            },
            launch_args: vec!["--model".into(), "opus".into()],
            ..Default::default()
        }
    }

    #[test]
    fn reboot_is_a_changed_boot_id() {
        assert!(is_reboot(Some("a"), Some("b")));
        assert!(!is_reboot(Some("a"), Some("a")), "daemon restart");
        assert!(!is_reboot(None, Some("b")), "first run");
        assert!(!is_reboot(Some("a"), None));
        assert_eq!(
            parse_boottime("{ sec = 1793300000, usec = 412345 } Thu Oct 29 21:00:00 2026\n")
                .as_deref(),
            Some("1793300000")
        );
        assert_eq!(parse_boottime("garbage"), None);
    }

    #[test]
    fn snapshot_keeps_live_terms_with_the_agent_session_each_runs_now() {
        let terms = [
            info("t1", &["claude", "--session-id", "launch"], false),
            info("t2", &["zsh"], false),
            info("t3", &["claude"], true),
            info(
                "t4",
                &["sh", "-c", "trap : INT; sb stub L9; exec sh"],
                false,
            ),
            info("t5", &["claude", "--resume", "dead"], false),
            info(
                "t6",
                &["sh", "-c", "trap : INT; sb stub L6; exec sh"],
                false,
            ),
        ];
        let lines = [
            line("L1", "t1", "current", true),
            line("L5", "t5", "dead", false),
            line("L6", "t6", "resumed", true),
        ];
        let mut terms = terms;
        terms[1].workspace_id = Some("w1".into());
        terms[1].title_override = Some("build".into());
        let chip = crate::profiles::Chip {
            id: "economy".into(),
            name: "Economy".into(),
            model: "sonnet".into(),
            effort: "low".into(),
        };
        terms[0].profile = Some(chip.clone());
        terms[1].profile = Some(chip.clone());
        let s = snapshot(&terms, &lines);
        assert_eq!(s[0].profile, Some(chip), "the agent keeps its profile chip");
        assert_eq!(s[1].profile, None, "a shell left by the agent has no chip");
        assert_eq!(
            s.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["t1", "t2", "t5", "t6"]
        );
        assert_eq!(
            s[3].session.as_deref(),
            Some("resumed"),
            "an agent resumed under the held wrapper is restored like any other"
        );
        assert_eq!(s[1].workspace_id.as_deref(), Some("w1"));
        assert_eq!(s[1].title_override.as_deref(), Some("build"));
        assert_eq!(s[0].session.as_deref(), Some("current"));
        assert_eq!(s[0].agent.as_deref(), Some("claude"));
        assert_eq!(s[0].line_id.as_deref(), Some("L1"));
        assert_eq!(s[0].flags, ["--model", "opus"]);
        assert_eq!(s[1].agent, None);
        assert_eq!(
            s[2].session, None,
            "an agent that was not running stays stopped"
        );
    }

    #[test]
    fn a_stored_initial_prompt_is_not_passed_to_resume() {
        let resumed = |agent: &str, flags: &[&str]| {
            let s = Saved {
                id: "t".into(),
                cwd: "/w".into(),
                agent: Some(agent.into()),
                session: Some("sid".into()),
                flags: flags.iter().map(|f| f.to_string()).collect(),
                ..Default::default()
            };
            plan(vec![s], AfterReboot::Agents, |_| true).remove(0).cmd
        };
        assert_eq!(
            resumed("claude", &["--effort", "high", "File this issue"]),
            ["claude", "--effort", "high", "--resume", "sid"]
        );
        assert_eq!(
            resumed("codex", &["-m", "o3", "File this issue"]),
            ["codex", "resume", "-m", "o3", "sid"]
        );
        assert_eq!(
            resumed("gemini", &["-m", "pro"]),
            ["gemini", "-m", "pro", "--resume", "sid"]
        );
    }

    #[test]
    fn plan_follows_the_setting() {
        let terms = [
            info("t1", &["claude"], false),
            info("t2", &["zsh"], false),
            info("t3", &["zsh"], false),
        ];
        let saved = snapshot(&terms, &[line("L1", "t1", "s1", true)]);
        let exists = |p: &str| p != "/w/t3";
        let agents = plan(saved.clone(), AfterReboot::Agents, exists);
        assert_eq!(agents.len(), 2, "missing cwd is skipped");
        assert_eq!(
            agents[0].cmd,
            ["claude", "--model", "opus", "--resume", "s1"]
        );
        assert_eq!(agents[0].saved.line_id.as_deref(), Some("L1"));
        assert_eq!(
            agents[1].cmd,
            Vec::<String>::new(),
            "shells start the login shell"
        );
        let mut shell = saved[1].clone();
        shell.profile = Some(crate::profiles::Chip::default());
        assert_eq!(
            plan(vec![shell], AfterReboot::Agents, exists)[0]
                .saved
                .profile,
            None,
            "a term restored as a shell drops the chip"
        );
        let shells = plan(saved.clone(), AfterReboot::Shells, exists);
        assert_eq!(shells.len(), 1);
        assert_eq!(shells[0].saved.id, "t2");
        assert!(plan(saved, AfterReboot::Nothing, exists).is_empty());
    }

    #[test]
    fn on_start_restores_after_a_reboot_or_a_restart_but_not_a_takeover() {
        let d = std::env::temp_dir().join(format!(
            "sb-lifecycle-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        std::fs::create_dir_all(&d).unwrap();
        let reg = Registry::open(&d).unwrap();
        let cwd = d.to_string_lossy().into_owned();
        let saved = vec![Saved {
            id: "sbtest-restore-1".into(),
            cwd: cwd.clone(),
            cols: 80,
            rows: 24,
            workspace_id: Some("w1".into()),
            title_override: Some("build".into()),
            ..Default::default()
        }];
        reg.put(TERMS_KEY, &serde_json::to_string(&saved).unwrap())
            .unwrap();
        let Some(now) = boot_id() else {
            return;
        };
        reg.put(BOOT_KEY, "an-older-boot").unwrap();
        on_start(&d, true);
        assert!(
            crate::term::get("sbtest-restore-1").is_none(),
            "takeover: nothing"
        );
        reg.put(BOOT_KEY, "an-older-boot").unwrap();
        reg.put(AFTER_KEY, "nothing").unwrap();
        on_start(&d, false);
        assert!(
            crate::term::get("sbtest-restore-1").is_none(),
            "setting: nothing"
        );
        assert_eq!(reg.get(BOOT_KEY).unwrap(), Some(now.clone()));
        reg.put(BOOT_KEY, "an-older-boot").unwrap();
        reg.put(AFTER_KEY, "shells").unwrap();
        on_start(&d, false);
        let t = crate::term::get("sbtest-restore-1").expect("restored after a reboot");
        assert_eq!(t.info().cwd, cwd);
        assert_eq!(t.info().workspace_id.as_deref(), Some("w1"));
        assert_eq!(t.info().title_override.as_deref(), Some("build"));
        let _ = t.kill();
        crate::term::remove("sbtest-restore-1");
        reg.put(BOOT_KEY, &now).unwrap();
        on_start(&d, false);
        let t = crate::term::get("sbtest-restore-1")
            .expect("restored after a restart on the same boot");
        let _ = t.kill();
        let _ = std::fs::remove_dir_all(&d);
    }
}
