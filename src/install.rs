use crate::hooks;
use crate::util::shell_quote;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const MARKER: &str = "#overseer-hook";
pub const OLD_MARKER: &str = "#switchboard-hook";

const CODEX_TRUST: &str = "Codex runs edited hooks only after you trust them again: open codex and accept the hooks prompt (Overseer never edits config.toml).";

pub fn shim_path() -> PathBuf {
    let name = if cfg!(windows) {
        "overseer.exe"
    } else {
        "overseer"
    };
    crate::home().join(".overseer/bin").join(name)
}

pub fn path_with_shim(path: &str, shim: &Path) -> String {
    let Some(dir) = shim.parent() else {
        return path.to_string();
    };
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path)
        .filter(|d| !d.as_os_str().is_empty())
        .collect();
    if dirs.iter().any(|d| d == dir) {
        return path.to_string();
    }
    dirs.insert(0, dir.to_path_buf());
    std::env::join_paths(dirs)
        .map_or_else(|_| path.to_string(), |p| p.to_string_lossy().into_owned())
}

pub fn ensure_shim(shim: &Path, real: &Path, replace: bool) -> std::io::Result<bool> {
    if real == shim || real.to_string_lossy().ends_with(" (deleted)") {
        return Ok(false);
    }
    if !replace && shim.exists() {
        return Ok(false);
    }
    if std::fs::read_link(shim).is_ok_and(|p| p == real) {
        return Ok(false);
    }
    if let Some(dir) = shim.parent() {
        crate::mkdir_private(dir)?;
    }
    let tmp = shim.with_extension("sb-tmp");
    let _ = std::fs::remove_file(&tmp);
    link(real, &tmp)?;
    std::fs::rename(&tmp, shim)?;
    Ok(true)
}

#[cfg(unix)]
fn link(real: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(real, at)
}

#[cfg(not(unix))]
fn link(real: &Path, at: &Path) -> std::io::Result<()> {
    std::fs::copy(real, at).map(|_| ())
}

pub fn hook_command(bin: &Path, state_dir: &Path, agent: &str, event: &str) -> String {
    let reply = if agent == "cursor" && event == "beforeSubmitPrompt" {
        r#"'{"continue":true}'"#
    } else {
        "{}"
    };
    format!(
        "OVERSEER_STATE_DIR={} {} hook {agent} {event} \"$PPID\" 2>/dev/null||:;echo {reply};{MARKER}",
        shell_quote(&state_dir.to_string_lossy()),
        shell_quote(&bin.to_string_lossy()),
    )
}

fn ours(h: &Value, marker: &str) -> bool {
    h["command"].as_str().is_some_and(|c| c.contains(marker))
}

fn strip(groups: &mut Vec<Value>, marker: &str) {
    groups.retain(|g| !ours(g, marker));
    for g in groups.iter_mut() {
        if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
            hs.retain(|h| !ours(h, marker));
        }
    }
    groups.retain(|g| {
        g.get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|hs| !hs.is_empty())
    });
}

fn has(entry: &Value, cmd: &str) -> bool {
    entry["command"].as_str() == Some(cmd)
        || entry["hooks"]
            .as_array()
            .is_some_and(|hs| hs.iter().any(|h| h["command"].as_str() == Some(cmd)))
}

fn merge_with(
    existing: &Value,
    events: &[&str],
    command: impl Fn(&str) -> String,
    entry: impl Fn(&str, String) -> Value,
    update: impl Fn(&str, &mut Value),
) -> Value {
    let mut root = match existing {
        Value::Object(_) => existing.clone(),
        _ => json!({}),
    };
    if !root["hooks"].is_object() {
        root["hooks"] = Value::Object(Map::new());
    }
    let hooks = root["hooks"].as_object_mut().expect("object");
    for ev in events {
        let cmd = command(ev);
        let groups = hooks.entry(*ev).or_insert_with(|| json!([]));
        if !groups.is_array() {
            *groups = json!([]);
        }
        let arr = groups.as_array_mut().expect("array");
        if arr.iter().any(|g| has(g, &cmd)) {
            for group in arr.iter_mut().filter(|g| has(g, &cmd)) {
                update(ev, group);
            }
            continue;
        }
        strip(arr, MARKER);
        arr.push(entry(ev, cmd));
    }
    root
}

pub fn merge(existing: &Value, events: &[&str], command: impl Fn(&str) -> String) -> Value {
    merge_groups(existing, events, command, 5)
}

pub fn merge_groups(
    existing: &Value,
    events: &[&str],
    command: impl Fn(&str) -> String,
    timeout: u64,
) -> Value {
    merge_groups_with_timeout(existing, events, command, |_| timeout)
}

fn merge_groups_with_timeout(
    existing: &Value,
    events: &[&str],
    command: impl Fn(&str) -> String,
    timeout: impl Fn(&str) -> u64,
) -> Value {
    merge_with(
        existing,
        events,
        command,
        |event, cmd| json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": timeout(event) }] }),
        |event, group| {
            if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                for hook in hooks.iter_mut().filter(|hook| ours(hook, MARKER)) {
                    hook["timeout"] = json!(timeout(event));
                }
            }
        },
    )
}

pub fn merge_flat(existing: &Value, events: &[&str], command: impl Fn(&str) -> String) -> Value {
    let mut root = merge_with(
        existing,
        events,
        command,
        |_event, cmd| json!({ "command": cmd }),
        |_event, _group| {},
    );
    if root.get("version").is_none() {
        root["version"] = json!(1);
    }
    root
}

pub fn unmerge(existing: &Value) -> Value {
    unmerge_marker(existing, MARKER)
}

pub fn unmerge_marker(existing: &Value, marker: &str) -> Value {
    let mut root = existing.clone();
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        let keys: Vec<String> = hooks.keys().cloned().collect();
        for k in keys {
            if let Some(arr) = hooks.get_mut(&k).and_then(Value::as_array_mut) {
                let had = arr.iter().any(|g| {
                    ours(g, marker)
                        || g["hooks"]
                            .as_array()
                            .is_some_and(|hs| hs.iter().any(|h| ours(h, marker)))
                });
                strip(arr, marker);
                if had && arr.is_empty() {
                    hooks.remove(&k);
                }
            }
        }
    }
    root
}

fn commands(v: &Value) -> Vec<String> {
    let entries = v["hooks"]
        .as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .filter_map(Value::as_array)
        .flatten();
    let mut out = vec![];
    for e in entries {
        out.extend(e["command"].as_str().map(str::to_string));
        for h in e["hooks"].as_array().into_iter().flatten() {
            out.extend(h["command"].as_str().map(str::to_string));
        }
    }
    out
}

fn hook_binary(cmd: &str) -> Option<&str> {
    cmd.split_whitespace()
        .find(|w| !w.contains('='))
        .map(|w| w.trim_matches(['\'', '"']))
}

fn orphaned_switchboard(v: &Value) -> bool {
    let old: Vec<String> = commands(v)
        .into_iter()
        .filter(|c| c.contains(OLD_MARKER))
        .collect();
    !old.is_empty()
        && old
            .iter()
            .all(|c| hook_binary(c).is_some_and(|b| !Path::new(b).exists()))
}

pub fn validate(original: &Value, written: &[u8], expect_ours: &[String]) -> Result<(), String> {
    let v: Value = serde_json::from_slice(written).map_err(|e| format!("invalid JSON: {e}"))?;
    if !v.is_object() || !v["hooks"].is_object() {
        return Err("top level or .hooks is not an object".into());
    }
    let after = commands(&v);
    let orphaned = orphaned_switchboard(original);
    for c in commands(original)
        .into_iter()
        .filter(|c| !c.contains(MARKER) && !(orphaned && c.contains(OLD_MARKER)))
    {
        if !after.contains(&c) {
            return Err("an existing hook would be lost".into());
        }
    }
    if let (Some(a), Some(b)) = (original.as_object(), v.as_object()) {
        if a.keys().any(|k| !b.contains_key(k)) {
            return Err("an existing top-level key would be lost".into());
        }
    }
    for c in expect_ours {
        if !after.contains(c) {
            return Err("overseer hook missing after merge".into());
        }
    }
    Ok(())
}

pub fn pretty(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).unwrap_or_default();
    s.push('\n');
    s
}

pub fn diff(path: &Path, before: &str, after: &str, tmp: &Path) -> String {
    if before == after {
        return String::new();
    }
    let a = tmp.join("sb-diff-a");
    let b = tmp.join("sb-diff-b");
    let ok = crate::mkdir_private(tmp).is_ok()
        && std::fs::write(&a, before).is_ok()
        && std::fs::write(&b, after).is_ok();
    let label = path.to_string_lossy();
    let out = ok
        .then(|| {
            std::process::Command::new("diff")
                .args(["-u", "--label", &format!("{label} (current)")])
                .args(["--label", &format!("{label} (merged)")])
                .arg(&a)
                .arg(&b)
                .output()
                .ok()
        })
        .flatten()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .filter(|s| !s.is_empty());
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    out.unwrap_or_else(|| format!("--- current\n{before}+++ merged\n{after}"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Groups(u64),
    GroupsWithOverrides {
        default: u64,
        overrides: &'static [(&'static str, u64)],
    },
    Flat,
    File,
}

pub struct Target {
    pub agent: &'static str,
    pub path: PathBuf,
    pub events: &'static [&'static str],
    pub format: Format,
    pub home: PathBuf,
}

pub fn targets() -> Vec<Target> {
    const NONE: &[&str] = &[];
    let env_dir = crate::env_dir;
    let config = env_dir("XDG_CONFIG_HOME", ".config");
    let t = |agent, home: PathBuf, file: &str, events, format| Target {
        agent,
        path: home.join(file),
        events,
        format,
        home,
    };
    vec![
        t(
            "claude",
            env_dir("CLAUDE_CONFIG_DIR", ".claude"),
            "settings.json",
            hooks::CLAUDE_EVENTS,
            Format::Groups(5),
        ),
        t(
            "codex",
            env_dir("CODEX_HOME", ".codex"),
            "hooks.json",
            hooks::CODEX_EVENTS,
            Format::GroupsWithOverrides {
                default: 5,
                overrides: &[("SessionEnd", 3)],
            },
        ),
        t(
            "gemini",
            crate::home().join(".gemini"),
            "settings.json",
            hooks::GEMINI_EVENTS,
            Format::Groups(5000),
        ),
        t(
            "copilot",
            env_dir("COPILOT_HOME", ".copilot"),
            "hooks/overseer.json",
            hooks::COPILOT_EVENTS,
            Format::File,
        ),
        t(
            "vibe",
            crate::vibe::home(),
            "hooks.toml",
            hooks::VIBE_EVENTS,
            Format::File,
        ),
        t(
            "cursor",
            crate::home().join(".cursor"),
            "hooks.json",
            hooks::CURSOR_EVENTS,
            Format::Flat,
        ),
        t(
            "opencode",
            config.join("opencode"),
            "plugins/overseer.js",
            NONE,
            Format::File,
        ),
        t(
            "amp",
            config.join("amp"),
            "plugins/overseer.js",
            NONE,
            Format::File,
        ),
    ]
}

pub fn available(t: &Target, cli: bool) -> bool {
    cli || (!matches!(t.agent, "cursor" | "gemini") && t.home.is_dir())
}

pub fn mechanism(agent: &str) -> &'static str {
    match agent {
        "claude" | "codex" | "gemini" | "copilot" | "cursor" | "vibe" => "hooks",
        "opencode" | "amp" => "plugin",
        _ => "polling",
    }
}

pub fn note(agent: &str) -> Option<&'static str> {
    Some(match agent {
        "codex" => CODEX_TRUST,
        "amp" => "Amp has no needs-input event: Overseer reports turn start and end only.",
        "vibe" => "Needs Mistral Vibe 3.0 or later. An interrupted turn fires no hook: the line stays busy until the next turn ends.",
        "cursor" => "Cursor Agent fires only some hooks in the CLI; a missing turn start falls back to terminal activity.",
        "agy" => "No stable hook file for the Antigravity CLI yet: finished turns come from terminal activity.",
        "shell" => return None,
        a if mechanism(a) == "polling" => "No hook API: finished turns come from terminal activity.",
        _ => return None,
    })
}

fn js_str(p: &Path) -> String {
    Value::String(p.to_string_lossy().into_owned()).to_string()
}

const JS_SEND: &str = r##"import { spawn } from "node:child_process";

const MARKER = "#overseer-hook";

function send(event, session) {
  try {
    const child = spawn(BIN, ["hook", AGENT, event, String(process.pid)], {
      env: { ...process.env, OVERSEER_STATE_DIR: STATE },
      stdio: ["pipe", "ignore", "ignore"],
      detached: true,
    });
    child.on("error", () => {});
    child.stdin.on("error", () => {});
    child.stdin.end(JSON.stringify({ session_id: session || "", cwd: process.cwd() }));
    child.unref();
  } catch {}
}
"##;

const OPENCODE_EVENTS: &str = r#"
export const Overseer = async () => ({
  event: async ({ event }) => {
    const p = event?.properties ?? event?.data ?? {};
    switch (event?.type) {
      case "session.created":
        return send("SessionStart", p.info?.id ?? p.sessionID);
      case "session.status":
        if (p.status?.type === "busy") send("UserPromptSubmit", p.sessionID);
        if (p.status?.type === "idle") send("Stop", p.sessionID);
        return;
      case "session.idle":
        return send("Stop", p.sessionID);
      case "session.error":
        return send("StopFailure", p.sessionID);
      case "permission.asked":
      case "permission.updated":
        return send("PermissionRequest", p.sessionID);
      case "permission.replied":
        return send("UserPromptSubmit", p.sessionID);
    }
  },
});
"#;

const AMP_EVENTS: &str = r#"
export default function (amp) {
  amp.on("session.start", (e) => send("SessionStart", e?.thread?.id));
  amp.on("agent.start", (e) => send("UserPromptSubmit", e?.thread?.id));
  amp.on("agent.end", (e) => send("Stop", e?.thread?.id));
}
"#;

pub fn plugin_js(agent: &str, bin: &Path, state_dir: &Path) -> String {
    let events = if agent == "amp" {
        AMP_EVENTS
    } else {
        OPENCODE_EVENTS
    };
    format!(
        "const BIN = {};\nconst STATE = {};\nconst AGENT = {};\n{JS_SEND}{events}",
        js_str(bin),
        js_str(state_dir),
        Value::String(agent.into()),
    )
}

pub fn vibe_toml(bin: &Path, state_dir: &Path) -> String {
    let mut out = String::new();
    for ev in hooks::VIBE_EVENTS {
        let main = if matches!(*ev, "pre_agent" | "post_agent") {
            "match = \"main\"\n"
        } else {
            ""
        };
        out.push_str(&format!(
            "[[hooks]]\nname = \"overseer-{ev}\"\ntype = \"{ev}\"\n{main}timeout = 5.0\ncommand = {}\n\n",
            Value::String(hook_command(bin, state_dir, "vibe", ev))
        ));
    }
    out
}

pub fn copilot_json(bin: &Path, state_dir: &Path) -> String {
    let mut hooks = Map::new();
    for ev in hooks::COPILOT_EVENTS {
        hooks.insert(
            ev.to_string(),
            json!([{ "type": "command", "bash": hook_command(bin, state_dir, "copilot", ev), "timeoutSec": 5 }]),
        );
    }
    pretty(&json!({ "version": 1, "hooks": hooks }))
}

fn owned_text(t: &Target, bin: &Path, state_dir: &Path) -> String {
    match t.agent {
        "copilot" => copilot_json(bin, state_dir),
        "vibe" => vibe_toml(bin, state_dir),
        a => plugin_js(a, bin, state_dir),
    }
}

pub struct Planned {
    pub before: Option<Value>,
    pub before_text: String,
    pub existed: bool,
    pub after: Value,
    pub after_text: String,
    pub ours: Vec<String>,
    pub owned: bool,
}

impl Planned {
    pub fn changed(&self) -> bool {
        if self.owned {
            return if self.after_text.is_empty() {
                self.existed
            } else {
                !self.existed || self.before_text != self.after_text
            };
        }
        match &self.before {
            Some(b) => *b != self.after,
            None => self.after != json!({}),
        }
    }
}

pub fn installed(t: &Target) -> bool {
    std::fs::read_to_string(&t.path).is_ok_and(|s| s.contains(MARKER))
}

pub fn plan(t: &Target, uninstall: bool, bin: &Path, state_dir: &Path) -> Result<Planned, String> {
    let text = match std::fs::read_to_string(&t.path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", t.path.display())),
    };
    let existed = text.is_some();
    if t.format == Format::File {
        let after_text = if uninstall {
            String::new()
        } else {
            owned_text(t, bin, state_dir)
        };
        if existed && !text.as_deref().unwrap_or("").contains(MARKER) {
            return Err(format!(
                "{} exists and was not written by Overseer; not touching it",
                t.path.display()
            ));
        }
        return Ok(Planned {
            before: None,
            before_text: text.unwrap_or_default(),
            existed,
            after: Value::Null,
            after_text,
            ours: vec![],
            owned: true,
        });
    }
    let before: Option<Value> = match &text {
        Some(s) if !s.trim().is_empty() => Some(serde_json::from_str(s).map_err(|e| {
            format!(
                "{} is not valid JSON ({e}); not touching it",
                t.path.display()
            )
        })?),
        _ => None,
    };
    if before.as_ref().is_some_and(|b| !b.is_object()) {
        return Err(format!(
            "{} is not a JSON object; not touching it",
            t.path.display()
        ));
    }
    let mut base = before.clone().unwrap_or_else(|| json!({}));
    if !uninstall && orphaned_switchboard(&base) {
        base = unmerge_marker(&base, OLD_MARKER);
    }
    let cmd = |ev: &str| hook_command(bin, state_dir, t.agent, ev);
    let after = match (uninstall, t.format) {
        (true, _) => unmerge(&base),
        (false, Format::Flat) => merge_flat(&base, t.events, cmd),
        (false, Format::Groups(timeout)) => merge_groups(&base, t.events, cmd, timeout),
        (false, Format::GroupsWithOverrides { default, overrides }) => {
            merge_groups_with_timeout(&base, t.events, cmd, |event| {
                overrides
                    .iter()
                    .find(|(override_event, _)| *override_event == event)
                    .map_or(default, |(_, timeout)| *timeout)
            })
        }
        (false, Format::File) => unreachable!(),
    };
    let ours = if uninstall {
        vec![]
    } else {
        t.events.iter().map(|e| cmd(e)).collect()
    };
    Ok(Planned {
        before,
        before_text: text.unwrap_or_default(),
        existed,
        after_text: pretty(&after),
        after,
        ours,
        owned: false,
    })
}

fn resolved(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    let path = resolved(path);
    let path = path.as_path();
    if let Some(parent) = path.parent() {
        crate::mkdir_private(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let tmp = PathBuf::from(format!("{}.sb-tmp", path.display()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    if let Ok(m) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn backup(src: &Path, state_dir: &Path, stamp: &str) -> Result<PathBuf, String> {
    let dir = state_dir.join("backups");
    crate::mkdir_private(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let flat: String = src
        .to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let b = dir.join(format!("{flat}.bak-{stamp}"));
    std::fs::copy(src, &b).map_err(|e| format!("backup {}: {e}", b.display()))?;
    Ok(b)
}

pub fn write(
    t: &Target,
    p: &Planned,
    state_dir: &Path,
    stamp: &str,
) -> Result<Option<PathBuf>, String> {
    if p.owned {
        if p.after_text.is_empty() {
            if p.existed {
                std::fs::remove_file(&t.path).map_err(|e| format!("{}: {e}", t.path.display()))?;
            }
        } else {
            atomic_write(&t.path, &p.after_text)?;
        }
        return Ok(None);
    }
    let original = p.before.clone().unwrap_or_else(|| json!({}));
    validate(&original, p.after_text.as_bytes(), &p.ours)?;
    let target = resolved(&t.path);
    let backup = if t.path.exists() && !stamp.is_empty() {
        Some(backup(&t.path, state_dir, stamp)?)
    } else {
        None
    };
    if let Some(parent) = target.parent() {
        crate::mkdir_private(parent).map_err(|e| e.to_string())?;
    }
    let tmp = PathBuf::from(format!("{}.sb-tmp", target.display()));
    std::fs::write(&tmp, &p.after_text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    if let Ok(m) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp, m.permissions());
    }
    let reread = std::fs::read(&tmp).map_err(|e| e.to_string())?;
    if let Err(e) = validate(&original, &reread, &p.ours) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(backup)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
    Repair,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub agent: &'static str,
    pub path: PathBuf,
    pub changed: bool,
    pub written: bool,
    pub diff: String,
    pub backup: Option<PathBuf>,
    pub error: Option<String>,
    pub skipped: Option<String>,
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "agent": self.agent,
            "path": self.path.to_string_lossy(),
            "changed": self.changed,
            "written": self.written,
            "diff": self.diff,
        });
        if let Some(b) = &self.backup {
            v["backup"] = json!(b.to_string_lossy());
        }
        if let Some(e) = &self.error {
            v["error"] = json!(e);
        }
        if let Some(s) = &self.skipped {
            v["skipped"] = json!(s);
        }
        if let Some(n) = note(self.agent).filter(|_| self.agent == "codex" && self.written) {
            v["note"] = json!(n);
        }
        v
    }
}

pub struct Run<'a> {
    pub mode: Mode,
    pub apply: bool,
    pub bin: &'a Path,
    pub state_dir: &'a Path,
    pub stamp: &'a str,
}

pub fn run(
    targets: &[Target],
    agents: Option<&[String]>,
    r: &Run,
    available: impl Fn(&Target) -> bool,
) -> Vec<Outcome> {
    let tmp = r.state_dir.join("tmp");
    let mut out = vec![];
    for t in targets {
        if agents.is_some_and(|a| !a.iter().any(|x| x == t.agent)) {
            continue;
        }
        let mut o = Outcome {
            agent: t.agent,
            path: t.path.clone(),
            ..Default::default()
        };
        let present = installed(t);
        let skip = match r.mode {
            Mode::Repair => (!present).then_some("not installed"),
            Mode::Uninstall => (!t.path.exists()).then_some("nothing to remove"),
            Mode::Install => {
                (!present && !available(t)).then_some("not configured on this station")
            }
        };
        if let Some(s) = skip {
            o.skipped = Some(s.into());
            out.push(o);
            continue;
        }
        let planned = match plan(t, r.mode == Mode::Uninstall, r.bin, r.state_dir) {
            Ok(p) => p,
            Err(e) => {
                o.error = Some(e);
                out.push(o);
                continue;
            }
        };
        o.changed = planned.changed();
        if o.changed {
            o.diff = diff(&t.path, &planned.before_text, &planned.after_text, &tmp);
        }
        if o.changed && r.apply {
            match write(t, &planned, r.state_dir, r.stamp) {
                Ok(b) => {
                    o.written = true;
                    o.backup = b;
                }
                Err(e) => o.error = Some(e),
            }
        }
        out.push(o);
    }
    out
}

pub fn status(
    targets: &[Target],
    bin: &Path,
    state_dir: &Path,
    cli: impl Fn(&str) -> bool,
) -> Value {
    let rows: Vec<Value> = crate::agents::AGENTS
        .iter()
        .filter(|a| a.id != "shell")
        .map(|a| {
            let t = targets.iter().find(|t| t.agent == a.id);
            let present = t.is_some_and(installed);
            let current = t.is_some_and(|t| {
                present && plan(t, false, bin, state_dir).is_ok_and(|p| !p.changed())
            });
            let installed_cli = cli(a.id);
            let mut v = json!({
                "agent": a.id,
                "name": a.name,
                "mechanism": mechanism(a.id),
                "cliInstalled": installed_cli,
                "configured": t.is_some_and(|t| available(t, installed_cli)),
                "installed": present,
                "current": current,
            });
            if let Some(t) = t {
                v["path"] = json!(t.path.to_string_lossy());
            }
            if let Some(n) = note(a.id) {
                v["note"] = json!(n);
            }
            v
        })
        .collect();
    json!({ "shim": bin.to_string_lossy(), "agents": rows })
}

pub fn stamp(now_ms: i64) -> String {
    crate::util::format_rfc3339(now_ms)
        .chars()
        .filter(char::is_ascii_digit)
        .take(12)
        .collect()
}

pub fn hook_bin() -> PathBuf {
    let shim = shim_path();
    if shim.exists() {
        return shim;
    }
    std::env::current_exe()
        .and_then(|p| p.canonicalize())
        .unwrap_or(shim)
}

pub fn routes(method: &str, segs: &[&str], body: &Value, state_dir: &Path) -> Option<(u16, Value)> {
    let cli = |id: &str| crate::agents::get(id).is_some_and(crate::agents::is_installed);
    let mode = match (method, segs) {
        ("GET", ["v1", "hooks", "status"]) => {
            return Some((200, status(&targets(), &hook_bin(), state_dir, cli)));
        }
        ("POST", ["v1", "hooks", "install"]) => Mode::Install,
        ("POST", ["v1", "hooks", "uninstall"]) => Mode::Uninstall,
        _ => return None,
    };
    let agents: Option<Vec<String>> = match &body["agents"] {
        Value::Null => None,
        Value::Array(a) => Some(
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        ),
        _ => {
            return Some((
                400,
                json!({"error": "agents must be an array of agent ids"}),
            ))
        }
    };
    let apply = body["dryRun"].as_bool() == Some(false);
    let bin = hook_bin();
    let stamp = stamp(crate::procs::now_ms());
    let r = Run {
        mode,
        apply,
        bin: &bin,
        state_dir,
        stamp: &stamp,
    };
    let results: Vec<Value> = run(&targets(), agents.as_deref(), &r, |t| {
        available(t, cli(t.agent))
    })
    .iter()
    .map(Outcome::to_json)
    .collect();
    Some((200, json!({ "dryRun": !apply, "results": results })))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let c = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    c(a) == c(b)
}

pub fn owns_globals(state_dir: &Path) -> bool {
    same_dir(state_dir, &crate::platform_state_dir())
}

pub fn points_elsewhere(t: &Target, state_dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(&t.path) else {
        return false;
    };
    if !text.contains(MARKER) {
        return false;
    }
    let sh = format!(
        "OVERSEER_STATE_DIR={} ",
        shell_quote(&state_dir.to_string_lossy())
    );
    !text.contains(&sh) && !text.contains(&js_str(state_dir))
}

pub fn on_daemon_start(state_dir: &Path) {
    let owner = owns_globals(state_dir);
    if owner {
        let env = crate::autostart::Env::current(state_dir);
        if let Err(e) = crate::autostart::refresh(&env, &crate::autostart::run_cmd) {
            crate::dlog::line("warn", &format!("overseer daemon: service unit: {e}"));
        }
    }
    let shim = shim_path();
    if let Ok(exe) = std::env::current_exe() {
        if let Err(e) = ensure_shim(&shim, &exe, owner) {
            eprintln!("overseer daemon: {}: {e}", shim.display());
        }
    }
    let bin = hook_bin();
    let stamp = stamp(crate::procs::now_ms());
    let r = Run {
        mode: Mode::Repair,
        apply: true,
        bin: &bin,
        state_dir,
        stamp: &stamp,
    };
    let mut ts = targets();
    if !owner {
        ts.retain(|t| {
            let keep = !points_elsewhere(t, state_dir);
            if !keep {
                crate::dlog::line(
                    "warn",
                    &format!(
                        "overseer daemon: not repairing hooks in {}: they call another state dir and {} is not the default one ({}); only the daemon on the default state dir manages them",
                        t.path.display(),
                        state_dir.display(),
                        crate::platform_state_dir().display()
                    ),
                );
            }
            keep
        });
    }
    for o in run(&ts, None, &r, |_| false) {
        if o.written {
            eprintln!(
                "overseer daemon: hooks in {} now call {}",
                o.path.display(),
                r.bin.display()
            );
        }
        if o.written && o.agent == "codex" {
            eprintln!("overseer daemon: {CODEX_TRUST}");
        }
        if let Some(e) = o.error {
            eprintln!("overseer daemon: hook repair skipped: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn path_with_shim_prepends_the_shim_dir_once() {
        let shim = Path::new("/h/.overseer/bin/overseer");
        assert_eq!(
            path_with_shim("/usr/bin:/bin", shim),
            "/h/.overseer/bin:/usr/bin:/bin"
        );
        assert_eq!(
            path_with_shim("/usr/bin:/h/.overseer/bin", shim),
            "/usr/bin:/h/.overseer/bin"
        );
        assert_eq!(path_with_shim("", shim), "/h/.overseer/bin");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_follows_a_symlinked_config() {
        let d = std::env::temp_dir().join(format!("sb-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("dotfiles")).unwrap();
        let real = d.join("dotfiles/settings.json");
        let link = d.join("settings.json");
        std::fs::write(&real, "{}").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_write(&link, "{\"a\":1}").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "{\"a\":1}");
        let _ = std::fs::remove_dir_all(&d);
    }

    use super::*;

    #[test]
    fn backups_live_in_the_state_dir_with_the_full_path_in_the_name() {
        let d = std::env::temp_dir().join(format!("sb-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("a")).unwrap();
        std::fs::create_dir_all(d.join("b")).unwrap();
        std::fs::write(d.join("a/settings.json"), "one").unwrap();
        std::fs::write(d.join("b/settings.json"), "two").unwrap();
        let state = d.join("state");
        let a = backup(&d.join("a/settings.json"), &state, "202601010000").unwrap();
        let b = backup(&d.join("b/settings.json"), &state, "202601010000").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(state.join("backups")));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "two");
        assert!(!d.join("a/settings.json.bak-202601010000").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    fn cmd(ev: &str) -> String {
        format!("/sb hook claude {ev} {MARKER}")
    }

    fn existing() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "Stop": [{"hooks": [{"type": "command", "command": "~/.claude/hooks/notify.sh"}]}],
                "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "rtk.sh"}]}]
            }
        })
    }

    #[test]
    fn merge_keeps_existing_and_is_idempotent() {
        let once = merge(&existing(), &["Stop", "SessionStart"], cmd);
        assert_eq!(once["model"], "opus");
        assert_eq!(once["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(
            once["hooks"]["Stop"][0]["hooks"][0]["command"],
            "~/.claude/hooks/notify.sh"
        );
        assert_eq!(
            once["hooks"]["SessionStart"][0]["hooks"][0]["command"],
            cmd("SessionStart")
        );
        assert_eq!(
            once["hooks"]["PreToolUse"],
            existing()["hooks"]["PreToolUse"]
        );
        assert_eq!(merge(&once, &["Stop", "SessionStart"], cmd), once);
        let keys: Vec<_> = once.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["model", "hooks"]);
    }

    #[test]
    fn merge_replaces_stale_command() {
        let once = merge(&existing(), &["Stop"], cmd);
        let moved = merge(&once, &["Stop"], |e| format!("/new/sb hook {e} {MARKER}"));
        let stop = moved["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert!(stop[1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .starts_with("/new/sb"));
    }

    #[test]
    fn unmerge_restores_original() {
        let once = merge(&existing(), &["Stop", "SessionStart"], cmd);
        assert_eq!(unmerge(&once), existing());
        assert_eq!(unmerge(&existing()), existing());
    }

    #[test]
    fn validate_detects_loss() {
        let orig = existing();
        let ok = pretty(&merge(&orig, &["Stop"], cmd));
        assert!(validate(&orig, ok.as_bytes(), &[cmd("Stop")]).is_ok());
        assert!(validate(&orig, b"{", &[]).is_err());
        assert!(validate(&orig, br#"{"hooks":{}}"#, &[]).is_err());
        assert!(validate(&orig, ok.as_bytes(), &[cmd("Other")]).is_err());
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-install-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn target(
        d: &Path,
        agent: &'static str,
        file: &str,
        events: &'static [&'static str],
        format: Format,
    ) -> Target {
        Target {
            agent,
            path: d.join(file),
            events,
            format,
            home: d.to_path_buf(),
        }
    }

    fn go(ts: &[Target], mode: Mode, bin: &Path, state: &Path) -> Vec<Outcome> {
        let r = Run {
            mode,
            apply: true,
            bin,
            state_dir: state,
            stamp: "202610290000",
        };
        run(ts, None, &r, |_| true)
    }

    fn read(p: &Path) -> Value {
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn codex_session_end_uses_supported_timeout() {
        let d = dir("codex-timeout");
        let mut codex = targets().into_iter().find(|t| t.agent == "codex").unwrap();
        codex.path = d.join("hooks.json");
        let codex_command = |event: &str| hook_command(Path::new("/sb"), &d, "codex", event);
        let old_config = merge_groups(&json!({}), hooks::CODEX_EVENTS, codex_command, 5);
        std::fs::write(&codex.path, pretty(&old_config)).unwrap();

        let planned = plan(&codex, false, Path::new("/sb"), &d).unwrap();
        assert_eq!(
            planned.after["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"],
            3
        );
        for event in hooks::CODEX_EVENTS
            .iter()
            .filter(|event| **event != "SessionEnd")
        {
            assert_eq!(planned.after["hooks"][*event][0]["hooks"][0]["timeout"], 5);
        }
    }

    fn switchboard_hooks(bin: &Path) -> Value {
        let cmd = |event: &str| {
            format!(
                "SWITCHBOARD_STATE_DIR=/s {} hook codex {event} \"$PPID\" 2>/dev/null||:;echo {{}};{OLD_MARKER}",
                bin.display()
            )
        };
        let mut v = merge_groups(&json!({}), hooks::CODEX_EVENTS, cmd, 5);
        v["hooks"]["SessionEnd"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "hooks": [{ "type": "command", "command": "keep-me", "timeout": 2 }] }));
        v
    }

    fn codex_at(path: PathBuf) -> Target {
        let mut codex = targets().into_iter().find(|t| t.agent == "codex").unwrap();
        codex.path = path;
        codex
    }

    #[test]
    fn install_drops_hooks_of_a_removed_switchboard() {
        let d = dir("switchboard-orphan");
        let codex = codex_at(d.join("hooks.json"));
        let gone = d.join(".switchboard/bin/switchboard");
        std::fs::write(&codex.path, pretty(&switchboard_hooks(&gone))).unwrap();

        let out = go(
            std::slice::from_ref(&codex),
            Mode::Install,
            Path::new("/sb"),
            &d,
        );
        assert!(out[0].error.is_none(), "{:?}", out[0].error);

        let text = std::fs::read_to_string(&codex.path).unwrap();
        assert!(!text.contains(OLD_MARKER));
        let after = read(&codex.path);
        let end = after["hooks"]["SessionEnd"].as_array().unwrap();
        assert_eq!(end.len(), 2);
        assert_eq!(end[0]["hooks"][0]["command"], "keep-me");
        assert_eq!(end[1]["hooks"][0]["timeout"], 3);
    }

    #[test]
    fn install_keeps_hooks_of_an_installed_switchboard() {
        let d = dir("switchboard-alive");
        let codex = codex_at(d.join("hooks.json"));
        let bin = d.join("switchboard");
        std::fs::write(&bin, "").unwrap();
        std::fs::write(&codex.path, pretty(&switchboard_hooks(&bin))).unwrap();

        let planned = plan(&codex, false, Path::new("/sb"), &d).unwrap();
        assert_eq!(
            planned.after["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"],
            5
        );
        assert!(planned.after_text.contains(OLD_MARKER));
    }

    #[test]
    fn repair_moves_versioned_paths_to_the_shim() {
        let d = dir("repair");
        let state = d.join("state");
        let old = Path::new("/home/u/.overseer/server/0.10.0/overseer");
        let shim = Path::new("/home/u/.overseer/bin/overseer");
        let claude = target(
            &d,
            "claude",
            "claude.json",
            hooks::CLAUDE_EVENTS,
            Format::Groups(5),
        );
        let stale = merge(&existing(), hooks::CLAUDE_EVENTS, |e| {
            hook_command(old, &state, "claude", e)
        });
        std::fs::write(&claude.path, pretty(&stale)).unwrap();
        let foreign = target(
            &d,
            "codex",
            "codex.json",
            hooks::CODEX_EVENTS,
            Format::Groups(5),
        );
        std::fs::write(&foreign.path, pretty(&existing())).unwrap();
        let ts = [claude, foreign];
        let out = go(&ts, Mode::Repair, shim, &state);
        assert!(out[0].written && out[0].backup.is_some());
        assert_eq!(out[1].skipped.as_deref(), Some("not installed"));
        let text = std::fs::read_to_string(&ts[0].path).unwrap();
        assert!(!text.contains("0.10.0") && text.contains("/home/u/.overseer/bin/overseer"));
        assert!(text.contains("~/.claude/hooks/notify.sh") && text.contains("rtk.sh"));
        assert_eq!(read(&ts[1].path), existing());
        let again = go(&ts, Mode::Repair, shim, &state);
        assert!(!again[0].changed && !again[0].written);
    }

    #[test]
    fn gemini_and_cursor_merges_keep_user_config() {
        let d = dir("merges");
        let state = d.join("state");
        let bin = Path::new("/sb");
        let gemini = target(
            &d,
            "gemini",
            "gemini.json",
            hooks::GEMINI_EVENTS,
            Format::Groups(5000),
        );
        std::fs::write(&gemini.path, r#"{"theme":"dark","hooks":{"AfterAgent":[{"matcher":"","hooks":[{"type":"command","command":"say done"}]}]}}"#).unwrap();
        let cursor = target(
            &d,
            "cursor",
            "cursor.json",
            hooks::CURSOR_EVENTS,
            Format::Flat,
        );
        std::fs::write(
            &cursor.path,
            r#"{"version":1,"hooks":{"stop":[{"command":"./mine.sh"}]}}"#,
        )
        .unwrap();
        let ts = [gemini, cursor];
        let out = go(&ts, Mode::Install, bin, &state);
        assert!(out.iter().all(|o| o.written && o.error.is_none()));
        let g = read(&ts[0].path);
        assert_eq!(g["theme"], "dark");
        assert_eq!(
            g["hooks"]["AfterAgent"][0]["hooks"][0]["command"],
            "say done"
        );
        assert_eq!(g["hooks"]["AfterAgent"][1]["hooks"][0]["timeout"], 5000);
        assert!(g["hooks"]["BeforeAgent"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook gemini BeforeAgent"));
        let c = read(&ts[1].path);
        assert_eq!(c["version"], 1);
        assert_eq!(c["hooks"]["stop"][0]["command"], "./mine.sh");
        assert!(c["hooks"]["stop"][1]["command"]
            .as_str()
            .unwrap()
            .ends_with(MARKER));
        assert!(c["hooks"]["beforeSubmitPrompt"][0]["command"]
            .as_str()
            .unwrap()
            .contains(r#"echo '{"continue":true}'"#));
        assert!(go(&ts, Mode::Install, bin, &state)
            .iter()
            .all(|o| !o.changed));
        go(&ts, Mode::Uninstall, bin, &state);
        assert_eq!(
            read(&ts[1].path)["hooks"]["stop"],
            json!([{"command": "./mine.sh"}])
        );
        assert!(read(&ts[1].path)["hooks"]
            .get("beforeSubmitPrompt")
            .is_none());
        assert_eq!(
            read(&ts[0].path)["hooks"]["AfterAgent"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn owned_files_are_generated_and_removed() {
        let d = dir("owned");
        let state = d.join("state");
        let bin = Path::new("/home/u/.overseer/bin/overseer");
        let ts = [
            target(
                &d,
                "opencode",
                "opencode/plugins/overseer.js",
                &[],
                Format::File,
            ),
            target(&d, "amp", "amp/plugins/overseer.js", &[], Format::File),
            target(
                &d,
                "copilot",
                "copilot/hooks/overseer.json",
                hooks::COPILOT_EVENTS,
                Format::File,
            ),
            target(
                &d,
                "vibe",
                "vibe/hooks.toml",
                hooks::VIBE_EVENTS,
                Format::File,
            ),
        ];
        let out = go(&ts, Mode::Install, bin, &state);
        assert!(out.iter().all(|o| o.written), "{out:?}");
        let js = std::fs::read_to_string(&ts[0].path).unwrap();
        assert!(js.contains(r#"const BIN = "/home/u/.overseer/bin/overseer";"#));
        assert!(js.contains(r#"const AGENT = "opencode";"#));
        assert!(js.contains(MARKER));
        for ev in [
            "session.status",
            "session.idle",
            "permission.asked",
            "\"Stop\"",
            "\"PermissionRequest\"",
        ] {
            assert!(js.contains(ev), "{ev}");
        }
        assert!(js.contains("export const Overseer"));
        let amp = std::fs::read_to_string(&ts[1].path).unwrap();
        assert!(amp.contains("agent.end") && amp.contains("export default function"));
        let cp = read(&ts[2].path);
        assert_eq!(cp["version"], 1);
        assert!(cp["hooks"]["agentStop"][0]["bash"]
            .as_str()
            .unwrap()
            .contains("hook copilot agentStop"));
        let vibe = std::fs::read_to_string(&ts[3].path).unwrap();
        assert_eq!(vibe.matches("[[hooks]]").count(), hooks::VIBE_EVENTS.len());
        for ev in hooks::VIBE_EVENTS {
            assert!(vibe.contains(&format!("type = \"{ev}\"\n")), "{ev}");
        }
        assert_eq!(vibe.matches("match = \"main\"").count(), 2);
        assert!(vibe.contains(r#"hook vibe post_agent \"$PPID\""#) && vibe.contains(MARKER));
        assert!(go(&ts, Mode::Repair, bin, &state)
            .iter()
            .all(|o| !o.changed));
        let moved = go(&ts, Mode::Repair, Path::new("/new/sb"), &state);
        assert!(moved.iter().all(|o| o.written));
        assert!(std::fs::read_to_string(&ts[0].path)
            .unwrap()
            .contains("/new/sb"));
        go(&ts, Mode::Uninstall, bin, &state);
        assert!(ts.iter().all(|t| !t.path.exists()));
        std::fs::write(&ts[0].path, "export default {}").unwrap();
        let out = go(&ts[..1], Mode::Install, bin, &state);
        assert!(out[0].error.is_some());
        assert_eq!(
            std::fs::read_to_string(&ts[0].path).unwrap(),
            "export default {}"
        );
    }

    #[test]
    fn install_skips_unconfigured_agents() {
        let d = dir("skip");
        let t = target(
            &d,
            "codex",
            "codex.json",
            hooks::CODEX_EVENTS,
            Format::Groups(5),
        );
        let r = Run {
            mode: Mode::Install,
            apply: false,
            bin: Path::new("/sb"),
            state_dir: &d,
            stamp: "x",
        };
        let out = run(std::slice::from_ref(&t), None, &r, |_| false);
        assert!(out[0].skipped.is_some() && !t.path.exists());
        let dry = run(std::slice::from_ref(&t), None, &r, |_| true);
        assert!(dry[0].changed && !dry[0].written && !t.path.exists());
        assert!(dry[0].diff.contains("hook codex Stop"));
    }

    #[test]
    fn hooks_calling_another_state_dir_are_foreign() {
        let d = dir("foreign");
        let t = target(
            &d,
            "claude",
            "c.json",
            hooks::CLAUDE_EVENTS,
            Format::Groups(5),
        );
        let (a, b) = (d.join("a"), d.join("b"));
        assert!(!points_elsewhere(&t, &a));
        let text = pretty(&merge(&json!({}), hooks::CLAUDE_EVENTS, |e| {
            hook_command(Path::new("/sb"), &a, "claude", e)
        }));
        std::fs::write(&t.path, text).unwrap();
        assert!(!points_elsewhere(&t, &a));
        assert!(points_elsewhere(&t, &b));
        std::fs::write(&t.path, pretty(&existing())).unwrap();
        assert!(!points_elsewhere(&t, &b));
    }

    #[test]
    fn only_the_default_state_dir_owns_globals() {
        assert!(owns_globals(&crate::platform_state_dir()));
        assert!(!owns_globals(&dir("not-default")));
    }

    #[cfg(unix)]
    #[test]
    fn shim_points_at_the_running_server() {
        let d = dir("shim");
        let shim = d.join("bin/overseer");
        let (a, b) = (d.join("0.10.0"), d.join("0.11.0"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        assert!(ensure_shim(&shim, &a, false).unwrap());
        assert!(!ensure_shim(&shim, &b, false).unwrap());
        assert_eq!(std::fs::read_link(&shim).unwrap(), a);
        assert!(ensure_shim(&shim, &b, true).unwrap());
        assert!(!ensure_shim(&shim, &b, true).unwrap());
        assert_eq!(std::fs::read_to_string(&shim).unwrap(), "b");
        assert!(!ensure_shim(&shim, &shim, true).unwrap());
    }

    #[test]
    fn command_is_quoted_and_safe() {
        let c = hook_command(Path::new("/a b/sb"), Path::new("/st"), "claude", "Stop");
        assert_eq!(
            c,
            "OVERSEER_STATE_DIR=/st '/a b/sb' hook claude Stop \"$PPID\" 2>/dev/null||:;echo {};#overseer-hook"
        );
    }
}
