use crate::install::{hook_file, MARKER};
use crate::sys::PermissionsExt;
use crate::{actions, daemon, scan, Ctx};
use serde_json::Value;
use std::collections::HashMap;
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

fn snake(event: &str) -> String {
    let mut out = String::new();
    for (i, c) in event.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn quoted_key(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    let (_, q) = chars.next()?;
    if q == '\'' {
        let end = s[1..].find('\'')? + 1;
        return Some((s[1..end].to_string(), &s[end + 1..]));
    }
    if q != '"' {
        return None;
    }
    let mut key = String::new();
    let mut esc = false;
    for (i, c) in chars {
        if esc {
            key.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '"' {
            return Some((key, &s[i + 1..]));
        } else {
            key.push(c);
        }
    }
    None
}

fn has_value(rest: &str) -> bool {
    rest.trim_start()
        .strip_prefix('=')
        .map(str::trim)
        .is_some_and(|v| v.len() > 2 && (v.starts_with('"') || v.starts_with('\'')))
}

#[derive(Default)]
struct Entry {
    hashed: bool,
    disabled: bool,
}

fn note_field(entries: &mut HashMap<String, Entry>, key: &str, line: &str) {
    let e = entries.entry(key.to_string()).or_default();
    if line.strip_prefix("trusted_hash").is_some_and(has_value) {
        e.hashed = true;
    } else if line
        .strip_prefix("enabled")
        .and_then(|r| r.trim_start().strip_prefix('='))
        .is_some_and(|v| v.trim().starts_with("false"))
    {
        e.disabled = true;
    }
}

fn hook_entries(config: &str) -> HashMap<String, Entry> {
    const TABLE: &str = "hooks.state.";
    let mut out = HashMap::new();
    let mut current: Option<String> = None;
    for line in config.lines() {
        let line = line.trim();
        if let Some(h) = line.strip_prefix('[') {
            let h = h.trim_start_matches('[').trim_start();
            current = h
                .strip_prefix(TABLE)
                .and_then(quoted_key)
                .filter(|(_, rest)| rest.trim_start().starts_with(']'))
                .map(|(k, _)| k);
        } else if let Some(k) = &current {
            note_field(&mut out, k, line);
        } else if let Some((k, rest)) = line.strip_prefix(TABLE).and_then(quoted_key) {
            if let Some(field) = rest.strip_prefix('.') {
                note_field(&mut out, &k, field);
            }
        }
    }
    out
}

#[derive(Debug, Default, PartialEq)]
struct CodexTrust {
    total: usize,
    missing: Vec<String>,
    disabled: Vec<String>,
}

fn codex_trust(hooks: &Path, config: &Path, events: &[&str]) -> CodexTrust {
    let v: Value = std::fs::read(hooks)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null);
    let entries = std::fs::read_to_string(config)
        .map(|c| hook_entries(&c))
        .unwrap_or_default();
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let want = canon(hooks);
    let find = |label: &str, g: usize, h: usize| {
        entries.iter().find_map(|(k, e)| {
            let mut parts = k.rsplitn(4, ':');
            let (hh, gg, ll, path) = (parts.next(), parts.next(), parts.next(), parts.next());
            (ll == Some(label)
                && gg == Some(g.to_string().as_str())
                && hh == Some(h.to_string().as_str())
                && path.is_some_and(|p| Path::new(p) == hooks || canon(Path::new(p)) == want))
            .then_some(e)
        })
    };
    let mut out = CodexTrust::default();
    for e in events {
        let label = snake(e);
        let (mut seen, mut miss, mut off) = (false, false, false);
        for (g, group) in v["hooks"][*e].as_array().into_iter().flatten().enumerate() {
            for (h, hook) in group["hooks"].as_array().into_iter().flatten().enumerate() {
                if hook["command"].as_str().is_some_and(|c| c.contains(MARKER)) {
                    seen = true;
                    match find(&label, g, h) {
                        Some(en) if en.hashed => off |= en.disabled,
                        _ => miss = true,
                    }
                }
            }
        }
        out.total += usize::from(seen);
        if miss {
            out.missing.push((*e).to_string());
        } else if off {
            out.disabled.push((*e).to_string());
        }
    }
    out
}

fn codex_trust_item(hooks: &Path, config: &Path, events: &[&str]) -> Item {
    let t = codex_trust(hooks, config, events);
    const FIX: &str = "start codex and choose Review hooks or Trust all at the \"Hooks need review\" prompt, or run /hooks in it (codex exec cannot review hooks)";
    if !t.missing.is_empty() {
        item(
                    "warn",
                    "codex trust",
                    format!(
                        "{}/{} events have no trust entry in config.toml ({}); Codex skips them until you trust them: {FIX}",
                        t.missing.len(),
                        t.total,
                        t.missing.join(", ")
                    ),
                )
    } else if !t.disabled.is_empty() {
        item(
                    "warn",
                    "codex trust",
                    format!(
                        "{}/{} events are disabled in config.toml ({}); Codex does not run them: turn them on with /hooks in codex",
                        t.disabled.len(),
                        t.total,
                        t.disabled.join(", ")
                    ),
                )
    } else {
        item(
                    "info",
                    "codex trust",
                    format!(
                        "{}/{} events have a trust entry in config.toml, but doctor cannot tell whether it matches the current hooks (Codex recomputes a hash, so an entry left from before a rewrite counts as changed); if sessions show no events, run /hooks in codex: hooks needing review are listed there",
                        t.total, t.total
                    ),
                )
    }
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
            format!("{n}/{total} events in {}", codex_hooks.display()),
        ));
        if n > 0 {
            out.push(codex_trust_item(
                &codex_hooks,
                &ctx.codex_dir.join("config.toml"),
                codex_events,
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: &[&str] = &["SessionStart", "UserPromptSubmit", "Stop"];

    fn fixture(name: &str, config: Option<&str>) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("doctor-trust-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let hook = |e: &str| {
            format!(r#""{e}":[{{"hooks":[{{"type":"command","command":"x {MARKER}"}}]}}]"#)
        };
        let events: Vec<_> = EVENTS.iter().map(|e| hook(e)).collect();
        let hooks = dir.join("hooks.json");
        std::fs::write(&hooks, format!(r#"{{"hooks":{{{}}}}}"#, events.join(","))).unwrap();
        let cfg = dir.join("config.toml");
        if let Some(c) = config {
            std::fs::write(&cfg, c.replace("$H", &hooks.display().to_string())).unwrap();
        }
        (hooks, cfg)
    }

    #[test]
    fn snake_cases_event_names() {
        assert_eq!(snake("UserPromptSubmit"), "user_prompt_submit");
        assert_eq!(snake("Stop"), "stop");
    }

    #[test]
    fn hook_entries_reads_table_and_dotted_forms() {
        let t = hook_entries(
            "[hooks.state.\"/a/hooks.json:stop:0:0\"]\ntrusted_hash = \"sha256:ab\"\n\n\
             [hooks.state.'/b/hooks.json:stop:0:0']\nenabled = false\ntrusted_hash = 'x'\n\n\
             [hooks.state.\"/c\\\\hooks.json:stop:0:0\"]\nenabled = true\n\n\
             [other]\ntrusted_hash = \"nope\"\n\
             hooks.state.\"/d/hooks.json:stop:0:0\".trusted_hash = \"h\"\n\
             hooks.state.\"/d/hooks.json:stop:0:0\".enabled = false\n",
        );
        let flags = |k: &str| t.get(k).map(|e| (e.hashed, e.disabled));
        assert_eq!(flags("/a/hooks.json:stop:0:0"), Some((true, false)));
        assert_eq!(flags("/b/hooks.json:stop:0:0"), Some((true, true)));
        assert_eq!(flags("/c\\hooks.json:stop:0:0"), Some((false, false)));
        assert_eq!(flags("/d/hooks.json:stop:0:0"), Some((true, true)));
        assert_eq!(t.len(), 4);
    }

    #[test]
    fn hook_entries_ignores_garbage() {
        assert!(
            hook_entries("\u{0}[[[ = = \n[hooks.state.\"unterminated\ntrusted_hash=\"x\"")
                .is_empty()
        );
        assert!(hook_entries("").is_empty());
    }

    #[test]
    fn every_event_is_untrusted_without_a_config_toml() {
        let (hooks, cfg) = fixture("nocfg", None);
        let t = codex_trust(&hooks, &cfg, EVENTS);
        assert_eq!(t.total, 3);
        assert_eq!(t.missing, EVENTS);
    }

    #[test]
    fn only_events_without_a_trust_entry_are_reported() {
        let (hooks, cfg) = fixture(
            "partial",
            Some(
                "[hooks.state.\"$H:session_start:0:0\"]\ntrusted_hash = \"a\"\n\
                 [hooks.state.\"$H:stop:0:0\"]\nenabled = true\n\
                 [hooks.state.\"$H:user_prompt_submit:0:1\"]\ntrusted_hash = \"a\"\n",
            ),
        );
        let t = codex_trust(&hooks, &cfg, EVENTS);
        assert_eq!(t.total, 3);
        assert_eq!(t.missing, ["UserPromptSubmit", "Stop"]);
    }

    #[test]
    fn all_trusted_reports_nothing() {
        let (hooks, cfg) = fixture(
            "all",
            Some(
                "[hooks.state.\"$H:session_start:0:0\"]\ntrusted_hash = \"a\"\n\
                 [hooks.state.\"$H:stop:0:0\"]\ntrusted_hash = \"a\"\n\
                 [hooks.state.\"$H:user_prompt_submit:0:0\"]\ntrusted_hash = \"a\"\n",
            ),
        );
        assert_eq!(
            codex_trust(&hooks, &cfg, EVENTS),
            CodexTrust {
                total: 3,
                ..Default::default()
            }
        );
    }

    #[test]
    fn a_symlinked_hooks_path_still_matches() {
        let (hooks, cfg) = fixture("link", None);
        let real = hooks.with_file_name("real.json");
        std::fs::rename(&hooks, &real).unwrap();
        std::os::unix::fs::symlink(&real, &hooks).unwrap();
        std::fs::write(
            &cfg,
            format!(
                "[hooks.state.\"{}:stop:0:0\"]\ntrusted_hash = \"a\"\n",
                real.display()
            ),
        )
        .unwrap();
        let t = codex_trust(&hooks, &cfg, EVENTS);
        assert_eq!(t.missing, ["SessionStart", "UserPromptSubmit"]);
    }

    #[test]
    fn a_missing_hooks_file_has_nothing_to_trust() {
        let p = std::env::temp_dir().join("doctor-trust-none.json");
        assert_eq!(codex_trust(&p, &p, EVENTS), CodexTrust::default());
    }

    #[test]
    fn a_disabled_entry_is_not_counted_as_trusted() {
        let (hooks, cfg) = fixture(
            "off",
            Some(
                "[hooks.state.\"$H:session_start:0:0\"]\ntrusted_hash = \"a\"\n\
                 [hooks.state.\"$H:stop:0:0\"]\ntrusted_hash = \"a\"\nenabled = false\n\
                 [hooks.state.\"$H:user_prompt_submit:0:0\"]\nenabled = false\n",
            ),
        );
        let t = codex_trust(&hooks, &cfg, EVENTS);
        assert_eq!(t.missing, ["UserPromptSubmit"]);
        assert_eq!(t.disabled, ["Stop"]);
    }

    fn level_and_detail(name: &str, config: Option<&str>) -> (&'static str, String) {
        let (hooks, cfg) = fixture(name, config);
        let i = codex_trust_item(&hooks, &cfg, EVENTS);
        assert_eq!(i.name, "codex trust");
        (i.level, i.detail)
    }

    const ALL_TRUSTED: &str = "[hooks.state.\"$H:session_start:0:0\"]\ntrusted_hash = \"a\"\n\
         [hooks.state.\"$H:user_prompt_submit:0:0\"]\ntrusted_hash = \"a\"\n\
         [hooks.state.\"$H:stop:0:0\"]\ntrusted_hash = \"a\"\n";

    #[test]
    fn item_is_info_never_ok_when_every_event_has_an_enabled_entry() {
        let (level, detail) = level_and_detail("item-all", Some(ALL_TRUSTED));
        assert_eq!(level, "info");
        assert!(detail.starts_with("3/3 "), "{detail}");
    }

    #[test]
    fn item_warns_naming_a_disabled_event() {
        let cfg = ALL_TRUSTED.replace(
            "[hooks.state.\"$H:stop:0:0\"]\n",
            "[hooks.state.\"$H:stop:0:0\"]\nenabled = false\n",
        );
        let (level, detail) = level_and_detail("item-off", Some(&cfg));
        assert_eq!(level, "warn");
        assert!(
            detail.starts_with("1/3 ") && detail.contains("Stop"),
            "{detail}"
        );
        assert!(!detail.contains("SessionStart"), "{detail}");
    }

    #[test]
    fn item_with_missing_and_disabled_events_warns_about_both_kinds() {
        let cfg = "[hooks.state.\"$H:session_start:0:0\"]\ntrusted_hash = \"a\"\nenabled = false\n\
             [hooks.state.\"$H:stop:0:0\"]\ntrusted_hash = \"a\"\n";
        let (hooks, config) = fixture("item-mixed", Some(cfg));
        let t = codex_trust(&hooks, &config, EVENTS);
        assert_eq!(t.missing, ["UserPromptSubmit"]);
        assert_eq!(t.disabled, ["SessionStart"]);
        let i = codex_trust_item(&hooks, &config, EVENTS);
        assert_eq!(i.level, "warn");
        assert!(
            i.detail.starts_with("1/3 ") && i.detail.contains("UserPromptSubmit"),
            "{}",
            i.detail
        );
    }

    #[test]
    fn item_warns_for_every_event_without_a_config_toml() {
        let (level, detail) = level_and_detail("item-none", None);
        assert_eq!(level, "warn");
        assert!(detail.starts_with("3/3 "), "{detail}");
    }
}
