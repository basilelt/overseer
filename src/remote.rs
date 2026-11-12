use crate::actions::find_line;
use crate::sync::{endpoint, private_file, safe_id};
use crate::util::{parse_rfc3339, shell_quote};
use crate::wire::{
    STATE_ACTIVE, STATE_HELD, STATE_HUNG_UP, STATE_IDLE, STATUS_FAILED, STATUS_REFUSED,
    STATUS_SUCCEEDED,
};
use crate::{mkdir_private, Ctx, Line};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

pub struct User {
    pub api: String,
    pub token: String,
}

pub fn user(state_dir: &Path) -> Option<User> {
    let v: Value =
        serde_json::from_slice(&std::fs::read(state_dir.join(crate::paths::USER_FILE)).ok()?)
            .ok()?;
    Some(User {
        api: v["api"].as_str()?.to_string(),
        token: v["token"].as_str()?.to_string(),
    })
}

pub fn get(u: &User, path: &str) -> Result<(u16, Value), String> {
    call(u, "GET", path, None)
}

pub fn call(
    u: &User,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let (base, sa) = endpoint(&u.api)?;
    let (host, prefix) = match base.split_once('/') {
        Some((h, p)) => (h.to_string(), format!("/{p}")),
        None => (base.clone(), String::new()),
    };
    let mut s = TcpStream::connect_timeout(&sa, Duration::from_secs(5))
        .map_err(|e| format!("api {}: {e}", u.api))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let b = body.map(Value::to_string).unwrap_or_default();
    write!(
        s,
        "{method} {prefix}{path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
        u.token,
        b.len()
    )
    .map_err(|e| e.to_string())?;
    let mut raw = vec![];
    s.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&raw);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or("bad response from the api")?;
    let code = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("bad response from the api")?;
    Ok((code, serde_json::from_str(body).unwrap_or(Value::Null)))
}

pub fn relay_timeout() -> Duration {
    Duration::from_millis(
        std::env::var("OVERSEER_RELAY_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(15_000),
    )
}

pub fn relay(ctx: &Ctx, line_id: &str, kind: &str, args: Value) -> (u16, Value) {
    let err = |c: u16, e: String| (c, json!({ "error": e }));
    let Some(u) = user(&ctx.state_dir) else {
        return err(409, "not logged in to the api on this station: run `overseer login --api URL` to act on other stations".into());
    };
    let id = format!("sb-{}", crate::util::ulid(ctx.now_ms));
    let body = json!({ "id": id, "kind": kind, "args": args });
    match call(
        &u,
        "POST",
        &format!("/v1/lines/{line_id}/commands"),
        Some(&body),
    ) {
        Ok((202, _)) => {}
        Ok((code, v)) => {
            return err(
                if code == 404 { 404 } else { 409 },
                format!(
                    "api answered HTTP {code}: {}",
                    v["error"].as_str().unwrap_or("")
                ),
            )
        }
        Err(e) => return err(502, e),
    }
    let end = std::time::Instant::now() + relay_timeout();
    while std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(200));
        let Ok((200, list)) = get(&u, &format!("/v1/lines/{line_id}/commands")) else {
            continue;
        };
        let Some(c) = list
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["id"] == id.as_str())
        else {
            continue;
        };
        let reason = c["reason"].as_str().unwrap_or_default().to_string();
        match c["status"].as_str() {
            Some(STATUS_SUCCEEDED) => {
                return (
                    200,
                    json!({
                        "lineId": line_id, "relayed": true, "commandId": id,
                        "action": reason, "steps": [reason], "predicates": {},
                        "argv": [], "cwd": "", "where": reason,
                        "ticketKey": args["ticketKey"],
                    }),
                )
            }
            Some(STATUS_REFUSED | STATUS_FAILED) => return err(409, reason),
            _ => {}
        }
    }
    err(
        504,
        format!(
            "command {id} sent, but the station did not answer within {} s",
            relay_timeout().as_secs()
        ),
    )
}

fn get_ok(u: &User, path: &str) -> Result<Value, String> {
    match get(u, path)? {
        (200, v) => Ok(v),
        (401, _) => Err("the api refused the user token; run `overseer login` again".into()),
        (code, v) => Err(format!(
            "api answered HTTP {code}: {}",
            v["error"].as_str().unwrap_or("")
        )),
    }
}

pub fn to_line(row: &Value) -> Option<Line> {
    let l = &row["line"];
    let s = |k: &str| l[k].as_str().unwrap_or_default().to_string();
    let id = l["id"].as_str()?.to_string();
    Some(Line {
        resume_command: format!("overseer resume {id}"),
        id,
        owner: s("owner"),
        station_id: s("stationId"),
        agent: crate::agents::id_of(l["agent"].as_str()?),
        session_id: s("agentSessionId"),
        ticket: l["ticketKey"].as_str().map(str::to_string),
        repo: s("repo"),
        branch: s("branch"),
        worktree_path: s("worktreePath"),
        cwd: s("worktreePath"),
        state: match l["state"].as_str()? {
            STATE_ACTIVE => STATE_ACTIVE,
            STATE_IDLE => STATE_IDLE,
            STATE_HELD => STATE_HELD,
            STATE_HUNG_UP => STATE_HUNG_UP,
            _ => "unknown",
        },
        last_activity_ms: l["lastActivityAt"]
            .as_str()
            .and_then(parse_rfc3339)
            .unwrap_or(0),
        offline: row["stationOnline"] != true,
        ..Default::default()
    })
}

pub fn lines(ctx: &Ctx, u: &User, owner: Option<&str>) -> Result<Vec<(Line, Value)>, String> {
    let mut path = "/v1/lines?includeHungUp=true".to_string();
    if let Some(o) = owner {
        path.push_str(&format!("&owner={}", crate::util::percent_encode(o)));
    }
    let v = get_ok(u, &path)?;
    Ok(v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| Some((to_line(row)?, row.clone())))
        .filter(|(l, _)| l.station_id != ctx.config.station_id)
        .collect())
}

pub fn find(ctx: &Ctx, u: &User, q: &str) -> Result<Line, String> {
    let all: Vec<Line> = lines(ctx, u, None)?.into_iter().map(|(l, _)| l).collect();
    find_line(&all, q)
        .cloned()
        .map_err(|e| format!("{e} on this station or through the api"))
}

pub fn ssh_argv(ctx: &Ctx, l: &Line) -> Result<Vec<String>, String> {
    if l.offline {
        return Err(format!(
            "line {} runs on station {}, which is offline; resume it once the station is back",
            l.id, l.station_id
        ));
    }
    let target = ctx
        .config
        .ssh
        .get(&l.station_id)
        .cloned()
        .unwrap_or_else(|| l.station_id.clone());
    if target.is_empty() || target.starts_with('-') || target.contains(char::is_whitespace) {
        return Err(format!(
            "invalid ssh target '{target}' for station {}",
            l.station_id
        ));
    }
    if !safe_id(&l.id) {
        return Err(format!("invalid line id '{}'", l.id));
    }
    Ok(vec![
        "ssh".into(),
        "-t".into(),
        target,
        "--".into(),
        ctx.config.remote_command.clone(),
        "resume".into(),
        l.id.clone(),
        "--attach".into(),
    ])
}

pub fn resume(ctx: &Ctx, u: &User, q: &str, print: bool) -> Result<(), String> {
    use crate::sys::CommandExt;
    let l = find(ctx, u, q)?;
    let argv = ssh_argv(ctx, &l)?;
    if print {
        let quoted: Vec<String> = argv.iter().map(|a| shell_quote(a)).collect();
        println!("{}", quoted.join(" "));
        return Ok(());
    }
    eprintln!(
        "overseer: {} is on station {}; connecting with ssh",
        l.id, l.station_id
    );
    let err = std::process::Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(format!("exec ssh: {err}"))
}

pub fn parse_url(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("overseer://")
        .ok_or("not a overseer:// URL")?;
    match rest.trim_end_matches('/').split_once('/') {
        Some(("resume", id)) if safe_id(id) => Ok(id.to_string()),
        Some(("resume", _)) => Err("invalid line id in the URL".into()),
        _ => Err(format!(
            "unsupported URL {url}: only overseer://resume/<lineId>"
        )),
    }
}

pub fn cli_login(args: &[String]) -> ExitCode {
    let api = match args {
        [flag, url] if flag == "--api" && url.starts_with("http://") => url.trim_end_matches('/'),
        _ => {
            eprintln!("usage: overseer login --api http://HOST:PORT  (user token on stdin)");
            return ExitCode::from(2);
        }
    };
    let mut token = String::new();
    if std::io::stdin().read_line(&mut token).is_err() || token.trim().is_empty() {
        eprintln!("overseer: no user token on stdin");
        return ExitCode::from(2);
    }
    let u = User {
        api: api.into(),
        token: token.trim().into(),
    };
    let state = crate::default_state_dir();
    let r = get_ok(&u, "/v1/stations").and_then(|st| {
        mkdir_private(&state).map_err(|e| e.to_string())?;
        private_file(
            &state.join(crate::paths::USER_FILE),
            json!({ "api": u.api, "token": u.token })
                .to_string()
                .as_bytes(),
        )?;
        Ok(st.as_array().map_or(0, Vec::len))
    });
    match r {
        Ok(n) => {
            println!("logged in to {api} ({n} stations)");
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

    #[test]
    fn parses_only_resume_urls() {
        assert_eq!(
            parse_url("overseer://resume/vm-01JABC").unwrap(),
            "vm-01JABC"
        );
        assert_eq!(
            parse_url("overseer://resume/vm-01JABC/").unwrap(),
            "vm-01JABC"
        );
        assert!(parse_url("overseer://resume/../x").is_err());
        assert!(parse_url("overseer://resume/a?b=c").is_err());
        assert!(parse_url("overseer://hold/vm-1").is_err());
        assert!(parse_url("https://resume/vm-1").is_err());
    }

    #[test]
    fn maps_api_rows_to_lines() {
        let row = json!({"line": {"id": "vm2-01J", "owner": "a@b", "stationId": "vm2",
            "agent": "claude", "agentSessionId": "s", "repo": "r", "branch": "b",
            "worktreePath": "/w", "state": "idle", "lastActivityAt": "2026-10-28T10:00:00Z"},
            "stationOnline": false});
        let l = to_line(&row).unwrap();
        assert_eq!(
            (l.state, l.offline, l.station_id.as_str()),
            ("idle", true, "vm2")
        );
        assert_eq!(l.resume_command, "overseer resume vm2-01J");
        assert!(l.last_activity_ms > 0);
    }
}
