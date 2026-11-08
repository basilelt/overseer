use overseer::actions::Launch;
use overseer::install;
use overseer::{
    actions, board, daemon, doctor, hooks, procs::System, remote, scan, util::ticket, Ctx, Line,
};
use serde_json::Value;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage:
  overseer ls [--all] [--json] [--local] [--owner EMAIL|all] [--ticket KEY-N] [--repo TEXT] [--state active|idle|held|hung-up]
  overseer status [--json]
  overseer board
  overseer resume <line> [--print] [--attach]
  overseer open overseer://resume/<line> [--yes]
  overseer patch <KEY-N> [--local] [--print]
  overseer link <line> <KEY-N|none>
  overseer link <line> --env <envRef|none>
  overseer pin <line> [--off]
  overseer hold <line> [--yes] [--json]
  overseer close <line> [--yes] [--json]
  overseer doctor
  overseer daemon [--interval SECONDS] [--once] [--http 127.0.0.1:PORT] [--detach [--replace]]
  overseer socket-path
  overseer version [--json]
  overseer mcp                                          (stdio MCP server for agents)
  overseer proxy                                        (stdio multiplexer for the remote app)
  overseer uninstall [--apply]                          (remove everything Overseer added; dry run by default)
  overseer hooks install|uninstall [AGENT...] [--apply]
  overseer hook <agent> <Event> [agent-pid]            (called by the agents' hooks)
  overseer statusline-tap install|uninstall [--apply]   (Claude plan limits from its status line)
  overseer stub <line>                                  (runs in a held pane)
  overseer titles <line>                                (used by patch over ssh)
  overseer enroll --api URL                             (station token on stdin)
  overseer sync [--once]
  overseer login --api URL                              (user token on stdin)";

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("overseer: {e}");
    ExitCode::FAILURE
}

fn age(now_ms: i64, ms: i64) -> String {
    let s = (now_ms - ms).max(0) / 1000;
    match s {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86400),
    }
}

fn table(lines: &[Line], now_ms: i64) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let short = |p: &str| match p.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => p.to_string(),
    };
    let mut rows = vec![[
        "ID", "STATION", "STATE", "AGENT", "TICKET", "REPO", "BRANCH", "WORKTREE", "LAST", "RESUME",
    ]
    .map(String::from)];
    for l in lines {
        let mut state = match l.held_reason {
            Some(r) => format!("{}({r})", l.state),
            None => l.state.to_string(),
        };
        for (on, flag) in [
            (l.needs_input, "!input"),
            (l.pinned, "pinned"),
            (l.orphan, "ORPHAN"),
            (l.duplicate, "DUP"),
            (l.offline, "OFFLINE"),
        ] {
            if on {
                state.push(' ');
                state.push_str(flag);
            }
        }
        let mut wt = short(&l.worktree_path);
        if l.cwd_missing {
            wt.push_str(" [missing]");
        } else if l.volatile {
            wt.push_str(" [tmp]");
        }
        rows.push([
            l.id.clone(),
            l.station_id.clone(),
            state,
            l.agent.to_string(),
            l.ticket.clone().unwrap_or_else(|| "-".into()),
            l.repo.clone(),
            if l.branch.is_empty() {
                "-".into()
            } else {
                l.branch.clone()
            },
            wt,
            age(now_ms, l.last_activity_ms),
            l.resume_command.clone(),
        ]);
    }
    let widths: Vec<usize> = (0..10)
        .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
        .collect();
    rows.iter()
        .map(|r| {
            let mut s: String = (0..9)
                .map(|c| format!("{:<w$}  ", r[c], w = widths[c]))
                .collect();
            s.push_str(&r[9]);
            s
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ls(ctx: &Ctx, args: &[String]) -> ExitCode {
    let (mut json, mut all, mut tk, mut repo, mut state) = (false, false, None, None, None);
    let (mut local_only, mut owner) = (false, Some(ctx.config.owner.clone()));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--all" => all = true,
            "--local" => local_only = true,
            "--ticket" | "--repo" | "--state" | "--owner" => {
                let Some(v) = it.next() else {
                    return usage();
                };
                match a.as_str() {
                    "--ticket" => tk = Some(ticket(v).unwrap_or_else(|| v.to_ascii_uppercase())),
                    "--repo" => repo = Some(v.to_ascii_lowercase()),
                    "--owner" => owner = (v != "all").then(|| v.clone()),
                    _ => state = Some(v.clone()),
                }
            }
            _ => return usage(),
        }
    }
    let lines = match scan(ctx) {
        Ok(l) => l,
        Err(e) => return fail(e),
    };
    let st = actions::status(ctx, &lines);
    let mut rows: Vec<(Line, Value)> = lines
        .into_iter()
        .map(|l| {
            let j = l.to_json();
            (l, j)
        })
        .collect();
    if let Some(u) = remote::user(&ctx.state_dir).filter(|_| !local_only) {
        match remote::lines(ctx, &u, owner.as_deref()) {
            Ok(r) => rows.extend(r),
            Err(e) => eprintln!("overseer: api unreachable, showing this station only: {e}"),
        }
    }
    rows.sort_by_key(|(l, _)| -l.last_activity_ms);
    let rows: Vec<(Line, Value)> = rows
        .into_iter()
        .filter(|(l, _)| all || l.recent(ctx.now_ms))
        .filter(|(l, _)| tk.as_ref().is_none_or(|t| l.ticket.as_ref() == Some(t)))
        .filter(|(l, _)| {
            repo.as_ref()
                .is_none_or(|r| l.repo.to_ascii_lowercase().contains(r.as_str()))
        })
        .filter(|(l, _)| state.as_ref().is_none_or(|s| l.state == s))
        .collect();
    if json {
        let v: Vec<&Value> = rows.iter().map(|(_, j)| j).collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        let lines: Vec<Line> = rows.into_iter().map(|(l, _)| l).collect();
        println!("{}", table(&lines, ctx.now_ms));
        println!("\n{}", actions::status_line(&st));
    }
    ExitCode::SUCCESS
}

fn confirm(id: &str) -> bool {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() {
        eprintln!("overseer: not a terminal; pass --yes to resume {id}");
        return false;
    }
    eprint!("resume line {id}? [y/N] ");
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).is_ok() && s.trim().eq_ignore_ascii_case("y")
}

fn resume(ctx: &Ctx, q: &str, print: bool, launch: Launch) -> ExitCode {
    let unknown = scan(ctx).is_ok_and(|ls| {
        actions::find_line(&ls, q).is_err_and(|e| e.starts_with("no line matches"))
    });
    if let Some(u) = remote::user(&ctx.state_dir).filter(|_| unknown) {
        return match remote::resume(ctx, &u, q, print) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(e),
        };
    }
    match actions::resume(ctx, q, print, launch) {
        Ok(p) if print && p.r#where == "running" => {
            println!("tmux attach -t {}", p.pane.as_deref().unwrap_or_default());
            ExitCode::SUCCESS
        }
        Ok(p) if print => {
            println!("{}", p.shell());
            ExitCode::SUCCESS
        }
        Ok(p) => {
            let pane = p
                .pane
                .as_deref()
                .map(|x| format!(" {x}"))
                .unwrap_or_default();
            println!("resumed {} ({}{pane})", p.line_id, p.r#where);
            if let (Launch::Attach, Some(s)) = (launch, &p.session) {
                println!("attach with: tmux attach -t {s}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(e),
    }
}

fn report(r: &actions::Report, verb: &str, json: bool, apply: bool) -> ExitCode {
    if json {
        println!("{}", r.to_json());
    } else {
        let mode = if apply {
            ""
        } else {
            " (dry run; add --yes to act)"
        };
        println!("{verb} {}{mode}\n{}", r.line_id, r.text());
    }
    if r.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(3)
    }
}

fn hooks_cmd(args: &[String]) -> ExitCode {
    let mode = match args.first().map(String::as_str) {
        Some("install") => install::Mode::Install,
        Some("uninstall") => install::Mode::Uninstall,
        _ => return usage(),
    };
    let apply = args.iter().any(|a| a == "--apply");
    let agents: Vec<String> = args[1..]
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect();
    if let Ok(exe) = std::env::current_exe().and_then(|p| p.canonicalize()) {
        let _ = install::ensure_shim(&install::shim_path(), &exe, false);
    }
    let bin = install::hook_bin();
    let state = overseer::default_state_dir();
    let state = state.canonicalize().unwrap_or(state);
    let stamp = install::stamp(overseer::procs::now_ms());
    let r = install::Run {
        mode,
        apply,
        bin: &bin,
        state_dir: &state,
        stamp: &stamp,
    };
    let available = |t: &install::Target| {
        install::available(
            t,
            overseer::agents::get(t.agent).is_some_and(overseer::agents::is_installed),
        )
    };
    let only = (!agents.is_empty()).then_some(agents.as_slice());
    let mut code = ExitCode::SUCCESS;
    for o in install::run(&install::targets(), only, &r, available) {
        let path = o.path.display();
        if let Some(s) = &o.skipped {
            println!("== {path}: {} ({s}), skipped", o.agent);
            continue;
        }
        if let Some(e) = &o.error {
            eprintln!("overseer: {path}: {e}; file left unchanged");
            code = ExitCode::FAILURE;
            continue;
        }
        if !o.changed {
            println!("== {path}: nothing to do");
            continue;
        }
        if !apply {
            println!("== {path} (dry run; --apply to write, with a backup)");
            print!("{}", o.diff);
            continue;
        }
        println!("== {path}: written");
        if let Some(b) = &o.backup {
            println!("   backup: {}", b.display());
        }
        if o.agent == "codex" && mode == install::Mode::Install {
            println!("   {}", install::note("codex").unwrap_or_default());
        }
    }
    code
}

fn uninstall_cmd(ctx: &Ctx, apply: bool, had_state: bool) -> ExitCode {
    let sock = daemon::socket_path(&ctx.state_dir);
    let body = serde_json::json!({ "dryRun": !apply });
    let v = match daemon::request(&sock, "POST", "/v1/uninstall", Some(&body)) {
        Ok((200, v)) => v,
        Ok((code, v)) => return fail(format!("the daemon answered {code}: {v}")),
        Err(_) => {
            let mut rep = overseer::uninstall::run(ctx, apply);
            if !had_state {
                rep.removed.retain(|(k, _)| k != "stateDir");
            }
            let v = rep.to_json();
            let finished = if apply {
                overseer::uninstall::finish(&ctx.state_dir)
            } else {
                Ok(())
            };
            if let Err(e) = finished {
                eprintln!("overseer: {e}");
                return ExitCode::FAILURE;
            }
            v
        }
    };
    for l in overseer::uninstall::lines(&v) {
        println!("{l}");
    }
    if v["dryRun"] == true {
        println!("dry run: nothing was changed; add --apply to remove everything listed above");
    }
    if v["errors"].as_array().is_some_and(|e| !e.is_empty()) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        return usage();
    };
    let rest = &args[1..];
    if overseer::cli::wants_help(cmd, rest) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if let Err(e) = overseer::cli::check(cmd, rest) {
        eprintln!("overseer: {e}\n{USAGE}");
        return ExitCode::from(2);
    }
    match cmd {
        "login" => return remote::cli_login(rest),
        "enroll" => return overseer::sync::cli_enroll(rest),
        "sync" => return overseer::sync::cli_sync(rest),
        "mcp" => return overseer::mcp::run(),
        "proxy" => return overseer::proxy::run(),
        "statusline-tap" => return overseer::statusline::tap(rest),
        "socket-path" => {
            let state = overseer::default_state_dir();
            println!("{}", daemon::socket_path(&state).display());
            return ExitCode::SUCCESS;
        }
        "version" => {
            let v = daemon::version();
            if rest.iter().any(|a| a == "--json") {
                println!("{v}");
            } else {
                println!(
                    "overseer {} ({}, {}-{}, protocol {})",
                    v["version"].as_str().unwrap_or_default(),
                    v["commit"].as_str().unwrap_or_default(),
                    v["os"].as_str().unwrap_or_default(),
                    v["arch"].as_str().unwrap_or_default(),
                    v["protocol"]
                );
            }
            return ExitCode::SUCCESS;
        }
        "daemon" if rest.iter().any(|a| a == "--takeover-check") => {
            let file = rest
                .iter()
                .position(|a| a == "--takeover-check")
                .and_then(|i| rest.get(i + 1))
                .filter(|a| !a.starts_with("--"));
            return match overseer::upgrade::check(file.map(std::path::Path::new)) {
                Ok(v) => {
                    println!("{v}");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            };
        }
        "daemon" if rest.iter().any(|a| a == "--detach") => {
            return match daemon::detach(rest) {
                Ok(p) => {
                    println!("{}", p.display());
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        _ => {}
    }
    if cmd == "hook" {
        if let (Some(agent), Some(event)) = (rest.first(), rest.get(1)) {
            hooks::run_hook(
                &overseer::default_state_dir(),
                agent,
                event,
                rest.get(2).map(String::as_str),
            );
        }
        return ExitCode::SUCCESS;
    }
    let had_state = overseer::default_state_dir().exists();
    let probe = System;
    let ctx = Ctx::from_env(&probe);
    let flag = |f: &str| rest.iter().any(|a| a == f);
    let target = rest.iter().find(|a| !a.starts_with("--"));
    match cmd {
        "ls" => ls(&ctx, rest),
        "status" => match scan(&ctx) {
            Ok(lines) => {
                let st = actions::status(&ctx, &lines);
                if flag("--json") {
                    println!("{st}");
                } else {
                    println!("{}", actions::status_line(&st));
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        "hold" | "close" => {
            let Some(q) = target else {
                return usage();
            };
            let apply = flag("--yes");
            let r = if cmd == "hold" {
                actions::hold(&ctx, q, apply)
            } else {
                actions::close(&ctx, q, apply, false, false)
            };
            match r {
                Ok(r) => report(&r, cmd, flag("--json"), apply),
                Err(e) => fail(e),
            }
        }
        "resume" => {
            let Some(q) = target else {
                return usage();
            };
            let launch = if flag("--attach") {
                Launch::Attach
            } else {
                Launch::Terminal
            };
            resume(&ctx, q, flag("--print"), launch)
        }
        "patch" => {
            let Some(k) = target else {
                return usage();
            };
            let l = match overseer::runner::pick(&ctx, k) {
                Ok(l) => l,
                Err(e) => return fail(e),
            };
            if !flag("--local") {
                eprintln!(
                    "overseer: {k}: line {} on station {} (owner {})",
                    l.id, l.station_id, l.owner
                );
                return resume(&ctx, &l.id, flag("--print"), Launch::Attach);
            }
            match overseer::runner::patch_local(&ctx, &l, flag("--print")) {
                Ok(s) => {
                    println!("{s}");
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "titles" => {
            let Some(q) = target else {
                return usage();
            };
            match scan(&ctx).and_then(|ls| actions::find_line(&ls, q).cloned()) {
                Ok(l) => {
                    let t = overseer::claude::titles(&ctx.claude_dir, &l.session_id, 5);
                    println!("{}", Value::from(t));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "open" => {
            let Some(url) = target else {
                return usage();
            };
            match remote::parse_url(url) {
                Ok(id) if flag("--yes") || confirm(&id) => {
                    resume(&ctx, &id, false, Launch::Terminal)
                }
                Ok(_) => fail("not confirmed"),
                Err(e) => fail(e),
            }
        }
        "link" if rest.get(1).is_some_and(|a| a == "--env") => {
            let (Some(q), Some(e)) = (rest.first(), rest.get(2)) else {
                return usage();
            };
            match actions::set_env(&ctx, q, e) {
                Ok((id, e)) => {
                    println!("{id} env {}", e.as_deref().unwrap_or("none"));
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "link" => {
            let (Some(q), Some(k)) = (rest.first(), rest.get(1)) else {
                return usage();
            };
            match actions::link(&ctx, q, k) {
                Ok((id, t)) => {
                    println!(
                        "{id} -> {}",
                        t.as_deref().unwrap_or("no ticket (inferred from branch)")
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "pin" => {
            let Some(q) = target else {
                return usage();
            };
            match actions::pin(&ctx, q, !flag("--off")) {
                Ok(id) => {
                    println!("{id} {}", if flag("--off") { "unpinned" } else { "pinned" });
                    ExitCode::SUCCESS
                }
                Err(e) => fail(e),
            }
        }
        "stub" => {
            let Some(q) = target else {
                return usage();
            };
            match actions::stub(&ctx, q) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
        "doctor" => {
            let items = doctor::run(&ctx);
            for i in &items {
                println!("{:<5} {:<18} {}", i.level, i.name, i.detail);
            }
            if items.iter().any(|i| i.level == "fail") {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        "daemon" => {
            let opt = |f: &str| {
                rest.iter()
                    .position(|a| a == f)
                    .and_then(|i| rest.get(i + 1))
            };
            let secs = opt("--interval").and_then(|v| v.parse().ok()).unwrap_or(5);
            match daemon::run(
                Duration::from_secs(secs),
                flag("--once"),
                opt("--http").map(String::as_str),
                opt("--takeover").map(std::path::Path::new),
            ) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => fail(e),
            }
        }
        "board" => match board::cli(&ctx.state_dir) {
            Ok(s) => {
                print!("{s}");
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        },
        "hooks" => hooks_cmd(rest),
        "uninstall" => uninstall_cmd(&ctx, flag("--apply"), had_state),
        _ => usage(),
    }
}
