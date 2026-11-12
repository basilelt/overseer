use crate::actions::{self, find_line, hold_input, log_action, Launch};
use crate::git::{git, Git};
use crate::hold::{evaluate, input_empty};
use crate::registry::Registry;
use crate::sync::safe_id;
use crate::sys::OpenOptionsExt;
use crate::tmux::Tmux;
use crate::util::{ticket, ticket_exact};
use crate::{mkdir_private, scan, Ctx, Line};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Outcome = (&'static str, Option<String>);

const READY_CHECKS: [&str; 4] = ["alive", "inTmux", "atPrompt", "inputEmpty"];

fn refused(e: impl Into<String>) -> Outcome {
    (crate::wire::STATUS_REFUSED, Some(e.into()))
}

pub fn uuid() -> Result<String, String> {
    let mut b = [0u8; 16];
    crate::sys::random(&mut b).map_err(|e| format!("random: {e}"))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

fn ready_timeout() -> Duration {
    Duration::from_millis(
        std::env::var("OVERSEER_START_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(30_000),
    )
}

pub fn clone_for(ctx: &Ctx, repo: &str) -> Option<PathBuf> {
    let want = repo.trim_end_matches(".git").to_ascii_lowercase();
    if let Some(p) = ctx.config.repos.get(&want) {
        return Some(p.clone());
    }
    let mut g = Git::default();
    ctx.config
        .roots
        .iter()
        .flat_map(|r| {
            let mut c = vec![r.clone()];
            if let Ok(rd) = std::fs::read_dir(r) {
                c.extend(rd.flatten().map(|e| e.path()));
            }
            c
        })
        .filter(|c| c.join(".git").exists())
        .find(|c| {
            g.info(&c.to_string_lossy())
                .is_some_and(|i| i.repo.to_ascii_lowercase() == want)
        })
}

fn content_file(ctx: &Ctx, id: &str, text: &str) -> Result<PathBuf, String> {
    let dir = ctx.state_dir.join("tmp");
    mkdir_private(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.prompt"));
    let _ = std::fs::remove_file(&path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

fn wait_ready(tmux: &Tmux, target: &str) -> bool {
    let end = Instant::now() + ready_timeout();
    while Instant::now() < end {
        if input_empty(tmux.capture(target).as_deref()).ok {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

fn deliver(ctx: &Ctx, tmux: &Tmux, target: &str, id: &str, text: &str) -> Result<(), String> {
    if !wait_ready(tmux, target) {
        return Err(format!("the agent in {target} did not reach its prompt"));
    }
    let file = content_file(ctx, id, text)?;
    let buf = format!("sb-{id}");
    let f = file.to_string_lossy();
    let r = tmux
        .ok(&["load-buffer", "-b", &buf, &f])
        .and_then(|_| tmux.ok(&["paste-buffer", "-d", "-p", "-b", &buf, "-t", target]))
        .and_then(|_| {
            std::thread::sleep(Duration::from_millis(300));
            tmux.ok(&["send-keys", "-t", target, "Enter"])
        });
    let _ = std::fs::remove_file(&file);
    if r.is_err() {
        let _ = tmux.ok(&["delete-buffer", "-b", &buf]);
    }
    r
}

struct Plan {
    clone: PathBuf,
    branch: String,
    path: PathBuf,
    session: String,
}

fn plan(ctx: &Ctx, line_id: &str, args: &Value) -> Result<Plan, String> {
    let s = |k: &str| args[k].as_str().unwrap_or_default();
    if s("agent") != crate::agents::DEFAULT_ID {
        return Err(format!(
            "start supports agent claude only, not '{}'",
            s("agent")
        ));
    }
    if s("prompt").trim().is_empty() {
        return Err("start needs a prompt".into());
    }
    let repo = s("repo");
    let clone = clone_for(ctx, repo)
        .ok_or_else(|| format!("no clone of {repo} on this station (config repos or roots)"))?;
    let tk = args["ticketKey"].as_str().and_then(ticket);
    let branch = match (s("branch"), &tk) {
        ("", Some(t)) => format!("feedback/{t}"),
        ("", None) => return Err("start needs a branch or a ticketKey".into()),
        (b, _) => b.to_string(),
    };
    let c = clone.to_string_lossy();
    if git(&c, &["check-ref-format", "--branch", &branch]).is_none() {
        return Err(format!("'{branch}' is not a valid branch name"));
    }
    let path = crate::layout::feedback_checkout_dir(&ctx.config.worktree_dir, &clone, &branch);
    if path.exists() {
        return Err(format!("worktree path {} already exists", path.display()));
    }
    Ok(Plan {
        session: format!("sb-{}", &line_id[line_id.len().saturating_sub(6)..]),
        clone,
        branch,
        path,
    })
}

pub fn start(ctx: &Ctx, line_id: &str, args: &Value, dry: bool) -> Outcome {
    if !safe_id(line_id) {
        return refused("invalid line id");
    }
    let reg = match Registry::open(&ctx.state_dir) {
        Ok(r) => r,
        Err(e) => return (crate::wire::STATUS_FAILED, Some(e)),
    };
    let known: Option<String> = reg
        .conn
        .query_row(
            "SELECT worktree_path FROM lines WHERE id = ?1",
            [line_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(w) = known {
        return (
            crate::wire::STATUS_SUCCEEDED,
            Some(format!("already started in {w}")),
        );
    }
    let p = match plan(ctx, line_id, args) {
        Ok(p) => p,
        Err(e) => return refused(e),
    };
    let what = format!(
        "claude in {} on branch {} (tmux session {})",
        p.path.display(),
        p.branch,
        p.session
    );
    if dry {
        return (
            crate::wire::STATUS_SUCCEEDED,
            Some(format!("would start {what}")),
        );
    }
    let r = launch(ctx, &reg, line_id, args, &p);
    log_action(
        ctx,
        &json!({
            "action": crate::wire::CMD_START, "lineId": line_id, "worktree": p.path, "branch": p.branch,
            "session": p.session, "error": r.as_ref().err(),
        }),
    );
    match r {
        Ok(()) => (
            crate::wire::STATUS_SUCCEEDED,
            Some(format!("started {what}")),
        ),
        Err(e) => (crate::wire::STATUS_FAILED, Some(e)),
    }
}

fn launch(ctx: &Ctx, reg: &Registry, line_id: &str, args: &Value, p: &Plan) -> Result<(), String> {
    let tmux = ctx.tmux.clone().ok_or("tmux unavailable")?;
    let c = p.clone.to_string_lossy();
    let wt = p.path.to_string_lossy();
    if let Some(parent) = p.path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let exists = git(
        &c,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{}", p.branch),
        ],
    )
    .is_some();
    let added = if exists {
        git(&c, &["worktree", "add", &wt, &p.branch])
    } else {
        git(&c, &["worktree", "add", "-b", &p.branch, &wt])
    };
    added.ok_or_else(|| format!("git worktree add {wt} failed"))?;
    let sid = uuid()?;
    let s = |k: &str| args[k].as_str().unwrap_or_default().to_string();
    let wt_top = p.path.canonicalize().unwrap_or(p.path.clone());
    let line = Line {
        id: line_id.into(),
        agent: crate::agents::DEFAULT_ID,
        session_id: sid.clone(),
        owner: s("owner"),
        station_id: ctx.config.station_id.clone(),
        repo: s("repo"),
        branch: p.branch.clone(),
        worktree_path: wt_top.to_string_lossy().into_owned(),
        ticket: args["ticketKey"].as_str().and_then(ticket),
        ..Default::default()
    };
    reg.reserve(
        &line,
        args["origin"].as_str().unwrap_or("feedback"),
        ctx.now_ms,
    )?;
    let argv = claude_argv(reg, &ctx.config.start_args, sid)?;
    let cmd = format!(
        "{}; {}",
        argv.iter()
            .map(|a| crate::util::shell_quote(a))
            .collect::<Vec<_>>()
            .join(" "),
        crate::util::SHELL_FALLBACK
    );
    tmux.ok(&[
        "new-session",
        "-d",
        "-s",
        &p.session,
        "-c",
        &wt_top.to_string_lossy(),
        &cmd,
    ])?;
    deliver(ctx, &tmux, &p.session, line_id, &s("prompt"))
}

fn claude_argv(reg: &Registry, start_args: &[String], sid: String) -> Result<Vec<String>, String> {
    let mut argv = vec!["claude".to_string()];
    argv.extend(start_args.iter().cloned());
    if crate::agents::auto_approve_enabled(reg, "claude")? {
        argv.extend(crate::agents::auto_approve_flags("claude", start_args));
    }
    argv.extend(["--session-id".into(), sid]);
    Ok(argv)
}

pub fn relaunch(ctx: &Ctx, line_id: &str, message: &str, dry: bool) -> Outcome {
    if message.trim().is_empty() {
        return refused("relaunch needs a message");
    }
    let lines = match scan(ctx) {
        Ok(l) => l,
        Err(e) => return (crate::wire::STATUS_FAILED, Some(e)),
    };
    let Ok(l) = find_line(&lines, line_id) else {
        return refused(format!("line {line_id} is not on this station"));
    };
    let tmux = ctx.tmux.clone().unwrap_or_default();
    let r = if l.alive() {
        let failed: Vec<String> = evaluate(&hold_input(ctx, l))
            .into_iter()
            .filter(|c| READY_CHECKS.contains(&c.name) && !c.ok)
            .map(|c| format!("{}: {}", c.name, c.detail))
            .collect();
        if !failed.is_empty() {
            return refused(failed.join("; "));
        }
        let pane = l.pane.clone().unwrap_or_default();
        if dry {
            return (
                crate::wire::STATUS_SUCCEEDED,
                Some(format!("would send the message to {pane}")),
            );
        }
        deliver(ctx, &tmux, &pane, &l.id, message).map(|_| format!("message sent to {pane}"))
    } else {
        if dry {
            return actions::resume_plan(ctx, &lines, l, Launch::Detached)
                .map(|p| {
                    (
                        crate::wire::STATUS_SUCCEEDED,
                        Some(format!(
                            "would resume in {} and send the message",
                            p.r#where
                        )),
                    )
                })
                .unwrap_or_else(refused);
        }
        actions::resume(ctx, &l.id, false, Launch::Detached).and_then(|p| {
            let target = p
                .pane
                .clone()
                .or(p.session.clone())
                .ok_or("resumed, but no tmux target")?;
            deliver(ctx, &tmux, &target, &l.id, message)
                .map(|_| format!("resumed in {} and sent the message", p.r#where))
        })
    };
    log_action(
        ctx,
        &json!({"action": crate::wire::CMD_RELAUNCH, "lineId": l.id, "error": r.as_ref().err()}),
    );
    match r {
        Ok(m) => (crate::wire::STATUS_SUCCEEDED, Some(m)),
        Err(e) => (crate::wire::STATUS_FAILED, Some(e)),
    }
}

pub fn summary(l: &Line, pr: Option<&str>, titles: &[String]) -> String {
    let mut s = format!(
        "You take over line {} ({}) from station {}.\nRepository: {}\nBranch: {}\n",
        l.id, l.agent, l.station_id, l.repo, l.branch
    );
    if let Some(t) = &l.ticket {
        s.push_str(&format!("Ticket: {t}\n"));
    }
    if let Some(p) = pr {
        s.push_str(&format!("Pull request: {p}\n"));
    }
    if !titles.is_empty() {
        s.push_str("Recent turns of the previous agent:\n");
        for t in titles {
            s.push_str(&format!("- {t}\n"));
        }
    }
    s.push_str("Read the branch's commits and diff before changing anything.\n");
    s
}

pub fn pick(ctx: &Ctx, key: &str) -> Result<Line, String> {
    let key = ticket_exact(key)?;
    let mut all = scan(ctx)?;
    if let Some(u) = crate::remote::user(&ctx.state_dir) {
        match crate::remote::lines(ctx, &u, None) {
            Ok(r) => all.extend(r.into_iter().map(|(l, _)| l)),
            Err(e) => {
                eprintln!("overseer: api unreachable, looking on this station only: {e}")
            }
        }
    }
    all.retain(|l| {
        l.ticket.as_deref() == Some(key.as_str()) && l.state != crate::wire::STATE_HUNG_UP
    });
    all.sort_by_key(|l| (l.owner == ctx.config.owner, -l.last_activity_ms));
    all.into_iter()
        .next()
        .ok_or_else(|| format!("no open line for {key} on this station or through the api"))
}

fn remote_titles(ctx: &Ctx, l: &Line) -> Vec<String> {
    let Ok(mut argv) = crate::remote::ssh_argv(ctx, l) else {
        return vec![];
    };
    argv.truncate(4);
    argv[1] = "-T".into();
    argv.extend([
        ctx.config.remote_command.clone(),
        "titles".into(),
        l.id.clone(),
    ]);
    crate::git::output_within(
        std::process::Command::new(&argv[0]).args(&argv[1..]),
        crate::git::PROBE_TIMEOUT,
    )
    .ok()
    .and_then(|o| serde_json::from_slice::<Vec<String>>(&o.stdout).ok())
    .unwrap_or_default()
}

fn add_worktree(clone: &Path, dir: &Path, branch: &str) -> Result<(), String> {
    let c = clone.to_string_lossy();
    let d = dir.to_string_lossy();
    let _ = git(&c, &["fetch", "-q", "origin", branch]);
    let has = |r: &str| git(&c, &["rev-parse", "--verify", "-q", r]).is_some();
    let added = if has(&format!("refs/heads/{branch}")) {
        git(&c, &["worktree", "add", &d, branch])
    } else if has(&format!("refs/remotes/origin/{branch}")) {
        let up = format!("origin/{branch}");
        git(&c, &["worktree", "add", "--track", "-b", branch, &d, &up])
    } else {
        return Err(format!(
            "branch {branch} is neither in {c} nor on its origin"
        ));
    };
    added.map(|_| ()).ok_or_else(|| {
        format!("worktree add {d} {branch} failed (is the branch checked out elsewhere?)")
    })
}

pub fn patch_local(ctx: &Ctx, l: &Line, print: bool) -> Result<String, String> {
    let clone = clone_for(ctx, &l.repo).ok_or_else(|| {
        format!(
            "no clone of {} on this station (config repos or roots)",
            l.repo
        )
    })?;
    let dir = patch_dir(ctx, l);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    let session = crate::layout::patch_session_name(&l.branch);
    if print {
        return Ok(format!(
            "would add worktree {} on branch {} from {}, start claude in tmux session {session} and send it a summary of line {}",
            dir.display(),
            l.branch,
            clone.display(),
            l.id
        ));
    }
    let titles = if l.station_id == ctx.config.station_id {
        crate::claude::titles(&ctx.claude_dir, &l.session_id, 5)
    } else {
        remote_titles(ctx, l)
    };
    let pr = crate::gh::pr_info(&l.repo, &l.branch)["pr"]["url"]
        .as_str()
        .map(str::to_string);
    let text = summary(l, pr.as_deref(), &titles);
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    add_worktree(&clone, &dir, &l.branch)?;
    let id = format!(
        "{}-{}",
        ctx.config.station_id,
        crate::util::ulid(ctx.now_ms)
    );
    let sid = uuid()?;
    let top = dir.canonicalize().unwrap_or(dir.clone());
    let line = Line {
        id: id.clone(),
        agent: crate::agents::DEFAULT_ID,
        session_id: sid.clone(),
        owner: ctx.config.owner.clone(),
        station_id: ctx.config.station_id.clone(),
        repo: l.repo.clone(),
        branch: l.branch.clone(),
        worktree_path: top.to_string_lossy().into_owned(),
        ticket: l.ticket.clone(),
        ..Default::default()
    };
    Registry::open(&ctx.state_dir)?.reserve(&line, "dev", ctx.now_ms)?;
    let tmux = ctx.tmux.clone().ok_or("tmux unavailable")?;
    let cmd = format!(
        "claude --session-id {}; {}",
        crate::util::shell_quote(&sid),
        crate::util::SHELL_FALLBACK
    );
    tmux.ok(&[
        "new-session",
        "-d",
        "-s",
        &session,
        "-c",
        &top.to_string_lossy(),
        &cmd,
    ])?;
    deliver(ctx, &tmux, &session, &id, &text)?;
    log_action(
        ctx,
        &json!({"action": "patch", "lineId": id, "from": l.id, "worktree": top, "titles": titles.len()}),
    );
    Ok(format!(
        "new line {id} in {} (tmux session {session}), seeded with a summary of {}\nattach with: tmux attach -t {session}",
        top.display(),
        l.id
    ))
}

pub fn patch_dir(ctx: &Ctx, l: &Line) -> PathBuf {
    crate::layout::patch_checkout_dir(&ctx.config.worktree_dir, Path::new(&l.repo), &l.branch)
}

#[cfg(test)]
mod claude_argv_tests {
    use super::*;

    fn reg(name: &str) -> (Registry, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("sb-runner-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (Registry::open(&dir).unwrap(), dir)
    }

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn runner_claude_line_honours_auto_approve_setting() {
        let (reg, dir) = reg("setting");
        let on = claude_argv(&reg, &v(&["--verbose"]), "sid".into()).unwrap();
        assert_eq!(
            on,
            v(&[
                "claude",
                "--verbose",
                "--permission-mode",
                "auto",
                "--session-id",
                "sid"
            ])
        );
        crate::agents::set_auto_approve(&reg, "claude", false).unwrap();
        let off = claude_argv(&reg, &v(&["--verbose"]), "sid".into()).unwrap();
        assert_eq!(off, v(&["claude", "--verbose", "--session-id", "sid"]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn runner_claude_line_keeps_explicit_permission_mode() {
        let (reg, dir) = reg("explicit");
        let args = v(&["--permission-mode", "acceptEdits"]);
        let argv = claude_argv(&reg, &args, "sid".into()).unwrap();
        assert_eq!(
            argv,
            v(&[
                "claude",
                "--permission-mode",
                "acceptEdits",
                "--session-id",
                "sid"
            ])
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod slug_tests {
    use crate::layout::checkout_slug as slug;

    #[test]
    fn keeps_dots_and_underscores_and_trims_edges() {
        let table = [
            ("feature/AS-77_x", "feature-AS-77_x"),
            ("a..b", "a..b"),
            ("-lead-and-trail-", "lead-and-trail"),
            ("..hidden.", "hidden"),
            ("a//b  c", "a--b--c"),
            ("Été", "t"),
            ("é", ""),
            ("", ""),
            ("---", ""),
            ("my_repo.git", "my_repo.git"),
        ];
        for (i, o) in table {
            assert_eq!(slug(i), o, "{i:?}");
        }
    }
}
