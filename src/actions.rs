use crate::hold::{evaluate, Check, HoldInput};
use crate::lifecycle::is_stub_wrapper;
use crate::procs::mem_bytes;
use crate::registry::Registry;
use crate::sys::CommandExt;
use crate::sys::OpenOptionsExt;
use crate::util::{format_rfc3339, shell_quote, ticket_exact};
use crate::{scan, Ctx, Line};
use serde_json::{json, Value};
use std::io::Write;
use std::time::Duration;

pub fn find_line<'a>(lines: &'a [Line], q: &str) -> Result<&'a Line, String> {
    if let Some(l) = lines.iter().find(|l| l.id == q || l.session_id == q) {
        return Ok(l);
    }
    let hits: Vec<&Line> = lines
        .iter()
        .filter(|l| {
            l.session_id.starts_with(q)
                || l.id.starts_with(q)
                || l.id
                    .rsplit('-')
                    .next()
                    .is_some_and(|u| u.starts_with(&q.to_ascii_uppercase()))
        })
        .collect();
    match hits[..] {
        [l] => Ok(l),
        [] => Err(format!("no line matches '{q}'")),
        _ => Err(format!(
            "'{q}' matches {} lines; use more characters",
            hits.len()
        )),
    }
}

pub fn log_action(ctx: &Ctx, rec: &Value) {
    let mut rec = rec.clone();
    rec["at"] = json!(format_rfc3339(ctx.now_ms));
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(ctx.state_dir.join("actions.log"))
    else {
        return;
    };
    let _ = writeln!(f, "{rec}");
}

pub fn hold_input(ctx: &Ctx, l: &Line) -> HoldInput {
    let tmux = ctx.tmux.as_ref();
    let term = l
        .pane
        .is_none()
        .then(|| crate::deliver::line_term(l))
        .flatten();
    HoldInput {
        agent: l.agent.to_string(),
        alive: l.alive(),
        busy: l.busy_known.then_some(l.busy),
        last_event: l.hook.as_ref().map(|h| h.event.clone()),
        pane: l.pane.clone(),
        term: term.as_ref().map(|t| t.info().id),
        screen: l
            .pane
            .as_deref()
            .and_then(|p| tmux.and_then(|t| t.capture(p)))
            .or_else(|| term.map(|t| t.grid().ansi())),
        children: l
            .pid
            .map(|p| ctx.probe.children(p).into_iter().map(|c| c.1).collect())
            .unwrap_or_default(),
        mcp_hints: ctx.config.mcp_hints.clone(),
        subagents: l.hook.as_ref().map_or(0, |h| h.subagents),
        tasks: l.hook.as_ref().map_or(0, |h| h.tasks),
        pinned: l.pinned,
        needs_input: l.needs_input,
        duplicate: l.duplicate,
    }
}

pub struct Report {
    pub line_id: String,
    pub checks: Vec<Check>,
    pub action: &'static str,
    pub steps: Vec<String>,
    pub error: Option<String>,
    /// The hold is queued for the end of the turn: the failed checks are transient.
    pub pending: bool,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.error.is_none() && (self.pending || self.checks.iter().all(|c| c.ok))
    }

    pub fn http_status(&self) -> u16 {
        if self.ok() {
            200
        } else {
            409
        }
    }

    pub fn exit_code(&self) -> u8 {
        if self.ok() {
            0
        } else {
            3
        }
    }

    pub fn outcome(&self) -> (&'static str, Option<String>) {
        if self.error.is_some() {
            return (crate::wire::STATUS_FAILED, self.error.clone());
        }
        if !self.ok() {
            let failed: Vec<String> = self
                .checks
                .iter()
                .filter(|c| !c.ok)
                .map(|c| format!("{}: {}", c.name, c.detail))
                .collect();
            return (crate::wire::STATUS_REFUSED, Some(failed.join("; ")));
        }
        (crate::wire::STATUS_SUCCEEDED, Some(self.action.to_string()))
    }

    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "lineId": self.line_id,
            "predicates": crate::hold::to_json(&self.checks),
            "action": self.action,
            "steps": self.steps,
        });
        if let Some(e) = &self.error {
            v["error"] = json!(e);
        }
        v
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for c in &self.checks {
            let mark = if c.ok { "ok  " } else { "FAIL" };
            s.push_str(&format!(
                "  {mark} {}{}\n",
                c.name,
                if c.ok {
                    String::new()
                } else {
                    format!(": {}", c.detail)
                }
            ));
        }
        for st in &self.steps {
            s.push_str(&format!("  - {st}\n"));
        }
        if let Some(e) = &self.error {
            s.push_str(&format!("  error: {e}\n"));
        }
        s.push_str(&format!("  action: {}\n", self.action));
        s
    }
}

pub(crate) fn self_exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "overseer".into())
}

pub fn stub_command(ctx: &Ctx, line_id: &str) -> String {
    format!(
        "trap : INT; OVERSEER_STATE_DIR={} {} stub {}; {}",
        shell_quote(&ctx.state_dir.to_string_lossy()),
        shell_quote(&self_exe()),
        shell_quote(line_id),
        crate::util::SHELL_FALLBACK
    )
}

fn hold_steps(ctx: &Ctx, l: &Line, stub: bool) -> Vec<String> {
    if l.pane.is_none() {
        if let Some(t) = crate::deliver::line_term(l) {
            let id = t.info().id;
            return vec![
                format!(
                    "record {} as held(user) with launch flags {:?}",
                    l.id, l.launch_args
                ),
                format!("send SIGTERM to agent pid {}", l.pid.unwrap_or(0)),
                "wait up to 10 s for the agent to exit (abort and revert on timeout)".into(),
                if stub {
                    format!(
                        "start the held stub in term {id}: {}",
                        stub_command(ctx, &l.id)
                    )
                } else {
                    format!("close term {id}")
                },
            ];
        }
    }
    let Some(pane) = l.pane.as_deref() else {
        return vec![
            format!(
                "record {} as held(user) with launch flags {:?}",
                l.id, l.launch_args
            ),
            format!(
                "send SIGTERM to agent pid {} (it runs outside Overseer)",
                l.pid.unwrap_or(0)
            ),
            "wait up to 10 s for the agent to exit (abort and revert on timeout)".into(),
        ];
    };
    vec![
        format!(
            "record {} as held(user) with launch flags {:?}",
            l.id, l.launch_args
        ),
        format!("set tmux pane option @sb_held={} on {pane}", l.id),
        format!("send SIGTERM to agent pid {}", l.pid.unwrap_or(0)),
        "wait up to 10 s for the agent to exit (abort and revert on timeout)".into(),
        format!(
            "start the held stub in {pane}: {}",
            stub_command(ctx, &l.id)
        ),
    ]
}

fn timeout() -> Duration {
    Duration::from_millis(
        std::env::var("OVERSEER_HOLD_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000),
    )
}

fn wait_exit(ctx: &Ctx, pid: u32, start: &Option<String>) -> bool {
    let deadline = std::time::Instant::now() + timeout();
    loop {
        match ctx.probe.info(pid) {
            None => return true,
            Some(i) if i.start != *start => return true,
            _ => {}
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn terminate(pid: u32, _t: &crate::term::Term) -> bool {
    unsafe { libc::kill(pid as i32, libc::SIGTERM) == 0 }
}

#[cfg(not(unix))]
fn terminate(_pid: u32, t: &std::sync::Arc<crate::term::Term>) -> bool {
    t.kill().is_ok()
}

fn record_held(ctx: &Ctx, reg: &Registry, l: &Line, pid: u32) -> Result<(), String> {
    reg.set_mark(&l.id, "held_by_user", Some(&pid.to_string()))?;
    reg.set_mark(&l.id, "state", Some(crate::wire::STATE_HELD))?;
    reg.set_mark(&l.id, "held_at", Some(&format_rfc3339(ctx.now_ms)))?;
    reg.set_mark(&l.id, "hold_pending", None)
}

fn revert_held(reg: &Registry, l: &Line) {
    let _ = reg.set_mark(&l.id, "held_by_user", None);
    let _ = reg.set_mark(&l.id, "held_at", l.held_at.as_deref());
    let _ = reg.set_mark(&l.id, "state", Some(l.state));
}

type Stamp = (Option<String>, Option<i64>);

fn stamp(probe: &dyn crate::procs::Probe, pid: u32) -> Option<Stamp> {
    probe.info(pid).map(|i| (i.start, i.start_ms))
}

/// Same process as when the stamp was taken. macOS has no `start`, so `start_ms` counts too;
/// a stamp with neither can never be verified.
#[cfg(unix)]
fn same_process(now: &Stamp, then: &Stamp) -> bool {
    match (&now.0, &then.0, now.1, then.1) {
        (None, None, None, None) => false,
        (a, b, x, y) => a == b && x.zip(y).is_none_or(|(x, y)| (x - y).abs() <= 2000),
    }
}

/// Every process below `pid`, with its start stamp so a reused pid is never signalled.
/// Collected before the agent exits: its children are re-parented to init afterwards.
fn descendants(ctx: &Ctx, pid: u32) -> Vec<(u32, Option<Stamp>)> {
    let mut seen = std::collections::HashSet::from([pid]);
    let mut out = vec![];
    let mut todo = vec![pid];
    while let Some(p) = todo.pop() {
        for (c, _) in ctx.probe.children(p) {
            if out.len() < 4096 && seen.insert(c) {
                out.push((c, stamp(ctx.probe, c)));
                todo.push(c);
            }
        }
    }
    out
}

#[cfg(unix)]
fn alive_same(probe: &dyn crate::procs::Probe, pid: u32, then: &Option<Stamp>) -> bool {
    match (stamp(probe, pid), then) {
        (Some(now), Some(then)) => same_process(&now, then),
        _ => false,
    }
}

#[cfg(unix)]
fn signal_tree(
    probe: &dyn crate::procs::Probe,
    tree: &[(u32, Option<Stamp>)],
    sig: i32,
    send: &dyn Fn(u32, i32),
) -> usize {
    let mut live = 0;
    for (p, then) in tree {
        if alive_same(probe, *p, then) {
            live += 1;
            send(*p, sig);
        }
    }
    live
}

#[cfg(unix)]
fn send_signal(pid: u32, sig: i32) {
    unsafe { libc::kill(pid as i32, sig) };
}

#[cfg(unix)]
fn stop_tree(ctx: &Ctx, tree: &[(u32, Option<Stamp>)]) -> Vec<String> {
    let signalled = signal_tree(ctx.probe, tree, libc::SIGTERM, &send_signal);
    if signalled == 0 {
        return vec![];
    }
    let deadline = std::time::Instant::now() + timeout();
    let gone = |ctx: &Ctx| tree.iter().all(|(p, s)| !alive_same(ctx.probe, *p, s));
    while !gone(ctx) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let killed = if gone(ctx) {
        0
    } else {
        signal_tree(ctx.probe, tree, libc::SIGKILL, &send_signal)
    };
    let mut done = vec![format!(
        "stopped {signalled} process(es) the agent left running"
    )];
    if killed > 0 {
        done.push(format!("{killed} ignored SIGTERM and got SIGKILL"));
    }
    done
}

#[cfg(not(unix))]
fn stop_tree(_: &Ctx, _: &[(u32, Option<Stamp>)]) -> Vec<String> {
    vec![]
}

fn execute_hold_term(
    ctx: &Ctx,
    reg: &Registry,
    l: &Line,
    t: &std::sync::Arc<crate::term::Term>,
    stub: bool,
    shell: bool,
) -> Result<Vec<String>, String> {
    let pid = l.pid.ok_or("no pid")?;
    let start = ctx.probe.info(pid).and_then(|i| i.start);
    let id = t.info().id;
    let root = t.info().pid == pid || (stub && is_stub_wrapper(&t.info().cmd));
    let mut done = vec![];
    let tree = descendants(ctx, pid);
    record_held(ctx, reg, l, pid)?;
    done.push(format!("recorded {} as held(user)", l.id));
    t.hold(root);
    if !terminate(pid, t) || !wait_exit(ctx, pid, &start) {
        t.hold(false);
        revert_held(reg, l);
        return Err(format!(
            "agent pid {pid} did not exit; reverted the registry"
        ));
    }
    done.push(format!("agent pid {pid} exited after SIGTERM"));
    done.extend(stop_tree(ctx, &tree));
    let shell = shell && cfg!(unix) && t.info().task_id.is_none();
    match (root, stub) {
        (true, true) => {
            t.respawn(vec![
                "/bin/sh".into(),
                "-c".into(),
                stub_command(ctx, &l.id),
            ])?;
            done.push(format!("started the held stub in term {id}"));
        }
        (true, false) if shell => {
            t.to_shell("closed")?;
            done.push(format!("left a shell in term {id}"));
        }
        (true, false) => {
            t.finish(None);
            done.push(format!("closed term {id}"));
        }
        (false, true) => done.push(format!("term {id} runs a shell; stub not started")),
        (false, false) if shell && is_stub_wrapper(&t.info().cmd) => {
            t.to_shell("closed")?;
            done.push(format!("left a shell in term {id}"));
        }
        (false, false) if shell => {
            t.detach();
            done.push(format!("left the shell of term {id}"));
        }
        (false, false) => {
            t.kill()?;
            done.push(format!("closed term {id}"));
        }
    }
    Ok(done)
}

#[cfg(unix)]
fn sigterm(pid: u32) -> Result<bool, String> {
    if unsafe { libc::kill(pid as i32, libc::SIGTERM) } == 0 {
        return Ok(true);
    }
    let e = std::io::Error::last_os_error();
    if e.raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(format!("cannot stop agent pid {pid}: {e}"))
    }
}

#[cfg(unix)]
fn execute_hold_external(ctx: &Ctx, reg: &Registry, l: &Line) -> Result<Vec<String>, String> {
    let pid = l.pid.ok_or("no pid")?;
    let then = stamp(ctx.probe, pid);
    let tree = descendants(ctx, pid);
    record_held(ctx, reg, l, pid)?;
    let mut done = vec![format!("recorded {} as held(user)", l.id)];
    if !alive_same(ctx.probe, pid, &then) {
        done.push(format!("agent pid {pid} was already gone"));
        return Ok(done);
    }
    match sigterm(pid) {
        Ok(true) => {}
        Ok(false) => {
            done.push(format!("agent pid {pid} was already gone"));
            return Ok(done);
        }
        Err(e) => {
            revert_held(reg, l);
            return Err(e);
        }
    }
    let start = then.and_then(|t| t.0);
    if !wait_exit(ctx, pid, &start) {
        revert_held(reg, l);
        return Err(format!(
            "agent pid {pid} did not exit; reverted the registry"
        ));
    }
    done.push(format!("agent pid {pid} exited after SIGTERM"));
    done.extend(stop_tree(ctx, &tree));
    Ok(done)
}

#[cfg(not(unix))]
fn execute_hold_external(_: &Ctx, _: &Registry, _: &Line) -> Result<Vec<String>, String> {
    Err("closing a session that runs outside Overseer is not supported on this platform".into())
}

fn execute_hold(
    ctx: &Ctx,
    reg: &Registry,
    l: &Line,
    stub: bool,
    shell: bool,
) -> Result<Vec<String>, String> {
    if l.pane.is_none() {
        if let Some(t) = crate::deliver::line_term(l) {
            return execute_hold_term(ctx, reg, l, &t, stub, shell);
        }
        return execute_hold_external(ctx, reg, l);
    }
    let tmux = ctx.tmux.as_ref().ok_or("tmux unavailable")?;
    let pane = l.pane.as_deref().ok_or("no pane")?;
    let pid = l.pid.ok_or("no pid")?;
    let start = ctx.probe.info(pid).and_then(|i| i.start);
    let tree = descendants(ctx, pid);
    let mut done = vec![];
    record_held(ctx, reg, l, pid)?;
    done.push(format!("recorded {} as held(user)", l.id));
    tmux.set_pane_option(pane, "@sb_held", Some(&l.id))?;
    done.push(format!("set @sb_held on {pane}"));
    let prev_remain = tmux.pane_option(pane, "remain-on-exit");
    tmux.set_pane_option(pane, "remain-on-exit", Some("on"))?;
    let restore_remain = || tmux.set_pane_option(pane, "remain-on-exit", prev_remain.as_deref());
    let killed = crate::exec::output_within(
        std::process::Command::new("kill").args(["-TERM", &pid.to_string()]),
        crate::exec::PROBE_TIMEOUT,
    )
    .is_ok_and(|o| o.status.success());
    if !killed || !wait_exit(ctx, pid, &start) {
        revert_held(reg, l);
        let _ = tmux.set_pane_option(pane, "@sb_held", None);
        let _ = restore_remain();
        return Err(format!(
            "agent pid {pid} did not exit; reverted the registry and the pane option"
        ));
    }
    done.push(format!("agent pid {pid} exited after SIGTERM"));
    done.extend(stop_tree(ctx, &tree));
    let now = tmux.panes().into_iter().find(|p| p.id == pane);
    let stub = stub_command(ctx, &l.id);
    let cwd = if l.cwd_missing { "/" } else { l.cwd.as_str() };
    let mut args = vec!["respawn-pane", "-t", pane, "-c", cwd];
    let env_state = format!("OVERSEER_STATE_DIR={}", ctx.state_dir.display());
    let env_sock = tmux
        .socket
        .as_ref()
        .map(|s| format!("OVERSEER_TMUX_SOCKET={s}"));
    args.extend(["-e", env_state.as_str()]);
    if let Some(e) = &env_sock {
        args.extend(["-e", e.as_str()]);
    }
    match now {
        Some(p) if p.dead => {
            args.push(&stub);
            tmux.ok(&args)?;
            done.push(format!("started the held stub in {pane}"));
        }
        Some(p) if ctx.probe.children(p.pid).is_empty() => {
            args.insert(1, "-k");
            args.push(&stub);
            tmux.ok(&args)?;
            done.push(format!(
                "replaced the idle shell in {pane} with the held stub"
            ));
        }
        Some(_) => done.push(format!("{pane} runs another program; stub not started")),
        None => done.push(format!("{pane} is gone; stub not started")),
    }
    let _ = restore_remain();
    Ok(done)
}

pub fn hold(ctx: &Ctx, q: &str, apply: bool) -> Result<Report, String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    Ok(hold_line(ctx, l, apply, true, false, false))
}

pub(crate) fn hold_line(
    ctx: &Ctx,
    l: &Line,
    apply: bool,
    stub: bool,
    shell: bool,
    force: bool,
) -> Report {
    hold_line_with(ctx, l, apply, stub, shell, force, false)
}

fn is_external(l: &Line) -> bool {
    l.pane.is_none() && crate::deliver::line_term(l).is_none()
}

fn hold_line_with(
    ctx: &Ctx,
    l: &Line,
    apply: bool,
    stub: bool,
    shell: bool,
    force: bool,
    closing: bool,
) -> Report {
    let mut checks = if force {
        vec![]
    } else {
        evaluate(&hold_input(ctx, l))
    };
    if closing && is_external(l) {
        checks.retain(|c| !["inTmux", "inputEmpty"].contains(&c.name));
    }
    let mut r = Report {
        line_id: l.id.clone(),
        steps: hold_steps(ctx, l, stub),
        checks,
        action: "none",
        error: None,
        pending: false,
    };
    if r.ok() {
        r.action = "would-hold";
        if apply {
            match Registry::open(&ctx.state_dir)
                .and_then(|reg| execute_hold(ctx, &reg, l, stub, shell))
            {
                Ok(done) => {
                    r.action = "held";
                    r.steps = done;
                }
                Err(e) => {
                    r.action = "none";
                    r.error = Some(e);
                }
            }
        }
    }
    log_action(
        ctx,
        &json!({"action": "hold", "dryRun": !apply, "result": r.to_json()}),
    );
    r
}

/// Predicates that fail only while the agent is mid-turn and are re-checked when it ends.
const TURN_CHECKS: [&str; 4] = ["atPrompt", "inputEmpty", "noToolRunning", "notNeedsInput"];

pub fn hold_after_turn(ctx: &Ctx, q: &str, apply: bool) -> Result<Report, String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    let checks = evaluate(&hold_input(ctx, l));
    if checks.iter().all(|c| c.ok)
        || !l.busy_known
        || !checks.iter().all(|c| c.ok || TURN_CHECKS.contains(&c.name))
    {
        return Ok(hold_line(ctx, l, apply, true, false, false));
    }
    let mut r = Report {
        line_id: l.id.clone(),
        steps: vec![
            format!("record a pending hold on {}", l.id),
            "hold it when the turn ends and the agent is not waiting for you".into(),
        ],
        checks,
        action: "would-hold-pending",
        error: None,
        pending: true,
    };
    if apply {
        match Registry::open(&ctx.state_dir)
            .and_then(|reg| reg.set_mark(&l.id, "hold_pending", Some("1")))
        {
            Ok(()) => {
                forget_pending_clocks(&l.id);
                r.action = "hold-pending";
            }
            Err(e) => {
                r.action = "none";
                r.pending = false;
                r.error = Some(e);
            }
        }
    }
    log_action(
        ctx,
        &json!({"action": "hold", "dryRun": !apply, "result": r.to_json()}),
    );
    Ok(r)
}

/// Cancels a pending hold. 409 when it was already claimed and is being applied right now.
pub fn cancel_pending_hold(ctx: &Ctx, q: &str) -> Result<String, (u16, String)> {
    let lines = scan(ctx).map_err(|e| (500, e))?;
    let l = find_line(&lines, q).map_err(|e| (404, e))?;
    let claimed = Registry::open(&ctx.state_dir)
        .and_then(|r| r.claim_hold_pending(&l.id))
        .map_err(|e| (500, e))?;
    forget_pending_clocks(&l.id);
    if !claimed
        && INFLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&l.id)
    {
        return Err((409, "the hold is already being applied".into()));
    }
    Ok(l.id.clone())
}

#[derive(Debug, PartialEq)]
pub(crate) enum Pending {
    /// Mid-turn, waiting for the user, or only input-bound checks fail: stay queued.
    Wait,
    Run,
    /// A failure that outlasts the turn (tool, pinned, unknown turn state...).
    Blocked(Vec<Check>),
}

pub(crate) fn pending_verdict(l: &Line, checks: &[Check]) -> Pending {
    if l.busy_known && l.busy || l.needs_input {
        return Pending::Wait;
    }
    // Subagents outlive the parent's turn: their end is the real end of the work.
    if l.hook.as_ref().is_some_and(|h| h.subagents > 0) {
        return Pending::Wait;
    }
    let failed: Vec<Check> = checks.iter().filter(|c| !c.ok).cloned().collect();
    if failed.is_empty() {
        Pending::Run
    } else if l.busy_known
        && failed
            .iter()
            .all(|c| ["atPrompt", "inputEmpty"].contains(&c.name))
    {
        Pending::Wait
    } else {
        Pending::Blocked(failed)
    }
}

/// A non-transient failure must last this long before the pending hold is dropped, so a tool
/// process still exiting right after the turn ended does not cancel it.
const BLOCK_GRACE_MS: i64 = 10_000;

fn parse_grace_or(v: Option<&str>, default: i64) -> i64 {
    v.and_then(|v| v.parse().ok())
        .filter(|ms| *ms >= 0)
        .unwrap_or(default)
}

fn parse_grace(v: Option<&str>) -> i64 {
    parse_grace_or(v, BLOCK_GRACE_MS)
}

fn grace_ms() -> i64 {
    parse_grace(std::env::var("OVERSEER_HOLD_GRACE_MS").ok().as_deref())
}

static BLOCKED: std::sync::Mutex<Option<std::collections::HashMap<String, i64>>> =
    std::sync::Mutex::new(None);

const SUBAGENT_WAIT_MS: i64 = 1_800_000;

fn subagent_wait_ms() -> i64 {
    parse_grace_or(
        std::env::var("OVERSEER_HOLD_SUBAGENT_MS").ok().as_deref(),
        SUBAGENT_WAIT_MS,
    )
}

fn elapsed_over(key: String, now_ms: Option<i64>, limit: i64) -> bool {
    let mut g = BLOCKED.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(Default::default);
    match now_ms {
        Some(now) => now - *map.entry(key).or_insert(now) >= limit,
        None => {
            map.remove(&key);
            false
        }
    }
}

fn blocked_long_enough(id: &str, now_ms: Option<i64>) -> bool {
    elapsed_over(id.to_string(), now_ms, grace_ms())
}

fn subagents_waited_too_long(id: &str, now_ms: Option<i64>) -> bool {
    elapsed_over(format!("sub:{id}"), now_ms, subagent_wait_ms())
}

fn forget_pending_clocks(id: &str) {
    blocked_long_enough(id, None);
    subagents_waited_too_long(id, None);
}

static INFLIGHT: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

struct Inflight(String);

impl Inflight {
    fn begin(id: &str) -> Option<Inflight> {
        let fresh = INFLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string());
        fresh.then(|| Inflight(id.to_string()))
    }
}

impl Drop for Inflight {
    fn drop(&mut self) {
        INFLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
    }
}

fn checks_json(checks: &[Check]) -> Value {
    Value::Array(
        checks
            .iter()
            .filter(|c| !c.ok)
            .map(|c| json!({"name": c.name, "detail": c.detail}))
            .collect(),
    )
}

fn refuse_pending(ctx: &Ctx, l: &Line, failed: Value, error: Option<&str>) {
    let rec = json!({"lineId": l.id, "failed": failed, "error": error});
    log_action(
        ctx,
        &json!({"action": "hold-pending-refused", "result": rec.clone()}),
    );
    crate::board::publish("line.hold_refused", rec);
}

/// Applies the pending holds whose turn ended, each on its own worker so a slow hold never
/// stalls the scan loop; the others stay queued, or are dropped when something lasting blocks them.
pub fn apply_pending_holds(ctx: &Ctx, lines: &[Line]) {
    for l in lines.iter().filter(|l| l.hold_pending && l.alive()) {
        let turn_active = l.busy_known && l.busy || l.needs_input;
        let on_subagents = !turn_active && l.hook.as_ref().is_some_and(|h| h.subagents > 0);
        if !on_subagents {
            subagents_waited_too_long(&l.id, None);
        }
        let checks = if turn_active || on_subagents {
            vec![]
        } else {
            evaluate(&hold_input(ctx, l))
        };
        match pending_verdict(l, &checks) {
            Pending::Wait => {
                blocked_long_enough(&l.id, None);
                if on_subagents
                    && subagents_waited_too_long(&l.id, Some(ctx.now_ms))
                    && Registry::open(&ctx.state_dir).and_then(|r| r.claim_hold_pending(&l.id))
                        == Ok(true)
                {
                    forget_pending_clocks(&l.id);
                    let failed =
                        json!([{"name": "subagents", "detail": "subagents never finished"}]);
                    refuse_pending(ctx, l, failed, None);
                }
            }
            Pending::Blocked(failed) => {
                if blocked_long_enough(&l.id, Some(ctx.now_ms))
                    && Registry::open(&ctx.state_dir).and_then(|r| r.claim_hold_pending(&l.id))
                        == Ok(true)
                {
                    forget_pending_clocks(&l.id);
                    refuse_pending(ctx, l, checks_json(&failed), None);
                }
            }
            Pending::Run => {
                forget_pending_clocks(&l.id);
                let Some(guard) = Inflight::begin(&l.id) else {
                    continue;
                };
                let line = l.clone();
                let spawned = std::thread::Builder::new()
                    .name("hold-pending".into())
                    .spawn(move || {
                        let _guard = guard;
                        let done = std::panic::catch_unwind(|| run_pending(&line));
                        if let Err(p) = done {
                            crate::dlog::line(
                                "error",
                                &format!(
                                    "overseer daemon: pending hold panicked ({})",
                                    crate::util::panic_text(p.as_ref())
                                ),
                            );
                        }
                    });
                if let Err(e) = spawned {
                    crate::dlog::line("error", &format!("overseer daemon: pending hold: {e}"));
                }
            }
        }
    }
}

fn run_pending(l: &Line) {
    let probe = crate::procs::System;
    let ctx = Ctx::from_env(&probe);
    // DELETE or a manual hold may have cleared it since the scan: only the claimer holds.
    let claimed = Registry::open(&ctx.state_dir).and_then(|r| r.claim_hold_pending(&l.id));
    if claimed != Ok(true) {
        return;
    }
    let r = hold_line(&ctx, l, true, true, false, false);
    if r.action != "held" {
        refuse_pending(&ctx, l, checks_json(&r.checks), r.error.as_deref());
    }
}

pub fn close(ctx: &Ctx, q: &str, apply: bool, shell: bool, force: bool) -> Result<Report, String> {
    let lines = scan(ctx)?;
    close_line(ctx, find_line(&lines, q)?, apply, shell, force)
}

pub(crate) fn close_line(
    ctx: &Ctx,
    l: &Line,
    apply: bool,
    shell: bool,
    force: bool,
) -> Result<Report, String> {
    let mut r = if l.alive() {
        let mut h = hold_line_with(ctx, l, apply, false, shell, force, true);
        if h.ok() {
            h.steps.push(format!("mark {} hung-up", l.id));
        }
        h
    } else {
        Report {
            line_id: l.id.clone(),
            checks: vec![],
            action: if apply { "held" } else { "would-hold" },
            steps: vec![format!("mark {} hung-up (no live process)", l.id)],
            error: None,
            pending: false,
        }
    };
    if r.ok() {
        if apply {
            let reg = Registry::open(&ctx.state_dir)?;
            reg.set_mark(&l.id, "closed_at", Some(&format_rfc3339(ctx.now_ms)))?;
            reg.set_mark(&l.id, "state", Some(crate::wire::STATE_HUNG_UP))?;
            for t in pending_terms(l) {
                if t.kill().is_ok() {
                    r.steps.push(format!("closed held term {}", t.info().id));
                }
            }
            if let (Some(t), Some(p)) = (
                ctx.tmux.as_ref(),
                l.held_pane.as_deref().or(l.pane.as_deref()),
            ) {
                let _ = t.set_pane_option(p, "@sb_held", None);
                r.steps.push(format!("cleared @sb_held on {p}"));
            }
            r.action = "closed";
        } else {
            r.action = "would-close";
        }
    }
    log_action(
        ctx,
        &json!({"action": "close", "dryRun": !apply, "result": r.to_json()}),
    );
    Ok(r)
}

// Terms of a line with no agent pid yet: its held stub, or an agent still starting, such as
// a resumed Claude waiting on its folder trust prompt before any hook reports the session.
pub fn pending_terms(l: &Line) -> Vec<std::sync::Arc<crate::term::Term>> {
    if l.alive() {
        return vec![];
    }
    crate::term::all()
        .into_iter()
        .filter(|t| {
            let i = t.info();
            !i.exited
                && i.line_id.as_deref() == Some(l.id.as_str())
                && (is_stub_wrapper(&i.cmd)
                    || i.agent
                        .as_deref()
                        .is_some_and(|a| !crate::agents::is_shell(a)))
        })
        .collect()
}

// Only the stub of a line held by the user waits for Enter; once pressed, the agent runs in
// the same term and an Enter would answer its prompt instead.
pub fn waits_for_enter(l: &Line, t: &crate::term::Term) -> bool {
    l.held_reason == Some("user") && is_stub_wrapper(&t.info().cmd)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    Terminal,
    Detached,
    Attach,
}

pub struct ResumePlan {
    pub line_id: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub r#where: &'static str,
    pub pane: Option<String>,
    pub session: Option<String>,
}

impl ResumePlan {
    pub fn to_json(&self) -> Value {
        json!({"lineId": self.line_id, "argv": self.argv, "cwd": self.cwd, "where": self.r#where, "pane": self.pane, "session": self.session})
    }

    pub fn shell(&self) -> String {
        let argv: Vec<String> = self.argv.iter().map(|a| shell_quote(a)).collect();
        format!("cd {} && {}", shell_quote(&self.cwd), argv.join(" "))
    }
}

// Two processes resumed on one conversation both write its transcript.
fn no_second_writer(lines: &[Line], l: &Line) -> Result<(), String> {
    match lines.iter().find_map(|o| {
        let pid = o.pid?;
        (o.id != l.id && o.agent == l.agent && o.session_id == l.session_id).then_some((o, pid))
    }) {
        Some((o, pid)) => Err(format!(
            "conversation {} is already running in line {} (pid {pid})",
            l.session_id, o.id
        )),
        None => Ok(()),
    }
}

pub fn resume_plan(
    ctx: &Ctx,
    lines: &[Line],
    l: &Line,
    launch: Launch,
) -> Result<ResumePlan, String> {
    let mut plan = ResumePlan {
        line_id: l.id.clone(),
        argv: crate::resume_argv(l.agent, &l.session_id, &l.launch_args),
        cwd: l.cwd.clone(),
        r#where: "here",
        pane: None,
        session: None,
    };
    if let Some(pid) = l.pid {
        if let (Launch::Attach, Some(p)) = (launch, &l.pane) {
            plan.r#where = "running";
            plan.pane = Some(p.clone());
            return Ok(plan);
        }
        let at = l
            .pane
            .as_deref()
            .map(|p| format!(", tmux pane {p}"))
            .unwrap_or_default();
        return Err(format!("line is already running (pid {pid}{at})"));
    }
    no_second_writer(lines, l)?;
    if l.cwd_missing {
        return Err(format!(
            "working directory {} does not exist on this station",
            l.cwd
        ));
    }
    let in_tmux = std::env::var_os("TMUX").is_some_and(|v| !v.is_empty());
    let sock = ctx.tmux.as_ref().is_some_and(|t| t.socket.is_some());
    (plan.r#where, plan.pane) = match &l.held_pane {
        Some(p) if ctx.tmux.is_some() => ("tmux-pane", Some(p.clone())),
        _ if in_tmux && launch == Launch::Terminal => ("tmux-window", None),
        _ if sock || launch != Launch::Terminal => ("tmux-session", None),
        _ => ("here", None),
    };
    Ok(plan)
}

pub fn resume(ctx: &Ctx, q: &str, dry_run: bool, launch: Launch) -> Result<ResumePlan, String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    let mut plan = resume_plan(ctx, &lines, l, launch)?;
    let tmux = ctx.tmux.clone().unwrap_or_default();
    if dry_run {
        return Ok(plan);
    }
    if plan.r#where == "running" {
        plan.session = plan.pane.as_deref().and_then(|p| tmux.session_of(p));
        return attach(&tmux, plan, launch);
    }
    let reg = Registry::open(&ctx.state_dir)?;
    reg.set_mark(&l.id, "held_by_user", None)?;
    reg.set_mark(&l.id, "closed_at", None)?;
    log_action(
        ctx,
        &json!({"action": "resume", "dryRun": false, "result": plan.to_json()}),
    );
    let argv: Vec<String> = plan.argv.iter().map(|a| shell_quote(a)).collect();
    let cmd = format!("{}; {}", argv.join(" "), crate::util::SHELL_FALLBACK);
    match plan.r#where {
        "tmux-pane" => {
            let pane = plan.pane.clone().unwrap_or_default();
            tmux.ok(&["respawn-pane", "-k", "-t", &pane, "-c", &plan.cwd, &cmd])?;
            tmux.set_pane_option(&pane, "@sb_held", None)?;
            plan.session = tmux.session_of(&pane);
        }
        "tmux-window" => {
            let name = l.ticket.clone().unwrap_or_else(|| l.agent.to_string());
            tmux.ok(&["new-window", "-n", &name, "-c", &plan.cwd, &cmd])?;
        }
        "tmux-session" => {
            let name = format!("sb-{}", &l.id[l.id.len().saturating_sub(6)..]);
            tmux.ok(&["new-session", "-d", "-s", &name, "-c", &plan.cwd, &cmd])?;
            plan.session = Some(name);
        }
        _ => {
            let err = std::process::Command::new(&plan.argv[0])
                .args(&plan.argv[1..])
                .current_dir(&plan.cwd)
                .exec();
            return Err(format!("exec {}: {err}", plan.argv[0]));
        }
    }
    attach(&tmux, plan, launch)
}

fn attach(
    tmux: &crate::tmux::Tmux,
    plan: ResumePlan,
    launch: Launch,
) -> Result<ResumePlan, String> {
    use std::io::IsTerminal;
    if launch != Launch::Attach || !std::io::stdin().is_terminal() {
        return Ok(plan);
    }
    let session = plan
        .session
        .as_deref()
        .ok_or("cannot find the tmux session")?;
    if let Some(p) = &plan.pane {
        let _ = tmux.ok(&["select-window", "-t", p]);
        let _ = tmux.ok(&["select-pane", "-t", p]);
    }
    Err(format!("tmux attach: {}", tmux.attach(session)))
}

pub fn link(ctx: &Ctx, q: &str, key: &str) -> Result<(String, Option<String>), String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    let t = match key {
        "none" | "-" => None,
        k => Some(ticket_exact(k)?),
    };
    Registry::open(&ctx.state_dir)?.set_mark(&l.id, "ticket_override", t.as_deref())?;
    Ok((l.id.clone(), t))
}

pub fn set_env(ctx: &Ctx, q: &str, env: &str) -> Result<(String, Option<String>), String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    let e = match env {
        "none" | "-" => None,
        e if !e.is_empty() && e.len() <= 128 && !e.contains(char::is_whitespace) => Some(e),
        e => return Err(format!("'{e}' is not an env ref")),
    };
    Registry::open(&ctx.state_dir)?.set_mark(&l.id, "env_ref", e)?;
    Ok((l.id.clone(), e.map(str::to_string)))
}

pub fn pin(ctx: &Ctx, q: &str, on: bool) -> Result<String, String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    Registry::open(&ctx.state_dir)?.set_mark(&l.id, "pinned", on.then_some("1"))?;
    Ok(l.id.clone())
}

pub const TITLE_MAX: usize = 80;

pub fn clean_title(raw: Option<&str>) -> Option<String> {
    let one: String = raw?
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let t: String = one.trim().chars().take(TITLE_MAX).collect();
    Some(t.trim_end().to_string()).filter(|t| !t.is_empty())
}

pub fn set_title(ctx: &Ctx, q: &str, title: Option<&str>) -> Result<Line, (u16, String)> {
    let lines = scan(ctx).map_err(|e| (500, e))?;
    let mut l = find_line(&lines, q).map_err(|e| (404, e))?.clone();
    Registry::open(&ctx.state_dir)
        .and_then(|r| r.set_mark(&l.id, "title_override", title))
        .map_err(|e| (500, e))?;
    l.title_override = title.map(str::to_string);
    Ok(l)
}

pub fn notes<T>(
    ctx: &Ctx,
    q: &str,
    f: impl FnOnce(&mut Vec<serde_json::Value>) -> Result<T, crate::notes::Err>,
) -> Result<(Line, T, Vec<serde_json::Value>), crate::notes::Err> {
    let lines = scan(ctx).map_err(|e| (500, e))?;
    let mut l = find_line(&lines, q).map_err(|e| (404, e))?.clone();
    let mut reg = Registry::open(&ctx.state_dir).map_err(|e| (500, e))?;
    let (out, notes) = crate::notes::update(&mut reg, &l.id, f)?;
    l.notes = notes.clone();
    Ok((l, out, notes))
}

pub fn stub(ctx: &Ctx, q: &str) -> Result<(), String> {
    let lines = scan(ctx)?;
    let l = find_line(&lines, q)?;
    let label = l.ticket.clone().unwrap_or_else(|| {
        if l.branch.is_empty() {
            l.agent.to_string()
        } else {
            l.branch.clone()
        }
    });
    println!(
        "\n  held — {label} — {}\n  {}\n\n  press Enter to resume (Ctrl-C for a shell)",
        l.agent, l.cwd
    );
    let mut s = String::new();
    if std::io::stdin()
        .read_line(&mut s)
        .map_err(|e| e.to_string())?
        == 0
    {
        return Ok(());
    }
    let id = l.id.clone();
    let lines = scan(ctx)?;
    let l = find_line(&lines, &id)?;
    let plan = resume_plan(ctx, &lines, l, Launch::Terminal)?;
    let reg = Registry::open(&ctx.state_dir)?;
    reg.set_mark(&l.id, "held_by_user", None)?;
    reg.set_mark(&l.id, "closed_at", None)?;
    if let (Some(t), Ok(p)) = (ctx.tmux.as_ref(), std::env::var("TMUX_PANE")) {
        let _ = t.set_pane_option(&p, "@sb_held", None);
    }
    log_action(
        ctx,
        &json!({"action": "resume", "dryRun": false, "result": plan.to_json(), "via": "stub"}),
    );
    let err = std::process::Command::new(&plan.argv[0])
        .args(&plan.argv[1..])
        .current_dir(&plan.cwd)
        .exec();
    Err(format!("exec {}: {err}", plan.argv[0]))
}

pub fn status(ctx: &Ctx, lines: &[Line]) -> Value {
    let count = |s: &str| lines.iter().filter(|l| l.state == s).count();
    let rss: u64 = lines.iter().filter_map(|l| l.rss).sum();
    let mut v = json!({
        "stationId": ctx.config.station_id,
        "version": env!("CARGO_PKG_VERSION"),
        "apiConnected": false,
        "outboxPending": 0,
        "activeCount": count(crate::wire::STATE_ACTIVE),
        "idleCount": count(crate::wire::STATE_IDLE),
        "heldCount": count(crate::wire::STATE_HELD),
        "aliveCount": lines.iter().filter(|l| l.alive()).count(),
        "orphans": lines.iter().filter(|l| l.orphan).map(|l| l.id.clone()).collect::<Vec<_>>(),
        "duplicates": lines.iter().filter(|l| l.duplicate).map(|l| l.id.clone()).collect::<Vec<_>>(),
        "agentRssBytes": rss,
    });
    if let Some((total, avail)) = mem_bytes() {
        v["memTotalBytes"] = json!(total);
        if let Some(a) = avail {
            v["memUsedBytes"] = json!(total.saturating_sub(a));
        }
    }
    if let Some(threshold) = ctx.config.memory_policy {
        v["memoryPolicy"] = memory_policy(ctx, lines, &v, threshold);
    }
    v
}

pub fn memory_policy(ctx: &Ctx, lines: &[Line], st: &Value, threshold: f64) -> Value {
    let used = match (st["memUsedBytes"].as_u64(), st["memTotalBytes"].as_u64()) {
        (Some(u), Some(t)) if t > 0 => Some(u as f64 * 100.0 / t as f64),
        _ => None,
    };
    let over = used.is_some_and(|u| u > threshold);
    let mut idle: Vec<&Line> = lines
        .iter()
        .filter(|l| over && l.state == crate::wire::STATE_IDLE && l.alive())
        .collect();
    idle.sort_by_key(|l| l.last_activity_ms);
    let candidates: Vec<Value> = idle
        .iter()
        .map(|l| {
            let failed: Vec<String> = evaluate(&hold_input(ctx, l))
                .into_iter()
                .filter(|c| !c.ok)
                .map(|c| format!("{}: {}", c.name, c.detail))
                .collect();
            json!({"lineId": l.id, "holdable": failed.is_empty(), "failed": failed, "rssBytes": l.rss})
        })
        .collect();
    json!({
        "thresholdPercent": threshold,
        "usedPercent": used.map(|u| (u * 10.0).round() / 10.0),
        "over": over,
        "wouldHold": candidates,
    })
}

pub fn memory_line(v: &Value) -> Option<String> {
    let p = v.get("memoryPolicy")?;
    let Some(used) = p["usedPercent"].as_f64() else {
        return Some("memory policy: enabled, but used memory is unknown on this station".into());
    };
    let t = p["thresholdPercent"].as_f64().unwrap_or(0.0);
    if p["over"] != true {
        return Some(format!("memory policy: {used}% used, under {t}%"));
    }
    let ids: Vec<&str> = p["wouldHold"]
        .as_array()?
        .iter()
        .filter(|c| c["holdable"] == true)
        .filter_map(|c| c["lineId"].as_str())
        .collect();
    Some(format!(
        "memory policy: {used}% used, over {t}%; idle lines that would be held (report only, run overseer hold): {}",
        if ids.is_empty() { "none".into() } else { ids.join(", ") }
    ))
}

pub fn gib(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1_073_741_824.0)
}

pub fn status_line(v: &Value) -> String {
    let n = |k: &str| v[k].as_u64().unwrap_or(0);
    let mut s = format!(
        "station {}: {} active, {} idle, {} held in total; agents RSS {}",
        v["stationId"].as_str().unwrap_or("?"),
        n("activeCount"),
        n("idleCount"),
        n("heldCount"),
        gib(n("agentRssBytes"))
    );
    if let Some(t) = v["memTotalBytes"].as_u64() {
        match v["memUsedBytes"].as_u64() {
            Some(u) => s.push_str(&format!("; memory {} / {}", gib(u), gib(t))),
            None => s.push_str(&format!("; memory total {}", gib(t))),
        }
    }
    let o = v["orphans"].as_array().map_or(0, Vec::len);
    let d = v["duplicates"].as_array().map_or(0, Vec::len);
    if o > 0 {
        s.push_str(&format!("; {o} ORPHAN(S)"));
    }
    if d > 0 {
        s.push_str(&format!("; {d} DUPLICATE(S)"));
    }
    if let Some(m) = memory_line(v) {
        s.push_str(&format!("\n{m}"));
    }
    s
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn sigterm_of_a_pid_that_is_gone_is_not_an_error() {
        assert_eq!(super::sigterm(u32::MAX / 2), Ok(false));
    }

    use super::*;

    fn report(ok: bool, error: Option<&str>, pending: bool) -> Report {
        Report {
            line_id: "l".into(),
            checks: vec![crate::hold::Check {
                name: "idle",
                ok,
                detail: "busy".into(),
            }],
            action: "hold",
            steps: vec![],
            error: error.map(String::from),
            pending,
        }
    }

    #[test]
    fn a_report_maps_to_one_status_exit_code_and_command_outcome() {
        let done = report(true, None, false);
        assert_eq!((done.http_status(), done.exit_code()), (200, 0));
        assert_eq!(done.outcome(), ("succeeded", Some("hold".into())));
        let refused = report(false, None, false);
        assert_eq!((refused.http_status(), refused.exit_code()), (409, 3));
        assert_eq!(refused.outcome(), ("refused", Some("idle: busy".into())));
        let queued = report(false, None, true);
        assert_eq!((queued.http_status(), queued.exit_code()), (200, 0));
        let failed = report(true, Some("boom"), false);
        assert_eq!((failed.http_status(), failed.exit_code()), (409, 3));
        assert_eq!(failed.outcome(), ("failed", Some("boom".into())));
    }

    fn line(id: &str, sid: &str) -> Line {
        Line {
            id: id.into(),
            session_id: sid.into(),
            ..Default::default()
        }
    }

    struct Reuse(std::cell::Cell<i64>);
    impl crate::procs::Probe for Reuse {
        fn info(&self, pid: u32) -> Option<crate::procs::ProcInfo> {
            (pid == 7).then(|| crate::procs::ProcInfo {
                start: None,
                argv: vec![],
                start_ms: Some(self.0.get()),
            })
        }
        fn list(&self) -> Vec<(u32, Vec<String>)> {
            vec![]
        }
        fn cwd(&self, _: u32) -> Option<String> {
            None
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_reused_pid_is_not_signalled_even_without_a_start_tick_stamp() {
        let probe = Reuse(std::cell::Cell::new(1_000));
        let tree = vec![(7, stamp(&probe, 7))];
        let sent = std::cell::RefCell::new(vec![]);
        let send = |p: u32, s: i32| sent.borrow_mut().push((p, s));
        assert_eq!(signal_tree(&probe, &tree, 15, &send), 1);
        probe.0.set(900_000);
        assert_eq!(signal_tree(&probe, &tree, 15, &send), 0);
        assert_eq!(*sent.borrow(), vec![(7, 15)]);
        assert_eq!(signal_tree(&probe, &[(7, None)], 15, &send), 0, "no stamp");
    }

    fn failing(name: &'static str) -> Check {
        Check {
            name,
            ok: false,
            detail: "x".into(),
        }
    }

    fn pending_line() -> Line {
        Line {
            pid: Some(1),
            hold_pending: true,
            busy_known: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_pending_hold_waits_mid_turn_and_while_the_agent_asks_for_input() {
        let mut l = pending_line();
        l.busy = true;
        assert_eq!(pending_verdict(&l, &[failing("atPrompt")]), Pending::Wait);
        l.busy = false;
        l.needs_input = true;
        assert_eq!(
            pending_verdict(&l, &[failing("notNeedsInput")]),
            Pending::Wait
        );
    }

    #[test]
    fn a_pending_hold_waits_while_a_subagent_runs_after_the_turn() {
        let mut l = pending_line();
        l.hook = Some(crate::registry::HookState {
            subagents: 1,
            ..Default::default()
        });
        assert_eq!(
            pending_verdict(&l, &[failing("noToolRunning")]),
            Pending::Wait
        );
        l.hook.as_mut().unwrap().subagents = 0;
        assert_eq!(pending_verdict(&l, &[]), Pending::Run);
    }

    #[test]
    fn a_pending_hold_runs_once_the_turn_ended_and_every_check_passes() {
        assert_eq!(pending_verdict(&pending_line(), &[]), Pending::Run);
    }

    #[test]
    fn a_pending_hold_is_blocked_by_failures_that_outlast_the_turn() {
        let l = pending_line();
        let tool = [failing("noToolRunning")];
        assert!(matches!(pending_verdict(&l, &tool), Pending::Blocked(f) if f.len() == 1));
        let pinned = [failing("notPinned")];
        assert!(matches!(pending_verdict(&l, &pinned), Pending::Blocked(_)));
        assert_eq!(pending_verdict(&l, &[failing("inputEmpty")]), Pending::Wait);
        let mut unknown = pending_line();
        unknown.busy_known = false;
        assert!(matches!(
            pending_verdict(&unknown, &[failing("atPrompt")]),
            Pending::Blocked(_)
        ));
    }

    #[test]
    fn the_grace_period_can_be_overridden_by_a_valid_number_of_milliseconds() {
        assert_eq!(parse_grace(None), BLOCK_GRACE_MS);
        assert_eq!(parse_grace(Some("2000")), 2_000);
        assert_eq!(parse_grace(Some("-1")), BLOCK_GRACE_MS);
        assert_eq!(parse_grace(Some("soon")), BLOCK_GRACE_MS);
    }

    #[test]
    fn forgetting_a_pending_hold_restarts_the_grace_and_subagent_clocks() {
        blocked_long_enough("forget-1", Some(1_000));
        subagents_waited_too_long("forget-1", Some(1_000));
        forget_pending_clocks("forget-1");
        assert!(!blocked_long_enough(
            "forget-1",
            Some(1_000 + BLOCK_GRACE_MS)
        ));
        assert!(!subagents_waited_too_long(
            "forget-1",
            Some(1_000 + SUBAGENT_WAIT_MS)
        ));
        forget_pending_clocks("forget-1");
    }

    #[test]
    fn the_subagent_wait_is_bounded_and_independent_of_the_grace_clock() {
        assert!(!subagents_waited_too_long("subwait-1", Some(1_000)));
        blocked_long_enough("subwait-1", None);
        assert!(!subagents_waited_too_long(
            "subwait-1",
            Some(1_000 + SUBAGENT_WAIT_MS - 1)
        ));
        assert!(subagents_waited_too_long(
            "subwait-1",
            Some(1_000 + SUBAGENT_WAIT_MS)
        ));
        forget_pending_clocks("subwait-1");
        assert_eq!(parse_grace_or(Some("5"), SUBAGENT_WAIT_MS), 5);
        assert_eq!(parse_grace_or(None, SUBAGENT_WAIT_MS), SUBAGENT_WAIT_MS);
    }

    #[test]
    fn a_blocked_hold_is_only_dropped_after_the_grace_period() {
        assert!(!blocked_long_enough("grace-1", Some(1_000)));
        assert!(!blocked_long_enough(
            "grace-1",
            Some(1_000 + BLOCK_GRACE_MS - 1)
        ));
        assert!(blocked_long_enough("grace-1", Some(1_000 + BLOCK_GRACE_MS)));
        blocked_long_enough("grace-1", None);
        assert!(!blocked_long_enough("grace-1", Some(99_000)));
        blocked_long_enough("grace-1", None);
    }

    #[test]
    fn only_one_hold_per_line_is_in_flight() {
        let a = Inflight::begin("inflight-1").unwrap();
        assert!(Inflight::begin("inflight-1").is_none());
        assert!(Inflight::begin("inflight-2").is_some());
        drop(a);
        assert!(Inflight::begin("inflight-1").is_some());
    }

    #[test]
    fn custom_title_is_trimmed_capped_and_cleared_when_blank() {
        assert_eq!(
            clean_title(Some("  Payments  ")).as_deref(),
            Some("Payments")
        );
        assert_eq!(clean_title(Some("   ")), None);
        assert_eq!(clean_title(None), None);
        assert_eq!(clean_title(Some("a\nb\tc")).as_deref(), Some("a b c"));
        let long = "é".repeat(200);
        assert_eq!(clean_title(Some(&long)).unwrap().chars().count(), TITLE_MAX);
    }

    #[test]
    fn finds_by_id_session_or_prefix() {
        let ls = vec![line("vm-01JABCDEF", "aaaa-1"), line("vm-01JXYZ", "bbbb-2")];
        assert_eq!(find_line(&ls, "vm-01JXYZ").unwrap().session_id, "bbbb-2");
        assert_eq!(find_line(&ls, "aaaa").unwrap().id, "vm-01JABCDEF");
        assert_eq!(find_line(&ls, "01jab").unwrap().id, "vm-01JABCDEF");
        assert!(find_line(&ls, "01J").unwrap_err().contains("matches 2"));
        assert!(find_line(&ls, "zz").is_err());
    }

    #[test]
    fn a_stub_wrapper_command_is_recognised_whoever_the_agent_pid_is() {
        let wrapper = vec![
            "/bin/sh".into(),
            "-c".into(),
            "trap : INT; /usr/bin/overseer stub 'vm-1'; exec \"${SHELL:-/bin/sh}\"".into(),
        ];
        let agent = vec!["claude".into(), "--resume".into(), "S".into()];
        assert!(is_stub_wrapper(&wrapper));
        assert!(!is_stub_wrapper(&agent));
        assert!(!is_stub_wrapper(&[]));
    }

    #[test]
    fn resume_is_refused_while_another_process_runs_the_same_conversation() {
        let held = line("vm-held", "S");
        let mut other = line("vm-other", "S");
        other.pid = Some(42);
        let elsewhere = Line {
            agent: "codex",
            pid: Some(43),
            ..line("vm-codex", "S")
        };
        let err =
            no_second_writer(&[held.clone(), other.clone(), elsewhere.clone()], &held).unwrap_err();
        assert!(err.contains("vm-other") && err.contains("42"), "{err}");
        other.pid = None;
        assert!(no_second_writer(&[held.clone(), other, elsewhere], &held).is_ok());
    }
}
