use crate::install::{hook_file, MARKER};
use crate::sys::PermissionsExt;
use crate::{actions, daemon, scan, Ctx};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

pub struct Item {
    pub level: &'static str,
    pub name: &'static str,
    pub detail: String,
}

fn item(level: &'static str, name: &'static str, detail: impl Into<String>) -> Item {
    Item {
        level,
        name,
        detail: detail.into(),
    }
}

fn version(cmd: &str) -> Option<String> {
    let out = crate::exec::output_within(
        Command::new(cmd).arg("--version"),
        crate::exec::PROBE_TIMEOUT,
    )
    .ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    })
}

fn hooks_check(path: &Path, events: &[&str]) -> (usize, usize) {
    let v: Value = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let installed = events
        .iter()
        .filter(|e| {
            v["hooks"][**e].as_array().is_some_and(|gs| {
                gs.iter().any(|g| {
                    g["hooks"].as_array().is_some_and(|hs| {
                        hs.iter()
                            .any(|h| h["command"].as_str().is_some_and(|c| c.contains(MARKER)))
                    })
                })
            })
        })
        .count();
    (installed, events.len())
}

fn agent_bin(id: &str) -> &'static str {
    crate::agents::get(id).map_or("", |a| a.bin)
}

pub fn run(ctx: &Ctx) -> Vec<Item> {
    let mut out = vec![];
    let mode = std::fs::metadata(&ctx.state_dir).map(|m| m.permissions().mode() & 0o777);
    out.push(match mode {
        Ok(0o700) => item("ok", "state dir", ctx.state_dir.display().to_string()),
        Ok(m) => item(
            "warn",
            "state dir",
            format!("{} has mode {m:o}, expected 700", ctx.state_dir.display()),
        ),
        Err(_) => item(
            "warn",
            "state dir",
            format!("{} does not exist yet", ctx.state_dir.display()),
        ),
    });
    let sock = daemon::socket_path(&ctx.state_dir);
    out.push(match daemon::request(&sock, "GET", "/v1/status", None) {
        Ok((200, _)) => item("ok", "daemon", format!("running on {}", sock.display())),
        _ => item(
            "warn",
            "daemon",
            "not running (ls/hold/resume work without it; hooks spool to disk)",
        ),
    });
    out.push(match daemon::request(&sock, "GET", "/v1/env", None) {
        Ok((200, v)) => item(
            "info",
            "daemon PATH",
            format!(
                "{} ({})",
                v["path"].as_str().unwrap_or_default(),
                v["source"].as_str().unwrap_or("unknown")
            ),
        ),
        Ok((code, _)) => item(
            "warn",
            "daemon PATH",
            format!(
                "the daemon answered HTTP {code} on /v1/env; restart it to pick up this version"
            ),
        ),
        Err(_) => item(
            "info",
            "daemon PATH",
            format!(
                "{} (inherited (no daemon))",
                std::env::var("PATH").unwrap_or_default()
            ),
        ),
    });
    let (claude_file, claude_events) = hook_file("claude").unwrap_or_default();
    let claude_settings = ctx.claude_dir.join(claude_file);
    let (n, total) = hooks_check(&claude_settings, claude_events);
    out.push(item(
        if n == total { "ok" } else { "warn" },
        "claude hooks",
        format!(
            "{n}/{total} events in {} (install: overseer hooks install)",
            claude_settings.display()
        ),
    ));
    let (codex_file, codex_events) = hook_file("codex").unwrap_or_default();
    let codex_hooks = ctx.codex_dir.join(codex_file);
    if ctx.codex_dir.is_dir() {
        let (n, total) = hooks_check(&codex_hooks, codex_events);
        out.push(item(
            if n == total { "ok" } else { "warn" },
            "codex hooks",
            format!(
                "{n}/{total} events in {}; Codex runs them only after you re-trust them",
                codex_hooks.display()
            ),
        ));
    }
    out.push(match version(agent_bin("claude")) {
        Some(v) => item("ok", "claude", v),
        None => item("warn", "claude", "not on PATH"),
    });
    out.push(match version(agent_bin("codex")) {
        Some(v) => item("ok", "codex", v),
        None => item("warn", "codex", "not runnable on this station"),
    });
    out.push(match ctx.tmux.as_ref().and_then(|t| t.version()) {
        Some(v) => item("ok", "tmux", v),
        None => item("warn", "tmux", "not found; hold needs tmux"),
    });
    let api = |f: &str| -> Option<String> {
        let v: Value = serde_json::from_slice(&std::fs::read(ctx.state_dir.join(f)).ok()?).ok()?;
        v["api"].as_str().map(str::to_string)
    };
    out.push(match api(crate::paths::SYNC_FILE) {
        Some(a) => item("ok", "station enrolment", format!("enrolled with {a}")),
        None => item(
            "skip",
            "station enrolment",
            "not enrolled (overseer enroll)",
        ),
    });
    if let Some(a) = api(crate::paths::SYNC_FILE).or_else(|| api(crate::paths::USER_FILE)) {
        let probe = crate::remote::User {
            api: a.clone(),
            token: String::new(),
        };
        out.push(match crate::remote::get(&probe, "/healthz") {
            Ok((200, _)) => item("ok", "api reachable", format!("{a}/healthz answers")),
            Ok((code, _)) => item(
                "fail",
                "api reachable",
                format!("{a}/healthz answered HTTP {code}"),
            ),
            Err(e) => item("fail", "api reachable", e),
        });
    } else {
        out.push(item(
            "skip",
            "api reachable",
            "no api configured (enroll or login)",
        ));
    }
    out.push(match api(crate::paths::USER_FILE) {
        Some(a) => item("ok", "user login", format!("logged in to {a}")),
        None => item("skip", "user login", "not logged in (overseer login)"),
    });
    out.push(item(
        "info",
        "remote commands",
        if ctx.config.accept_remote_commands {
            "accepted (acceptRemoteCommands)"
        } else {
            "refused (acceptRemoteCommands is false)"
        },
    ));
    if ctx.config.accept_remote_commands {
        out.push(runner(ctx));
    }
    private_tmux_toolkit(ctx, &claude_settings, &mut out);
    out.push(if ctx.snapshot.exists() {
        item(
            "ok",
            "tmux-snapshot",
            format!(
                "{} read for launch flags of dead sessions",
                ctx.snapshot.display()
            ),
        )
    } else {
        item("skip", "tmux-snapshot", "no snapshot state file")
    });
    match scan(ctx) {
        Ok(lines) => {
            let st = actions::status(ctx, &lines);
            let text = actions::status_line(&st);
            out.push(item("ok", "lines", text.lines().next().unwrap_or_default()));
            out.push(match actions::memory_line(&st) {
                None => item("skip", "memory policy", "off (config memoryPolicy)"),
                Some(m) if st["memoryPolicy"]["over"] == true => item("warn", "memory policy", m),
                Some(m) => item("ok", "memory policy", m),
            });
            for l in lines.iter().filter(|l| l.orphan) {
                out.push(item(
                    "warn",
                    "orphan",
                    format!(
                        "{} pid {} in pane {} (no client attached); not killed",
                        l.id,
                        l.pid.unwrap_or(0),
                        l.pane.as_deref().unwrap_or("?")
                    ),
                ));
            }
            for l in lines.iter().filter(|l| l.duplicate) {
                out.push(item(
                    "warn",
                    "duplicate",
                    format!("{}: two live processes on session {}", l.id, l.session_id),
                ));
            }
            for l in lines
                .iter()
                .filter(|l| l.held_reason == Some("user") && l.held_pane.is_none())
            {
                out.push(item(
                    "warn",
                    "held without stub",
                    format!("{}: resume with overseer resume {}", l.id, l.id),
                ));
            }
        }
        Err(e) => out.push(item("fail", "lines", e)),
    }
    out
}

fn private_tmux_toolkit(ctx: &Ctx, claude_settings: &Path, out: &mut Vec<Item>) {
    let bin = crate::paths::user_bin();
    let snap = bin.join("tmux-snapshot");
    let restore = bin.join("tmux-restore");
    if snap.exists() || restore.exists() {
        let patched = |p: &Path| std::fs::read_to_string(p).is_ok_and(|s| s.contains("sb_held"));
        out.push(if patched(&snap) && patched(&restore) {
            item("ok", "toolkit", "tmux-snapshot/tmux-restore know @sb_held")
        } else {
            item(
                "warn",
                "toolkit",
                "tmux-snapshot/tmux-restore not patched for @sb_held: a held stub comes back as a plain shell after a reboot (still held); see docs/toolkit-patch.md",
            )
        });
    }
    let idhook = ctx.claude_dir.join("hooks/tmux-claude-id.sh");
    if idhook.exists() {
        let s = std::fs::read_to_string(claude_settings).unwrap_or_default();
        out.push(if s.contains("tmux-claude-id.sh") {
            item("ok", "existing hooks", "tmux-claude-id.sh still declared")
        } else {
            item(
                "warn",
                "existing hooks",
                "tmux-claude-id.sh exists but is not declared in settings.json",
            )
        });
    }
}

fn runner(ctx: &Ctx) -> Item {
    let mut missing = vec![];
    if version(agent_bin("claude")).is_none() {
        missing.push("claude not on PATH".to_string());
    }
    if ctx.tmux.as_ref().and_then(|t| t.version()).is_none() {
        missing.push("tmux missing".into());
    }
    for (repo, path) in &ctx.config.repos {
        if !path.join(".git").exists() {
            missing.push(format!("repos.{repo}: {} is not a clone", path.display()));
        }
    }
    if ctx.config.repos.is_empty() && ctx.config.roots.is_empty() {
        missing.push("no repos or roots to start lines from".into());
    }
    if missing.is_empty() {
        item(
            "ok",
            "runner",
            format!(
                "start/relaunch ready; worktrees under {}",
                ctx.config.worktree_dir.display()
            ),
        )
    } else {
        item("warn", "runner", missing.join("; "))
    }
}
