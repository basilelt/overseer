use crate::install::{atomic_write, diff, pretty, stamp};
use crate::usage::{Limits, Window};
use crate::util::shell_quote;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

pub const MARKER: &str = "#overseer-statusline";
pub const FILE: &str = "claude-statusline.json";
const MAX_INPUT: u64 = 1024 * 1024;
const REWRITE_MS: i64 = 60_000;

pub fn parse(v: &Value, now: i64) -> Option<Limits> {
    let rl = v.get("rate_limits")?;
    let mut windows = Vec::new();
    for (k, key, minutes) in [("five_hour", "5h", 300), ("seven_day", "wk", 10_080)] {
        let Some(used) = rl[k]["used_percentage"].as_f64() else {
            continue;
        };
        let resets_at = rl[k]["resets_at"]
            .as_f64()
            .filter(|s| *s > 0.0)
            .map(|s| (s * 1000.0) as i64)
            .unwrap_or(0);
        windows.push(Window {
            key: key.into(),
            minutes,
            used: used.clamp(0.0, 100.0),
            resets_at,
        });
    }
    (!windows.is_empty()).then_some(Limits {
        at: now,
        windows,
        plan: None,
    })
}

pub fn path(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

pub fn read(state_dir: &Path) -> Option<Limits> {
    let s = std::fs::read_to_string(path(state_dir)).ok()?;
    serde_json::from_str(&s).ok()
}

pub fn record(state_dir: &Path, input: &[u8], now: i64) -> Result<bool, String> {
    let v: Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    let Some(l) = parse(&v, now) else {
        return Ok(false);
    };
    if read(state_dir).is_some_and(|old| old.windows == l.windows && now - old.at < REWRITE_MS) {
        return Ok(false);
    }
    let text = serde_json::to_string(&l).map_err(|e| e.to_string())?;
    crate::mkdir_private(state_dir).map_err(|e| format!("{}: {e}", state_dir.display()))?;
    let dest = path(state_dir);
    let tmp = state_dir.join(format!("{FILE}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", dest.display())
    })?;
    Ok(true)
}

pub fn command(bin: &Path, state_dir: &Path, original: Option<&str>) -> String {
    let b = shell_quote(&bin.to_string_lossy());
    let s = shell_quote(&state_dir.to_string_lossy());
    match original.filter(|o| !o.trim().is_empty()) {
        Some(o) => {
            let o = shell_quote(o);
            format!("[ -x {b} ] && exec {b} statusline-tap --state {s} -- {o} || exec sh -c {o} {MARKER}")
        }
        None => format!("[ -x {b} ] && exec {b} statusline-tap --state {s} || : {MARKER}"),
    }
}

fn unquote_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '\\' => {
                started = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

pub fn original(cmd: &str) -> Option<Option<String>> {
    original_marker(cmd, MARKER)
}

pub fn original_marker(cmd: &str, marker: &str) -> Option<Option<String>> {
    if !cmd.contains(marker) {
        return None;
    }
    let words = unquote_words(cmd);
    let i = words.iter().position(|w| w == "statusline-tap")?;
    let rest = &words[i + 1..];
    Some(match rest.iter().position(|w| w == "--") {
        Some(j) if j + 1 < rest.len() && rest.get(j + 2).map(String::as_str) == Some("||") => {
            Some(rest[j + 1].clone())
        }
        _ => None,
    })
}

pub fn settings_path() -> PathBuf {
    crate::env_dir("CLAUDE_CONFIG_DIR", ".claude").join("settings.json")
}

pub struct Plan {
    pub before_text: String,
    pub after: Value,
    pub after_text: String,
    pub changed: bool,
}

pub fn plan(
    before_text: &str,
    install: bool,
    bin: &Path,
    state_dir: &Path,
) -> Result<Plan, String> {
    let before: Value = if before_text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(before_text)
            .map_err(|e| format!("settings.json is not valid JSON ({e}); not touching it"))?
    };
    if !before.is_object() {
        return Err("settings.json is not a JSON object; not touching it".into());
    }
    let mut after = before.clone();
    let current = before["statusLine"]["command"].as_str().map(str::to_string);
    let wrapped = current.as_deref().and_then(original);
    if install {
        if before["statusLine"]
            .get("type")
            .is_some_and(|t| t != "command")
        {
            return Err("the status line is not a command; not touching it".into());
        }
        let orig = match wrapped {
            Some(o) => o,
            None => current,
        };
        let cmd = command(bin, state_dir, orig.as_deref());
        if !after["statusLine"].is_object() {
            after["statusLine"] = json!({"type": "command"});
        }
        after["statusLine"]["type"] = json!("command");
        after["statusLine"]["command"] = json!(cmd);
    } else if let Some(orig) = wrapped {
        match orig {
            Some(o) => after["statusLine"]["command"] = json!(o),
            None => {
                if let Some(m) = after.as_object_mut() {
                    m.remove("statusLine");
                }
            }
        }
    }
    let changed = after != before;
    let after_text = if changed {
        pretty(&after)
    } else {
        before_text.to_string()
    };
    Ok(Plan {
        before_text: before_text.to_string(),
        after,
        after_text,
        changed,
    })
}

pub fn status(settings: &Path) -> Value {
    let text = std::fs::read_to_string(settings).unwrap_or_default();
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let cmd = v["statusLine"]["command"].as_str();
    let wrapped = cmd.and_then(original);
    json!({
        "path": settings.to_string_lossy(),
        "installed": wrapped.is_some(),
        "command": match &wrapped {
            Some(o) => o.clone().map(Value::String).unwrap_or(Value::Null),
            None => cmd.map(|c| Value::String(c.into())).unwrap_or(Value::Null),
        },
    })
}

pub fn apply(
    settings: &Path,
    install: bool,
    dry_run: bool,
    bin: &Path,
    state_dir: &Path,
    now: i64,
) -> Result<Value, String> {
    let before = match std::fs::read_to_string(settings) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", settings.display())),
    };
    let p = plan(&before, install, bin, state_dir)?;
    let d = diff(
        settings,
        &p.before_text,
        &p.after_text,
        &std::env::temp_dir().join("overseer-statusline"),
    );
    let mut backup = None;
    if p.changed && !dry_run {
        if settings.exists() {
            let b = PathBuf::from(format!("{}.bak-{}", settings.display(), stamp(now)));
            std::fs::copy(settings, &b).map_err(|e| format!("backup {}: {e}", b.display()))?;
            backup = Some(b);
        }
        let reread: Value = serde_json::from_str(&p.after_text).map_err(|e| e.to_string())?;
        if reread != p.after {
            return Err("merged settings do not round-trip; not writing".into());
        }
        atomic_write(settings, &p.after_text)?;
    }
    if !install && !dry_run {
        let _ = std::fs::remove_file(path(state_dir));
    }
    let mut s = status(settings);
    s["changed"] = json!(p.changed);
    s["dryRun"] = json!(dry_run);
    s["diff"] = json!(d);
    s["backup"] = json!(backup.map(|b| b.to_string_lossy().into_owned()));
    Ok(s)
}

pub fn routes(method: &str, segs: &[&str], body: &Value, state_dir: &Path) -> Option<(u16, Value)> {
    let install = match (method, segs) {
        ("GET", ["v1", "usage", "statusline"]) => return Some((200, status(&settings_path()))),
        ("POST", ["v1", "usage", "statusline", "install"]) => true,
        ("POST", ["v1", "usage", "statusline", "uninstall"]) => false,
        _ => return None,
    };
    let dry = body["dryRun"].as_bool() == Some(true);
    let bin = crate::install::hook_bin();
    Some(
        match apply(
            &settings_path(),
            install,
            dry,
            &bin,
            state_dir,
            crate::procs::now_ms(),
        ) {
            Ok(v) => (200, v),
            Err(e) => (409, json!({"error": e})),
        },
    )
}

fn spawn(original: &str) -> std::io::Result<std::process::Child> {
    let mut c = if cfg!(windows) {
        let mut c = Command::new("sh");
        c.arg("-c").arg(original);
        c
    } else {
        let mut c = Command::new("/bin/sh");
        c.arg("-c").arg(original);
        c
    };
    c.stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

pub fn wrap(original: &str, input: &[u8]) -> std::io::Result<i32> {
    let mut child = spawn(original)?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    Ok(child.wait()?.code().unwrap_or(1))
}

pub fn tap(args: &[String]) -> ExitCode {
    let mut state: Option<PathBuf> = None;
    let mut original: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--state" => state = it.next().map(PathBuf::from),
            "--" => {
                original = it.next().cloned();
                break;
            }
            "install" | "uninstall" => return cli(a == "install", args),
            _ => {}
        }
    }
    let mut input = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut input);
    let state = state.unwrap_or_else(crate::default_state_dir);
    if input.len() as u64 <= MAX_INPUT {
        let _ = record(&state, &input, crate::procs::now_ms());
    }
    match original {
        Some(o) => match wrap(&o, &input) {
            Ok(code) => ExitCode::from(code.clamp(0, 255) as u8),
            Err(e) => {
                eprintln!("overseer statusline-tap: {e}");
                ExitCode::FAILURE
            }
        },
        None => ExitCode::SUCCESS,
    }
}

fn cli(install: bool, args: &[String]) -> ExitCode {
    let apply_it = args.iter().any(|a| a == "--apply");
    let state = crate::default_state_dir();
    let state = state.canonicalize().unwrap_or(state);
    let bin = crate::install::hook_bin();
    let settings = settings_path();
    match apply(
        &settings,
        install,
        !apply_it,
        &bin,
        &state,
        crate::procs::now_ms(),
    ) {
        Ok(v) => {
            if !v["changed"].as_bool().unwrap_or(false) {
                println!("== {}: nothing to change", settings.display());
            } else if apply_it {
                println!("== {}: written", settings.display());
                if let Some(b) = v["backup"].as_str() {
                    println!("   backup: {b}");
                }
            } else {
                println!(
                    "== {} (dry run; --apply to write, with a backup)",
                    settings.display()
                );
                print!("{}", v["diff"].as_str().unwrap_or_default());
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("overseer: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"{"session_id":"abc","model":{"id":"claude-opus-5-5","display_name":"Opus"},
      "rate_limits":{"five_hour":{"used_percentage":97.4,"resets_at":1738425600},
                     "seven_day":{"used_percentage":49,"resets_at":1738857600}}}"#;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-statusline-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parses_documented_rate_limits() {
        let l = parse(&serde_json::from_str(DOC).unwrap(), 5).unwrap();
        assert_eq!(l.at, 5);
        assert_eq!(l.windows.len(), 2);
        assert_eq!(
            (
                l.windows[0].key.as_str(),
                l.windows[0].minutes,
                l.windows[0].used
            ),
            ("5h", 300, 97.4)
        );
        assert_eq!(l.windows[0].resets_at, 1_738_425_600_000);
        assert_eq!(
            (
                l.windows[1].key.as_str(),
                l.windows[1].minutes,
                l.windows[1].used
            ),
            ("wk", 10_080, 49.0)
        );
    }

    #[test]
    fn missing_or_partial_rate_limits() {
        assert!(parse(&json!({"model": {}}), 1).is_none());
        assert!(parse(&json!({"rate_limits": {}}), 1).is_none());
        let l = parse(
            &json!({"rate_limits": {"seven_day": {"used_percentage": 12}}}),
            1,
        )
        .unwrap();
        assert_eq!(l.windows.len(), 1);
        assert_eq!(l.windows[0].key, "wk");
        assert_eq!(l.windows[0].resets_at, 0);
    }

    #[test]
    fn record_writes_only_on_change() {
        let d = tmp("record");
        assert!(record(&d, DOC.as_bytes(), 1000).unwrap());
        assert!(!record(&d, DOC.as_bytes(), 2000).unwrap());
        assert!(record(&d, DOC.as_bytes(), 1000 + REWRITE_MS).unwrap());
        assert!(!record(&d, br#"{"model":{}}"#, 3000).unwrap());
        assert_eq!(read(&d).unwrap().windows[0].used, 97.4);
        assert!(record(&d, b"not json", 1).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn command_round_trips_the_original() {
        let bin = Path::new("/home/u/.overseer/bin/overseer");
        let st = Path::new("/home/u/.local/state/overseer");
        for o in [
            "~/.claude/statusline.sh",
            "jq -r '\"[\\(.model.display_name)]\"'",
            "a; b | c 'it''s'",
        ] {
            let c = command(bin, st, Some(o));
            assert!(c.contains(MARKER));
            assert_eq!(original(&c), Some(Some(o.to_string())), "{c}");
        }
        assert_eq!(original(&command(bin, st, None)), Some(None));
        assert_eq!(original(&command(bin, st, Some("  "))), Some(None));
        assert_eq!(original("~/.claude/statusline.sh"), None);
    }

    #[test]
    fn install_wraps_and_uninstall_restores() {
        let bin = Path::new("/b/overseer");
        let st = Path::new("/s");
        let before = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"~/.claude/sl.sh\",\n    \"padding\": 2\n  }\n}\n";
        let p = plan(before, true, bin, st).unwrap();
        assert!(p.changed);
        assert_eq!(p.after["statusLine"]["padding"], 2);
        assert_eq!(p.after["model"], "opus");
        let cmd = p.after["statusLine"]["command"].as_str().unwrap();
        assert_eq!(original(cmd), Some(Some("~/.claude/sl.sh".into())));
        let again = plan(&p.after_text, true, bin, st).unwrap();
        assert!(!again.changed);
        let back = plan(&p.after_text, false, bin, st).unwrap();
        assert!(back.changed);
        assert_eq!(back.after, serde_json::from_str::<Value>(before).unwrap());
        let none = plan(before, false, bin, st).unwrap();
        assert!(!none.changed);
    }

    #[test]
    fn install_without_a_status_line_and_back() {
        let bin = Path::new("/b/overseer");
        let p = plan("", true, bin, Path::new("/s")).unwrap();
        assert_eq!(p.after["statusLine"]["type"], "command");
        assert_eq!(
            original(p.after["statusLine"]["command"].as_str().unwrap()),
            Some(None)
        );
        let back = plan(&p.after_text, false, bin, Path::new("/s")).unwrap();
        assert_eq!(back.after, json!({}));
        assert!(plan("[1]", true, bin, Path::new("/s")).is_err());
        assert!(plan("{oops", true, bin, Path::new("/s")).is_err());
    }

    #[test]
    fn uninstall_forgets_the_recorded_limits() {
        let d = tmp("apply");
        let settings = d.join("settings.json");
        std::fs::write(
            &settings,
            r#"{"statusLine":{"type":"command","command":"echo hi"}}"#,
        )
        .unwrap();
        let bin = d.join("overseer");
        let v = apply(&settings, true, false, &bin, &d, 0).unwrap();
        assert_eq!(v["installed"], true);
        assert_eq!(v["command"], "echo hi");
        assert!(record(&d, DOC.as_bytes(), 1).unwrap());
        let dry = apply(&settings, false, true, &bin, &d, 0).unwrap();
        assert_eq!(dry["installed"], true);
        assert!(read(&d).is_some());
        let v = apply(&settings, false, false, &bin, &d, 0).unwrap();
        assert_eq!(v["installed"], false);
        assert_eq!(v["command"], "echo hi");
        assert!(read(&d).is_none());
        assert!(std::fs::read_dir(&d).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("settings.json.bak-")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn shell_snippet_runs_the_original_with_the_same_input() {
        let d = tmp("wrap");
        let out = d.join("out");
        let o = format!("cat > {}; echo shown", shell_quote(&out.to_string_lossy()));
        assert_eq!(wrap(&o, DOC.as_bytes()).unwrap(), 0);
        assert_eq!(std::fs::read_to_string(&out).unwrap(), DOC);
        assert_eq!(wrap("exit 3", b"").unwrap(), 3);
        let missing = command(&d.join("nope"), &d, Some(&o));
        let mut c = Command::new("/bin/sh")
            .arg("-c")
            .arg(&missing)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(b"hello").unwrap();
        let r = c.wait_with_output().unwrap();
        assert_eq!(String::from_utf8_lossy(&r.stdout), "shown\n");
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&d);
    }
}
