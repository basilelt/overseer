use serde_json::{json, Value};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const EFFORTS: &[&str] = &["low", "medium", "high", "max"];
pub const AGY_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

type Argv = &'static [&'static str];
type Renames = &'static [(&'static str, &'static str)];

pub enum Install {
    None,
    Command(&'static str),
    Manual(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Transcript {
    Claude,
    Codex,
    Gemini,
    Vibe,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Process {
    ArgContains(&'static str),
    BinOrScript {
        bin: &'static str,
        script: &'static str,
    },
    BinOrPackage {
        bin: &'static str,
        package: &'static str,
    },
    BinOrNative {
        bins: &'static [&'static str],
    },
}

impl Process {
    pub fn matches(self, arg: &str) -> bool {
        match self {
            Process::ArgContains(s) => arg.contains(s),
            Process::BinOrScript { bin, script } => {
                let base = arg.rsplit('/').next().unwrap_or(arg);
                base == bin || base == script
            }
            Process::BinOrPackage { bin, package } => {
                let base = arg.rsplit('/').next().unwrap_or(arg);
                base == bin || arg.contains(package)
            }
            Process::BinOrNative { bins } => {
                let base = arg.rsplit(['/', '\\']).next().unwrap_or(arg);
                bins.contains(&base)
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mcp {
    None,
    Claude,
    Codex,
    VibeTrust,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hooks {
    Native,
    Plugin,
    Polling,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfileKind {
    Claude,
    Codex,
    MarkdownSubagents,
    Opencode,
    Vibe,
    Plain,
    Shell,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repair {
    None,
    Claude,
    Codex,
}

pub const CODEX_TRUST: &str = "Codex runs edited hooks only after you trust them again: open codex and accept the hooks prompt (Overseer never edits config.toml).";

pub struct StartupPrompt {
    pub marker: &'static str,
    pub cursor: char,
    pub accept_label: &'static str,
    pub accept: &'static [u8],
    pub select_then_accept: &'static [u8],
}

pub struct ConfigDir {
    pub env: Option<&'static str>,
    pub default: &'static str,
}

pub struct Agent {
    pub id: &'static str,
    pub name: &'static str,
    pub bin: &'static str,
    pub install: Install,
    pub config: Option<ConfigDir>,
    pub auto_approve: Option<&'static [&'static str]>,
    pub approval_overrides: &'static [&'static str],
    pub effort_levels: &'static [&'static str],
    pub transcript: Option<Transcript>,
    pub process: Option<Process>,
    pub subagent_models: Option<&'static [&'static str]>,
    pub subagent_effort: bool,
    pub oauth_limits: bool,
    pub embedded_daemon: bool,
    pub metered: bool,
    pub mcp: Mcp,
    pub hooks: Hooks,
    pub hook_note: Option<&'static str>,
    pub retrust_hooks: bool,
    pub hooks_need_cli: bool,
    pub profile: ProfileKind,
    pub repair: Repair,
    pub start_args: bool,
    pub tmux_snapshot: bool,
    pub orchestrator: bool,
    pub positional_prompt: bool,
    pub verified: bool,
    pub turns: bool,
    pub resume_verified: bool,
    pub models: Option<&'static [&'static str]>,
    pub model_flag: Option<&'static str>,
    pub effort: Option<(Argv, Renames)>,
    pub session: Option<&'static [&'static str]>,
    pub prompt: Option<&'static str>,
    pub resume: Option<&'static [&'static str]>,
    pub cont: Option<&'static [&'static str]>,
    pub startup_prompt: Option<StartupPrompt>,
}

const NONE: Agent = Agent {
    id: "",
    name: "",
    bin: "",
    install: Install::None,
    config: None,
    auto_approve: None,
    approval_overrides: &[],
    effort_levels: EFFORTS,
    transcript: None,
    process: None,
    subagent_models: None,
    subagent_effort: false,
    oauth_limits: false,
    embedded_daemon: false,
    metered: false,
    mcp: Mcp::None,
    hooks: Hooks::Polling,
    hook_note: None,
    retrust_hooks: false,
    hooks_need_cli: false,
    profile: ProfileKind::Plain,
    repair: Repair::None,
    start_args: false,
    tmux_snapshot: false,
    orchestrator: false,
    positional_prompt: false,
    verified: true,
    turns: false,
    resume_verified: false,
    models: None,
    model_flag: None,
    effort: None,
    session: None,
    prompt: None,
    resume: None,
    cont: None,
    startup_prompt: None,
};

pub static AGENTS: &[Agent] = &[
    Agent {
        id: CLAUDE,
        startup_prompt: Some(StartupPrompt {
            marker: "Itrustthisfolder",
            cursor: '❯',
            accept_label: "Yes",
            accept: b"\r",
            select_then_accept: b"\x1b[B\r",
        }),
        transcript: Some(Transcript::Claude),
        process: Some(Process::ArgContains("claude")),
        subagent_effort: true,
        oauth_limits: true,
        metered: true,
        mcp: Mcp::Claude,
        hooks: Hooks::Native,
        profile: ProfileKind::Claude,
        repair: Repair::Claude,
        start_args: true,
        tmux_snapshot: true,
        orchestrator: true,
        positional_prompt: true,
        auto_approve: Some(&["--permission-mode", "auto"]),
        approval_overrides: &[
            "--permission-mode",
            "--dangerously-skip-permissions",
            "--allow-dangerously-skip-permissions",
        ],
        config: Some(ConfigDir {
            env: Some("CLAUDE_CONFIG_DIR"),
            default: ".claude",
        }),
        name: "Claude Code",
        bin: "claude",
        install: Install::Command("curl -fsSL https://claude.ai/install.sh | bash"),
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
        id: CODEX,
        transcript: Some(Transcript::Codex),
        process: Some(Process::BinOrScript {
            bin: "codex",
            script: "codex.js",
        }),
        metered: true,
        mcp: Mcp::Codex,
        hooks: Hooks::Native,
        hook_note: Some(CODEX_TRUST),
        retrust_hooks: true,
        profile: ProfileKind::Codex,
        repair: Repair::Codex,
        orchestrator: true,
        positional_prompt: true,
        auto_approve: Some(&["--ask-for-approval", "never"]),
        approval_overrides: &[
            "--ask-for-approval",
            "-a",
            "--full-auto",
            "--yolo",
            "--dangerously-bypass-approvals-and-sandbox",
        ],
        config: Some(ConfigDir {
            env: Some("CODEX_HOME"),
            default: ".codex",
        }),
        name: "Codex",
        bin: "codex",
        install: Install::Command("npm install -g @openai/codex"),
        turns: true,
        resume_verified: true,
        model_flag: Some("-m"),
        effort: Some((&["-c", "model_reasoning_effort={}"], &[("max", "xhigh")])),
        resume: Some(&["resume", "{flags}", "{id}"]),
        cont: Some(&["resume", "--last"]),
        embedded_daemon: true,
        ..NONE
    },
    Agent {
        id: GEMINI,
        transcript: Some(Transcript::Gemini),
        process: Some(Process::BinOrPackage {
            bin: "gemini",
            package: "/@google/gemini-cli/",
        }),
        metered: true,
        hooks: Hooks::Native,
        hooks_need_cli: true,
        profile: ProfileKind::MarkdownSubagents,
        auto_approve: Some(&["--approval-mode=yolo"]),
        approval_overrides: &["--approval-mode", "--yolo", "-y"],
        config: Some(ConfigDir {
            env: None,
            default: ".gemini",
        }),
        name: "Gemini CLI",
        bin: "gemini",
        install: Install::Command("npm install -g @google/gemini-cli"),
        model_flag: Some("-m"),
        session: Some(&["--session-id", "{}"]),
        prompt: Some("-i"),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["--resume", "latest"]),
        ..NONE
    },
    Agent {
        id: "agy",
        subagent_models: Some(&["inherit", "flash", "pro"]),
        hook_note: Some("No stable hook file for the Antigravity CLI yet: finished turns come from terminal activity."),
        profile: ProfileKind::MarkdownSubagents,
        effort_levels: AGY_EFFORTS,
        auto_approve: Some(&["--mode=auto"]),
        approval_overrides: &["--mode"],
        name: "Antigravity",
        bin: "agy",
        install: Install::Manual("see https://antigravity.google/download"),
        model_flag: Some("--model"),
        effort: Some((&["--effort", "{}"], &[])),
        prompt: Some("-i"),
        resume: Some(&["{flags}", "--conversation", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: OPENCODE,
        hooks: Hooks::Plugin,
        profile: ProfileKind::Opencode,
        auto_approve: Some(&["--auto"]),
        approval_overrides: &["--auto"],
        name: "opencode",
        bin: "opencode",
        install: Install::Command("npm install -g --allow-scripts=opencode-ai opencode-ai"),
        model_flag: Some("-m"),
        prompt: Some("--prompt"),
        resume: Some(&["{flags}", "--session", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: AMP,
        hooks: Hooks::Plugin,
        hook_note: Some("Amp has no needs-input event: Overseer reports turn start and end only."),
        name: "Amp",
        bin: "amp",
        install: Install::Command("npm install -g --allow-scripts=@ampcode/cli @sourcegraph/amp"),
        models: Some(&[]),
        effort: Some((&["--mode", "{}"], &[("max", "ultra")])),
        resume: Some(&["threads", "continue", "{id}"]),
        cont: Some(&["threads", "continue", "--last"]),
        ..NONE
    },
    Agent {
        id: COPILOT,
        hooks: Hooks::Native,
        auto_approve: Some(&["--allow-all"]),
        approval_overrides: &["--allow-all", "--allow-all-tools", "--yolo"],
        config: Some(ConfigDir {
            env: Some("COPILOT_HOME"),
            default: ".copilot",
        }),
        name: "Copilot CLI",
        bin: "copilot",
        install: Install::Command("npm install -g @github/copilot"),
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
        id: CURSOR,
        hooks: Hooks::Native,
        hook_note: Some("Cursor Agent fires only some hooks in the CLI; a missing turn start falls back to terminal activity."),
        hooks_need_cli: true,
        auto_approve: Some(&["--force"]),
        approval_overrides: &["--force", "-f", "--yolo"],
        config: Some(ConfigDir {
            env: None,
            default: ".cursor",
        }),
        name: "Cursor Agent",
        bin: "cursor-agent",
        install: Install::Command("curl https://cursor.com/install -fsS | bash"),
        verified: false,
        model_flag: Some("--model"),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["resume"]),
        ..NONE
    },
    Agent {
        id: VIBE,
        transcript: Some(Transcript::Vibe),
        process: Some(Process::BinOrNative {
            bins: &["vibe", "vibe.exe", "vibe-rs", "vibe-rs.exe"],
        }),
        mcp: Mcp::VibeTrust,
        hooks: Hooks::Native,
        hook_note: Some("Needs Mistral Vibe 3.0 or later. An interrupted turn fires no hook: the line stays busy until the next turn ends."),
        profile: ProfileKind::Vibe,
        auto_approve: Some(&["--auto-approve"]),
        approval_overrides: &["--auto-approve", "--yolo", "--agent", "--smart-approve"],
        config: Some(ConfigDir {
            env: Some("VIBE_HOME"),
            default: ".vibe",
        }),
        name: "Mistral Vibe",
        bin: "vibe",
        install: Install::Command("uv tool install mistral-vibe"),
        turns: true,
        models: Some(&[]),
        prompt: Some("--"),
        resume: Some(&["{flags}", "--resume", "{id}"]),
        cont: Some(&["--continue"]),
        ..NONE
    },
    Agent {
        id: "shell",
        profile: ProfileKind::Shell,
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

pub fn config_dir(id: &str) -> std::path::PathBuf {
    get(id).and_then(Agent::config_dir).unwrap_or_default()
}

pub const SHELL: &str = "shell";

pub fn is_shell(id: &str) -> bool {
    id == SHELL
}

pub const CLAUDE: &str = "claude";
pub const CODEX: &str = "codex";
pub const GEMINI: &str = "gemini";
pub const VIBE: &str = "vibe";
pub const COPILOT: &str = "copilot";
pub const CURSOR: &str = "cursor";
pub const OPENCODE: &str = "opencode";
pub const AMP: &str = "amp";

pub const DEFAULT_ID: &str = CLAUDE;

pub fn default_agent() -> &'static Agent {
    get(DEFAULT_ID).unwrap_or(&AGENTS[0])
}

pub fn transcript_of(id: &str) -> Option<Transcript> {
    get(id).and_then(|a| a.transcript)
}

pub fn task_agent(id: &str) -> bool {
    get(id).is_some_and(Agent::coding)
}

fn base(arg: &str) -> &str {
    arg.rsplit(['/', '\\']).next().unwrap_or(arg)
}

pub fn runs_as(id: &str, arg: &str) -> bool {
    get(id)
        .and_then(|a| a.process)
        .is_some_and(|p| p.matches(arg))
}

pub fn of_launch_argv(argv: &[String]) -> Option<&'static Agent> {
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
    pub auto_approve: bool,
}

const AUTO_APPROVE_KEY: &str = "agent.autoApprove.";

pub fn auto_approve_args(id: &str) -> Option<&'static [&'static str]> {
    get(id)?.auto_approve
}

pub fn auto_approve_flags(id: &str, extra: &[String]) -> Vec<String> {
    let Some(flags) = auto_approve_args(id) else {
        return vec![];
    };
    let overrides = get(id).map_or(&[][..], |a| a.approval_overrides);
    let set = extra.iter().any(|arg| {
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        overrides.contains(&name)
    });
    if set {
        vec![]
    } else {
        flags.iter().map(|f| (*f).to_string()).collect()
    }
}

pub fn auto_approve_enabled(reg: &crate::registry::Registry, id: &str) -> Result<bool, String> {
    if auto_approve_args(id).is_none() {
        return Ok(false);
    }
    Ok(reg
        .get(&format!("{AUTO_APPROVE_KEY}{id}"))?
        .is_none_or(|value| value == "true"))
}

pub fn auto_approve_settings(reg: &crate::registry::Registry) -> Result<Value, String> {
    let mut settings = serde_json::Map::new();
    for agent in AGENTS {
        if auto_approve_args(agent.id).is_some() {
            settings.insert(
                agent.id.to_string(),
                json!(auto_approve_enabled(reg, agent.id)?),
            );
        }
    }
    Ok(json!({"autoApprove": settings}))
}

pub fn set_auto_approve(
    reg: &crate::registry::Registry,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    if auto_approve_args(id).is_none() {
        return Err(format!("auto approval is not configurable for agent {id}"));
    }
    reg.put(
        &format!("{AUTO_APPROVE_KEY}{id}"),
        if enabled { "true" } else { "false" },
    )
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
            self.effort_levels
        } else {
            &[]
        }
    }

    pub fn valid_efforts(&self) -> &'static [&'static str] {
        self.effort_levels
    }

    pub fn config_dir(&self) -> Option<std::path::PathBuf> {
        let c = self.config.as_ref()?;
        Some(match c.env {
            Some(var) => crate::env_dir(var, c.default),
            None => crate::home().join(c.default),
        })
    }

    pub fn default_config_dir(&self) -> Option<std::path::PathBuf> {
        Some(crate::home().join(self.config.as_ref()?.default))
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

    pub fn mode_flags(&self, effort: &str, extra: &[String]) -> Vec<String> {
        let needs_embedded = self.embedded_daemon
            && (!effort.is_empty()
                || extra
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "-c" | "--config")));
        if needs_embedded && !extra.iter().any(|arg| arg == "--no-daemon") {
            vec!["--no-daemon".to_string()]
        } else {
            vec![]
        }
    }

    pub fn launch(&self, l: &Launch) -> Vec<String> {
        let mut a = vec![self.program()];
        if self.bin.is_empty() {
            return a;
        }
        a.extend(self.mode_flags(l.effort, l.extra));
        a.extend(l.extra.iter().cloned());
        if l.auto_approve {
            a.extend(auto_approve_flags(self.id, l.extra));
        }
        if let (Some(parts), Some(s)) = (self.session, l.session) {
            a.extend(fill(parts, s));
        }
        a.extend(self.flags(l.model, l.effort));
        if let (Some(flag), Some(p)) = (self.prompt, l.prompt) {
            a.extend([flag.to_string(), p.to_string()]);
        }
        a
    }

    pub fn coding(&self) -> bool {
        !is_shell(self.id)
    }

    pub fn sessions(&self) -> bool {
        self.transcript.is_some()
    }

    pub fn polled(&self) -> bool {
        !self.sessions() && !self.bin.is_empty()
    }

    pub fn has_ids(&self) -> bool {
        self.sessions() || self.session.is_some()
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
            "verified": self.verified, "sessions": self.sessions(), "turns": self.turns,
        });
        if let Some(ver) = version.filter(|v| !v.is_empty()) {
            v["version"] = json!(ver);
        }
        if let Some(m) = self.models {
            v["models"] = json!(m);
        }
        match self.install {
            Install::None => {}
            Install::Command(t) | Install::Manual(t) => v["install"] = json!(t),
        }
        if cfg!(unix) && self.install_cmd().is_some() {
            v["installable"] = json!(true);
        }
        v
    }

    pub fn install_cmd(&self) -> Option<&'static str> {
        match self.install {
            Install::Command(c) => Some(c),
            Install::None | Install::Manual(_) => None,
        }
    }
}

#[cfg(unix)]
pub fn install_argv(a: &Agent) -> Result<Vec<String>, String> {
    let cmd = a
        .install_cmd()
        .ok_or_else(|| format!("{} has no install command", a.name))?;
    Ok(vec![
        "/bin/sh".into(),
        "-c".into(),
        format!(
            "printf '%s\\r\\n\\r\\n' \"$1\"; {cmd}; s=$?; [ $s -eq 0 ] || printf '\\r\\n%s %s\\r\\n' \"$2\" \"$s\"; exit $s"
        ),
        "sh".into(),
        format!("$ {cmd}"),
        "Install failed with exit code".into(),
    ])
}

#[cfg(not(unix))]
pub fn install_argv(_: &Agent) -> Result<Vec<String>, String> {
    Err("installing agents from the app is not supported on this platform".into())
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
        let path = std::ffi::OsString::from(crate::shellenv::path());
        let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        if let Some(p) = on_path(bin, &path, &exts) {
            return p;
        }
    }
    bin.into()
}

pub fn version(bin: &str) -> Option<String> {
    let o = crate::exec::output_within(
        Command::new(program(bin))
            .arg("--version")
            .env("PATH", crate::shellenv::path()),
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
    let path = crate::shellenv::path();
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
    crate::shellenv::refresh();
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_runnable_install_commands_are_installable() {
        let j = |id: &str| {
            list()
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["id"] == id)
                .cloned()
                .unwrap()
        };
        assert_eq!(j("codex")["installable"], cfg!(unix));
        assert!(j("agy")["installable"].is_null());
        assert_eq!(
            j("agy")["install"],
            "see https://antigravity.google/download"
        );
        assert!(j("shell")["installable"].is_null());
    }

    #[cfg(unix)]
    #[test]
    fn install_argv_wraps_the_command_and_exits_with_its_status() {
        let argv = install_argv(get("codex").unwrap()).unwrap();
        assert_eq!(argv[..2], ["/bin/sh", "-c"]);
        assert!(argv[2].contains("; npm install -g @openai/codex; s=$?;"));
        assert!(argv[2].ends_with("exit $s"));
        assert_eq!(argv[4], "$ npm install -g @openai/codex");
        assert!(install_argv(get("agy").unwrap()).is_err());
    }

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
            auto_approve: false,
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
            s(&["codex", "--no-daemon", "-c", "model_reasoning_effort=xhigh"])
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
    fn codex_config_override_selects_embedded_mode() {
        let extra = s(&["-c", "mcp_servers.overseer.args=[\"mcp\"]"]);
        let argv = get("codex").unwrap().launch(&Launch {
            model: "",
            effort: "",
            session: None,
            prompt: None,
            extra: &extra,
            auto_approve: false,
        });
        assert_eq!(
            argv,
            s(&[
                "codex",
                "--no-daemon",
                "-c",
                "mcp_servers.overseer.args=[\"mcp\"]",
            ])
        );
    }

    #[test]
    fn auto_approval_uses_each_cli_native_mode() {
        let expected = [
            ("claude", &["--permission-mode", "auto"][..]),
            ("codex", &["--ask-for-approval", "never"][..]),
            ("gemini", &["--approval-mode=yolo"][..]),
            ("agy", &["--mode=auto"][..]),
            ("opencode", &["--auto"][..]),
            ("copilot", &["--allow-all"][..]),
            ("cursor", &["--force"][..]),
        ];
        for (id, flags) in expected {
            assert_eq!(auto_approve_args(id), Some(flags), "{id}");
            let argv = get(id).unwrap().launch(&Launch {
                model: "",
                effort: "",
                session: None,
                prompt: None,
                extra: &[],
                auto_approve: true,
            });
            assert!(
                argv.windows(flags.len()).any(|w| w == flags),
                "{id}: {argv:?}"
            );
        }
        assert_eq!(auto_approve_args("amp"), None);
        assert_eq!(auto_approve_args("shell"), None);
    }

    #[test]
    fn explicit_approval_flags_in_extra_suppress_auto_approve() {
        let cases: [(&str, &[&str]); 9] = [
            ("claude", &["--permission-mode", "acceptEdits"]),
            ("claude", &["--permission-mode=plan"]),
            ("claude", &["--dangerously-skip-permissions"]),
            ("codex", &["-a", "on-request"]),
            ("codex", &["--ask-for-approval=untrusted"]),
            ("gemini", &["--approval-mode", "default"]),
            ("gemini", &["--yolo"]),
            ("agy", &["--mode", "ask"]),
            ("cursor", &["--force"]),
        ];
        for (id, extra) in cases {
            let extra: Vec<String> = extra.iter().map(|a| (*a).to_string()).collect();
            assert!(auto_approve_flags(id, &extra).is_empty(), "{id} {extra:?}");
            let argv = get(id).unwrap().launch(&Launch {
                model: "",
                effort: "",
                session: None,
                prompt: None,
                extra: &extra,
                auto_approve: true,
            });
            let n = auto_approve_args(id).unwrap()[0].split('=').next().unwrap();
            let count = argv
                .iter()
                .filter(|a| a.split('=').next() == Some(n))
                .count();
            assert!(count <= 1, "{id}: {argv:?}");
        }
        let unrelated = vec!["--verbose".to_string()];
        assert_eq!(
            auto_approve_flags("claude", &unrelated),
            vec!["--permission-mode".to_string(), "auto".to_string()]
        );
        assert!(auto_approve_flags("amp", &[]).is_empty());
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
    fn descriptor_tables_match_the_per_agent_behaviour() {
        let ids = |f: fn(&Agent) -> bool| -> Vec<&str> {
            AGENTS.iter().filter(|a| f(a)).map(|a| a.id).collect()
        };
        assert_eq!(ids(|a| a.orchestrator), ["claude", "codex"]);
        assert_eq!(ids(|a| a.positional_prompt), ["claude", "codex"]);
        assert_eq!(
            ids(|a| a.auto_approve.is_some()),
            ["claude", "codex", "gemini", "agy", "opencode", "copilot", "cursor", "vibe"]
        );
        assert_eq!(get("agy").unwrap().efforts(), AGY_EFFORTS);
        assert_eq!(get("claude").unwrap().efforts(), EFFORTS);
        assert!(get("opencode").unwrap().efforts().is_empty());
        assert_eq!(get("opencode").unwrap().valid_efforts(), EFFORTS);
        assert_eq!(get("agy").unwrap().valid_efforts(), AGY_EFFORTS);
        assert!(matches!(get("agy").unwrap().install, Install::Manual(_)));
        assert!(get("shell").unwrap().install_cmd().is_none());
    }

    #[test]
    fn predicates_name_the_agent_groups() {
        let ids = |f: fn(&Agent) -> bool| -> Vec<&str> {
            AGENTS.iter().filter(|a| f(a)).map(|a| a.id).collect()
        };
        assert_eq!(ids(|a| !a.coding()), ["shell"]);
        assert_eq!(ids(Agent::sessions), ["claude", "codex", "gemini", "vibe"]);
        assert_eq!(
            ids(Agent::polled),
            ["agy", "opencode", "amp", "copilot", "cursor"]
        );
        assert!(is_shell("shell") && !is_shell("claude") && !is_shell("nope"));
        assert!(task_agent("claude") && !task_agent("shell") && !task_agent("nope"));
    }

    #[test]
    fn capability_enums_name_every_agent_explicitly() {
        let t = |id: &str| get(id).unwrap();
        let kinds: Vec<_> = AGENTS.iter().filter_map(|a| a.transcript).collect();
        assert_eq!(
            kinds,
            [
                Transcript::Claude,
                Transcript::Codex,
                Transcript::Gemini,
                Transcript::Vibe
            ]
        );
        let metered: Vec<_> = AGENTS.iter().filter(|a| a.metered).map(|a| a.id).collect();
        assert_eq!(metered, ["claude", "codex", "gemini"]);
        assert_eq!(t("claude").mcp, Mcp::Claude);
        assert_eq!(t("codex").mcp, Mcp::Codex);
        assert_eq!(t("vibe").mcp, Mcp::VibeTrust);
        assert_eq!(t("gemini").mcp, Mcp::None);
        assert_eq!(t("claude").repair, Repair::Claude);
        assert_eq!(t("codex").repair, Repair::Codex);
        assert_eq!(t("vibe").repair, Repair::None);
        assert_eq!(t("agy").profile, ProfileKind::MarkdownSubagents);
        assert_eq!(t("gemini").profile, ProfileKind::MarkdownSubagents);
        assert_eq!(t("shell").profile, ProfileKind::Shell);
        assert_eq!(t("copilot").profile, ProfileKind::Plain);
        let start: Vec<_> = AGENTS
            .iter()
            .filter(|a| a.start_args)
            .map(|a| a.id)
            .collect();
        assert_eq!(start, ["claude"]);
        assert_eq!(default_agent().id, DEFAULT_ID);
        assert_eq!(transcript_of("vibe"), Some(Transcript::Vibe));
        assert_eq!(transcript_of("cursor"), None);
        assert_eq!(transcript_of("nope"), None);
    }

    #[test]
    fn config_dirs_come_from_the_descriptor() {
        for id in ["claude", "codex", "gemini", "copilot", "vibe", "cursor"] {
            assert!(config_dir(id).starts_with(crate::home()), "{id}");
        }
        assert!(config_dir("gemini").ends_with(".gemini"));
        assert!(config_dir("opencode").as_os_str().is_empty());
        assert_eq!(
            get("claude").unwrap().default_config_dir(),
            Some(crate::home().join(".claude"))
        );
    }

    #[test]
    fn launch_binary_and_running_process_rules_side_by_side() {
        let table: &[(&str, Option<&str>, Option<&str>)] = &[
            ("/usr/bin/claude", Some("claude"), Some("claude")),
            ("claude.exe", Some("claude"), Some("claude")),
            ("/opt/claude-code/cli.js", None, Some("claude")),
            ("/usr/bin/codex", Some("codex"), Some("codex")),
            ("codex.exe", Some("codex"), None),
            ("/x/bin/codex.js", None, Some("codex")),
            ("C:\\bin\\codex", Some("codex"), None),
            ("/usr/bin/gemini", Some("gemini"), Some("gemini")),
            ("gemini.exe", Some("gemini"), None),
            ("/x/@google/gemini-cli/dist/index.js", None, Some("gemini")),
            ("/home/u/.local/bin/vibe", Some("vibe"), Some("vibe")),
            ("vibe.exe", Some("vibe"), Some("vibe")),
            ("C:\\bin\\vibe", Some("vibe"), Some("vibe")),
            ("/x/site-packages/vibe/_bin/vibe-rs", None, Some("vibe")),
            ("vibe-rs.exe", None, Some("vibe")),
            ("/usr/bin/cursor-agent", Some("cursor"), None),
            ("gemini-helper", None, None),
            ("vibe-app-server", None, None),
            ("/usr/bin/vibes", None, None),
            ("bash", None, None),
        ];
        for (arg, launch, running) in table {
            let launch_id = of_launch_argv(&s(&[arg])).map(|a| a.id);
            let running_id = [CLAUDE, CODEX, GEMINI, VIBE]
                .into_iter()
                .find(|id| runs_as(id, arg));
            assert_eq!(launch_id, *launch, "launch {arg}");
            assert_eq!(running_id, *running, "running {arg}");
        }
    }

    #[test]
    fn argv_detection() {
        assert_eq!(
            of_launch_argv(&s(&["/usr/bin/cursor-agent"])).map(|a| a.id),
            Some("cursor")
        );
        assert_eq!(
            of_launch_argv(&s(&["gemini", "-i", "x"])).map(|a| a.id),
            Some("gemini")
        );
        assert!(of_launch_argv(&s(&["bash"])).is_none());
        assert_eq!(id_of("nope"), "other");
        assert!(!task_agent("shell"));
    }
}
