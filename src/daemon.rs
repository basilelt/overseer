use crate::actions;
use crate::procs::{now_ms, System};
use crate::util::ticket;
use crate::{scan, Ctx};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub const PROTOCOL: u32 = 1;

pub trait Stream: Read + Write + Send {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()>;

    fn reader(&self) -> Option<Box<dyn Read + Send>> {
        None
    }

    fn hang_up(&self) {}
}

impl Stream for TcpStream {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        TcpStream::set_read_timeout(self, d)
    }

    fn reader(&self) -> Option<Box<dyn Read + Send>> {
        self.try_clone()
            .ok()
            .map(|s| Box::new(s) as Box<dyn Read + Send>)
    }

    fn hang_up(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(unix)]
impl Stream for std::os::unix::net::UnixStream {
    fn set_read_timeout(&self, d: Option<Duration>) -> std::io::Result<()> {
        std::os::unix::net::UnixStream::set_read_timeout(self, d)
    }

    fn reader(&self) -> Option<Box<dyn Read + Send>> {
        self.try_clone()
            .ok()
            .map(|s| Box::new(s) as Box<dyn Read + Send>)
    }

    fn hang_up(&self) {
        let _ = self.shutdown(std::net::Shutdown::Both);
    }
}

pub fn socket_path(state_dir: &Path) -> PathBuf {
    std::env::var_os("OVERSEER_SOCKET")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            state_dir.join(if cfg!(unix) {
                "daemon.sock"
            } else {
                "daemon.json"
            })
        })
}

pub fn version() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": option_env!("OVERSEER_COMMIT").unwrap_or("unknown"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "protocol": PROTOCOL,
    })
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub(crate) const HEAD_TIMEOUT: Duration = Duration::from_secs(30);

struct Head {
    req: Request,
    len: Option<u64>,
    chunked: bool,
}

const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);
const MAX_LINE: usize = 8 << 10;
const MAX_HEAD: usize = 64 << 10;
const MAX_HEADERS: usize = 100;

#[derive(Debug, PartialEq)]
enum HeadErr {
    Io(ErrorKind),
    TooLarge(u16),
}

impl From<ErrorKind> for HeadErr {
    fn from(k: ErrorKind) -> Self {
        HeadErr::Io(k)
    }
}

fn head_line(r: &mut impl BufRead, budget: &mut usize, code: u16) -> Result<String, HeadErr> {
    let cap = MAX_LINE.min(*budget);
    let mut buf = vec![];
    let n = r
        .take(cap as u64 + 1)
        .read_until(b'\n', &mut buf)
        .map_err(|e| e.kind())?;
    if n > cap {
        return Err(HeadErr::TooLarge(if *budget < MAX_LINE {
            431
        } else {
            code
        }));
    }
    *budget -= n;
    String::from_utf8(buf).map_err(|_| HeadErr::Io(ErrorKind::InvalidData))
}

fn read_head(r: &mut impl BufRead) -> Result<Head, HeadErr> {
    let mut budget = MAX_HEAD;
    let first = head_line(r, &mut budget, 414)?;
    let mut parts = first.split_whitespace();
    let bad = ErrorKind::InvalidData;
    let method = parts.next().ok_or(bad)?.to_string();
    let target = parts.next().ok_or(bad)?.to_string();
    let mut len = None;
    let mut chunked = false;
    let mut headers = vec![];
    loop {
        let h = head_line(r, &mut budget, 431)?;
        if h.is_empty() || h.trim().is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(HeadErr::TooLarge(431));
        }
        if let Some((k, v)) = h.split_once(':') {
            let k = k.trim().to_ascii_lowercase();
            if k == "content-length" {
                len = Some(v.trim().parse().map_err(|_| bad)?);
            }
            chunked |= k == "transfer-encoding";
            headers.push((k, v.trim().to_string()));
        }
    }
    let (path, q) = target.split_once('?').unwrap_or((&target, ""));
    let query = q
        .split('&')
        .filter_map(|kv| kv.split_once('=').or(Some((kv, ""))))
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Ok(Head {
        req: Request {
            method,
            path: path.to_string(),
            query,
            headers,
            body: vec![],
        },
        len,
        chunked,
    })
}

pub fn q<'a>(r: &'a Request, k: &str) -> Option<&'a str> {
    r.query
        .iter()
        .find(|(a, _)| a == k)
        .map(|(_, v)| v.as_str())
}

pub fn header<'a>(r: &'a Request, k: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|(a, _)| a == k)
        .map(|(_, v)| v.as_str())
}

pub fn dry_run(r: &Request) -> bool {
    q(r, "dryRun") != Some("false")
}

fn notes_reply(r: Result<(crate::Line, Value, Vec<Value>), crate::notes::Err>) -> (u16, Value) {
    match r {
        Ok((l, note, notes)) => {
            crate::board::publish(
                "line.updated",
                json!({"line": l.to_json()["line"], "local": {"notes": notes}}),
            );
            (200, json!({"lineId": l.id, "note": note, "notes": notes}))
        }
        Err((code, e)) => err(code, e),
    }
}

fn err(code: u16, msg: impl Into<String>) -> (u16, Value) {
    (code, json!({ "error": msg.into() }))
}

fn res(code: u16, r: Result<Value, (u16, String)>) -> (u16, Value) {
    r.map(|v| (code, v)).unwrap_or_else(|(c, e)| err(c, e))
}

fn term_spec(b: &Value, state_dir: &std::path::Path) -> Result<crate::term::Spec, String> {
    if !b.is_object() && !b.is_null() {
        return Err("JSON object expected".into());
    }
    let s = |k: &str| match &b[k] {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(format!("{k} must be a string")),
    };
    let cmd = match &b["cmd"] {
        Value::Null => vec![],
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or("cmd must be an array of strings")?,
        _ => return Err("cmd must be an array of strings".into()),
    };
    let env = match &b["env"] {
        Value::Null => vec![],
        Value::Object(m) => m
            .iter()
            .map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
            .collect::<Option<Vec<_>>>()
            .ok_or("env values must be strings")?,
        _ => return Err("env must be an object".into()),
    };
    let (cols, rows) = crate::term::size(b)?.unwrap_or((80, 24));
    let mut spec = crate::term::Spec {
        cwd: s("cwd")?.unwrap_or_else(|| crate::home().to_string_lossy().into_owned()),
        cmd,
        env,
        cols,
        rows,
        title: s("title")?,
        line_id: s("lineId")?,
        task_id: s("taskId")?,
        workspace_id: s("workspaceId")?,
        ..Default::default()
    };
    if let Some(id) = s("install")? {
        if !spec.cmd.is_empty() || !b["agent"].is_null() {
            return Err("give either install, agent or cmd".into());
        }
        let a = crate::agents::get(&id).ok_or_else(|| format!("unknown agent {id}"))?;
        spec.cmd = crate::agents::install_argv(a)?;
        spec.shown = a.install_cmd().map(|c| vec![c.to_string()]);
        if spec.title.is_none() {
            spec.title = Some(format!("Install {}", a.name));
        }
    }
    if let Some(id) = s("agent")? {
        if !spec.cmd.is_empty() {
            return Err("give either agent or cmd, not both".into());
        }
        let a = crate::agents::get(&id).ok_or_else(|| format!("unknown agent {id}"))?;
        if !crate::agents::is_installed(a) {
            return Err(format!("{} is not installed ({})", a.name, a.bin));
        }
        let reg = crate::registry::Registry::open(state_dir)?;
        let auto_approve = crate::agents::auto_approve_enabled(&reg, a.id)?;
        let (model, effort) = (
            s("model")?.unwrap_or_default(),
            s("effort")?.unwrap_or_default(),
        );
        if !effort.is_empty() && !a.valid_efforts().contains(&effort.as_str()) {
            return Err(format!(
                "effort must be one of {}",
                a.valid_efforts().join(", ")
            ));
        }
        let applied = crate::profiles::for_launch(
            state_dir,
            a,
            s("profile")?.as_deref().filter(|p| !p.is_empty()),
            &[spec.cwd.as_str()],
            model.trim(),
            &effort,
        )
        .map_err(|(_, e)| e)?;
        let session = a.session.and_then(|_| crate::runner::uuid().ok());
        spec.flags = a.flags(&applied.model, &applied.effort);
        if auto_approve {
            spec.flags
                .extend(crate::agents::auto_approve_flags(a.id, &applied.extra));
        }
        spec.flags.extend(applied.extra.iter().cloned());
        spec.cmd = a.launch(&crate::agents::Launch {
            model: &applied.model,
            effort: &applied.effort,
            session: session.as_deref(),
            prompt: None,
            extra: &applied.extra,
            auto_approve,
        });
        spec.env.extend(applied.env);
        spec.profile = applied.chip;
        spec.session = session;
        if spec.title.is_none() && !a.bin.is_empty() {
            spec.title = Some(a.name.to_string());
        }
    }
    Ok(spec)
}

fn in_worktree(ctx: &Ctx, body: &Value, spec: &mut crate::term::Spec) -> Result<(), (u16, Value)> {
    let opt = match &body["worktree"] {
        Value::Null | Value::Bool(false) => return Ok(()),
        Value::Bool(true) => json!({}),
        o @ Value::Object(_) => o.clone(),
        _ => return Err(err(400, "worktree must be true or an object")),
    };
    let agent = body["agent"]
        .as_str()
        .map(crate::agents::id_of)
        .or_else(|| crate::agents::of_argv(&spec.cmd).map(|a| a.id))
        .unwrap_or("shell");
    spec.cwd =
        crate::worktrees::for_term(ctx, &spec.cwd, agent, &opt).map_err(|(c, e)| err(c, e))?;
    Ok(())
}

fn terms(r: &Request, segs: &[&str], body: &Value, ctx: &Ctx) -> Option<(u16, Value)> {
    use crate::term;
    let found = |id: &str| term::get(id).ok_or_else(|| (404, format!("no term {id}")));
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "terms"]) => (
            200,
            Value::Array(term::all().iter().map(|t| t.to_json()).collect()),
        ),
        ("POST", ["v1", "terms"]) => {
            let started = match crate::workspaces::start_body(ctx, body) {
                Ok(b) => b,
                Err((c, e)) => return Some(err(c, e)),
            };
            let body = &started;
            match term_spec(body, &ctx.state_dir)
                .map_err(|e| err(400, e))
                .and_then(|mut s| {
                    in_worktree(ctx, body, &mut s)?;
                    Ok(s)
                }) {
                Ok(spec) => match term::spawn(spec) {
                    Ok(t) => (201, t.to_json()),
                    Err(e) => err(400, e),
                },
                Err(e) => e,
            }
        }
        ("GET", ["v1", "terms", id]) => res(200, found(id).map(|t| t.to_json())),
        ("POST", ["v1", "terms", id, "resize"]) => res(
            200,
            found(id).and_then(|t| {
                let (c, rw) = term::size(body)
                    .and_then(|s| s.ok_or_else(|| "cols and rows required".into()))
                    .map_err(|e| (400, e))?;
                if t.exited() {
                    return Err((409, "term has exited".into()));
                }
                t.resize(c, rw, term::client_kind(&body["client"]))
                    .map_err(|e| (500, e))?;
                Ok(t.to_json())
            }),
        ),
        ("PUT", ["v1", "terms", id, "title"]) => res(
            200,
            found(id).and_then(|t| {
                let raw = body
                    .get("title")
                    .filter(|v| v.is_null() || v.is_string())
                    .ok_or((400, "title (string or null) required".into()))?;
                t.set_title(crate::actions::clean_title(raw.as_str()));
                Ok(t.to_json())
            }),
        ),
        ("POST", ["v1", "terms", id, "kill"]) => res(
            202,
            found(id).and_then(|t| {
                if body["confirm"] != true {
                    return Err((400, "kill needs {\"confirm\":true}".into()));
                }
                if t.exited() {
                    return Err((409, "term has exited".into()));
                }
                t.kill().map_err(|e| (500, e))?;
                Ok(t.to_json())
            }),
        ),
        ("DELETE", ["v1", "terms", id]) => res(
            200,
            found(id).and_then(|t| {
                if !t.exited() {
                    return Err((409, "term is still running; kill it first".into()));
                }
                term::remove(id);
                Ok(json!({"deleted": id}))
            }),
        ),
        _ => return None,
    })
}

fn tasks(r: &Request, segs: &[&str], body: &Value, ctx: &Ctx) -> Option<(u16, Value)> {
    use crate::tasks;
    let st = ctx.config.station_id.as_str();
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "tasks"]) => {
            let project = q(r, "project").map(crate::board::decode);
            let status = q(r, "status").map(crate::board::decode);
            let wanted: Vec<&str> = status
                .as_deref()
                .map(|s| s.split(',').collect())
                .unwrap_or_default();
            res(
                200,
                crate::registry::Registry::open(&ctx.state_dir)
                    .map_err(|e| (500, e))
                    .and_then(|reg| tasks::all(&reg))
                    .map(|ts| {
                        ts.iter()
                            .filter(|t| project.as_ref().is_none_or(|p| *p == t.project))
                            .filter(|t| wanted.is_empty() || wanted.contains(&t.status.as_str()))
                            .map(|t| t.to_json(st))
                            .collect()
                    }),
            )
        }
        ("GET", ["v1", "tasks", "auto-organize"]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .map(|reg| {
                    json!({"enabled": ctx.config.auto_organize
                        || crate::task_agents::auto_organize_enabled(&reg)})
                }),
        ),
        ("PUT", ["v1", "tasks", "auto-organize"]) => match body["enabled"].as_bool() {
            None => (400, json!({"error": "enabled must be a boolean"})),
            Some(on) => res(
                200,
                crate::registry::Registry::open(&ctx.state_dir)
                    .map_err(|e| (500, e))
                    .and_then(|reg| {
                        crate::task_agents::set_auto_organize(&reg, on).map_err(|e| (500, e))
                    })
                    .map(|_| json!({"enabled": on})),
            ),
        },
        ("GET", ["v1", "tasks", "agent-messaging"]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .map(|reg| json!({"enabled": crate::task_agents::agent_messaging_enabled(&reg)})),
        ),
        ("PUT", ["v1", "tasks", "agent-messaging"]) => match body["enabled"].as_bool() {
            None => (400, json!({"error": "enabled must be a boolean"})),
            Some(on) => res(
                200,
                crate::registry::Registry::open(&ctx.state_dir)
                    .map_err(|e| (500, e))
                    .and_then(|reg| {
                        crate::task_agents::set_agent_messaging(&reg, on).map_err(|e| (500, e))
                    })
                    .map(|_| json!({"enabled": on})),
            ),
        },
        ("GET", ["v1", "tasks", "agent-tasks"]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .map(|reg| json!({"enabled": crate::task_agents::agent_tasks_enabled(&reg)})),
        ),
        ("PUT", ["v1", "tasks", "agent-tasks"]) => match body["enabled"].as_bool() {
            None => (400, json!({"error": "enabled must be a boolean"})),
            Some(on) => res(
                200,
                crate::registry::Registry::open(&ctx.state_dir)
                    .map_err(|e| (500, e))
                    .and_then(|reg| {
                        crate::task_agents::set_agent_tasks(&reg, on).map_err(|e| (500, e))
                    })
                    .map(|_| json!({"enabled": on})),
            ),
        },
        ("POST", ["v1", "tasks"]) => res(201, tasks::create(ctx, body).map(|t| t.to_json(st))),
        ("GET", ["v1", "tasks", id]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .and_then(|reg| tasks::get(&reg, id))
                .map(|t| t.to_json(st)),
        ),
        ("PATCH", ["v1", "tasks", id]) => {
            res(200, tasks::patch(ctx, id, body).map(|t| t.to_json(st)))
        }
        ("DELETE", ["v1", "tasks", id]) => res(200, tasks::delete(ctx, id)),
        ("POST", ["v1", "tasks", id, "start"]) => res(
            200,
            crate::term::size(body)
                .map_err(|e| (400, e))
                .and_then(|size| tasks::start_with(ctx, id, size, body["inPlace"] == true))
                .map(|t| t.to_json(st)),
        ),
        ("POST", ["v1", "tasks", "organize"]) => res(
            200,
            crate::task_agents::organize(ctx, body["project"].as_str().filter(|p| !p.is_empty()))
                .map(|ts| json!({"started": ts.iter().map(|t| t.to_json(st)).collect::<Vec<_>>()})),
        ),
        ("GET", ["v1", "tasks", id, "messages"]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .and_then(|reg| {
                    tasks::get(&reg, id)?;
                    crate::task_agents::messages(&reg, id)
                })
                .map(|ms| Value::from(ms.iter().map(|m| m.to_json()).collect::<Vec<_>>())),
        ),
        ("POST", ["v1", "tasks", id, "messages"]) => res(
            201,
            crate::task_agents::send_message(ctx, id, body).map(|m| m.to_json()),
        ),
        ("GET", ["v1", "tasks", id, "landed"]) => res(200, crate::worktrees::task_landed(ctx, id)),
        ("GET", ["v1", "tasks", id, "overlaps"]) => {
            res(200, crate::task_agents::overlaps(ctx, id).map(Value::from))
        }
        ("POST", ["v1", "tasks", id, "votes"]) => res(
            200,
            crate::quorum::vote(ctx, id, body).map(|t| t.to_json(st)),
        ),
        ("POST", ["v1", "tasks", id, "review"]) => res(200, tasks::review(ctx, id, body)),
        ("GET", ["v1", "projects"]) => res(
            200,
            crate::current_lines(ctx)
                .map_err(|e| (500, e))
                .and_then(|lines| tasks::projects(ctx, &lines))
                .map(Value::from),
        ),
        ("POST", ["v1", "projects"]) => match tasks::add_project(ctx, body) {
            Ok((created, p)) => {
                if created {
                    crate::board::publish("project.added", json!({"project": p}));
                    let _ = crate::workspaces::list(ctx);
                }
                (if created { 201 } else { 200 }, p)
            }
            Err((c, e)) => err(c, e),
        },
        ("POST", ["v1", "projects", "discover"]) => res(200, crate::discover::run(ctx)),
        ("GET", ["v1", "agents"]) => (200, crate::agents::list()),
        ("POST", ["v1", "agents", "rescan"]) => (200, crate::agents::rescan()),
        ("GET", ["v1", "agents", "settings"]) => res(
            200,
            crate::registry::Registry::open(&ctx.state_dir)
                .map_err(|e| (500, e))
                .and_then(|reg| crate::agents::auto_approve_settings(&reg).map_err(|e| (500, e))),
        ),
        ("PUT", ["v1", "agents", "settings"]) => {
            let (Some(agent), Some(enabled)) =
                (body["agent"].as_str(), body["autoApprove"].as_bool())
            else {
                return Some(err(400, "agent and autoApprove are required"));
            };
            res(
                200,
                crate::registry::Registry::open(&ctx.state_dir)
                    .map_err(|e| (500, e))
                    .and_then(|reg| {
                        crate::agents::set_auto_approve(&reg, agent, enabled)
                            .map_err(|e| (400, e))?;
                        crate::agents::auto_approve_settings(&reg).map_err(|e| (500, e))
                    }),
            )
        }
        ("GET", ["v1", "env"]) => (200, crate::shellenv::to_json()),
        _ => return None,
    })
}

fn resume_term(ctx: &Ctx, r: &Request, id: &str, body: &Value) -> (u16, Value) {
    if crate::board::remote_station(ctx, id).is_some() {
        return err(400, "target term needs a line of this station");
    }
    let size = match crate::term::size(body) {
        Ok(s) => s.unwrap_or((120, 32)),
        Err(e) => return err(400, e),
    };
    let lines = match scan(ctx) {
        Ok(l) => l,
        Err(e) => return err(500, e),
    };
    let l = match actions::find_line(&lines, id) {
        Ok(l) => l,
        Err(e) => return err(404, e),
    };
    let attach_only = body["attachOnly"] == true;
    if let Some(t) = actions::pending_terms(l).into_iter().next() {
        if !attach_only && q(r, "dryRun") != Some("true") && actions::waits_for_enter(l, &t) {
            if let Err(e) = t.input(b"\r") {
                return err(409, e.to_string());
            }
            if let Ok(reg) = crate::registry::Registry::open(&ctx.state_dir) {
                let _ = reg.set_mark(&l.id, "held_by_user", None);
            }
        }
        return (200, t.to_json());
    }
    if l.pid.is_some() {
        if let Some(t) = l.term_id.as_deref().and_then(crate::term::get) {
            return (200, t.to_json());
        }
        if let Some(pane) = l.pane.as_deref() {
            return attach_pane(ctx, l, pane, size, q(r, "dryRun") == Some("true"));
        }
    }
    if attach_only {
        return err(409, "the agent is not running: resume it to start it again");
    }
    let mut plan = match actions::resume_plan(ctx, &lines, l, actions::Launch::Detached) {
        Ok(p) => p,
        Err(e) => return err(409, e),
    };
    plan.r#where = "term";
    plan.pane = None;
    if q(r, "dryRun") == Some("true") {
        return (200, plan.to_json());
    }
    let applied = crate::agents::get(l.agent)
        .map(|a| crate::profiles::for_resume(&ctx.state_dir, a, &plan.cwd))
        .unwrap_or_default();
    let t = crate::term::spawn(crate::term::Spec {
        cwd: plan.cwd.clone(),
        cmd: plan.argv.clone(),
        env: applied.env,
        cols: size.0,
        rows: size.1,
        title: l.title.clone(),
        line_id: Some(l.id.clone()),
        session: Some(l.session_id.clone()),
        flags: l.launch_args.clone(),
        profile: applied.chip,
        ..Default::default()
    });
    match t {
        Ok(t) => {
            if let Ok(reg) = crate::registry::Registry::open(&ctx.state_dir) {
                let _ = reg.set_mark(&l.id, "held_by_user", None);
                let _ = reg.set_mark(&l.id, "closed_at", None);
            }
            actions::log_action(
                ctx,
                &json!({"action": "resume", "dryRun": false, "result": plan.to_json()}),
            );
            (201, t.to_json())
        }
        Err(e) => err(409, e),
    }
}

fn attach_pane(
    ctx: &Ctx,
    l: &crate::Line,
    pane: &str,
    size: (u16, u16),
    dry: bool,
) -> (u16, Value) {
    let tmux = ctx.tmux.clone().unwrap_or_default();
    let Some(session) = tmux.session_of(pane) else {
        return err(409, format!("tmux pane {pane} is gone"));
    };
    let mut cmd = vec!["tmux".to_string()];
    if let Some(s) = &tmux.socket {
        cmd.extend(["-L".to_string(), s.clone()]);
    }
    cmd.extend([
        "attach-session".to_string(),
        "-t".to_string(),
        session.clone(),
    ]);
    if dry {
        return (
            200,
            json!({"lineId": l.id, "argv": cmd, "cwd": l.cwd, "where": "running", "pane": pane, "session": session}),
        );
    }
    let _ = tmux.ok(&["select-window", "-t", pane]);
    let _ = tmux.ok(&["select-pane", "-t", pane]);
    let cwd = if l.cwd_missing {
        crate::home().to_string_lossy().into_owned()
    } else {
        l.cwd.clone()
    };
    match crate::term::spawn(crate::term::Spec {
        cwd,
        cmd,
        cols: size.0,
        rows: size.1,
        title: l.title.clone(),
        line_id: Some(l.id.clone()),
        ..Default::default()
    }) {
        Ok(t) => (201, t.to_json()),
        Err(e) => err(409, e),
    }
}

pub fn cached_lines(ctx: &Ctx, want: Option<&str>) -> Result<Vec<crate::Line>, String> {
    let cached = crate::current_lines(ctx)?;
    match want {
        Some(id) if actions::find_line(&cached, id).is_err() && crate::scanner::shared() => {
            scan(ctx)
        }
        _ => Ok(cached),
    }
}

fn activity(ctx: &Ctx, r: &Request, id: &str) -> (u16, Value) {
    let lines = match cached_lines(ctx, Some(id)) {
        Ok(l) => l,
        Err(e) => return err(500, e),
    };
    let l = match actions::find_line(&lines, id) {
        Ok(l) => l,
        Err(e) => return err(404, e),
    };
    let since = match q(r, "since").map(crate::board::decode) {
        None => None,
        Some(s) => match crate::util::parse_rfc3339(&s) {
            Some(ms) => Some(ms),
            None => return err(400, "since must be an RFC 3339 time"),
        },
    };
    let path = crate::activity::find_transcript(
        &ctx.claude_dir,
        &ctx.codex_dir,
        &ctx.gemini_dir,
        l.agent,
        &l.session_id,
    );
    let a = path
        .as_deref()
        .and_then(|p| crate::activity::activity(l.agent, p, since));
    let cost = path
        .as_deref()
        .and_then(|p| crate::activity::spend(l.agent, p))
        .map(|s| crate::activity::spend_json(&s, &l.launch_args));
    let transcript = a.is_some();
    let mut a = a.unwrap_or_else(
        || json!({"workLog": [], "todos": [], "editedFiles": [], "lastAssistantText": ""}),
    );
    a["transcript"] = json!(transcript);
    if let Some(c) = cost {
        a["cost"] = c;
    }
    if let Some(t) = &l.title {
        a["title"] = json!(t);
    }
    for k in ["title", "goal", "lastAssistantText"] {
        if a[k].is_null() {
            a[k] = json!("");
        }
    }
    a["lineId"] = json!(l.id);
    a["status"] = json!(l.state);
    a["busy"] = json!(l.busy);
    a["needsInput"] = json!(l.needs_input);
    (200, a)
}

fn chat_line(ctx: &Ctx, id: &str) -> Result<crate::Line, (u16, Value)> {
    let lines = cached_lines(ctx, Some(id)).map_err(|e| err(500, e))?;
    actions::find_line(&lines, id)
        .cloned()
        .map_err(|e| err(404, e))
}

fn chat_query(r: &Request) -> Result<crate::chat::Query, (u16, Value)> {
    let num = |k: &'static str| q(r, k).map(|v| v.parse::<u64>().map_err(|_| k));
    let cursor = match (num("after"), num("before")) {
        (Some(Ok(a)), None) => crate::chat::Cursor::After(a),
        (None, Some(Ok(b))) => crate::chat::Cursor::Before(b),
        (None, None) => crate::chat::Cursor::Tail,
        (Some(Err(k)), _) | (_, Some(Err(k))) => {
            return Err(err(400, format!("{k} must be a non-negative integer")))
        }
        (Some(Ok(_)), Some(Ok(_))) => return Err(err(400, "give after or before, not both")),
    };
    let limit = match num("limit") {
        None => crate::chat::DEFAULT_LIMIT,
        Some(Ok(n)) => n as usize,
        Some(Err(k)) => return Err(err(400, format!("{k} must be a non-negative integer"))),
    };
    Ok(crate::chat::Query {
        cursor,
        limit,
        live: None,
    })
}

fn chat(ctx: &Ctx, r: &Request, id: &str, agent: Option<&str>) -> (u16, Value) {
    let l = match chat_line(ctx, id) {
        Ok(l) => l,
        Err(e) => return e,
    };
    if !crate::chat::supported(l.agent) {
        return err(409, format!("no chat view for agent {}", l.agent));
    }
    let query = match chat_query(r) {
        Ok(q) => crate::chat::Query {
            live: Some(l.alive()),
            ..q
        },
        Err(e) => return e,
    };
    let path = crate::activity::find_transcript(
        &ctx.claude_dir,
        &ctx.codex_dir,
        &ctx.gemini_dir,
        l.agent,
        &l.session_id,
    );
    let page = match agent {
        None => path.and_then(|p| crate::chat::page(l.agent, &p, query)),
        Some(a) => {
            let page = path
                .filter(|_| {
                    crate::agents::transcript_of(l.agent) == Some(crate::agents::Transcript::Claude)
                })
                .and_then(|p| crate::chat::agent_page(&p, a, query));
            if page.is_none() {
                return err(404, format!("no sub-agent {a} for line {}", l.id));
            }
            page
        }
    };
    match page {
        Some(mut v) => {
            v["lineId"] = json!(l.id);
            (200, v)
        }
        None => err(409, format!("no transcript for line {}", l.id)),
    }
}

fn chat_image(ctx: &Ctx, r: &Request, s: &mut impl Write) -> bool {
    let segs: Vec<&str> = r.path.trim_matches('/').split('/').collect();
    let ("GET", ["v1", "lines", id, "chat", "image"]) = (r.method.as_str(), segs.as_slice()) else {
        return false;
    };
    let res = chat_line(ctx, id).and_then(|l| {
        let num = |k: &str| q(r, k).and_then(|v| v.parse::<u64>().ok());
        let (Some(at), Some(i)) = (num("at"), num("i")) else {
            return Err(err(400, "at and i must be non-negative integers"));
        };
        let path = crate::activity::find_transcript(
            &ctx.claude_dir,
            &ctx.codex_dir,
            &ctx.gemini_dir,
            l.agent,
            &l.session_id,
        )
        .ok_or_else(|| err(409, format!("no transcript for line {}", l.id)))?;
        crate::chat::image(&path, at, i as usize).map_err(|(c, e)| err(c, e))
    });
    match res {
        Ok((mime, bytes)) => respond_raw(s, 200, &mime, &bytes),
        Err((code, v)) => respond(s, code, &v),
    }
    true
}

fn commands(ctx: &Ctx, id: &str) -> (u16, Value) {
    match chat_line(ctx, id) {
        Ok(l) => (
            200,
            crate::chat::commands(l.agent, &ctx.claude_dir, &ctx.codex_dir, &l.cwd),
        ),
        Err(e) => e,
    }
}

pub fn handle(ctx: &Ctx, r: &Request) -> (u16, Value) {
    let segs: Vec<&str> = r.path.trim_matches('/').split('/').collect();
    let body: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
    if let ("POST", ["v1", "lines", id, "resume"]) = (r.method.as_str(), segs.as_slice()) {
        if body["target"] == "term" {
            return resume_term(ctx, r, id, &body);
        }
    }
    if let ("POST", ["v1", "lines", id, kind @ ("hold" | "close" | "resume" | "link")]) =
        (r.method.as_str(), segs.as_slice())
    {
        if let Some((station, online, seen)) = crate::board::remote_station(ctx, id) {
            if !online {
                return err(
                    409,
                    format!(
                        "station {station} offline since {}",
                        crate::util::format_rfc3339(seen)
                    ),
                );
            }
            let mut args = json!({ "dryRun": *kind != "link" && dry_run(r) });
            if *kind == "link" {
                let Some(k) = body["ticketKey"].as_str() else {
                    return err(400, "ticketKey required");
                };
                args["ticketKey"] = json!(k);
            }
            return crate::remote::relay(ctx, id, kind, args);
        }
    }
    if let Some(v) = terms(r, &segs, &body, ctx)
        .or_else(|| tasks(r, &segs, &body, ctx))
        .or_else(|| crate::review::routes(r, &segs, &body, ctx))
        .or_else(|| crate::workspaces::route(r, &segs, &body, ctx))
        .or_else(|| crate::gitgraph::routes(ctx, r, &segs, &body))
        .or_else(|| crate::gitops::routes(ctx, r, &segs, &body))
        .or_else(|| crate::worktrees::routes(ctx, &r.method, &segs, &body))
        .or_else(|| crate::install::routes(&r.method, &segs, &body, &ctx.state_dir))
        .or_else(|| crate::statusline::routes(&r.method, &segs, &body, &ctx.state_dir))
        .or_else(|| crate::uninstall::routes(ctx, &r.method, &segs, &body))
        .or_else(|| crate::import::routes(&r.method, &segs, &body, &ctx.state_dir))
        .or_else(|| crate::lifecycle::routes(&r.method, &segs, &body, &ctx.state_dir))
        .or_else(|| crate::termtheme::routes(&r.method, &segs, &body))
        .or_else(|| crate::power::routes(&r.method, &segs, &body, &ctx.state_dir))
        .or_else(|| crate::feedback::routes(ctx, &r.method, &segs, &body))
        .or_else(|| crate::github::routes(ctx, r, &segs, &body))
        .or_else(|| crate::profiles::routes(ctx, r, &segs, &body))
        .or_else(|| crate::organize::routes(ctx, r, &segs, &body))
        .or_else(|| crate::task_sources::routes(ctx, r, &segs, &body))
    {
        return v;
    }
    match (r.method.as_str(), segs.as_slice()) {
        ("GET", ["v1", "version"]) => (200, crate::upgrade::version_json()),
        ("GET", ["v1", "logs"]) => (200, crate::dlog::api(&ctx.state_dir, q(r, "tail"))),
        ("POST", ["v1", "upgrade"]) => err(501, "hot upgrade is not supported on this platform"),
        ("GET", ["v1", "ports"]) => (200, crate::ports::list()),
        ("POST", ["v1", "ports", port, "kill"]) => res(202, crate::ports::kill(port, &body)),
        ("GET", ["v1", "usage"]) => crate::usage::api(r, &lines(ctx)),
        ("GET", ["v1", "lines", id, "activity"]) => activity(ctx, r, id),
        ("GET", ["v1", "lines", id, "chat"]) => chat(ctx, r, id, None),
        ("GET", ["v1", "lines", id, "chat", "agents", agent]) => chat(ctx, r, id, Some(agent)),
        ("GET", ["v1", "lines", id, "commands"]) => commands(ctx, id),
        ("GET", ["v1", "lines", id, "screen"]) => crate::screen::screen_route(ctx, id),
        ("POST", ["v1", "lines", id, "keys"]) => crate::screen::keys_route(ctx, id, &body),
        ("GET", ["v1", "status"]) => match crate::current_lines(ctx) {
            Ok(lines) => {
                let mut v = actions::status(ctx, &lines);
                v["daemon"] = json!(true);
                (200, v)
            }
            Err(e) => err(500, e),
        },
        ("GET", ["v1", "lines"]) => match crate::current_lines(ctx) {
            Ok(lines) => {
                let all = q(r, "all") == Some("true");
                let tk = q(r, "ticketKey").and_then(ticket);
                let repo = q(r, "repo").map(str::to_ascii_lowercase);
                let state = q(r, "state");
                let v: Vec<Value> = lines
                    .iter()
                    .filter(|l| all || l.recent(ctx.now_ms))
                    .filter(|l| tk.is_none() || l.ticket == tk)
                    .filter(|l| {
                        repo.as_ref()
                            .is_none_or(|x| l.repo.to_ascii_lowercase().contains(x))
                    })
                    .filter(|l| state.is_none_or(|s| l.state == s))
                    .map(|l| l.to_json())
                    .collect();
                (200, Value::Array(v))
            }
            Err(e) => err(500, e),
        },
        ("POST", ["v1", "lines", id, "hold"]) => {
            let held = if q(r, "afterTurn") == Some("true") {
                actions::hold_after_turn(ctx, id, !dry_run(r))
            } else {
                actions::hold(ctx, id, !dry_run(r))
            };
            match held {
                Ok(rep) => (if rep.ok() { 200 } else { 409 }, rep.to_json()),
                Err(e) => err(404, e),
            }
        }
        ("DELETE", ["v1", "lines", id, "hold"]) => match actions::cancel_pending_hold(ctx, id) {
            Ok(id) => (200, json!({"lineId": id, "holdPending": false})),
            Err((code, e)) => err(code, e),
        },
        ("POST", ["v1", "lines", id, "close"]) => {
            match actions::close(
                ctx,
                id,
                !dry_run(r),
                q(r, "shell") == Some("true"),
                q(r, "force") == Some("true"),
            ) {
                Ok(rep) => (if rep.ok() { 200 } else { 409 }, rep.to_json()),
                Err(e) => err(404, e),
            }
        }
        ("POST", ["v1", "lines", id, "resume"]) => {
            match actions::resume(ctx, id, dry_run(r), actions::Launch::Detached) {
                Ok(p) => (200, p.to_json()),
                Err(e) => err(409, e),
            }
        }
        ("POST", ["v1", "lines", id, "link"]) => {
            let Some(k) = body["ticketKey"].as_str() else {
                return err(400, "ticketKey required");
            };
            match actions::link(ctx, id, k) {
                Ok((id, t)) => (200, json!({"lineId": id, "ticketKey": t})),
                Err(e) => err(400, e),
            }
        }
        ("PUT", ["v1", "lines", id, "pin"]) => {
            let Some(p) = body["pinned"].as_bool() else {
                return err(400, "pinned required");
            };
            match actions::pin(ctx, id, p) {
                Ok(id) => (200, json!({"lineId": id, "pinned": p})),
                Err(e) => err(400, e),
            }
        }
        ("PUT", ["v1", "lines", id, "title"]) => {
            let Some(raw) = body.get("title").filter(|v| v.is_null() || v.is_string()) else {
                return err(400, "title (string or null) required");
            };
            let title = actions::clean_title(raw.as_str());
            match actions::set_title(ctx, id, title.as_deref()) {
                Ok(l) => {
                    crate::board::publish(
                        "line.updated",
                        json!({"line": l.to_json()["line"], "local": {"titleOverride": title}}),
                    );
                    (200, json!({"lineId": l.id, "titleOverride": title}))
                }
                Err((code, e)) => err(code, e),
            }
        }
        ("POST", ["v1", "lines", id, "notes"]) => {
            let Some(text) = body["text"].as_str() else {
                return err(400, "text (string) required");
            };
            let Some(text) = crate::notes::clean_note(Some(text)) else {
                return err(400, "note text required");
            };
            let now = now_ms();
            notes_reply(actions::notes(ctx, id, |n| {
                crate::notes::add(n, &text, now)
            }))
        }
        ("PUT", ["v1", "lines", id, "notes", note]) => {
            let Some(text) = body["text"].as_str() else {
                return err(400, "text (string) required");
            };
            let Some(text) = crate::notes::clean_note(Some(text)) else {
                return err(400, "note text required");
            };
            let now = now_ms();
            notes_reply(actions::notes(ctx, id, |n| {
                crate::notes::edit(n, note, &text, now)
            }))
        }
        ("DELETE", ["v1", "lines", id, "notes", note]) => {
            notes_reply(actions::notes(ctx, id, |n| {
                crate::notes::remove(n, note).map(|_| Value::Null)
            }))
        }
        ("POST", ["v1", "hook-events"]) => {
            let (Some(agent), Some(event)) = (body["agent"].as_str(), body["event"].as_str())
            else {
                return err(400, "agent and event required");
            };
            if !crate::agents::task_agent(agent) {
                return err(400, "agent must be a known coding agent");
            }
            let event = crate::hooks::canonical(event, body["notificationType"].as_str());
            let mut rec = json!({"agent": agent, "event": event, "at": crate::util::parse_rfc3339(body["at"].as_str().unwrap_or("")).unwrap_or(ctx.now_ms)});
            for (k, src) in [
                ("sessionId", "sessionId"),
                ("cwd", "cwd"),
                ("tmuxPane", "tmuxPane"),
                ("termId", "termId"),
                ("notificationType", "notificationType"),
            ] {
                if let Some(v) = body[src].as_str() {
                    rec[k] = json!(v);
                }
            }
            if let Some(p) = body["pid"].as_u64() {
                rec["pid"] = json!(p);
            }
            match crate::hooks::append_spool(&ctx.state_dir, &rec) {
                Ok(()) => (202, json!({"accepted": true})),
                Err(e) => err(500, e.to_string()),
            }
        }
        _ => err(404, "not found"),
    }
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Content",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        504 => "Gateway Timeout",
        _ => "Internal Server Error",
    }
}

pub fn respond_raw(s: &mut (impl Write + ?Sized), code: u16, ctype: &str, body: &[u8]) {
    write_response(s, code, ctype, "", body)
}

fn write_response(s: &mut (impl Write + ?Sized), code: u16, ctype: &str, extra: &str, body: &[u8]) {
    let _ = write!(
        s,
        "HTTP/1.1 {code} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n{extra}Connection: close\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = s.write_all(body);
}

pub(crate) fn respond(s: &mut impl Write, code: u16, v: &Value) {
    respond_raw(
        s,
        code,
        "application/json",
        &serde_json::to_vec(v).unwrap_or_default(),
    );
}

fn upload(r: &mut BufReader<&mut impl Stream>, h: &Head) -> (u16, Value) {
    let res = crate::uploads::admit(h.len, h.chunked).and_then(|len| {
        if header(&h.req, "expect").is_some_and(|e| e.eq_ignore_ascii_case("100-continue")) {
            let _ = r.get_mut().write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
        }
        let _ = r.get_ref().set_read_timeout(Some(Duration::from_secs(30)));
        crate::uploads::receive(&crate::uploads::dir(), q(&h.req, "name"), len, &mut *r)
    });
    res.map(|v| (201, v)).unwrap_or_else(|(c, e)| err(c, e))
}

pub fn conn(mut s: impl Stream, token: Option<&str>) {
    let mut r = BufReader::new(&mut s);
    let head = match read_head(&mut r) {
        Ok(h) => h,
        Err(e) => {
            drop(r);
            return match e {
                HeadErr::Io(ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                    respond(&mut s, 408, &json!({"error": "request head timed out"}))
                }
                HeadErr::TooLarge(code) => respond(
                    &mut s,
                    code,
                    &json!({"error": "request line or headers too large"}),
                ),
                HeadErr::Io(_) => respond(&mut s, 400, &json!({"error": "bad request"})),
            };
        }
    };
    if token.is_some_and(|t| !crate::board::authorized(&head.req, t)) {
        drop(r);
        return respond(
            &mut s,
            403,
            &json!({"error": "token required: open the URL printed by `overseer board`"}),
        );
    }
    if head.req.method == "POST" && head.req.path.trim_matches('/') == "v1/uploads" {
        let (code, v) = upload(&mut r, &head);
        drop(r);
        return respond(&mut s, code, &v);
    }
    let Head { mut req, len, .. } = head;
    let mut body = vec![0; len.unwrap_or(0).min(1 << 20) as usize];
    let read = r.read_exact(&mut body);
    drop(r);
    if read.is_err() {
        return respond(&mut s, 400, &json!({"error": "bad request"}));
    }
    req.body = body;
    let segs: Vec<&str> = req.path.trim_matches('/').split('/').collect();
    if let ("GET", ["v1", "terms", id, "io"]) = (req.method.as_str(), segs.as_slice()) {
        return crate::term::ws(id, &req, s);
    }
    #[cfg(unix)]
    if let ("POST", ["v1", "upgrade"]) = (req.method.as_str(), segs.as_slice()) {
        return crate::upgrade::serve(&req, s);
    }
    let probe = System;
    let ctx = Ctx::from_env(&probe);
    if crate::board::serve(&ctx, &req, &mut s)
        || crate::sysmon::serve(&req, &mut s)
        || crate::files::serve(&ctx, &req, &mut s)
        || chat_image(&ctx, &req, &mut s)
        || crate::fsdirs::serve(&req, &mut s)
    {
        return;
    }
    let (code, v) = handle(&ctx, &req);
    if changes_lines(&req.method, &req.path, code) {
        crate::scanner::poke();
        if settles_lines(&req.method, &req.path) {
            if let Ok(lines) = crate::scan(&ctx) {
                crate::lifecycle::persist(&ctx.state_dir, &lines);
            }
        }
    }
    let extra = if crate::scanner::take_scanning() {
        "X-Overseer-Scanning: 1\r\n"
    } else {
        ""
    };
    write_response(
        &mut s,
        code,
        "application/json",
        extra,
        &serde_json::to_vec(&v).unwrap_or_default(),
    );
    if let Some(state_dir) = crate::uninstall::take_pending() {
        let _ = s.flush();
        std::thread::sleep(Duration::from_secs(1));
        if let Err(e) = crate::uninstall::finish(&state_dir) {
            eprintln!("overseer: uninstall: {e}");
        }
        std::process::exit(0);
    }
}

fn settles_lines(method: &str, path: &str) -> bool {
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    method == "PUT"
        && matches!(
            segs.as_slice(),
            ["v1", "lines", _, "pin" | "title" | "workspace"]
                | ["v1", "terms", _, "title" | "workspace"]
        )
}

fn changes_lines(method: &str, path: &str, code: u16) -> bool {
    if method == "GET" || !(200..300).contains(&code) {
        return false;
    }
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    matches!(
        (method, segs.as_slice()),
        (
            _,
            [
                "v1",
                "lines",
                _,
                "hold" | "close" | "resume" | "link" | "pin" | "title" | "workspace"
            ]
        ) | ("POST", ["v1", "terms"])
            | ("POST", ["v1", "terms", _, "kill"])
            | ("PUT", ["v1", "terms", _, "title" | "workspace"])
            | ("DELETE", ["v1", "terms" | "workspaces", _])
            | ("POST", ["v1", "tasks", _, "start"])
            | ("POST", ["v1", "tasks", "organize"])
            | ("POST", ["v1", "import", "switchboard"])
    )
}

pub(crate) fn accept<S: Stream + 'static>(s: S, token: Option<String>) {
    if std::thread::Builder::new()
        .spawn(move || conn(s, token.as_deref()))
        .is_err()
    {
        std::thread::sleep(ACCEPT_BACKOFF);
    }
}

pub(crate) fn accept_failed(e: &std::io::Error) {
    if !matches!(
        e.kind(),
        std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
    ) {
        std::thread::sleep(ACCEPT_BACKOFF);
    }
}

#[cfg(unix)]
pub fn bind(path: &Path) -> Result<std::os::unix::net::UnixListener, String> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    if UnixStream::connect(path).is_ok() {
        return Err(format!(
            "a daemon is already listening on {}",
            path.display()
        ));
    }
    let _ = std::fs::remove_file(path);
    let l = UnixListener::bind(path).map_err(|e| format!("bind {}: {e}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    Ok(l)
}

#[cfg(unix)]
fn listen(path: &Path, adopt: Option<i32>) -> Result<(), String> {
    use std::os::fd::AsRawFd;
    let l = match adopt {
        Some(fd) => crate::upgrade::adopt_listener(fd)?,
        None => bind(path)?,
    };
    crate::upgrade::set_listener(l.as_raw_fd());
    crate::util::supervise("listener", move || {
        for s in l.incoming() {
            match s {
                Ok(s) => {
                    let _ = s.set_read_timeout(Some(HEAD_TIMEOUT));
                    let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
                    accept(s, None);
                }
                Err(e) => accept_failed(&e),
            }
        }
    });
    Ok(())
}

#[cfg(not(unix))]
fn listen(path: &Path, adopt: Option<i32>) -> Result<(), String> {
    let _ = adopt;
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind: {e}"))?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    let tok = crate::board::token()?;
    let info = json!({"addr": addr.to_string(), "token": tok, "pid": std::process::id()});
    std::fs::write(path, info.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::util::supervise("listener", move || {
        for s in l.incoming() {
            match s {
                Ok(s) => {
                    let _ = s.set_read_timeout(Some(HEAD_TIMEOUT));
                    let _ = s.set_write_timeout(Some(Duration::from_secs(5)));
                    accept(s, Some(tok.clone()));
                }
                Err(e) => accept_failed(&e),
            }
        }
    });
    Ok(())
}

fn lock(state_dir: &Path) -> Result<std::fs::File, String> {
    let path = state_dir.join("daemon.lock");
    let f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => Err(format!(
            "a daemon is already running on {}",
            state_dir.display()
        )),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("lock {}: {e}", path.display())),
    }
}

static WATCH: Mutex<Vec<(String, &'static str, String)>> = Mutex::new(vec![]);
static STATUS: Mutex<Option<String>> = Mutex::new(None);

fn write_status(state_dir: &Path, text: String) {
    let path = state_dir.join("status.json");
    let mut last = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_deref() == Some(text.as_str()) && path.exists() {
        return;
    }
    if std::fs::write(&path, &text).is_ok() {
        *last = Some(text);
    }
}
pub fn lines(ctx: &Ctx) -> Vec<crate::Line> {
    crate::current_lines(ctx).unwrap_or_default()
}

fn watch_transcripts(claude_dir: PathBuf, codex_dir: PathBuf, gemini_dir: PathBuf) {
    let mut paths: std::collections::HashMap<String, (Option<PathBuf>, i64)> = Default::default();
    let mut seen: std::collections::HashMap<String, (u64, i64, bool)> = Default::default();
    let mut quiet = false;
    loop {
        std::thread::sleep(Duration::from_millis(if quiet { 1000 } else { 250 }));
        let lines = WATCH.lock().unwrap_or_else(|e| e.into_inner()).clone();
        quiet = lines.is_empty() || crate::board::subscribers() == 0;
        let now = now_ms();
        paths.retain(|id, _| lines.iter().any(|l| l.0 == *id));
        seen.retain(|id, _| paths.contains_key(id));
        for (id, agent, sid) in lines {
            let p = paths.entry(id.clone()).or_insert((None, 0));
            if p.0.is_none() && now - p.1 >= 2000 {
                *p = (
                    crate::activity::find_transcript(
                        &claude_dir,
                        &codex_dir,
                        &gemini_dir,
                        agent,
                        &sid,
                    ),
                    now,
                );
            }
            let Some(len) = p.0.as_deref().and_then(crate::chat::transcript_stamp) else {
                continue;
            };
            let len = len.wrapping_add(p.0.as_deref().map_or(0, crate::chat::agents_stamp));
            let e = seen.entry(id.clone()).or_insert((len, 0, false));
            if e.0 != len {
                e.0 = len;
                e.2 = true;
            }
            if e.2 && now - e.1 >= 1000 {
                e.1 = now;
                e.2 = false;
                crate::board::publish("line.activity", json!({"lineId": id}));
                if let Some(cursor) = p.0.as_deref().and_then(crate::chat::changed) {
                    crate::board::publish("line.chat", json!({"lineId": id, "cursor": cursor}));
                }
                crate::usage::poke(false);
            }
        }
    }
}

pub fn run(
    interval: Duration,
    once: bool,
    http: Option<&str>,
    takeover: Option<&Path>,
) -> Result<(), String> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    let probe = System;
    let state_dir = crate::default_state_dir();
    crate::mkdir_private(&state_dir).map_err(|e| e.to_string())?;
    let saved = takeover.map(crate::upgrade::load).transpose()?;
    let held = match take_lock(&state_dir, saved.as_ref()) {
        Ok(h) => h,
        Err(_) if saved.is_none() && supervised() && running(&state_dir) => {
            crate::dlog::line(
                "info",
                "overseer daemon: another daemon already serves this user; leaving it running",
            );
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    if saved.is_none() {
        if let Some(pid) = live_other_daemon(&state_dir) {
            return Err(format!(
                "a daemon is already running on {} (pid {pid})",
                state_dir.display()
            ));
        }
    }
    let restarted = saved.is_some();
    let _ = std::fs::write(state_dir.join("daemon.pid"), std::process::id().to_string());
    let _wal_keeper = crate::registry::Registry::open(&state_dir).ok();
    let adopted = crate::upgrade::resume(saved, &state_dir);
    crate::shellenv::init();
    crate::lifecycle::on_start(&state_dir, restarted || once);
    crate::power::on_start(&state_dir);
    let path = socket_path(&state_dir);
    crate::scanner::share();
    listen(&path, adopted)?;
    let _ = std::fs::remove_file(crate::board::url_file(&state_dir));
    let msg = format!("overseer daemon: listening on {}", path.display());
    crate::dlog::line("info", &msg);
    if let Some(addr) = http {
        let url = crate::board::serve_http(addr, &state_dir)?;
        crate::dlog::line("info", &format!("overseer daemon: board on {url}"));
    }
    let sd = state_dir.clone();
    std::thread::spawn(move || crate::install::on_daemon_start(&sd));
    let ctx = Ctx::from_env(&probe);
    let (cd, xd, gd) = (
        ctx.claude_dir.clone(),
        ctx.codex_dir.clone(),
        ctx.gemini_dir.clone(),
    );
    crate::usage::init(&state_dir, cd.clone(), xd.clone(), gd.clone());
    crate::github::resume_pending(&state_dir);
    crate::util::supervise("transcripts", move || {
        watch_transcripts(cd.clone(), xd.clone(), gd.clone())
    });
    crate::util::supervise("gitgraph", crate::gitgraph::watch_loop);
    let sd = state_dir.clone();
    crate::util::supervise("review-watch", move || {
        crate::review::watch::run(sd.clone())
    });
    crate::util::supervise("ports", crate::ports::watch);
    if !once {
        let ld = state_dir.clone();
        crate::util::supervise("log-rotate", move || {
            crate::dlog::rotate_forever(ld.clone())
        });
    }
    let mut chores = 0;
    loop {
        if !once && now_ms() >= chores {
            chores = now_ms() + 600_000;
            crate::uploads::cleanup(
                &crate::uploads::dir(),
                &state_dir.join("uploads"),
                std::time::SystemTime::now(),
            );
            crate::discover::background();
        }
        let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let ctx = Ctx::from_env(&probe);
            crate::inbox::process(&ctx);
            match scan(&ctx) {
                Ok(lines) => {
                    let st = actions::status(&ctx, &lines);
                    write_status(&state_dir, st.to_string());
                    crate::board::observe(&ctx, &lines);
                    crate::term::link_lines(&lines);
                    crate::term::trim_exited(ctx.now_ms);
                    crate::lifecycle::persist(&state_dir, &lines);
                    actions::apply_pending_holds(&ctx, &lines);
                    crate::tasks::tick(&ctx, &lines);
                    crate::review::tick(&ctx, &lines);
                    crate::broadcast::tick(&ctx, &lines);
                    *WATCH.lock().unwrap_or_else(|e| e.into_inner()) = lines
                        .iter()
                        .filter(|l| l.alive())
                        .map(|l| (l.id.clone(), l.agent, l.session_id.clone()))
                        .collect();
                    if crate::scanner::take_cold_served() {
                        for l in lines.iter().filter(|l| l.recent(ctx.now_ms)) {
                            crate::board::publish("line.created", l.to_json());
                        }
                    }
                }
                Err(e) => crate::dlog::line("error", &format!("overseer daemon: scan failed: {e}")),
            }
        }));
        if let Err(p) = ticked {
            crate::dlog::line(
                "error",
                &format!(
                    "overseer daemon: scan tick panicked ({}); continuing",
                    crate::util::panic_text(p.as_ref())
                ),
            );
        }
        if once {
            break;
        }
        crate::scanner::wait_poke(interval);
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(crate::board::url_file(&state_dir));
    drop(held);
    Ok(())
}

#[cfg(unix)]
fn take_lock(
    state_dir: &Path,
    saved: Option<&crate::upgrade::State>,
) -> Result<std::fs::File, String> {
    use std::os::fd::AsRawFd;
    let f = match saved {
        Some(st) => crate::upgrade::adopt_lock(st)?,
        None => lock(state_dir)?,
    };
    crate::upgrade::set_lock(f.as_raw_fd());
    Ok(f)
}

#[cfg(not(unix))]
fn take_lock(
    state_dir: &Path,
    saved: Option<&crate::upgrade::State>,
) -> Result<std::fs::File, String> {
    if saved.is_some() {
        return Err("hot upgrade is not supported on this platform".into());
    }
    lock(state_dir)
}

#[cfg(unix)]
fn live_other_daemon(state_dir: &Path) -> Option<i32> {
    let pid: i32 = std::fs::read_to_string(state_dir.join("daemon.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|p| *p > 1 && *p != std::process::id() as i32)?;
    (unsafe { libc::kill(pid, 0) } == 0 && command_line(pid).is_some_and(|c| looks_like_daemon(&c)))
        .then_some(pid)
}

#[cfg(not(unix))]
fn live_other_daemon(_: &Path) -> Option<i32> {
    None
}

pub fn looks_like_daemon(cmdline: &str) -> bool {
    let mut words = cmdline.split_whitespace();
    let named = words.next().is_some_and(|w| {
        Path::new(w)
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("overseer"))
    });
    named && words.any(|w| w == "daemon")
}

#[cfg(unix)]
fn command_line(pid: i32) -> Option<String> {
    if cfg!(target_os = "linux") {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        return Some(String::from_utf8_lossy(&raw).replace('\0', " "));
    }
    let out = crate::git::output_within(
        std::process::Command::new("ps").args(["-o", "command=", "-p", &pid.to_string()]),
        crate::git::PROBE_TIMEOUT,
    )
    .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(unix)]
fn replace(state_dir: &Path, sock: &Path) -> Result<(), String> {
    let own = env!("CARGO_PKG_VERSION");
    let theirs = request(sock, "GET", "/v1/version", None)
        .ok()
        .and_then(|(_, v)| v["version"].as_str().map(String::from));
    if theirs.as_deref() == Some(own) {
        return Ok(());
    }
    let pid: i32 = std::fs::read_to_string(state_dir.join("daemon.pid"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|p| *p > 1)
        .ok_or("the running daemon left no daemon.pid; stop it by hand")?;
    let cmd = command_line(pid).unwrap_or_default();
    if !looks_like_daemon(&cmd) {
        return Err(format!(
            "pid {pid} from daemon.pid is not a overseer daemon ({cmd:?}); stop the running daemon by hand"
        ));
    }
    let msg = format!(
        "overseer: replacing the running daemon {} (pid {pid}) with {own}",
        theirs.as_deref().unwrap_or("?")
    );
    crate::dlog::line("info", &msg);
    for sig in [libc::SIGTERM, libc::SIGKILL] {
        unsafe { libc::kill(pid, sig) };
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < end {
            if !running(state_dir) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err(format!("the running daemon (pid {pid}) did not stop"))
}

#[cfg(not(unix))]
fn replace(_: &Path, _: &Path) -> Result<(), String> {
    crate::dlog::line(
        "warn",
        "overseer: --replace is not supported on this platform; keeping the running daemon",
    );
    Ok(())
}

fn start_unit(state_dir: &Path) -> bool {
    let env = crate::autostart::Env::current(state_dir);
    if env.kind != crate::autostart::Kind::Systemd || !env.path().is_some_and(|p| p.is_file()) {
        return false;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ =
            crate::install::ensure_shim(&env.shim, &exe, crate::install::owns_globals(state_dir));
    }
    match crate::autostart::run_cmd("systemctl", &["--user", "start", crate::autostart::UNIT]) {
        Ok(_) => true,
        Err(e) => {
            crate::dlog::line(
                "warn",
                &format!(
                    "overseer: could not start {}: {e}; starting the daemon directly",
                    crate::autostart::UNIT
                ),
            );
            false
        }
    }
}

fn supervised() -> bool {
    std::env::var(crate::autostart::SUPERVISED).is_ok_and(|v| v == "1")
}

pub fn running(state_dir: &Path) -> bool {
    std::fs::create_dir_all(state_dir).is_ok() && lock(state_dir).is_err()
}

// DETACHED_PROCESS leaves the daemon without a console, so Windows opens a new
// console window for every git/gh/shell child it spawns. CREATE_NO_WINDOW gives
// it a hidden console that its children inherit instead.
#[cfg(any(windows, test))]
const WINDOWS_DETACH_FLAGS: u32 = 0x0800_0000 | 0x0000_0200;

pub fn detach(args: &[String]) -> Result<PathBuf, String> {
    let state_dir = crate::default_state_dir();
    crate::mkdir_private(&state_dir).map_err(|e| e.to_string())?;
    let path = socket_path(&state_dir);
    if args.iter().any(|a| a == "--replace") && running(&state_dir) {
        replace(&state_dir, &path)?;
    }
    if !running(&state_dir) && !start_unit(&state_dir) {
        let _ = crate::dlog::rotate(&state_dir, crate::dlog::MAX_BYTES, crate::dlog::KEEP);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(state_dir.join("daemon.log"))
            .map_err(|e| format!("daemon.log: {e}"))?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut c = std::process::Command::new(exe);
        c.arg("daemon")
            .args(
                args.iter()
                    .filter(|a| *a != "--detach" && *a != "--replace"),
            )
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        #[cfg(unix)]
        unsafe {
            use crate::sys::CommandExt;
            c.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut c, WINDOWS_DETACH_FLAGS);
        let child = c.spawn().map_err(|e| format!("spawn daemon: {e}"))?;
        let _ = std::fs::write(state_dir.join("daemon.pid"), child.id().to_string());
    }
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while request(&path, "GET", "/v1/version", None).is_err() {
        if std::time::Instant::now() > end {
            return Err(format!(
                "the daemon did not answer on {} (see {})",
                path.display(),
                state_dir.join("daemon.log").display()
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(path)
}

fn exchange(
    mut s: impl Read + Write,
    method: &str,
    target: &str,
    body: Option<&Value>,
    auth: Option<&str>,
) -> Result<(u16, Value), String> {
    let b = body.map(|b| b.to_string()).unwrap_or_default();
    let auth = auth
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    write!(
        s,
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n{auth}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}",
        b.len()
    )
    .map_err(|e| e.to_string())?;
    let mut out = String::new();
    s.read_to_string(&mut out).map_err(|e| e.to_string())?;
    let code = out
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("bad response")?;
    let body = out.split_once("\r\n\r\n").map(|x| x.1).unwrap_or("");
    Ok((code, serde_json::from_str(body).unwrap_or(Value::Null)))
}

#[cfg(unix)]
pub fn request(
    path: &Path,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let s = std::os::unix::net::UnixStream::connect(path)
        .map_err(|e| format!("connect {}: {e}", path.display()))?;
    exchange(s, method, target, body, None)
}

#[cfg(not(unix))]
pub fn request(
    path: &Path,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let info: Value = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or_else(|| format!("no daemon ({} missing)", path.display()))?;
    let addr = info["addr"].as_str().unwrap_or_default();
    let s = TcpStream::connect(addr).map_err(|e| format!("connect {addr}: {e}"))?;
    exchange(s, method, target, body, info["token"].as_str())
}

pub fn call(method: &str, target: &str, body: Option<&Value>) -> Result<(u16, Value), String> {
    request(
        &socket_path(&crate::default_state_dir()),
        method,
        target,
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_daemon_gets_a_hidden_console_its_children_inherit() {
        assert_ne!(WINDOWS_DETACH_FLAGS & 0x0800_0000, 0);
        assert_eq!(WINDOWS_DETACH_FLAGS & 0x0000_0008, 0);
    }

    struct Late(Vec<u8>, std::io::ErrorKind);

    impl Read for Late {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(self.1.into())
        }
    }

    impl Write for Late {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Stream for &mut Late {
        fn set_read_timeout(&self, _: Option<Duration>) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn answer(kind: ErrorKind) -> String {
        let mut s = Late(vec![], kind);
        conn(&mut s, None);
        String::from_utf8_lossy(&s.0).into_owned()
    }

    #[test]
    fn a_head_that_never_comes_times_out_with_408() {
        assert!(answer(ErrorKind::WouldBlock).starts_with("HTTP/1.1 408 Request Timeout\r\n"));
        assert!(answer(ErrorKind::TimedOut).starts_with("HTTP/1.1 408 "));
        assert!(answer(ErrorKind::InvalidData).starts_with("HTTP/1.1 400 "));
    }

    #[test]
    fn malformed_heads_are_400() {
        assert!(read_head(&mut &b"\r\n"[..]).is_err());
        assert!(read_head(&mut &b"GET / HTTP/1.1\r\nContent-Length: x\r\n\r\n"[..]).is_err());
        let h = read_head(&mut &b"GET /v1/terms/t/io HTTP/1.1\r\nUpgrade: WebSocket\r\n\r\n"[..])
            .unwrap();
        assert_eq!(header(&h.req, "upgrade"), Some("WebSocket"));
    }

    struct Endless(&'static [u8]);

    impl Read for Endless {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            let n = b.len().min(self.0.len());
            b[..n].copy_from_slice(&self.0[..n]);
            Ok(n)
        }
    }

    fn head_of(r: impl Read) -> Result<Head, HeadErr> {
        read_head(&mut BufReader::new(r))
    }

    #[test]
    fn oversized_heads_are_refused_without_buffering_them() {
        assert_eq!(
            head_of(Endless(b"GGGGGGGG")).err(),
            Some(HeadErr::TooLarge(414))
        );
        let long_header = Read::chain(&b"GET / HTTP/1.1\r\nX-A: "[..], Endless(b"aaaaaaaa"));
        assert_eq!(head_of(long_header).err(), Some(HeadErr::TooLarge(431)));
        let many = Read::chain(&b"GET / HTTP/1.1\r\n"[..], Endless(b"X-B: 1\r\n"));
        assert_eq!(head_of(many).err(), Some(HeadErr::TooLarge(431)));
        let mut big = b"GET / HTTP/1.1\r\n".to_vec();
        for i in 0..90 {
            big.extend_from_slice(format!("X-{i}: {}\r\n", "v".repeat(1000)).as_bytes());
        }
        big.extend_from_slice(b"\r\n");
        assert_eq!(head_of(&big[..]).err(), Some(HeadErr::TooLarge(431)));
        let line = format!(
            "GET /{} HTTP/1.1\r\nHost: x\r\n\r\n",
            "p".repeat(MAX_LINE - 20)
        );
        assert_eq!(head_of(line.as_bytes()).unwrap().req.method, "GET");
        let mut huge = Read::chain(&b"GET / HTTP/1.1\r\nX-A: "[..], Endless(b"a"));
        let mut out = vec![];
        let mut r = BufReader::new(&mut huge);
        let code = match read_head(&mut r) {
            Err(HeadErr::TooLarge(c)) => c,
            _ => 0,
        };
        respond(&mut out, code, &json!({}));
        assert!(String::from_utf8_lossy(&out)
            .starts_with("HTTP/1.1 431 Request Header Fields Too Large\r\n"));
    }

    #[test]
    fn successful_writes_ask_for_a_rescan() {
        assert!(changes_lines("POST", "/v1/lines/x/hold", 200));
        assert!(changes_lines("POST", "/v1/terms", 201));
        assert!(changes_lines("PUT", "/v1/lines/x/pin", 200));
        assert!(changes_lines("PUT", "/v1/lines/x/title", 200));
        assert!(changes_lines("PUT", "/v1/lines/x/workspace", 200));
        assert!(changes_lines("PUT", "/v1/terms/x/title", 200));
        assert!(changes_lines("PUT", "/v1/terms/x/workspace", 200));
        assert!(settles_lines("PUT", "/v1/lines/x/pin"));
        assert!(settles_lines("PUT", "/v1/terms/x/title"));
        assert!(!settles_lines("POST", "/v1/terms"));
        assert!(!settles_lines("PUT", "/v1/lines/x/hold"));
        assert!(changes_lines("DELETE", "/v1/workspaces/w", 200));
        assert!(!changes_lines("POST", "/v1/terms", 400));
        assert!(!changes_lines("GET", "/v1/lines", 200));
        assert!(!changes_lines("POST", "/v1/hook-events", 202));
        assert!(!changes_lines("POST", "/v1/terms/t1/resize", 200));
        assert!(!changes_lines("POST", "/v1/lines/x/keys", 200));
        assert!(changes_lines("POST", "/v1/tasks/t/start", 200));
    }

    #[test]
    fn only_overseer_daemons_look_like_daemons() {
        assert!(looks_like_daemon(
            "/home/u/.overseer/server/0.7.0/overseer daemon --interval 5"
        ));
        assert!(looks_like_daemon("overseer daemon"));
        assert!(!looks_like_daemon("/usr/bin/sleep daemon"));
        assert!(!looks_like_daemon(
            "/home/u/.overseer/server/0.7.0/overseer proxy"
        ));
        assert!(!looks_like_daemon(""));
    }
}
