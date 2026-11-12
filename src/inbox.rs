use crate::actions::{self, log_action, Launch, Report};
use crate::sync::{complete, inbox};
use crate::wire::*;
use crate::{mkdir_private, scan, Ctx};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const DISABLED: &str = "disabled: acceptRemoteCommands is false on this station";

pub fn running(state_dir: &Path) -> PathBuf {
    state_dir.join("commands").join("running")
}

#[derive(Debug)]
pub struct Outcome {
    pub id: String,
    pub status: &'static str,
    pub reason: Option<String>,
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn finish(
    ctx: &Ctx,
    cmd: &Value,
    id: &str,
    status: &'static str,
    reason: Option<String>,
) -> Outcome {
    log_action(
        ctx,
        &json!({
            "action": "remote-command", "commandId": id, "kind": cmd["kind"],
            "lineId": cmd["lineId"], "requestedBy": cmd["requestedBy"],
            "status": status, "reason": reason,
        }),
    );
    if let Err(e) = complete(&ctx.state_dir, id, status, reason.as_deref(), ctx.now_ms) {
        eprintln!("overseer: command {id}: {e}");
    }
    Outcome {
        id: id.into(),
        status,
        reason,
    }
}

fn recover(ctx: &Ctx, dir: &Path) -> Vec<Outcome> {
    let mut out = vec![];
    for f in json_files(dir) {
        let name = f
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Some((pid, id)) = name.split_once('-') else {
            continue;
        };
        if pid.parse().ok().and_then(|p| ctx.probe.info(p)).is_some() {
            continue;
        }
        let cmd = read(&f);
        out.push(finish(
            ctx,
            &cmd,
            id,
            STATUS_FAILED,
            Some("interrupted before completion".into()),
        ));
        let _ = std::fs::remove_file(&f);
    }
    out
}

fn read(path: &Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

pub fn process(ctx: &Ctx) -> Vec<Outcome> {
    let run_dir = running(&ctx.state_dir);
    let mut out = recover(ctx, &run_dir);
    let files = json_files(&inbox(&ctx.state_dir));
    if files.is_empty() || mkdir_private(&run_dir).is_err() {
        return out;
    }
    let me = std::process::id();
    for f in files {
        let id = f
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let claimed = run_dir.join(format!("{me}-{id}.json"));
        if std::fs::rename(&f, &claimed).is_err() {
            continue;
        }
        let cmd = read(&claimed);
        let (status, reason) = execute(ctx, &cmd);
        out.push(finish(ctx, &cmd, &id, status, reason));
        let _ = std::fs::remove_file(&claimed);
    }
    out
}

fn report(r: Result<Report, String>) -> (&'static str, Option<String>) {
    match r {
        Err(e) => (STATUS_REFUSED, Some(e)),
        Ok(r) => r.outcome(),
    }
}

pub fn execute(ctx: &Ctx, cmd: &Value) -> (&'static str, Option<String>) {
    if !ctx.config.accept_remote_commands {
        return (STATUS_REFUSED, Some(DISABLED.into()));
    }
    let line = cmd["lineId"].as_str().unwrap_or_default();
    let kind = cmd["kind"].as_str().unwrap_or_default();
    let args = &cmd["args"];
    let dry = args["dryRun"].as_bool().unwrap_or(true);
    if kind == CMD_START {
        return crate::runner::start(ctx, line, args, dry);
    }
    match scan(ctx) {
        Err(e) => return (STATUS_FAILED, Some(e)),
        Ok(lines) if line.is_empty() || !lines.iter().any(|l| l.id == line) => {
            return (
                STATUS_REFUSED,
                Some(format!("line {line} is not on this station")),
            )
        }
        Ok(_) => {}
    }
    match kind {
        CMD_HOLD => report(actions::hold(ctx, line, !dry)),
        CMD_CLOSE => report(actions::close(ctx, line, !dry, false, false)),
        CMD_RESUME => match actions::resume(ctx, line, dry, Launch::Detached) {
            Ok(p) if dry => (
                STATUS_SUCCEEDED,
                Some(format!("would resume in {}", p.r#where)),
            ),
            Ok(p) => (STATUS_SUCCEEDED, Some(format!("resumed in {}", p.r#where))),
            Err(e) => (STATUS_REFUSED, Some(e)),
        },
        CMD_LINK => match args["ticketKey"].as_str() {
            None => (STATUS_REFUSED, Some("link needs args.ticketKey".into())),
            Some(_) if dry => (STATUS_SUCCEEDED, Some("would link".into())),
            Some(k) => match actions::link(ctx, line, k) {
                Ok((_, t)) => (
                    STATUS_SUCCEEDED,
                    Some(format!(
                        "linked to {}",
                        t.as_deref().unwrap_or("branch ticket")
                    )),
                ),
                Err(e) => (STATUS_REFUSED, Some(e)),
            },
        },
        CMD_RELAUNCH => {
            crate::runner::relaunch(ctx, line, args["message"].as_str().unwrap_or_default(), dry)
        }
        _ => (
            STATUS_REFUSED,
            Some(format!("unknown command kind '{kind}'")),
        ),
    }
}
