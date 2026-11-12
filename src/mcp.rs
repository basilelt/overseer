use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub type Call<'a> = dyn FnMut(&str, &str, Option<&Value>) -> Result<(u16, Value), String> + 'a;

const PROTOCOL: &str = "2025-06-18";
const AGENT_STATUSES: [&str; 3] = ["in_testing", "merged", "in_progress"];

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tools() -> Value {
    let s = |d: &str| json!({"type": "string", "description": d});
    let b = |d: &str| json!({"type": "boolean", "description": d});
    let n = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let deps = || {
        json!({"type": "array", "items": {"type": ["string", "integer"]},
               "description": "Task ids or task numbers (#12 or 12) of the same project"})
    };
    let tool = |name: &str, description: &str, properties: Value, required: Value| {
        json!({"name": name, "description": description, "inputSchema": {
            "type": "object", "properties": properties, "required": required}})
    };
    json!([
        tool("list_tasks", "List Kanban tasks, optionally filtered by project and status. The project defaults to your own task's project.",
            json!({"project": s("Absolute path of the project's main worktree root"),
                   "status": s("Status or comma list: pending, auto, in_progress, in_testing, merged, completed")}),
            json!([])),
        tool("get_task", "Get one task by id.", json!({"id": s("Task id")}), json!(["id"])),
        tool("create_task", "Create a pending task. The project defaults to your own task's project, else the current git repository. A quorum orchestrator's subtasks start on their own.",
            json!({"title": s("Task title"), "description": s("Task description"),
                   "agent": s("Agent CLI that runs it (claude, codex, gemini, ...). For a quorum orchestrator, one of its workers (default the first)"),
                   "project": s("Absolute path of the project's main worktree root"),
                   "urgent": b("Needs doing soon"), "important": b("Matters for the project's goals"),
                   "taskType": {"type": "string", "enum": crate::profiles::TASK_TYPES, "description": "Kind of work; picks the agent profile mapped to it"},
                   "dependsOn": deps(),
                   "inheritBranch": b("With exactly one dependency, start from its branch (default true)")}),
            json!(["title"])),
        tool("cast_vote", "Vote on a quorum subtask's result: approve or reject, with a short reason. Reviewers vote on the subtask they review (the default); the orchestrator passes taskId.",
            json!({"taskId": s("Id of the subtask, defaults to the one you review"),
                   "approve": b("true to accept the result, false to reject it"),
                   "reason": s("One or two sentences naming the concrete problems, if any")}),
            json!(["approve", "reason"])),
        tool("set_task_dependencies", "Replace the tasks a task waits on. It does not auto-start until they are all completed. Cycles and self-dependencies are refused.",
            json!({"taskId": s("Task id"), "dependsOn": deps(),
                   "inheritBranch": b("With exactly one dependency, start from its branch (default true)")}),
            json!(["taskId", "dependsOn"])),
        tool("prioritize_task", "Set the Eisenhower flags and the column rank of a task. Cannot change its status.",
            json!({"id": s("Task id"), "urgent": b("Needs doing soon"),
                   "important": b("Matters for the project's goals"),
                   "rank": json!({"type": "number", "description": "Position in its column, lower first"})}),
            json!(["id"])),
        tool("send_message", "Send a message to another task's agent. It is typed into that agent's input at once when it runs, else delivered when it starts.",
            json!({"to": s("Id of the recipient task"), "body": s("Message text, at most 8000 characters")}),
            json!(["to", "body"])),
        tool("read_messages", "Read the messages sent to or from a task, oldest first (defaults to your own task).",
            json!({"id": s("Task id, defaults to your own task")}),
            json!([])),
        tool("find_overlaps", "List other running tasks of the project that changed the same files as a task (defaults to your own task).",
            json!({"id": s("Task id, defaults to your own task")}),
            json!([])),
        tool("update_task_progress", "Report progress on a task (defaults to your own task).",
            json!({"id": s("Task id, defaults to your own task"), "done": n("Steps done"),
                   "total": n("Total steps"), "note": s("Short note")}),
            json!(["done", "total"])),
        tool("set_task_status", "Move your own task: in_testing when your work is ready for review, merged once you merged its changes, in_progress when you resume work on it (from review, merged or closed).",
            json!({"id": s("Task id, defaults to your own task"),
                   "status": {"type": "string", "enum": AGENT_STATUSES}}),
            json!(["status"])),
        tool("list_review_comments", "List the code review comment threads on your worktree (Overseer live review). Each thread has an id, path, line range, the commented code and the comments.",
            json!({"status": s("Status or comma list: pending, sent, addressed, resolved (default sent: waiting for you)"),
                   "worktree": s("Absolute worktree path, defaults to your task's worktree, else the current git repository")}),
            json!([])),
        tool("resolve_review_comment", "Mark a review comment thread as addressed once you changed the code, with a one-line note for the reviewer.",
            json!({"id": s("Thread id (rt-…)"), "note": s("One line: what you changed, or why not")}),
            json!(["id", "note"])),
        tool("list_sessions", "List agent sessions (lines), newest first, with their workspace and task. Read only.",
            json!({"project": s("Absolute project path or repo name to filter on"),
                   "sinceDays": n("Only sessions active in the last N days"),
                   "limit": n("At most this many sessions (default 100, max 300)")}),
            json!([])),
        tool("get_session_brief", "Brief of one session: goal, todos, edited files and last message. Read only.",
            json!({"id": s("Line id")}), json!(["id"])),
        tool("list_projects", "List the git projects known to Overseer. Read only.", json!({}), json!([])),
        tool("list_worktrees", "List a project's worktrees with branch, merged and dirty state. Read only.",
            json!({"project": s("Absolute path of the project's main worktree root")}),
            json!(["project"])),
        tool("list_layout", "List the sidebar layout: workspaces, folders and groups. Read only.", json!({}), json!([])),
        tool("propose_organization", "Record the session configurator's organization plan for the user to review. Only for the session configurator; call it once.",
            json!({"plan": {"type": "object", "description": "Plan version 1: {version, summary, actions, pickUp, reasons}"}}),
            json!(["plan"])),
    ])
}

fn own_origin(call: &mut Call, own: Option<&str>) -> Option<String> {
    let o = own?;
    daemon(call, "GET", &format!("/v1/tasks/{}", encode(o)), None)
        .ok()
        .and_then(|t| t["origin"].as_str().map(str::to_string))
}

fn refuse_configurator(call: &mut Call, own: Option<&str>, what: &str) -> Result<(), String> {
    match own_origin(call, own).as_deref() {
        Some(crate::task_agents::CONFIGURATOR) => {
            Err(format!("the session configurator may not {what}"))
        }
        _ => Ok(()),
    }
}

fn ensure_landed(call: &mut Call, own: &str) -> Result<(), String> {
    let t = daemon(call, "GET", &format!("/v1/tasks/{}", encode(own)), None)?;
    let (project, branch) = (t["project"].as_str(), t["branch"].as_str());
    let (Some(project), Some(branch)) = (project, branch) else {
        return Err("the task has no branch: nothing to mark as merged".into());
    };
    let v = daemon(
        call,
        "GET",
        &format!("/v1/projects/{}/worktrees", encode(project)),
        None,
    )?;
    let landed = v["worktrees"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|w| w["branch"].as_str() == Some(branch) && w["merged"] == true);
    if landed {
        Ok(())
    } else {
        Err(format!(
            "branch {branch} is not merged into the base branch yet: merge it first, then set merged"
        ))
    }
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut o = json!({});
    for k in keys {
        if !v[*k].is_null() {
            o[*k] = v[*k].clone();
        }
    }
    o
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn daemon(
    call: &mut Call,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let (status, v) = call(method, target, body)?;
    if status >= 400 {
        let msg = v["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| v.to_string());
        return Err(format!("daemon answered {status}: {msg}"));
    }
    Ok(v)
}

fn git_top() -> Option<String> {
    let o = crate::git::output_within(
        std::process::Command::new("git").args(["rev-parse", "--show-toplevel"]),
        crate::git::TIMEOUT,
    )
    .ok()
    .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

fn run_tool(name: &str, a: &Value, own: Option<&str>, call: &mut Call) -> Result<Value, String> {
    let s = |k: &str| a[k].as_str().filter(|v| !v.is_empty());
    let id = || {
        s("id")
            .or(own)
            .map(encode)
            .ok_or("no id given and OVERSEER_TASK_ID is not set")
    };
    match name {
        "list_tasks" => {
            let own_project = match (s("project"), own) {
                (None, Some(o)) => daemon(call, "GET", &format!("/v1/tasks/{}", encode(o)), None)
                    .ok()
                    .filter(|t| t["origin"] != "configurator")
                    .and_then(|t| t["project"].as_str().map(str::to_string)),
                _ => None,
            };
            let mut q: Vec<String> = ["project", "status"]
                .iter()
                .filter_map(|k| s(k).map(|v| format!("{k}={}", encode(v))))
                .collect();
            if let Some(p) = own_project {
                q.insert(0, format!("project={}", encode(&p)));
            }
            let target = match q.is_empty() {
                true => "/v1/tasks".to_string(),
                false => format!("/v1/tasks?{}", q.join("&")),
            };
            daemon(call, "GET", &target, None)
        }
        "get_task" => {
            let id = s("id").ok_or("id is required")?;
            daemon(call, "GET", &format!("/v1/tasks/{}", encode(id)), None)
        }
        "create_task" => {
            let title = s("title").ok_or("title is required")?;
            let own_task = own
                .and_then(|o| daemon(call, "GET", &format!("/v1/tasks/{}", encode(o)), None).ok());
            if own_task
                .as_ref()
                .is_some_and(|t| t["origin"] == "organizer" || t["origin"] == "configurator")
            {
                return Err("the organizer may not create tasks".into());
            }
            let project = match s("project") {
                Some(p) => p.to_string(),
                None => own_task
                    .and_then(|t| t["project"].as_str().map(str::to_string))
                    .or_else(git_top)
                    .ok_or("no project given and none could be inferred")?,
            };
            let mut body = json!({"title": title, "description": s("description").unwrap_or(""),
                              "project": project, "status": "pending"});
            for k in ["urgent", "important"] {
                if let Some(v) = a[k].as_bool() {
                    body[k] = json!(v);
                }
            }
            for k in ["agent", "taskType", "dependsOn", "inheritBranch"] {
                if !a[k].is_null() {
                    body[k] = a[k].clone();
                }
            }
            if let Some(o) = own {
                body["origin"] = json!("agent");
                body["parentId"] = json!(o);
            }
            daemon(call, "POST", "/v1/tasks", Some(&body))
        }
        "cast_vote" => {
            let own = own.ok_or("OVERSEER_TASK_ID is not set: you have no task of your own")?;
            let approve = a["approve"].as_bool().ok_or("approve must be a boolean")?;
            let target = match s("taskId") {
                Some(t) => t.to_string(),
                None => daemon(call, "GET", &format!("/v1/tasks/{}", encode(own)), None)?
                    ["parentId"]
                    .as_str()
                    .map(str::to_string)
                    .ok_or("taskId is required: you do not review a subtask")?,
            };
            let body =
                json!({"from": own, "approve": approve, "reason": s("reason").unwrap_or("")});
            daemon(
                call,
                "POST",
                &format!("/v1/tasks/{}/votes", encode(&target)),
                Some(&body),
            )
        }
        "set_task_dependencies" => {
            refuse_configurator(call, own, "change task dependencies")?;
            let id = s("taskId").map(encode).ok_or("taskId is required")?;
            if !a["dependsOn"].is_array() {
                return Err("dependsOn must be an array of task ids or numbers".into());
            }
            let mut body = json!({"dependsOn": a["dependsOn"]});
            if let Some(v) = a["inheritBranch"].as_bool() {
                body["inheritBranch"] = json!(v);
            }
            daemon(call, "PATCH", &format!("/v1/tasks/{id}"), Some(&body))
        }
        "prioritize_task" => {
            refuse_configurator(call, own, "prioritize tasks")?;
            let id = s("id").map(encode).ok_or("id is required")?;
            let mut body = json!({});
            for k in ["urgent", "important"] {
                if let Some(v) = a[k].as_bool() {
                    body[k] = json!(v);
                }
            }
            if let Some(r) = a["rank"].as_f64() {
                body["rank"] = json!(r);
            }
            if body.as_object().is_some_and(|o| o.is_empty()) {
                return Err("give at least one of urgent, important, rank".into());
            }
            daemon(call, "PATCH", &format!("/v1/tasks/{id}"), Some(&body))
        }
        "send_message" => {
            let own = own.ok_or("OVERSEER_TASK_ID is not set: you have no task of your own")?;
            let to = s("to").ok_or("to is required")?;
            let body = s("body").ok_or("body is required")?;
            daemon(
                call,
                "POST",
                &format!("/v1/tasks/{}/messages", encode(to)),
                Some(&json!({"body": body, "from": own})),
            )
        }
        "read_messages" => {
            let id = id()?;
            daemon(call, "GET", &format!("/v1/tasks/{id}/messages"), None)
                .map(|v| json!({"messages": v}))
        }
        "find_overlaps" => {
            let id = id()?;
            daemon(call, "GET", &format!("/v1/tasks/{id}/overlaps"), None)
                .map(|v| json!({"overlaps": v}))
        }
        "update_task_progress" => {
            let id = id()?;
            let (Some(done), Some(total)) = (a["done"].as_u64(), a["total"].as_u64()) else {
                return Err("done and total must be non-negative integers".into());
            };
            let mut progress = json!({"done": done, "total": total});
            if let Some(note) = s("note") {
                progress["note"] = json!(note);
            }
            daemon(
                call,
                "PATCH",
                &format!("/v1/tasks/{id}"),
                Some(&json!({"progress": progress})),
            )
        }
        "set_task_status" => {
            let own = own.ok_or("OVERSEER_TASK_ID is not set: you have no task of your own")?;
            if s("id").is_some_and(|i| i != own) {
                return Err("an agent may only change the status of its own task".into());
            }
            let status = s("status")
                .filter(|x| AGENT_STATUSES.contains(x))
                .ok_or_else(|| {
                    format!(
                        "an agent may only move its task to {}",
                        AGENT_STATUSES.join(", ")
                    )
                })?;
            refuse_configurator(call, Some(own), "change task status")?;
            if status == "merged" {
                ensure_landed(call, own)?;
            }
            daemon(
                call,
                "PATCH",
                &format!("/v1/tasks/{}", encode(own)),
                Some(&json!({"status": status})),
            )
        }
        "list_review_comments" => {
            let wt = match s("worktree") {
                Some(w) => w.to_string(),
                None => own
                    .and_then(|o| {
                        daemon(call, "GET", &format!("/v1/tasks/{}", encode(o)), None).ok()
                    })
                    .and_then(|t| t["worktreePath"].as_str().map(str::to_string))
                    .or_else(git_top)
                    .ok_or("no worktree given and none could be inferred")?,
            };
            let status = s("status").unwrap_or("sent");
            daemon(
                call,
                "GET",
                &format!(
                    "/v1/review/comments?worktree={}&status={}",
                    encode(&wt),
                    encode(status)
                ),
                None,
            )
            .map(|v| json!({"threads": v}))
        }
        "resolve_review_comment" => {
            let id = s("id").ok_or("id is required")?;
            let note = s("note").ok_or("note is required")?;
            daemon(
                call,
                "PATCH",
                &format!("/v1/review/comments/{}", encode(id)),
                Some(&json!({"status": "addressed", "note": note})),
            )
        }
        "list_sessions" => {
            let mut q: Vec<String> = s("project")
                .map(|p| format!("project={}", encode(p)))
                .into_iter()
                .collect();
            for k in ["sinceDays", "limit"] {
                if let Some(v) = a[k].as_u64() {
                    q.push(format!("{k}={v}"));
                }
            }
            daemon(
                call,
                "GET",
                &format!("/v1/organize/sessions?{}", q.join("&")),
                None,
            )
        }
        "get_session_brief" => {
            let id = s("id").ok_or("id is required")?;
            let v = daemon(
                call,
                "GET",
                &format!("/v1/lines/{}/activity", encode(id)),
                None,
            )?;
            if v["transcript"] == false {
                return Err(format!("no transcript for line {id}"));
            }
            let mut b = pick(&v, &["title", "goal", "lastPrompt", "todos", "editedFiles"]);
            b["id"] = json!(id);
            b["lastMessage"] = v["lastAssistantText"].clone();
            Ok(b)
        }
        "list_projects" => {
            daemon(call, "GET", "/v1/organize/projects", None).map(|v| json!({"projects": v}))
        }
        "list_worktrees" => {
            let p = s("project").ok_or("project is required")?;
            let v = daemon(
                call,
                "GET",
                &format!("/v1/projects/{}/worktrees", encode(p)),
                None,
            )?;
            let keep = [
                "path",
                "branch",
                "kind",
                "state",
                "merged",
                "dirty",
                "ticketKey",
                "lastActivityAt",
            ];
            Ok(json!({"project": v["project"], "base": v["base"],
                "worktrees": v["worktrees"].as_array().into_iter().flatten()
                    .map(|w| pick(w, &keep)).collect::<Vec<_>>()}))
        }
        "list_layout" => daemon(call, "GET", "/v1/organize/layout", None),
        "propose_organization" => {
            let own = own
                .ok_or("OVERSEER_TASK_ID is not set: only the session configurator may propose")?;
            if own_origin(call, Some(own)).as_deref() != Some(crate::task_agents::CONFIGURATOR) {
                return Err("propose_organization is only for the session configurator".into());
            }
            if !a["plan"].is_object() {
                return Err("plan must be an object".into());
            }
            let v = daemon(
                call,
                "POST",
                "/v1/organize/proposals",
                Some(&json!({"plan": a["plan"], "taskId": own})),
            )?;
            Ok(json!({"proposalId": v["id"], "status": "pending_review"}))
        }
        _ => unreachable!(),
    }
}

fn call_tool(id: &Value, params: &Value, own: Option<&str>, call: &mut Call) -> Value {
    let name = params["name"].as_str().unwrap_or("");
    if ![
        "list_tasks",
        "get_task",
        "create_task",
        "cast_vote",
        "set_task_dependencies",
        "prioritize_task",
        "send_message",
        "read_messages",
        "find_overlaps",
        "update_task_progress",
        "set_task_status",
        "list_review_comments",
        "resolve_review_comment",
        "list_sessions",
        "get_session_brief",
        "list_projects",
        "list_worktrees",
        "list_layout",
        "propose_organization",
    ]
    .contains(&name)
    {
        return error(id, -32602, &format!("unknown tool: {name}"));
    }
    let result = match run_tool(name, &params["arguments"], own, call) {
        Ok(v) => {
            let text = serde_json::to_string_pretty(&v).unwrap_or_default();
            let structured = match v {
                Value::Object(_) => v,
                other => json!({"tasks": other}),
            };
            json!({"content": [{"type": "text", "text": text}],
                   "structuredContent": structured, "isError": false})
        }
        Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
    };
    reply(id, result)
}

pub fn handle(msg: &Value, own_task: Option<&str>, call: &mut Call) -> Option<Value> {
    let id = msg.get("id")?;
    let Some(method) = msg["method"].as_str() else {
        if msg.get("result").is_some() || msg.get("error").is_some() {
            return None;
        }
        return Some(error(id, -32600, "invalid request"));
    };
    Some(match method {
        "initialize" => reply(
            id,
            json!({
                "protocolVersion": PROTOCOL,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "overseer", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "Overseer Kanban tasks and code review. Read and create tasks (always pending), report progress on your task with update_task_progress, and call set_task_status with in_testing when your work is ready for review, with merged once you merged its changes, and with in_progress when you resume work on it after review, merge or close. When the reviewer sends comments, list_review_comments shows them and resolve_review_comment answers each one once addressed. When you find a bug or improvement unrelated to your task, create_task it (with urgent and important) instead of fixing it off-scope. Call find_overlaps before large edits and before moving your task to in_testing; if another task touches the same files, send_message its agent to agree who changes what. Overseer also warns you by message when your task moves to in_testing and overlaps another. Messages from other agents arrive in your input prefixed [Overseer]; reply with send_message. The session configurator reads sessions with list_sessions, get_session_brief, list_projects, list_worktrees and list_layout, and records one plan with propose_organization.",
            }),
        ),
        "ping" => reply(id, json!({})),
        "tools/list" => reply(id, json!({"tools": tools()})),
        "tools/call" => call_tool(id, &msg["params"], own_task, call),
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

pub fn serve(input: impl BufRead, mut output: impl Write, own_task: Option<&str>, call: &mut Call) {
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let out = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&msg, own_task, call),
            Err(_) => Some(error(&Value::Null, -32700, "parse error")),
        };
        if let Some(out) = out {
            if writeln!(output, "{out}")
                .and_then(|_| output.flush())
                .is_err()
            {
                break;
            }
        }
    }
}

pub fn vote_args(args: &[String]) -> Result<Value, String> {
    let usage = "usage: overseer vote approve|reject <reason> [--task <id>]";
    let mut it = args.iter();
    let approve = match it.next().map(String::as_str) {
        Some("approve") => true,
        Some("reject") => false,
        _ => return Err(usage.into()),
    };
    let (mut reason, mut task) = (vec![], None);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--task" => task = Some(it.next().ok_or(usage)?.clone()),
            _ => reason.push(a.as_str()),
        }
    }
    let mut v = json!({"approve": approve, "reason": reason.join(" ")});
    if let Some(t) = task {
        v["taskId"] = json!(t);
    }
    Ok(v)
}

pub fn cli_vote(args: &[String]) -> std::process::ExitCode {
    let own = std::env::var("OVERSEER_TASK_ID")
        .ok()
        .filter(|s| !s.is_empty());
    let socket = crate::daemon::socket_path(&crate::default_state_dir());
    let mut call = |m: &str, t: &str, b: Option<&Value>| crate::daemon::request(&socket, m, t, b);
    match vote_args(args).and_then(|a| run_tool("cast_vote", &a, own.as_deref(), &mut call)) {
        Ok(t) => {
            println!(
                "vote recorded on task #{}: {}",
                t["number"],
                t["votes"]["outcome"]
                    .as_str()
                    .unwrap_or("waiting for the other voters")
            );
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("overseer vote: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

pub fn run() -> std::process::ExitCode {
    let own = std::env::var("OVERSEER_TASK_ID")
        .ok()
        .filter(|s| !s.is_empty());
    let socket = crate::daemon::socket_path(&crate::default_state_dir());
    let mut call = |m: &str, t: &str, b: Option<&Value>| crate::daemon::request(&socket, m, t, b);
    serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        own.as_deref(),
        &mut call,
    );
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    type Log = Vec<(String, String, Option<Value>)>;

    fn with_fake(
        msg: Value,
        own: Option<&str>,
        answer: impl Fn(&str, &str) -> (u16, Value),
    ) -> (Option<Value>, Log) {
        let mut log = vec![];
        let mut call = |m: &str, t: &str, b: Option<&Value>| {
            log.push((m.to_string(), t.to_string(), b.cloned()));
            Ok(answer(m, t))
        };
        let out = handle(&msg, own, &mut call);
        (out, log)
    }

    fn tool(name: &str, args: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
               "params": {"name": name, "arguments": args}})
    }

    fn ok(_: &str, _: &str) -> (u16, Value) {
        (200, json!({"id": "T1", "project": "/p"}))
    }

    #[test]
    fn initialize_roundtrip() {
        let msg = json!({"jsonrpc": "2.0", "id": 0, "method": "initialize",
                         "params": {"protocolVersion": "2024-11-05", "capabilities": {}}});
        let (out, log) = with_fake(msg, None, ok);
        let out = out.unwrap();
        assert_eq!(out["id"], 0);
        assert_eq!(out["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(out["result"]["capabilities"]["tools"]["listChanged"], false);
        assert_eq!(out["result"]["serverInfo"]["name"], "overseer");
        assert!(log.is_empty());
    }

    #[test]
    fn ping_and_notification() {
        let (out, _) = with_fake(
            json!({"jsonrpc": "2.0", "id": "a", "method": "ping"}),
            None,
            ok,
        );
        assert_eq!(
            out.unwrap(),
            json!({"jsonrpc": "2.0", "id": "a", "result": {}})
        );
        let note = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        assert!(with_fake(note, None, ok).0.is_none());
    }

    #[test]
    fn tools_list_names() {
        let (out, _) = with_fake(
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
            None,
            ok,
        );
        let names: Vec<String> = out.unwrap()["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                assert_eq!(t["inputSchema"]["type"], "object");
                t["name"].as_str().unwrap().to_string()
            })
            .collect();
        assert_eq!(
            names,
            [
                "list_tasks",
                "get_task",
                "create_task",
                "cast_vote",
                "set_task_dependencies",
                "prioritize_task",
                "send_message",
                "read_messages",
                "find_overlaps",
                "update_task_progress",
                "set_task_status",
                "list_review_comments",
                "resolve_review_comment",
                "list_sessions",
                "get_session_brief",
                "list_projects",
                "list_worktrees",
                "list_layout",
                "propose_organization"
            ]
        );
    }

    #[test]
    fn list_tasks_encodes_query() {
        let (out, log) = with_fake(
            tool(
                "list_tasks",
                json!({"project": "/home/u/my x", "status": "pending,auto"}),
            ),
            None,
            |_, _| (200, json!([])),
        );
        assert_eq!(log[0].0, "GET");
        assert_eq!(
            log[0].1,
            "/v1/tasks?project=%2Fhome%2Fu%2Fmy%20x&status=pending%2Cauto"
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["isError"], false);
        assert_eq!(r["structuredContent"], json!({"tasks": []}));
        let (_, log) = with_fake(tool("list_tasks", json!({})), None, |_, _| (200, json!([])));
        assert_eq!(log[0].1, "/v1/tasks");
    }

    #[test]
    fn list_tasks_defaults_to_own_project() {
        let answer = |_: &str, t: &str| match t {
            "/v1/tasks/OWN" => (200, json!({"id": "OWN", "project": "/home/u/my x"})),
            _ => (200, json!([])),
        };
        let (_, log) = with_fake(
            tool("list_tasks", json!({"status": "pending"})),
            Some("OWN"),
            answer,
        );
        assert_eq!(log[0].1, "/v1/tasks/OWN");
        assert_eq!(
            log[1].1,
            "/v1/tasks?project=%2Fhome%2Fu%2Fmy%20x&status=pending"
        );
        let (_, log) = with_fake(
            tool("list_tasks", json!({"project": "/other"})),
            Some("OWN"),
            answer,
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].1, "/v1/tasks?project=%2Fother");
    }

    #[test]
    fn list_tasks_lists_every_project_for_the_configurator_or_without_own_task() {
        let (_, log) = with_fake(tool("list_tasks", json!({})), Some("CFG"), |_, t| match t {
            "/v1/tasks/CFG" => (
                200,
                json!({"id": "CFG", "project": "/p", "origin": "configurator"}),
            ),
            _ => (200, json!([])),
        });
        assert_eq!(log[1].1, "/v1/tasks");
        let (_, log) = with_fake(
            tool("list_tasks", json!({})),
            Some("GONE"),
            |_, t| match t {
                "/v1/tasks/GONE" => (404, json!({"error": "no such task"})),
                _ => (200, json!([])),
            },
        );
        assert_eq!(log[1].1, "/v1/tasks");
    }

    #[test]
    fn get_task_maps() {
        let (out, log) = with_fake(tool("get_task", json!({"id": "T9"})), None, ok);
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("GET", "/v1/tasks/T9")
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["structuredContent"]["id"], "T1");
        assert_eq!(r["content"][0]["type"], "text");
        assert_eq!(
            serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap()["id"],
            "T1"
        );
    }

    #[test]
    fn daemon_error_is_tool_error() {
        let (out, _) = with_fake(tool("get_task", json!({"id": "X"})), None, |_, _| {
            (404, json!({"error": "no such task"}))
        });
        let r = &out.unwrap()["result"];
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no such task"));
    }

    #[test]
    fn create_task_is_pending_with_own_project() {
        let (_, log) = with_fake(
            tool(
                "create_task",
                json!({"title": "t", "description": "d", "status": "auto"}),
            ),
            Some("OWN"),
            ok,
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("GET", "/v1/tasks/OWN")
        );
        assert_eq!(
            (log[1].0.as_str(), log[1].1.as_str()),
            ("POST", "/v1/tasks")
        );
        assert_eq!(
            log[1].2,
            Some(
                json!({"title": "t", "description": "d", "project": "/p", "status": "pending",
                        "origin": "agent", "parentId": "OWN"})
            )
        );
        let (_, log) = with_fake(
            tool("create_task", json!({"title": "t", "project": "/q"})),
            None,
            ok,
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].2.as_ref().unwrap()["project"], "/q");
        assert!(log[0].2.as_ref().unwrap().get("origin").is_none());
        let (res, log) = with_fake(
            tool("create_task", json!({"title": "t"})),
            Some("OWN"),
            |_, _| (200, json!({"origin": "organizer", "project": "/p"})),
        );
        assert_eq!(log.len(), 1);
        assert!(format!("{res:?}").contains("organizer may not create"));
        let (_, log) = with_fake(
            tool(
                "create_task",
                json!({"title": "t", "project": "/q", "urgent": true, "important": false, "taskType": "fix"}),
            ),
            None,
            ok,
        );
        assert_eq!(log[0].2.as_ref().unwrap()["taskType"], "fix");
        assert_eq!(log[0].2.as_ref().unwrap()["urgent"], true);
        assert_eq!(log[0].2.as_ref().unwrap()["important"], false);
    }

    #[test]
    fn agents_can_chain_tasks() {
        let (_, log) = with_fake(
            tool(
                "create_task",
                json!({"title": "t", "project": "/q", "dependsOn": ["#3", "ID9"], "inheritBranch": false}),
            ),
            None,
            ok,
        );
        assert_eq!(
            log[0].2.as_ref().unwrap()["dependsOn"],
            json!(["#3", "ID9"])
        );
        assert_eq!(log[0].2.as_ref().unwrap()["inheritBranch"], false);
        let (out, log) = with_fake(
            tool(
                "set_task_dependencies",
                json!({"taskId": "T2", "dependsOn": [3], "inheritBranch": true}),
            ),
            None,
            ok,
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("PATCH", "/v1/tasks/T2")
        );
        assert_eq!(
            log[0].2,
            Some(json!({"dependsOn": [3], "inheritBranch": true}))
        );
        assert_eq!(out.unwrap()["result"]["isError"], false);
        for args in [
            json!({"dependsOn": []}),
            json!({"taskId": "T2"}),
            json!({"taskId": "T2", "dependsOn": "x"}),
        ] {
            let (out, log) = with_fake(tool("set_task_dependencies", args), None, ok);
            assert!(log.is_empty());
            assert_eq!(out.unwrap()["result"]["isError"], true);
        }
        let (out, _) = with_fake(
            tool(
                "set_task_dependencies",
                json!({"taskId": "T2", "dependsOn": ["T2"]}),
            ),
            None,
            |_, _| (400, json!({"error": "a task cannot depend on itself"})),
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("400"));
    }

    #[test]
    fn collaboration_tools_map_to_routes() {
        let (_, log) = with_fake(
            tool(
                "prioritize_task",
                json!({"id": "T2", "urgent": true, "important": false, "rank": 1.5}),
            ),
            None,
            ok,
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("PATCH", "/v1/tasks/T2")
        );
        assert_eq!(
            log[0].2,
            Some(json!({"urgent": true, "important": false, "rank": 1.5}))
        );
        for args in [json!({}), json!({"id": "T2"})] {
            let (out, log) = with_fake(tool("prioritize_task", args), None, ok);
            assert!(log.is_empty());
            assert_eq!(out.unwrap()["result"]["isError"], true);
        }
        let (_, log) = with_fake(
            tool("send_message", json!({"to": "T3", "body": "hi"})),
            Some("OWN"),
            ok,
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("POST", "/v1/tasks/T3/messages")
        );
        assert_eq!(log[0].2, Some(json!({"body": "hi", "from": "OWN"})));
        for (args, own) in [
            (json!({"to": "T3", "body": "hi"}), None),
            (json!({"body": "hi"}), Some("OWN")),
            (json!({"to": "T3"}), Some("OWN")),
        ] {
            let (out, log) = with_fake(tool("send_message", args), own, ok);
            assert!(log.is_empty());
            assert_eq!(out.unwrap()["result"]["isError"], true);
        }
        let (out, log) = with_fake(tool("read_messages", json!({})), Some("OWN"), |_, _| {
            (200, json!([{"id": "m1"}]))
        });
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("GET", "/v1/tasks/OWN/messages")
        );
        assert_eq!(
            out.unwrap()["result"]["structuredContent"]["messages"][0]["id"],
            "m1"
        );
        let (out, log) = with_fake(tool("find_overlaps", json!({"id": "T4"})), None, |_, _| {
            (200, json!([]))
        });
        assert_eq!(log[0].1, "/v1/tasks/T4/overlaps");
        assert_eq!(
            out.unwrap()["result"]["structuredContent"],
            json!({"overlaps": []})
        );
        let (out, log) = with_fake(tool("find_overlaps", json!({})), None, ok);
        assert!(log.is_empty());
        assert_eq!(out.unwrap()["result"]["isError"], true);
    }

    #[test]
    fn update_progress_defaults_to_own() {
        let (_, log) = with_fake(
            tool(
                "update_task_progress",
                json!({"done": 2, "total": 5, "note": "tests"}),
            ),
            Some("OWN"),
            ok,
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("PATCH", "/v1/tasks/OWN")
        );
        assert_eq!(
            log[0].2,
            Some(json!({"progress": {"done": 2, "total": 5, "note": "tests"}}))
        );
        let (_, log) = with_fake(
            tool(
                "update_task_progress",
                json!({"id": "T2", "done": 1, "total": 1}),
            ),
            None,
            ok,
        );
        assert_eq!(log[0].1, "/v1/tasks/T2");
        assert_eq!(log[0].2, Some(json!({"progress": {"done": 1, "total": 1}})));
        let (out, log) = with_fake(
            tool("update_task_progress", json!({"done": 1, "total": 1})),
            None,
            ok,
        );
        assert!(log.is_empty());
        assert_eq!(out.unwrap()["result"]["isError"], true);
    }

    #[test]
    fn set_status_rules() {
        let refused = [
            (json!({"id": "OTHER", "status": "in_testing"}), Some("OWN")),
            (json!({"status": "completed"}), Some("OWN")),
            (json!({"status": "pending"}), Some("OWN")),
            (json!({"status": "in_testing"}), None),
        ];
        for (args, own) in refused {
            let (out, log) = with_fake(tool("set_task_status", args), own, ok);
            assert!(log.is_empty());
            assert_eq!(out.unwrap()["result"]["isError"], true);
        }
        for status in ["in_testing", "in_progress"] {
            let (out, log) = with_fake(
                tool("set_task_status", json!({"id": "OWN", "status": status})),
                Some("OWN"),
                ok,
            );
            assert_eq!(out.unwrap()["result"]["isError"], false, "{status}");
            assert_eq!(
                (log[1].0.as_str(), log[1].1.as_str()),
                ("PATCH", "/v1/tasks/OWN")
            );
            assert_eq!(log[1].2, Some(json!({"status": status})));
        }
    }

    fn landed_answer(merged: bool, branch: Option<&str>) -> impl Fn(&str, &str) -> (u16, Value) {
        let branch = branch.map(str::to_string);
        move |_, t| {
            if t.contains("/worktrees") {
                (
                    200,
                    json!({"worktrees": [{"branch": "sb/x", "merged": merged}, {"branch": "sb/y", "merged": true}]}),
                )
            } else {
                (200, json!({"id": "OWN", "project": "/p", "branch": branch}))
            }
        }
    }

    #[test]
    fn set_status_merged_needs_the_branch_to_have_landed() {
        let call = |a: &dyn Fn(&str, &str) -> (u16, Value)| {
            with_fake(
                tool("set_task_status", json!({"id": "OWN", "status": "merged"})),
                Some("OWN"),
                a,
            )
        };
        for answer in [
            landed_answer(false, Some("sb/x")),
            landed_answer(true, None),
        ] {
            let (out, log) = call(&answer);
            assert_eq!(out.unwrap()["result"]["isError"], true);
            assert!(log.iter().all(|l| l.0 == "GET"), "no PATCH was sent");
        }
        let (out, log) = call(&landed_answer(true, Some("sb/x")));
        assert_eq!(out.unwrap()["result"]["isError"], false);
        let last = log.last().unwrap();
        assert_eq!(
            (last.0.as_str(), last.1.as_str(), last.2.clone()),
            ("PATCH", "/v1/tasks/OWN", Some(json!({"status": "merged"})))
        );
    }

    fn configurator(m: &str, t: &str) -> (u16, Value) {
        match (m, t) {
            ("GET", "/v1/tasks/OWN") => (200, json!({"id": "OWN", "origin": "configurator"})),
            ("POST", "/v1/organize/proposals") => (201, json!({"id": "P1"})),
            _ => (200, json!({})),
        }
    }

    #[test]
    fn configurator_is_gated() {
        for (name, args) in [
            ("create_task", json!({"title": "t", "project": "/p"})),
            ("prioritize_task", json!({"id": "T2", "urgent": true})),
            ("set_task_status", json!({"status": "in_testing"})),
            (
                "set_task_dependencies",
                json!({"taskId": "T2", "dependsOn": ["T3"]}),
            ),
        ] {
            let (out, log) = with_fake(tool(name, args), Some("OWN"), configurator);
            assert_eq!(out.unwrap()["result"]["isError"], true, "{name}");
            assert!(log.iter().all(|l| l.0 == "GET"), "{name}");
        }
        let plan = json!({"version": 1, "actions": []});
        let (out, log) = with_fake(
            tool("propose_organization", json!({"plan": plan})),
            Some("OWN"),
            configurator,
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["isError"], false);
        assert_eq!(
            r["structuredContent"],
            json!({"proposalId": "P1", "status": "pending_review"})
        );
        assert_eq!(
            (log[1].0.as_str(), log[1].1.as_str()),
            ("POST", "/v1/organize/proposals")
        );
        assert_eq!(log[1].2, Some(json!({"plan": plan, "taskId": "OWN"})));
        for own in [None, Some("OWN")] {
            let (out, log) = with_fake(
                tool("propose_organization", json!({"plan": {}})),
                own,
                |_, _| (200, json!({"origin": "organizer"})),
            );
            assert_eq!(out.unwrap()["result"]["isError"], true);
            assert!(log.iter().all(|l| l.0 == "GET"));
        }
    }

    #[test]
    fn organize_tools_map_to_routes() {
        let (_, log) = with_fake(
            tool(
                "list_sessions",
                json!({"project": "/w/a b", "sinceDays": 30, "limit": 50}),
            ),
            None,
            |_, _| (200, json!({"sessions": []})),
        );
        assert_eq!(
            log[0].1,
            "/v1/organize/sessions?project=%2Fw%2Fa%20b&sinceDays=30&limit=50"
        );
        let (out, log) = with_fake(
            tool("get_session_brief", json!({"id": "L1"})),
            None,
            |_, _| {
                (
                    200,
                    json!({"goal": "g", "todos": [], "editedFiles": ["a"], "lastAssistantText": "bye", "workLog": [1, 2]}),
                )
            },
        );
        assert_eq!(log[0].1, "/v1/lines/L1/activity");
        let b = &out.unwrap()["result"]["structuredContent"];
        assert_eq!(b["lastMessage"], "bye");
        assert!(b.get("workLog").is_none());
        let (out, _) = with_fake(
            tool("get_session_brief", json!({"id": "L1"})),
            None,
            |_, _| (200, json!({"transcript": false, "workLog": []})),
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["isError"], true);
        assert_eq!(r["content"][0]["text"], "no transcript for line L1");
        let (out, log) = with_fake(
            tool("list_worktrees", json!({"project": "/r"})),
            None,
            |_, _| {
                (
                    200,
                    json!({"project": "/r", "worktrees": [{"path": "/r", "merged": false, "dirty": true, "sizeBytes": 9}]}),
                )
            },
        );
        assert_eq!(log[0].1, "/v1/projects/%2Fr/worktrees");
        let w = &out.unwrap()["result"]["structuredContent"]["worktrees"][0];
        assert_eq!(w["dirty"], true);
        assert!(w.get("sizeBytes").is_none());
        for (name, target) in [
            ("list_projects", "/v1/organize/projects"),
            ("list_layout", "/v1/organize/layout"),
        ] {
            let (_, log) = with_fake(tool(name, json!({})), None, |_, _| (200, json!({})));
            assert_eq!((log[0].0.as_str(), log[0].1.as_str()), ("GET", target));
        }
    }

    #[test]
    fn review_tools_map_to_routes() {
        let (out, log) = with_fake(
            tool("list_review_comments", json!({})),
            Some("OWN"),
            |_, t| {
                if t.starts_with("/v1/tasks/") {
                    (200, json!({"id": "OWN", "worktreePath": "/w/my wt"}))
                } else {
                    (200, json!([{"id": "rt-1"}]))
                }
            },
        );
        assert_eq!(log[0].1, "/v1/tasks/OWN");
        assert_eq!(
            log[1].1,
            "/v1/review/comments?worktree=%2Fw%2Fmy%20wt&status=sent"
        );
        let r = &out.unwrap()["result"];
        assert_eq!(r["structuredContent"]["threads"][0]["id"], "rt-1");
        let (_, log) = with_fake(
            tool(
                "list_review_comments",
                json!({"worktree": "/x", "status": "sent,addressed"}),
            ),
            None,
            |_, _| (200, json!([])),
        );
        assert_eq!(
            log[0].1,
            "/v1/review/comments?worktree=%2Fx&status=sent%2Caddressed"
        );
        let (out, log) = with_fake(
            tool(
                "resolve_review_comment",
                json!({"id": "rt-1", "note": "done"}),
            ),
            None,
            |_, _| (200, json!({"id": "rt-1", "status": "addressed"})),
        );
        assert_eq!(
            (log[0].0.as_str(), log[0].1.as_str()),
            ("PATCH", "/v1/review/comments/rt-1")
        );
        assert_eq!(
            log[0].2,
            Some(json!({"status": "addressed", "note": "done"}))
        );
        assert_eq!(
            out.unwrap()["result"]["structuredContent"]["status"],
            "addressed"
        );
        let (out, log) = with_fake(
            tool("resolve_review_comment", json!({"id": "rt-1"})),
            None,
            ok,
        );
        assert!(log.is_empty());
        assert_eq!(out.unwrap()["result"]["isError"], true);
    }

    #[test]
    fn errors() {
        let (out, _) = with_fake(
            json!({"jsonrpc": "2.0", "id": 3, "method": "nope"}),
            None,
            ok,
        );
        assert_eq!(out.unwrap()["error"]["code"], -32601);
        let (out, log) = with_fake(tool("drop_tables", json!({})), None, ok);
        assert_eq!(out.unwrap()["error"]["code"], -32602);
        assert!(log.is_empty());
    }

    #[test]
    fn serve_lines() {
        let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n\
{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n\
\n\
{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n\
not json\n";
        let mut out = vec![];
        let mut call = |_: &str, _: &str, _: Option<&Value>| Ok((200, Value::Null));
        serve(&input[..], &mut out, None, &mut call);
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["id"], 1);
        assert_eq!(lines[1]["id"], 2);
        assert_eq!(lines[2]["id"], Value::Null);
        assert_eq!(lines[2]["error"]["code"], -32700);
    }

    #[test]
    fn cast_vote_defaults_to_the_reviewed_subtask() {
        let parent = |m: &str, _: &str| match m {
            "GET" => (200, json!({"id": "R", "parentId": "W"})),
            _ => (200, json!({"id": "W"})),
        };
        let (out, log) = with_fake(
            tool("cast_vote", json!({"approve": false, "reason": "no tests"})),
            Some("R"),
            parent,
        );
        assert!(out.unwrap()["result"]["isError"] != true);
        assert_eq!(
            (log[1].0.as_str(), log[1].1.as_str()),
            ("POST", "/v1/tasks/W/votes")
        );
        assert_eq!(
            log[1].2,
            Some(json!({"from": "R", "approve": false, "reason": "no tests"}))
        );
        let (_, log) = with_fake(
            tool(
                "cast_vote",
                json!({"taskId": "W2", "approve": true, "reason": "ok"}),
            ),
            Some("O"),
            ok,
        );
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].1, "/v1/tasks/W2/votes");
    }

    #[test]
    fn cast_vote_refuses_without_an_own_task_a_target_or_a_boolean() {
        let err = |args: Value, own: Option<&str>| {
            let mut call = |_: &str, _: &str, _: Option<&Value>| Ok((200, json!({"id": "R"})));
            run_tool("cast_vote", &args, own, &mut call).unwrap_err()
        };
        assert!(err(json!({"approve": true, "reason": "x"}), None).contains("OVERSEER_TASK_ID"));
        assert_eq!(
            err(json!({"approve": "yes", "reason": "x"}), Some("R")),
            "approve must be a boolean"
        );
        assert_eq!(
            err(json!({"approve": true, "reason": "x"}), Some("R")),
            "taskId is required: you do not review a subtask"
        );
    }

    #[test]
    fn create_task_passes_the_agent_through() {
        let (_, log) = with_fake(
            tool("create_task", json!({"title": "t", "agent": "gemini"})),
            Some("OWN"),
            ok,
        );
        assert_eq!(log[1].2.as_ref().unwrap()["agent"], "gemini");
    }

    #[test]
    fn vote_args_reads_the_verdict_reason_and_task() {
        let a = |v: &[&str]| vote_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            a(&["approve", "all", "good"]).unwrap(),
            json!({"approve": true, "reason": "all good"})
        );
        assert_eq!(
            a(&["reject", "bad", "--task", "W"]).unwrap(),
            json!({"approve": false, "reason": "bad", "taskId": "W"})
        );
        assert!(a(&["maybe"])
            .unwrap_err()
            .starts_with("usage: overseer vote"));
        assert!(a(&[]).is_err());
        assert!(a(&["approve", "--task"]).is_err());
    }
}
