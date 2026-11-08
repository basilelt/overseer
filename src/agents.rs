use serde_json::{json, Value};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const EFFORTS: &[&str] = &["low", "medium", "high", "max"];
pub const AGY_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

type Argv = &'static [&'static str];
type Renames = &'static [(&'static str, &'static str)];

pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub bin: &'static str,
    pub install: &'static str,
    pub verified: bool,
    pub sessions: bool,
    pub turns: bool,
    pub resume_verified: bool,
    pub models: Option<&'static [&'static str]>,
    pub model_flag: Option<&'static str>,
    pub effort: Option<(Argv, Renames)>,
    pub session: Option<&'static [&'static str]>,
    pub prompt: Option<&'static str>,
    pub resume: Option<&'static [&'static str]>,
    pub cont: Option<&'static [&'static str]>,
}

const NONE: Agent = Agent {
    id: "",
    name: "",
    bin: "",
    install: "",
    verified: true,
    sessions: false,
    turns: false,
    resume_verified: false,
    models: None,
    model_flag: None,
    effort: None,
    session: None,
    prompt: None,
    resume: None,
    cont: None,
};

pub static AGENTS: &[Agent] = &[
    Agent {
        id: "claude",
        name: "Claude Code",
        bin: "claude",
        install: "curl -fsSL https://claude.ai/install.sh | bash",
        sessions: true,
        turns: true,
        resume_verified: true,
        models: Some(&["opus", "sonnet", "haiku", "fable"]),
        model_flag: Some("--model"),
        effort: Some((&["--effort", "{}"], &[])),
        session: Some(&["--session-id", "{}"]),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: "codex",
        name: "Codex",
        bin: "codex",
        install: "npm install -g @openai/codex",
        sessions: true,
        turns: true,
        resume_verified: true,
        model_flag: Some("-m"),
        effort: Some((&["-c", "model_reasoning_effort={}"], &[("max", "xhigh")])),
        resume: Some(&["resume", "{flags}", "{id}"]),
        cont: Some(&["resume", "--last"]),
        ..NONE
    },
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        bin: "gemini",
        install: "npm install -g @google/gemini-cli",
        sessions: true,
        model_flag: Some("-m"),
        session: Some(&["--session-id", "{}"]),
        prompt: Some("-i"),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["--resume", "latest"]),
        ..NONE
    },
    Agent {
        id: "agy",
        name: "Antigravity",
        bin: "agy",
        install: "see https://antigravity.google/download",
        model_flag: Some("--model"),
        effort: Some((&["--effort", "{}"], &[])),
        prompt: Some("-i"),
        resume: Some(&["{flags}", "--conversation", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: "opencode",
        name: "opencode",
        bin: "opencode",
        install: "npm install -g --allow-scripts=opencode-ai opencode-ai",
        model_flag: Some("-m"),
        prompt: Some("--prompt"),
        resume: Some(&["{flags}", "--session", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: "amp",
        name: "Amp",
        bin: "amp",
        install: "npm install -g --allow-scripts=@ampcode/cli @sourcegraph/amp",
        models: Some(&[]),
        effort: Some((&["--mode", "{}"], &[("max", "ultra")])),
        resume: Some(&["threads", "continue", "{id}"]),
        cont: Some(&["threads", "continue", "--last"]),
        ..NONE
    },
    Agent {
        id: "copilot",
        name: "Copilot CLI",
        bin: "copilot",
        install: "npm install -g @github/copilot",
        resume_verified: true,
        models: Some(&[
            "claude-sonnet-4.6",
            "claude-haiku-4.5",
            "claude-opus-4.7",
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.3-codex",
            "gpt-5-mini",
        ]),
        model_flag: Some("--model"),
        effort: Some((&["--effort", "{}"], &[("max", "xhigh")])),
        session: Some(&["--session-id={}"]),
        prompt: Some("-i"),
        resume: Some(&["{flags}", "--resume={id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: "cursor",
        name: "Cursor Agent",
        bin: "cursor-agent",
        install: "curl https://cursor.com/install -fsS | bash",
        verified: false,
        model_flag: Some("--model"),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["resume"]),
        ..NONE
    },
    Agent {
        id: "shell",
        name: "Shell",
        models: Some(&[]),
        ..NONE
    },
];

pub fn get(id: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.id == id)
}

pub fn id_of(id: &str) -> &'static str {
    get(id).map(|a| a.id).unwrap_or("other")
}

pub fn task_agent(id: &str) -> bool {
    get(id).is_some_and(|a| a.id != "shell")
}

fn base(arg: &str) -> &str {
    arg.rsplit(['/', '\\']).next().unwrap_or(arg)
}

pub fn of_argv(argv: &[String]) -> Option<&'static Agent> {
    let first = argv.first().map(|a| base(a))?;
    AGENTS
        .iter()
        .find(|a| !a.bin.is_empty() && (first == a.bin || first == format!("{}.exe", a.bin)))
}

fn fill(parts: &[&str], v: &str) -> Vec<String> {
    parts.iter().map(|p| p.replace("{}", v)).collect()
}

pub struct Launch<'a> {
    pub model: &'a str,
    pub effort: &'a str,
    pub session: Option<&'a str>,
    pub prompt: Option<&'a str>,
    pub extra: &'a [String],
}

impl Agent {
    pub fn program(&self) -> String {
        if self.bin.is_empty() {
            crate::term::default_shell()
        } else {
            self.bin.to_string()
        }
    }

    pub fn efforts(&self) -> &'static [&'static str] {
        if self.effort.is_some() {
            if self.id == "agy" {
                AGY_EFFORTS
            } else {
                EFFORTS
            }
        } else {
            &[]
        }
    }

    pub fn valid_efforts(&self) -> &'static [&'static str] {
        if self.effort.is_some() {
            self.efforts()
        } else {
            EFFORTS
        }
    }

    pub fn flags(&self, model: &str, effort: &str) -> Vec<String> {
        let mut a = vec![];
        if let (Some(f), false) = (self.model_flag, model.is_empty()) {
            a.extend([f.to_string(), model.to_string()]);
        }
        if let (Some((parts, map)), false) = (self.effort, effort.is_empty()) {
            let e = map
                .iter()
                .find(|(k, _)| *k == effort)
                .map_or(effort, |(_, v)| v);
            a.extend(fill(parts, e));
        }
        a
    }

    pub fn launch(&self, l: &Launch) -> Vec<String> {
        let mut a = vec![self.program()];
        if self.bin.is_empty() {
            return a;
        }
        a.extend(l.extra.iter().cloned());
        if let (Some(parts), Some(s)) = (self.session, l.session) {
            a.extend(fill(parts, s));
        }
        a.extend(self.flags(l.model, l.effort));
        if let (Some(flag), Some(p)) = (self.prompt, l.prompt) {
            a.extend([flag.to_string(), p.to_string()]);
        }
        a
    }

    pub fn has_ids(&self) -> bool {
        self.sessions || self.session.is_some()
    }

    pub fn resume_argv(&self, session_id: &str, flags: &[String]) -> Option<Vec<String>> {
        let tpl = if self.has_ids() {
            self.resume?
        } else {
            self.cont?
        };
        let mut a = vec![self.program()];
        for p in tpl {
            match *p {
                "{flags}" => a.extend(flags.iter().cloned()),
                p => a.push(p.replace("{id}", session_id)),
            }
        }
        Some(a)
    }

    fn to_json(&self, version: Option<&str>) -> Value {
        let mut v = json!({
            "id": self.id, "name": self.name, "installed": version.is_some(),
            "efforts": self.efforts(), "resume": self.resume.is_some() || self.cont.is_some(),
            "verified": self.verified, "sessions": self.sessions, "turns": self.turns,
        });
        if let Some(ver) = version.filter(|v| !v.is_empty()) {
            v["version"] = json!(ver);
        }
        if let Some(m) = self.models {
            v["models"] = json!(m);
        }
        if !self.install.is_empty() {
            v["install"] = json!(self.install);
        }
        v
    }
}

pub fn redact(argv: &[String], agent: &Agent, prompt: Option<&str>) -> Vec<String> {
    let (Some(flag), Some(p)) = (agent.prompt, prompt) else {
        return argv.to_vec();
    };
    let mut out = argv.to_vec();
    if let Some(i) = out.windows(2).position(|w| w[0] == flag && w[1] == p) {
        out[i + 1] = "<prompt>".into();
    }
    out
}

// Windows: std::process::Command only finds `<bin>.exe` on PATH, so an npm install (`claude.cmd`)
// went undetected while the terminal (portable-pty searches PATHEXT) still launched it.
pub fn on_path(bin: &str, path: &std::ffi::OsStr, exts: &str) -> Option<std::path::PathBuf> {
    std::env::split_paths(path).find_map(|dir| {
        exts.split(';')
            .filter(|e| !e.is_empty())
            .map(|e| dir.join(format!("{bin}{e}")))
            .find(|p| p.is_file())
    })
}

fn program(bin: &str) -> std::path::PathBuf {
    if cfg!(windows) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        if let Some(p) = on_path(bin, &path, &exts) {
            return p;
        }
    }
    bin.into()
}

pub fn version(bin: &str) -> Option<String> {
    let o = crate::git::output_within(
        Command::new(program(bin)).arg("--version"),
        Duration::from_secs(5),
    )
    .ok()?;
    let out = String::from_utf8_lossy(&o.stdout);
    o.status.success().then(|| {
        out.lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim()
            .chars()
            .take(80)
            .collect()
    })
}

type Detected = Vec<Option<String>>;

static CACHE: Mutex<Option<(Instant, String, Detected)>> = Mutex::new(None);

fn detect() -> Detected {
    let handles: Vec<_> = AGENTS
        .iter()
        .map(|a| {
            std::thread::spawn(move || {
                if a.bin.is_empty() {
                    Some(String::new())
                } else {
                    version(a.bin)
                }
            })
        })
        .collect();
    handles
        .into_iter()
        .map(|h| h.join().ok().flatten())
        .collect()
}

pub fn installed() -> Detected {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, p, d)) = g.as_ref() {
        if *p == path && at.elapsed() < Duration::from_secs(60) {
            return d.clone();
        }
    }
    let d = detect();
    *g = Some((Instant::now(), path, d.clone()));
    d
}

pub fn is_installed(a: &Agent) -> bool {
    AGENTS
        .iter()
        .position(|x| x.id == a.id)
        .and_then(|i| installed().get(i).cloned().flatten())
        .is_some()
}

pub fn list() -> Value {
    let d = installed();
    json!(AGENTS
        .iter()
        .zip(d.iter())
        .map(|(a, v)| a.to_json(v.as_deref()))
        .collect::<Vec<_>>())
}

pub fn rescan() -> Value {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_path_finds_a_cmd_shim_by_pathext() {
        let root = std::env::temp_dir().join(format!("ov-onpath-{}", std::process::id()));
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("claude"), "").unwrap();
        std::fs::write(b.join("claude.cmd"), "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(
            on_path("claude", &path, ".COM;.EXE;.BAT;.cmd"),
            Some(b.join("claude.cmd"))
        );
        std::fs::write(a.join("claude.exe"), "").unwrap();
        assert_eq!(
            on_path("claude", &path, ".exe;.cmd"),
            Some(a.join("claude.exe"))
        );
        assert_eq!(on_path("codex", &path, ".exe;.cmd"), None);
        assert_eq!(on_path("claude", &path, ""), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn launch(id: &str, model: &str, effort: &str, session: Option<&str>) -> Vec<String> {
        get(id).unwrap().launch(&Launch {
            model,
            effort,
            session,
            prompt: Some("do it"),
            extra: &[],
        })
    }

    #[test]
    fn launch_mapping() {
        assert_eq!(
            launch("claude", "opus", "max", Some("u1")),
            s(&[
                "claude",
                "--session-id",
                "u1",
                "--model",
                "opus",
                "--effort",
                "max"
            ])
        );
        assert_eq!(
            launch("codex", "", "max", None),
            s(&["codex", "-c", "model_reasoning_effort=xhigh"])
        );
        assert_eq!(
            launch("gemini", "gemini-2.5-pro", "high", Some("u2")),
            s(&[
                "gemini",
                "--session-id",
                "u2",
                "-m",
                "gemini-2.5-pro",
                "-i",
                "do it"
            ])
        );
        assert_eq!(
            launch("copilot", "gpt-5.2", "max", Some("u3")),
            s(&[
                "copilot",
                "--session-id=u3",
                "--model",
                "gpt-5.2",
                "--effort",
                "xhigh",
                "-i",
                "do it"
            ])
        );
        assert_eq!(
            launch("amp", "ignored", "max", None),
            s(&["amp", "--mode", "ultra"])
        );
        assert_eq!(
            launch("opencode", "anthropic/x", "low", None),
            s(&["opencode", "-m", "anthropic/x", "--prompt", "do it"])
        );
        assert_eq!(
            launch("agy", "", "high", None),
            s(&["agy", "--effort", "high", "-i", "do it"])
        );
        assert_eq!(launch("cursor", "", "high", None), s(&["cursor-agent"]));
    }

    #[test]
    fn resume_mapping() {
        let f = s(&["--model", "opus"]);
        let r = |id: &str| get(id).unwrap().resume_argv("S", &f);
        assert_eq!(
            r("claude").unwrap(),
            s(&["claude", "--model", "opus", "--resume", "S"])
        );
        assert_eq!(
            r("codex").unwrap(),
            s(&["codex", "resume", "--model", "opus", "S"])
        );
        assert_eq!(
            r("copilot").unwrap(),
            s(&["copilot", "--model", "opus", "--resume=S"])
        );
        assert_eq!(r("opencode").unwrap(), s(&["opencode", "--continue"]));
        assert_eq!(
            r("amp").unwrap(),
            s(&["amp", "threads", "continue", "--last"])
        );
        assert!(r("shell").is_none());
    }

    #[test]
    fn redacts_prompt() {
        let a = get("gemini").unwrap();
        let argv = launch("gemini", "", "", None);
        assert_eq!(
            redact(&argv, a, Some("do it")),
            s(&["gemini", "-i", "<prompt>"])
        );
    }

    #[test]
    fn argv_detection() {
        assert_eq!(
            of_argv(&s(&["/usr/bin/cursor-agent"])).map(|a| a.id),
            Some("cursor")
        );
        assert_eq!(
            of_argv(&s(&["gemini", "-i", "x"])).map(|a| a.id),
            Some("gemini")
        );
        assert!(of_argv(&s(&["bash"])).is_none());
        assert_eq!(id_of("nope"), "other");
        assert!(!task_agent("shell"));
    }
}
