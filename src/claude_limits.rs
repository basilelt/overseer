use crate::usage::{Limits, Window};
use crate::util::parse_rfc3339;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

pub const URL: &str = "https://api.anthropic.com/api/oauth/usage";
pub const REFRESH_MS: i64 = 5 * 60_000;
pub const ACTIVE_MS: i64 = 3_600_000;
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BODY: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    Ok,
    Expired,
    Unavailable,
}

pub struct Token(String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token([redacted])")
    }
}

#[derive(Debug)]
pub struct Creds {
    pub token: Token,
    pub expires_at: Option<i64>,
    pub plan: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Fresh(Limits),
    Failed(Auth),
    NoLogin,
}

pub fn parse_creds(raw: &str) -> Option<Creds> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let o = &v["claudeAiOauth"];
    let token = o["accessToken"].as_str().filter(|t| !t.is_empty())?;
    Some(Creds {
        token: Token(token.to_string()),
        expires_at: o["expiresAt"].as_i64(),
        plan: o["subscriptionType"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    })
}

pub fn read_creds(claude_dir: &Path) -> Option<Creds> {
    if let Ok(raw) = std::fs::read_to_string(claude_dir.join(".credentials.json")) {
        return parse_creds(&raw);
    }
    if !keychain_applies(claude_dir) {
        return None;
    }
    keychain().and_then(|raw| parse_creds(&raw))
}

/// Claude Code keeps the keychain login under one fixed name only for `~/.claude`.
fn keychain_applies(claude_dir: &Path) -> bool {
    crate::agents::get(crate::agents::CLAUDE).and_then(|a| a.default_config_dir())
        == Some(claude_dir.to_path_buf())
}

#[cfg(target_os = "macos")]
fn keychain() -> Option<String> {
    let mut c = Command::new("security");
    c.args([
        "find-generic-password",
        "-s",
        "Claude Code-credentials",
        "-w",
    ]);
    let (ok, out) = run(c, None)?;
    ok.then(|| String::from_utf8(out).ok()).flatten()
}

#[cfg(not(target_os = "macos"))]
fn keychain() -> Option<String> {
    None
}

fn window(key: &str, minutes: i64, used: f64, resets_at: Option<&str>) -> Window {
    Window {
        key: key.into(),
        minutes,
        used,
        resets_at: resets_at.and_then(parse_rfc3339).unwrap_or(0),
    }
}

pub fn parse_usage(v: &Value) -> Option<Vec<Window>> {
    let mut out = Vec::new();
    if let Some(limits) = v["limits"].as_array() {
        for l in limits {
            let Some(pct) = l["percent"].as_f64() else {
                continue;
            };
            let resets = l["resets_at"].as_str();
            match l["kind"].as_str() {
                Some("session") => out.push(window("5h", 300, pct, resets)),
                Some("weekly_all") => out.push(window("wk", 10_080, pct, resets)),
                Some("weekly_scoped") if pct > 0.0 => {
                    if let Some(m) = l["scope"]["model"]["display_name"].as_str() {
                        out.push(window(
                            &format!("wk {}", m.to_lowercase()),
                            10_080,
                            pct,
                            resets,
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if out.is_empty() {
        for (k, key, minutes) in [("five_hour", "5h", 300), ("seven_day", "wk", 10_080)] {
            if let Some(pct) = v[k]["utilization"].as_f64() {
                out.push(window(key, minutes, pct, v[k]["resets_at"].as_str()));
            }
        }
    }
    (!out.is_empty() || v.get("limits").is_some() || v.get("five_hour").is_some()).then_some(out)
}

pub fn tap_covers(tap_at: Option<i64>, now: i64) -> bool {
    tap_at.is_some_and(|t| now - t < REFRESH_MS)
}

pub fn due(wanted: bool, now: i64, last_attempt: i64, last_claude_activity: Option<i64>) -> bool {
    wanted
        && now - last_attempt >= REFRESH_MS
        && last_claude_activity.is_some_and(|t| now - t < ACTIVE_MS)
}

pub fn outcome(
    creds: Option<Creds>,
    now: i64,
    http: impl FnOnce(&Token) -> Option<(u16, Vec<u8>)>,
) -> Outcome {
    let Some(c) = creds else {
        return Outcome::NoLogin;
    };
    if c.expires_at.is_some_and(|t| t <= now) {
        return Outcome::Failed(Auth::Expired);
    }
    match http(&c.token) {
        Some((200, body)) => match serde_json::from_slice::<Value>(&body)
            .ok()
            .as_ref()
            .and_then(parse_usage)
        {
            Some(windows) => Outcome::Fresh(Limits {
                at: now,
                windows,
                plan: c.plan,
                ..Limits::default()
            }),
            None => Outcome::Failed(Auth::Unavailable),
        },
        Some((401 | 403, _)) => Outcome::Failed(Auth::Expired),
        _ => Outcome::Failed(Auth::Unavailable),
    }
}

pub fn fetch(t: &Token) -> Option<(u16, Vec<u8>)> {
    if t.0.chars().any(|c| c.is_control()) {
        return None;
    }
    let mut c = Command::new("curl");
    c.args([
        "-sS",
        "--proto",
        "=https",
        "--max-time",
        "5",
        "-H",
        "@-",
        "-w",
        "\n%{http_code}",
        URL,
    ]);
    let headers = format!(
        "Authorization: Bearer {}\nanthropic-beta: oauth-2025-04-20\nAccept: application/json\n",
        t.0
    );
    let (_, mut out) = run(c, Some(headers.into_bytes()))?;
    let nl = out.iter().rposition(|b| *b == b'\n')?;
    let code = std::str::from_utf8(&out[nl + 1..])
        .ok()?
        .trim()
        .parse()
        .ok()?;
    out.truncate(nl);
    Some((code, out))
}

fn run(mut c: Command, input: Option<Vec<u8>>) -> Option<(bool, Vec<u8>)> {
    let mut child = crate::git::Watchdog::guard(&mut c)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let dog = crate::git::Watchdog::start(child.id(), TIMEOUT + Duration::from_secs(1));
    if let (Some(mut stdin), Some(data)) = (child.stdin.take(), input) {
        let _ = stdin.write_all(&data);
    }
    let mut out = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let _ = stdout.take(MAX_BODY as u64).read_to_end(&mut out);
    }
    let status = child.wait().ok();
    if dog.finish() {
        return None;
    }
    status.map(|s| (s.success(), out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    const SECRET: &str = "sk-ant-oat01-SECRETSECRETSECRET";

    #[test]
    fn statusline_tap_only_replaces_oauth_while_fresh() {
        assert!(tap_covers(Some(1_000), 1_000 + REFRESH_MS - 1));
        assert!(!tap_covers(Some(1_000), 1_000 + REFRESH_MS));
        assert!(!tap_covers(Some(1_000), 1_000 + ACTIVE_MS / 2));
        assert!(!tap_covers(None, 1_000));
    }

    #[test]
    fn keychain_is_only_read_for_the_default_claude_dir() {
        let home = crate::home();
        assert!(keychain_applies(&home.join(".claude")));
        assert!(!keychain_applies(Path::new("/tmp/custom-claude")));
    }

    fn fixture() -> Value {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/usage/claude-oauth-usage.json");
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    fn creds(expires_at: Option<i64>) -> Option<Creds> {
        parse_creds(
            &serde_json::json!({"claudeAiOauth": {"accessToken": SECRET, "refreshToken": "r", "expiresAt": expires_at, "subscriptionType": "max"}})
                .to_string(),
        )
    }

    #[test]
    fn parses_observed_response() {
        let w = parse_usage(&fixture()).unwrap();
        assert_eq!(w.len(), 2);
        assert_eq!(
            (w[0].key.as_str(), w[0].minutes, w[0].used),
            ("5h", 300, 86.0)
        );
        assert_eq!(
            w[0].resets_at,
            parse_rfc3339("2026-10-30T00:20:00.095Z").unwrap()
        );
        assert_eq!(
            (w[1].key.as_str(), w[1].minutes, w[1].used),
            ("wk", 10_080, 48.0)
        );
        assert_eq!(
            w[1].resets_at,
            parse_rfc3339("2026-11-04T13:00:00.095Z").unwrap()
        );
    }

    #[test]
    fn scoped_weekly_kept_when_used_and_legacy_shape_falls_back() {
        let mut v = fixture();
        v["limits"][2]["percent"] = 12.into();
        let w = parse_usage(&v).unwrap();
        assert_eq!(w[2].key, "wk fable");
        v.as_object_mut().unwrap().remove("limits");
        let w = parse_usage(&v).unwrap();
        assert_eq!(
            w.iter()
                .map(|w| (w.key.as_str(), w.used))
                .collect::<Vec<_>>(),
            [("5h", 86.0), ("wk", 48.0)]
        );
        assert!(parse_usage(&serde_json::json!({"error": "x"})).is_none());
    }

    #[test]
    fn refresh_gating() {
        let now = 10 * ACTIVE_MS;
        let active = Some(now - 60_000);
        assert!(due(true, now, 0, active));
        assert!(!due(false, now, 0, active));
        assert!(!due(true, now, now - REFRESH_MS + 1, active));
        assert!(due(true, now, now - REFRESH_MS, active));
        assert!(!due(true, now, 0, None));
        assert!(!due(true, now, 0, Some(now - ACTIVE_MS)));
    }

    #[test]
    fn expired_token_never_calls_the_endpoint() {
        let called = Cell::new(false);
        let r = outcome(creds(Some(1_000)), 2_000, |_| {
            called.set(true);
            None
        });
        assert_eq!(r, Outcome::Failed(Auth::Expired));
        assert!(!called.get());
        let r = outcome(creds(Some(5_000)), 2_000, |_| Some((401, b"{}".to_vec())));
        assert_eq!(r, Outcome::Failed(Auth::Expired));
        assert_eq!(outcome(None, 2_000, |_| unreachable!()), Outcome::NoLogin);
    }

    #[test]
    fn ok_response_becomes_limits_with_plan() {
        let body = serde_json::to_vec(&fixture()).unwrap();
        let Outcome::Fresh(l) = outcome(creds(Some(5_000)), 2_000, |_| Some((200, body))) else {
            panic!()
        };
        assert_eq!(
            (l.at, l.plan.as_deref(), l.windows.len()),
            (2_000, Some("max"), 2)
        );
        assert_eq!(
            outcome(creds(None), 2_000, |_| Some((500, b"oops".to_vec()))),
            Outcome::Failed(Auth::Unavailable)
        );
        assert_eq!(
            outcome(creds(None), 2_000, |_| Some((200, b"<html>".to_vec()))),
            Outcome::Failed(Auth::Unavailable)
        );
    }

    #[test]
    fn token_is_redacted() {
        let c = creds(Some(5_000)).unwrap();
        let dbg = format!("{c:?}");
        assert!(!dbg.contains(SECRET), "{dbg}");
        assert!(dbg.contains("[redacted]"));
        let body = format!("{{\"echo\": \"{SECRET}\"}}").into_bytes();
        let r = outcome(Some(c), 2_000, |_| Some((200, body)));
        assert!(!format!("{r:?}").contains(SECRET));
        let ser = serde_json::to_string(&Auth::Expired).unwrap();
        assert_eq!(ser, "\"expired\"");
    }
}
