use crate::board::{git_out, publish};
use crate::git::Git;
use crate::registry::Registry;
use crate::term::{self, Spec, Term};
use crate::util::{format_rfc3339, ticket_any, ulid};
use crate::{Ctx, Line};
use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub(crate) type Res<T> = Result<T, (u16, String)>;

const COLS: &str = "id, number, title, description, project, status, agent, model, effort, ticket_key, line_id, term_id, branch, worktree_path, progress, error, started_ms, created_ms, updated_ms, rank, origin, manual_status, session_state, badge, profile, title_source, urgent, important, parent_id, depends_on, inherit_branch";

pub const TITLE_USER: &str = "user";
pub const TITLE_AUTO: &str = "auto";

static START: Mutex<()> = Mutex::new(());
static SEEN_BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[derive(Clone, Debug, Default)]
pub struct Task {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub description: String,
    pub project: String,
    pub status: String,
    pub agent: String,
    pub model: String,
    pub effort: String,
    pub ticket_key: Option<String>,
    pub line_id: Option<String>,
    pub term_id: Option<String>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub progress: Option<Value>,
    pub error: Option<String>,
    pub started_ms: Option<i64>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub rank: f64,
    pub origin: Option<String>,
    pub manual_status: Option<String>,
    pub session_state: Option<String>,
    pub badge: Option<String>,
    pub profile: Option<String>,
    pub title_source: Option<String>,
    pub urgent: Option<bool>,
    pub important: Option<bool>,
    pub parent_id: Option<String>,
    pub depends_on: Vec<String>,
    pub inherit_branch: Option<bool>,
}

fn row(r: &Row) -> rusqlite::Result<Task> {
    let created: i64 = r.get(17)?;
    Ok(Task {
        id: r.get(0)?,
        number: r.get(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        project: r.get(4)?,
        status: r.get(5)?,
        agent: r.get(6)?,
        model: r.get(7)?,
        effort: r.get(8)?,
        ticket_key: r.get(9)?,
        line_id: r.get(10)?,
        term_id: r.get(11)?,
        branch: r.get(12)?,
        worktree_path: r.get(13)?,
        progress: r
            .get::<_, Option<String>>(14)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        error: r.get(15)?,
        started_ms: r.get(16)?,
        created_ms: created,
        updated_ms: r.get(18)?,
        rank: r.get::<_, Option<f64>>(19)?.unwrap_or(created as f64),
        origin: r.get(20)?,
        manual_status: r.get(21)?,
        session_state: r.get(22)?,
        badge: r.get(23)?,
        profile: r.get(24)?,
        title_source: r.get(25)?,
        urgent: r.get(26)?,
        important: r.get(27)?,
        parent_id: r.get(28)?,
        depends_on: r
            .get::<_, Option<String>>(29)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        inherit_branch: r.get(30)?,
    })
}

impl Task {
    pub fn to_json(&self, station: &str) -> Value {
        let mut v = json!({
            "id": self.id, "number": self.number, "title": self.title,
            "description": self.description, "project": self.project, "status": self.status,
            "agent": self.agent, "model": self.model, "effort": self.effort, "station": station,
            "rank": self.rank, "origin": self.origin.as_deref().unwrap_or("user"),
            "createdAt": format_rfc3339(self.created_ms), "updatedAt": format_rfc3339(self.updated_ms),
            "dependsOn": self.depends_on,
        });
        for (k, val) in [
            ("ticketKey", &self.ticket_key),
            ("lineId", &self.line_id),
            ("termId", &self.term_id),
            ("branch", &self.branch),
            ("worktreePath", &self.worktree_path),
            ("error", &self.error),
            ("manualStatus", &self.manual_status),
            ("profile", &self.profile),
        ] {
            if let Some(x) = val {
                v[k] = json!(x);
            }
        }
        if let Some(p) = &self.progress {
            v["progress"] = p.clone();
        }
        for (k, val) in [
            ("urgent", self.urgent),
            ("important", self.important),
            ("inheritBranch", self.inherit_branch),
        ] {
            if let Some(x) = val {
                v[k] = json!(x);
            }
        }
        if let Some(x) = &self.parent_id {
            v["parentId"] = json!(x);
        }
        if let Some(b) = &self.badge {
            v["badges"] = json!([b]);
        }
        v
    }

    pub fn from_session(&self) -> bool {
        self.origin.as_deref() == Some(crate::session_tasks::ORIGIN)
    }

    pub(crate) fn live_term(&self) -> Option<Arc<Term>> {
        self.term_id
            .as_deref()
            .and_then(term::get)
            .filter(|t| !t.exited())
    }
}

pub(crate) fn sql<T>(r: rusqlite::Result<T>) -> Res<T> {
    r.map_err(|e| (500, e.to_string()))
}

pub(crate) fn db(ctx: &Ctx) -> Res<Registry> {
    Registry::open(&ctx.state_dir).map_err(|e| (500, e))
}

pub(crate) fn bad(msg: impl Into<String>) -> (u16, String) {
    (400, msg.into())
}

pub fn all(reg: &Registry) -> Res<Vec<Task>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "SELECT {COLS} FROM tasks ORDER BY project, number"
    )))?;
    let rows = sql(st.query_map([], row))?;
    sql(rows.collect())
}

pub fn get(reg: &Registry, id: &str) -> Res<Task> {
    sql(reg
        .conn
        .query_row(
            &format!("SELECT {COLS} FROM tasks WHERE id = ?1"),
            [id],
            row,
        )
        .optional())?
    .ok_or_else(|| (404, format!("no task {id}")))
}

fn save(ctx: &Ctx, reg: &Registry, t: &mut Task) -> Res<()> {
    store(reg, t, &ctx.config.station_id)
}

pub(crate) fn store(reg: &Registry, t: &mut Task, station: &str) -> Res<()> {
    t.updated_ms = crate::procs::now_ms();
    sql(reg.conn.execute(
        "UPDATE tasks SET title = ?2, description = ?3, status = ?4, agent = ?5, model = ?6, effort = ?7,
           ticket_key = ?8, line_id = ?9, term_id = ?10, branch = ?11, worktree_path = ?12, progress = ?13,
           error = ?14, started_ms = ?15, updated_ms = ?16, rank = ?17, manual_status = ?18,
           session_state = ?19, badge = ?20, profile = ?21, title_source = ?22, urgent = ?23, important = ?24,
           depends_on = ?25, inherit_branch = ?26 WHERE id = ?1",
        params![
            t.id,
            t.title,
            t.description,
            t.status,
            t.agent,
            t.model,
            t.effort,
            t.ticket_key,
            t.line_id,
            t.term_id,
            t.branch,
            t.worktree_path,
            t.progress.as_ref().map(Value::to_string),
            t.error,
            t.started_ms,
            t.updated_ms,
            t.rank,
            t.manual_status,
            t.session_state,
            t.badge,
            t.profile,
            t.title_source,
            t.urgent,
            t.important,
            deps_json(&t.depends_on),
            t.inherit_branch,
        ],
    ))?;
    publish("task.updated", json!({"task": t.to_json(station)}));
    Ok(())
}

fn pick<'a>(b: &'a Value, k: &str, allowed: &[&str], default: &'a str) -> Res<&'a str> {
    match &b[k] {
        Value::Null => Ok(default),
        Value::String(s) if allowed.contains(&s.as_str()) => Ok(s),
        _ => Err(bad(format!("{k} must be one of {}", allowed.join(", ")))),
    }
}

use crate::agents::EFFORTS;

fn agent_of(b: &Value, k: &str) -> Res<Option<&'static str>> {
    match &b[k] {
        Value::Null => Ok(None),
        Value::String(s) if crate::agents::task_agent(s) => Ok(Some(crate::agents::id_of(s))),
        _ => Err(bad(format!(
            "{k} must be one of {}",
            crate::agents::AGENTS
                .iter()
                .filter(|a| crate::agents::task_agent(a.id))
                .map(|a| a.id)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn rank_of(v: &Value) -> Res<f64> {
    v.as_f64()
        .filter(|r| r.is_finite())
        .ok_or_else(|| bad("rank must be a finite number"))
}

fn flag_of(k: &str, v: &Value) -> Res<Option<bool>> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(*b)),
        _ => Err(bad(format!("{k} must be a boolean or null"))),
    }
}

fn deps_json(d: &[String]) -> Option<String> {
    (!d.is_empty()).then(|| json!(d).to_string())
}

pub fn blockers<'a>(t: &Task, tasks: &'a [Task]) -> Vec<&'a Task> {
    t.depends_on
        .iter()
        .filter_map(|d| tasks.iter().find(|x| &x.id == d))
        .filter(|x| x.status != "completed")
        .collect()
}

fn reaches(from: &[String], target: &str, tasks: &[Task]) -> bool {
    let mut stack: Vec<&str> = from.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    while let Some(d) = stack.pop() {
        if d == target {
            return true;
        }
        if seen.insert(d) {
            if let Some(x) = tasks.iter().find(|x| x.id == d) {
                stack.extend(x.depends_on.iter().map(String::as_str));
            }
        }
    }
    false
}

fn check_deps(id: &str, raw: &[String], tasks: &[Task]) -> Res<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for d in raw {
        if d == id {
            return Err(bad("a task cannot depend on itself"));
        }
        if !tasks.iter().any(|x| &x.id == d) {
            return Err(bad(format!("dependsOn {d} is not a task")));
        }
        if !out.contains(d) {
            out.push(d.clone());
        }
    }
    if reaches(&out, id, tasks) {
        return Err(bad("dependsOn would create a cycle"));
    }
    Ok(out)
}

#[derive(Debug, PartialEq)]
enum DepRef {
    Id(String),
    Number(i64),
}

fn dep_ref(v: &Value) -> Option<DepRef> {
    match v {
        Value::Number(n) => n.as_i64().map(DepRef::Number),
        Value::String(s) => Some(match s.trim().trim_start_matches('#').parse() {
            Ok(n) => DepRef::Number(n),
            Err(_) => DepRef::Id(s.clone()),
        }),
        _ => None,
    }
}

fn deps_of(reg: &Registry, id: &str, project: &str, v: &Value) -> Res<Vec<String>> {
    const SHAPE: &str = "dependsOn must be an array of task ids or numbers";
    let refs: Vec<DepRef> = match v {
        Value::Null => return Ok(vec![]),
        Value::Array(a) => a
            .iter()
            .map(dep_ref)
            .collect::<Option<_>>()
            .ok_or_else(|| bad(SHAPE))?,
        _ => return Err(bad(SHAPE)),
    };
    if refs.is_empty() {
        return Ok(vec![]);
    }
    let tasks = all(reg)?;
    let raw = refs
        .into_iter()
        .map(|r| match r {
            DepRef::Id(id) => Ok(id),
            DepRef::Number(n) => tasks
                .iter()
                .find(|x| x.project == project && x.number == n)
                .map(|x| x.id.clone())
                .ok_or_else(|| bad(format!("dependsOn #{n} is not a task of {project}"))),
        })
        .collect::<Res<Vec<_>>>()?;
    check_deps(id, &raw, &tasks)
}

fn ticket_of(v: &Value) -> Res<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) => ticket_any(s)
            .map(Some)
            .ok_or_else(|| bad("ticketKey must look like PROJ-1234")),
        _ => Err(bad("ticketKey must be a string or null")),
    }
}

fn profile_of(ctx: &Ctx, v: &Value) -> Res<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s.is_empty() => Ok(None),
        Value::String(s) if s == "none" => Ok(Some(s.clone())),
        Value::String(s) => crate::profiles::find(&crate::profiles::load(&ctx.state_dir), s)
            .map(|p| Some(p.id))
            .ok_or_else(|| bad(format!("unknown profile {s}"))),
        _ => Err(bad("profile must be a string or null")),
    }
}

pub fn main_root(g: &mut Git, path: &str) -> Option<(String, String)> {
    let i = g.info(path)?;
    let common = Path::new(&i.common);
    let root = if common.file_name().is_some_and(|n| n == ".git") {
        common.parent()?.to_string_lossy().into_owned()
    } else {
        i.top.clone()
    };
    let repo = g.info(&root).map(|r| r.repo).unwrap_or(i.repo);
    Some((root, repo))
}

pub fn create(ctx: &Ctx, b: &Value) -> Res<Task> {
    let title = b["title"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("title required"))?;
    let path = b["project"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("project required"))?;
    let (project, _) = main_root(&mut Git::default(), path)
        .ok_or_else(|| bad(format!("{path} is not in a git repository")))?;
    let status = pick(b, "status", &["pending", "auto"], "pending")?;
    let agent = agent_of(b, "agent")?.unwrap_or("claude");
    let profile = profile_of(ctx, &b["profile"])?;
    let profiled = {
        let s = crate::profiles::load(&ctx.state_dir);
        crate::profiles::resolve(&s, profile.as_deref(), &[project.as_str()])
            .map_err(|(_, e)| bad(e))?
            .is_some_and(|p| p.agents.get(agent).is_some_and(|c| !c.effort.is_empty()))
    };
    let levels = crate::agents::get(agent)
        .map(|a| a.valid_efforts())
        .unwrap_or(EFFORTS);
    let effort = pick(b, "effort", levels, if profiled { "" } else { "high" })?;
    let model = match &b["model"] {
        Value::Null => "",
        Value::String(s) => s.trim(),
        _ => return Err(bad("model must be a string")),
    };
    let origin = pick(b, "origin", &["user", "agent"], "user")?;
    let reg = db(ctx)?;
    let parent_id = match &b["parentId"] {
        Value::Null => None,
        Value::String(p) => {
            let parent = get(&reg, p).map_err(|_| bad(format!("parentId {p} is not a task")))?;
            if crate::task_agents::is_organizer(&parent) {
                return Err(bad("the organizer may not create tasks"));
            }
            Some(p.clone())
        }
        _ => return Err(bad("parentId must be a string or null")),
    };
    let depends_on = deps_of(&reg, "", &project, &b["dependsOn"])?;
    let now = crate::procs::now_ms();
    let mut t = Task {
        id: ulid(now),
        title: title.into(),
        description: b["description"].as_str().unwrap_or_default().into(),
        project,
        status: status.into(),
        agent: agent.into(),
        model: model.into(),
        effort: effort.into(),
        ticket_key: ticket_of(&b["ticketKey"])?,
        profile,
        origin: (origin != "user").then(|| origin.to_string()),
        urgent: flag_of("urgent", &b["urgent"])?,
        important: flag_of("important", &b["important"])?,
        parent_id,
        depends_on,
        inherit_branch: flag_of("inheritBranch", &b["inheritBranch"])?,
        created_ms: now,
        updated_ms: now,
        rank: match &b["rank"] {
            Value::Null => now as f64,
            r => rank_of(r)?,
        },
        ..Default::default()
    };
    let files =
        crate::uploads::attachments(&crate::uploads::dir(), &b["attachments"]).map_err(bad)?;
    if !files.is_empty() {
        let kept =
            crate::uploads::keep(&crate::uploads::task_dir(&t.id), &files).map_err(|e| (500, e))?;
        t.description = with_attachments(&t.description, &kept);
    }
    insert(&reg, &mut t, &ctx.config.station_id)?;
    Ok(t)
}

fn with_attachments(description: &str, files: &[std::path::PathBuf]) -> String {
    let list: Vec<String> = files.iter().map(|f| format!("- {}", f.display())).collect();
    let head = description.trim_end();
    let sep = if head.is_empty() { "" } else { "\n\n" };
    format!("{head}{sep}Attachments:\n{}", list.join("\n"))
}

pub(crate) fn insert(reg: &Registry, t: &mut Task, station: &str) -> Res<bool> {
    let tx = sql(reg.conn.unchecked_transaction())?;
    t.number = sql(tx.query_row(
        "INSERT INTO task_numbers (project, last) VALUES (?1, 1)
         ON CONFLICT (project) DO UPDATE SET last = last + 1 RETURNING last",
        [&t.project],
        |r| r.get(0),
    ))?;
    let n = sql(tx.execute(
        "INSERT INTO tasks (id, number, title, description, project, status, agent, model, effort, ticket_key,
           line_id, branch, worktree_path, progress, created_ms, updated_ms, rank, origin, session_state, badge, profile, title_source, urgent, important, parent_id, depends_on, inherit_branch)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
         ON CONFLICT DO NOTHING",
        params![
            t.id,
            t.number,
            t.title,
            t.description,
            t.project,
            t.status,
            t.agent,
            t.model,
            t.effort,
            t.ticket_key,
            t.line_id,
            t.branch,
            t.worktree_path,
            t.progress.as_ref().map(Value::to_string),
            t.created_ms,
            t.updated_ms,
            t.rank,
            t.origin,
            t.session_state,
            t.badge,
            t.profile,
            t.title_source,
            t.urgent,
            t.important,
            t.parent_id,
            deps_json(&t.depends_on),
            t.inherit_branch,
        ],
    ))?;
    if n == 0 {
        return Ok(false);
    }
    sql(tx.commit())?;
    publish("task.created", json!({"task": t.to_json(station)}));
    Ok(true)
}

fn progress(v: &Value) -> Res<Option<Value>> {
    if v.is_null() {
        return Ok(None);
    }
    let (Some(done), Some(total)) = (v["done"].as_u64(), v["total"].as_u64()) else {
        return Err(bad("progress needs integer done and total"));
    };
    let mut p = json!({"done": done, "total": total});
    match &v["note"] {
        Value::Null => {}
        Value::String(n) => p["note"] = json!(n),
        _ => return Err(bad("progress.note must be a string")),
    }
    Ok(Some(p))
}

pub fn patch(ctx: &Ctx, id: &str, b: &Value) -> Res<Task> {
    let obj = b.as_object().ok_or_else(|| bad("JSON object expected"))?;
    let reg = db(ctx)?;
    let mut t = get(&reg, id)?;
    let effort_agent = b["agent"].as_str().unwrap_or(&t.agent).to_owned();
    let was = t.status.clone();
    for (k, v) in obj {
        let s = || {
            v.as_str()
                .ok_or_else(|| bad(format!("{k} must be a string")))
        };
        match k.as_str() {
            "title" => {
                let title = s()?.trim().to_string();
                if title.is_empty() {
                    return Err(bad("title must not be empty"));
                }
                if title != t.title {
                    t.title = title;
                    t.title_source = Some(TITLE_USER.into());
                }
            }
            "description" => t.description = s()?.to_string(),
            "model" => t.model = s()?.trim().to_string(),
            "status" if t.from_session() => {
                let s = pick(b, k, &["in_progress", "in_testing", "completed"], "")
                    .map_err(|_| bad("a session task follows its session: status must be one of in_progress, in_testing, completed"))?;
                t.status = s.into();
                t.manual_status = Some(s.into());
            }
            "status" if v == "in_progress" => {
                return Err(bad("use POST /v1/tasks/{id}/start to start a task"))
            }
            "status" => {
                t.status = pick(b, k, &["pending", "auto", "in_testing", "completed"], "")?.into()
            }
            "agent" => {
                t.agent = agent_of(b, k)?
                    .ok_or_else(|| bad("agent must not be null"))?
                    .into()
            }
            "rank" => t.rank = rank_of(v)?,
            "urgent" => t.urgent = flag_of(k, v)?,
            "important" => t.important = flag_of(k, v)?,
            "inheritBranch" => t.inherit_branch = flag_of(k, v)?,
            "dependsOn" => t.depends_on = deps_of(&reg, id, &t.project, v)?,
            "effort" => {
                let levels = crate::agents::get(&effort_agent)
                    .map(|a| a.valid_efforts())
                    .unwrap_or(EFFORTS);
                t.effort = pick(b, k, levels, "")?.into();
            }
            "profile" => t.profile = profile_of(ctx, v)?,
            "ticketKey" => t.ticket_key = ticket_of(v)?,
            "progress" => t.progress = progress(v)?,
            _ => {}
        }
    }
    save(ctx, &reg, &mut t)?;
    if t.status == "in_testing" && was != "in_testing" {
        crate::task_agents::spawn_overlap_alert(ctx, &t);
    }
    Ok(t)
}

pub fn delete(ctx: &Ctx, id: &str) -> Res<Value> {
    let reg = db(ctx)?;
    let t = get(&reg, id)?;
    if t.from_session() {
        return Err((
            409,
            format!(
                "task #{} follows its session: close the session or hide session tasks",
                t.number
            ),
        ));
    }
    if let Some(term) = t.live_term() {
        return Err((
            409,
            format!("task #{} still runs in term {}", t.number, term.info().id),
        ));
    }
    let dependents = remove(&reg, id)?;
    let _ = std::fs::remove_dir_all(crate::uploads::task_dir(id));
    publish("task.deleted", json!({"id": id}));
    for d in dependents {
        publish(
            "task.updated",
            json!({"task": d.to_json(&ctx.config.station_id)}),
        );
    }
    Ok(json!({"deleted": id}))
}

fn remove(reg: &Registry, id: &str) -> Res<Vec<Task>> {
    let now = crate::procs::now_ms();
    let dependents: Vec<Task> = all(reg)?
        .into_iter()
        .filter(|d| d.id != id && d.depends_on.iter().any(|x| x == id))
        .map(|mut d| {
            d.depends_on.retain(|x| x != id);
            d.updated_ms = now;
            d
        })
        .collect();
    let tx = sql(reg.conn.unchecked_transaction())?;
    for d in &dependents {
        sql(tx.execute(
            "UPDATE tasks SET depends_on = ?2, updated_ms = ?3 WHERE id = ?1",
            params![d.id, deps_json(&d.depends_on), d.updated_ms],
        ))?;
    }
    sql(tx.execute("DELETE FROM tasks WHERE id = ?1", [id]))?;
    sql(tx.execute(
        "DELETE FROM task_messages WHERE task_id = ?1 OR from_id = ?1",
        [id],
    ))?;
    sql(tx.commit())?;
    Ok(dependents)
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "task".into()
    } else {
        out
    }
}

fn dep_base(reg: &Registry, t: &Task) -> Option<String> {
    let [d] = t.depends_on.as_slice() else {
        return None;
    };
    if t.inherit_branch == Some(false) {
        return None;
    }
    get(reg, d).ok()?.branch.filter(|b| {
        crate::git::git(
            &t.project,
            &["rev-parse", "--verify", "-q", &format!("{b}^{{commit}}")],
        )
        .is_some()
    })
}

fn worktree(ctx: &Ctx, reg: &Registry, t: &mut Task) -> Res<String> {
    if let Some(p) = t.worktree_path.as_ref().filter(|p| Path::new(p).is_dir()) {
        return Ok(p.clone());
    }
    let branch = t
        .branch
        .clone()
        .unwrap_or_else(|| format!("task/{}-{}", t.number, slug(&t.title)));
    let base = dep_base(reg, t);
    let (root, path, branch) = crate::worktrees::create(
        ctx,
        &t.project,
        &crate::worktrees::New {
            branch: Some(&branch),
            base: base.as_deref(),
            ..Default::default()
        },
    )
    .map_err(|(c, e)| (if c == 500 { 500 } else { 409 }, e))?;
    crate::worktrees::announce(root, path.clone());
    t.branch = Some(branch);
    t.worktree_path = Some(path.clone());
    Ok(path)
}

fn in_place(t: &mut Task) -> String {
    t.branch = crate::git::git(&t.project, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|b| !b.is_empty());
    t.worktree_path = Some(t.project.clone());
    t.project.clone()
}

pub fn applied(ctx: &Ctx, t: &Task) -> crate::profiles::Applied {
    let a = crate::agents::get(&t.agent).unwrap_or(&crate::agents::AGENTS[0]);
    crate::profiles::for_task(&ctx.state_dir, a, t)
}

pub fn argv(
    ctx: &Ctx,
    t: &Task,
    p: &crate::profiles::Applied,
    session: &str,
    prompt: Option<&str>,
    auto_approve: bool,
) -> Vec<String> {
    let a = crate::agents::get(&t.agent).unwrap_or(&crate::agents::AGENTS[0]);
    let mut extra: Vec<String> = if a.id == "claude" {
        ctx.config.start_args.clone()
    } else {
        vec![]
    };
    extra.extend(p.extra.iter().cloned());
    a.launch(&crate::agents::Launch {
        model: &p.model,
        effort: &p.effort,
        session: Some(session),
        prompt,
        extra: &extra,
        auto_approve,
    })
}

fn reserve(
    ctx: &Ctx,
    reg: &Registry,
    t: &Task,
    agent: &'static str,
    session: &str,
    wt: &str,
) -> Res<String> {
    let repo = Git::default()
        .info(wt)
        .map(|i| i.repo)
        .unwrap_or_else(|| "unknown".into());
    let l = Line {
        id: format!("{}-{}", ctx.config.station_id, ulid(crate::procs::now_ms())),
        agent,
        session_id: session.into(),
        owner: ctx.config.owner.clone(),
        station_id: ctx.config.station_id.clone(),
        repo,
        branch: t.branch.clone().unwrap_or_default(),
        worktree_path: wt.into(),
        ticket: t.ticket_key.clone(),
        ..Default::default()
    };
    reg.reserve(&l, "dev", ctx.now_ms).map_err(|e| (500, e))?;
    Ok(l.id)
}

fn seen_busy(f: impl FnOnce(&mut HashSet<String>) -> bool) -> bool {
    let mut g = SEEN_BUSY.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashSet::new))
}

pub fn start(ctx: &Ctx, id: &str, size: Option<(u16, u16)>) -> Res<Task> {
    start_with(ctx, id, size, false)
}

pub fn start_with(ctx: &Ctx, id: &str, size: Option<(u16, u16)>, here: bool) -> Res<Task> {
    start_with_args(ctx, id, size, here, &[])
}

pub(crate) fn start_with_args(
    ctx: &Ctx,
    id: &str,
    size: Option<(u16, u16)>,
    here: bool,
    extra: &[String],
) -> Res<Task> {
    let _g = START.lock().unwrap_or_else(|e| e.into_inner());
    let reg = db(ctx)?;
    let mut t = get(&reg, id)?;
    if t.from_session() {
        return Err((
            409,
            format!(
                "task #{} follows a running session: open it instead",
                t.number
            ),
        ));
    }
    if let Some(term) = t.live_term() {
        return Err((
            409,
            format!("task #{} already runs in term {}", t.number, term.info().id),
        ));
    }
    let agent =
        crate::agents::get(&t.agent).ok_or_else(|| bad(format!("unknown agent {}", t.agent)))?;
    if !crate::agents::is_installed(agent) {
        return Err((
            409,
            format!("{} is not installed ({})", agent.name, agent.bin),
        ));
    }
    let auto_approve = crate::agents::auto_approve_enabled(&reg, agent.id).map_err(|e| (500, e))?;
    let wt = if here {
        in_place(&mut t)
    } else {
        worktree(ctx, &reg, &mut t)?
    };
    let session = crate::runner::uuid().map_err(|e| (500, e))?;
    t.line_id = match agent.session {
        Some(_) => Some(reserve(ctx, &reg, &t, agent.id, &session, &wt)?),
        None => None,
    };
    let waiting = crate::task_agents::undelivered(&reg, &t.id)?;
    let mut prompt = format!("{}\n\n{}", t.title, t.description)
        .trim()
        .to_string();
    if !waiting.is_empty() {
        prompt.push_str("\n\nMessages received before you started:\n");
        prompt.push_str(&crate::task_agents::backlog(&waiting));
    }
    let by_flag = agent.prompt.is_some();
    let p = applied(ctx, &t);
    let mcp = match agent.id {
        "codex" => crate::codex::overseer_mcp(
            &crate::actions::self_exe(),
            &ctx.state_dir.to_string_lossy(),
            &t.id,
        ),
        _ => vec![],
    };
    let mut launch = p.clone();
    launch.extra.extend(mcp.iter().cloned());
    launch.extra.extend(extra.iter().cloned());
    let cmd = argv(
        ctx,
        &t,
        &launch,
        &session,
        by_flag.then_some(prompt.as_str()),
        auto_approve,
    );
    let mut env = vec![("OVERSEER_TASK_ID".into(), t.id.clone())];
    env.extend(p.env.iter().cloned());
    let mut flags = agent.flags(&p.model, &p.effort);
    flags.extend(p.extra.iter().cloned());
    if auto_approve {
        if let Some(auto_approve_flags) = crate::agents::auto_approve_args(agent.id) {
            flags.extend(auto_approve_flags.iter().map(|flag| (*flag).to_string()));
        }
    }
    flags.extend(mcp);
    let (cols, rows) = size.unwrap_or((120, 32));
    let term = term::spawn(Spec {
        cwd: wt,
        shown: by_flag.then(|| crate::agents::redact(&cmd, agent, Some(&prompt))),
        cmd,
        env,
        cols,
        rows,
        title: Some(format!("#{} {}", t.number, t.title)),
        line_id: t.line_id.clone(),
        task_id: Some(t.id.clone()),
        session: agent.session.map(|_| session.clone()),
        flags,
        profile: p.chip.clone(),
        id: None,
        workspace_id: None,
    })
    .map_err(|e| (409, e))?;
    t.term_id = Some(term.info().id);
    t.status = "in_progress".into();
    t.error = None;
    t.session_state = None;
    t.started_ms = Some(crate::procs::now_ms());
    seen_busy(|s| s.remove(&t.id));
    save(ctx, &reg, &mut t)?;
    crate::task_agents::mark_delivered(&reg, &waiting);
    let late: Vec<_> = crate::task_agents::undelivered(&reg, &t.id)?
        .into_iter()
        .filter(|m| !waiting.iter().any(|w| w.id == m.id))
        .collect();
    crate::task_agents::mark_delivered(&reg, &late);
    let mut text = if by_flag { String::new() } else { prompt };
    if !late.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&crate::task_agents::backlog(&late));
    }
    let own_dir = t.origin.as_deref() == Some(crate::task_agents::CONFIGURATOR);
    if !text.is_empty() {
        std::thread::spawn(move || crate::deliver::when_started(&term, &text, own_dir));
    }
    Ok(t)
}

pub fn review(ctx: &Ctx, id: &str, b: &Value) -> Res<Value> {
    let action = pick(b, "action", &["approve", "reject", "commit"], "")?;
    if action.is_empty() {
        return Err(bad("action required"));
    }
    let message = b["message"]
        .as_str()
        .map(str::trim)
        .filter(|m| !m.is_empty());
    let reg = db(ctx)?;
    let mut t = get(&reg, id)?;
    let mut out = json!({});
    match action {
        "reject" if t.from_session() => {
            if message.is_some() {
                return Err((
                    409,
                    "a session task takes feedback in its session tab".into(),
                ));
            }
            t.status = "in_progress".into();
        }
        "approve" => {
            if let Some(text) = pr_followup(b["merge"] == true, b["approvePr"] == true) {
                let term = t.live_term().ok_or_else(|| {
                    (
                        409,
                        "the agent is not running; start the task again".to_string(),
                    )
                })?;
                crate::deliver::send(&crate::deliver::Channel::Term(term), &text, None, "")
                    .map_err(|e| (500, e))?;
            }
            t.status = "completed".into();
        }
        "reject" => match (t.live_term(), message) {
            (Some(term), m) => {
                if let Some(m) = m {
                    crate::deliver::send(&crate::deliver::Channel::Term(term), m, None, "")
                        .map_err(|e| (500, e))?;
                }
                t.status = "in_progress".into();
                t.started_ms = Some(crate::procs::now_ms());
                seen_busy(|s| s.remove(&t.id));
            }
            (None, Some(_)) => {
                return Err((409, "the agent is not running; start the task again".into()))
            }
            (None, None) => t.status = "pending".into(),
        },
        _ => {
            let wt = t
                .worktree_path
                .clone()
                .filter(|p| Path::new(p).is_dir())
                .ok_or_else(|| (409, "the task has no worktree".to_string()))?;
            let dirty = git_out(&wt, &["status", "--porcelain"]).map_err(|e| (409, e))?;
            if dirty.trim().is_empty() {
                return Err((409, "nothing to commit: the worktree is clean".into()));
            }
            let msg = message
                .map(str::to_string)
                .unwrap_or_else(|| format!("task #{}: {}", t.number, t.title));
            let sha = git_out(&wt, &["add", "-A"])
                .and_then(|_| git_out(&wt, &["commit", "-q", "-m", &msg]))
                .and_then(|_| git_out(&wt, &["rev-parse", "HEAD"]))
                .map_err(|e| (409, e))?;
            out["commit"] = json!(sha.trim());
            t.status = "completed".into();
        }
    }
    if t.from_session() {
        t.manual_status = Some(t.status.clone());
    }
    save(ctx, &reg, &mut t)?;
    out["task"] = t.to_json(&ctx.config.station_id);
    Ok(out)
}

fn pr_followup(merge: bool, approve: bool) -> Option<String> {
    let step = match (approve, merge) {
        (false, false) => return None,
        (true, false) => "approve the pull request of this branch on the forge (`gh pr review --approve`)",
        (false, true) => "merge the pull request of this branch (`gh pr merge`), following the repository's merge rules",
        (true, true) => "approve the pull request of this branch on the forge (`gh pr review --approve`), then merge it (`gh pr merge`), following the repository's merge rules",
    };
    Some(format!(
        "The review is approved. Push any remaining commits, open the pull request if it does not exist yet, and {step}. Report if it fails."
    ))
}

fn merge_candidate(t: &Task) -> bool {
    !t.from_session() && t.status == "in_testing" && t.session_state.as_deref() != Some("merged")
}

pub fn tick(ctx: &Ctx, lines: &[Line]) {
    let Ok(reg) = db(ctx) else {
        return;
    };
    let Ok(tasks) = all(&reg) else {
        return;
    };
    for mut t in tasks
        .into_iter()
        .filter(|t| t.status == "in_progress" && !t.from_session())
    {
        let term = t.term_id.as_deref().and_then(term::get);
        let mut changed = false;
        if t.line_id.is_none() {
            t.line_id = term.as_ref().and_then(|x| x.info().line_id);
            changed = t.line_id.is_some();
        }
        let line = t
            .line_id
            .as_deref()
            .and_then(|id| lines.iter().find(|l| l.id == id));
        if line.is_some_and(|l| l.busy) {
            seen_busy(|s| s.insert(t.id.clone()));
        }
        let after = t.started_ms.unwrap_or(0);
        let turn = line.is_some_and(|l| {
            l.hook.as_ref().is_some_and(|h| {
                matches!(h.event.as_str(), "Stop" | "StopFailure") && h.at_ms > after
            }) || (l.busy_known && !l.busy && seen_busy(|s| s.contains(&t.id)))
        });
        let ended = match &term {
            None => Some(Some("the term is gone (daemon restarted)".to_string())),
            Some(x) if x.exited() => Some(
                x.info()
                    .exit_code
                    .filter(|c| *c != 0)
                    .map(|c| format!("agent exited with code {c}")),
            ),
            _ => None,
        };
        let done = if crate::task_agents::is_organizer(&t) {
            "completed"
        } else {
            "in_testing"
        };
        if let Some(err) = ended {
            t.status = done.into();
            t.error = err;
            changed = true;
        } else if turn {
            t.status = done.into();
            changed = true;
        }
        if changed {
            seen_busy(|s| s.remove(&t.id));
            let _ = save(ctx, &reg, &mut t);
            if t.status == "in_testing" {
                crate::task_agents::spawn_overlap_alert(ctx, &t);
            }
            if turn && crate::task_agents::is_organizer(&t) {
                if let Some(x) = term.as_ref().filter(|x| !x.exited()) {
                    let _ = x.kill();
                }
            }
        }
    }
    if let Ok(tasks) = all(&reg) {
        for mut t in tasks.into_iter().filter(merge_candidate) {
            let busy = t
                .line_id
                .as_deref()
                .is_some_and(|id| lines.iter().any(|l| l.id == id && l.busy));
            if !busy
                && t.branch.as_deref().is_some_and(|b| {
                    crate::worktrees::landed(&t.project, b, ctx.config.worktree_fetch)
                })
            {
                t.status = "completed".into();
                t.session_state = Some("merged".into());
                let _ = save(ctx, &reg, &mut t);
            }
        }
    }
    if let Ok(_g) = START.try_lock() {
        crate::session_tasks::sync(ctx, &reg, lines);
    }
    if ctx.config.auto_organize || crate::task_agents::auto_organize_enabled(&reg) {
        crate::task_agents::auto_organize(ctx, &reg);
    }
    loop {
        let Ok(tasks) = all(&reg) else {
            return;
        };
        let running = tasks
            .iter()
            .filter(|t| {
                t.status == "in_progress"
                    && !crate::task_agents::is_organizer(t)
                    && t.live_term().is_some()
            })
            .count();
        if running >= ctx.config.auto_concurrency {
            return;
        }
        let Some(next) = tasks
            .iter()
            .filter(|t| t.status == "auto" && !t.from_session() && blockers(t, &tasks).is_empty())
            .min_by(|a, b| {
                (a.rank, a.created_ms)
                    .partial_cmp(&(b.rank, b.created_ms))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            return;
        };
        if let Err((_, e)) = start(ctx, &next.id, None) {
            let mut n = next.clone();
            n.status = "pending".into();
            n.error = Some(e);
            let _ = save(ctx, &reg, &mut n);
        }
    }
}

pub fn projects(ctx: &Ctx, lines: &[Line]) -> Res<Vec<Value>> {
    let mut g = Git::default();
    let mut paths: Vec<String> = lines
        .iter()
        .filter(|l| !l.cwd_missing)
        .map(|l| l.worktree_path.clone())
        .collect();
    for root in &ctx.config.roots {
        paths.push(root.to_string_lossy().into_owned());
        if let Ok(rd) = std::fs::read_dir(root) {
            paths.extend(
                rd.flatten()
                    .filter(|e| e.path().join(".git").exists())
                    .map(|e| e.path().to_string_lossy().into_owned()),
            );
        }
    }
    paths.extend(
        ctx.config
            .repos
            .values()
            .map(|p| p.to_string_lossy().into_owned()),
    );
    let reg = db(ctx)?;
    paths.extend(all(&reg)?.into_iter().map(|t| t.project));
    let (present, gone): (Vec<String>, Vec<String>) = added_projects(&reg)?
        .into_iter()
        .partition(|p| Path::new(p).is_dir());
    paths.extend(present);
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut seen = HashSet::new();
    for p in paths {
        if !seen.insert(p.clone()) {
            continue;
        }
        if let Some((root, repo)) = main_root(&mut g, &p) {
            out.entry(root).or_insert(repo);
        }
    }
    let missing: Vec<String> = gone.into_iter().filter(|p| !out.contains_key(p)).collect();
    let mut v: Vec<Value> = out
        .into_iter()
        .map(|(path, repo)| project_json(&path, &repo, false))
        .collect();
    v.extend(missing.iter().map(|p| project_json(p, "", true)));
    v.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(v)
}

fn added_projects(reg: &Registry) -> Res<Vec<String>> {
    let mut st = sql(reg.conn.prepare("SELECT path FROM projects ORDER BY path"))?;
    let rows = sql(st.query_map([], |r| r.get(0)))?;
    sql(rows.collect())
}

fn project_json(path: &str, repo: &str, missing: bool) -> Value {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let mut v = json!({"path": path, "name": name, "repo": repo, "missing": missing});
    if !missing {
        v["defaultBranch"] = json!(crate::worktrees::default_branch_cached(path));
    }
    v
}

pub fn add_project(ctx: &Ctx, b: &Value) -> Res<(bool, Value)> {
    let path = b["path"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("path required"))?;
    let path = match path.strip_prefix("~/") {
        Some(rest) => crate::home().join(rest).to_string_lossy().into_owned(),
        None => path.to_string(),
    };
    if !Path::new(&path).is_absolute() {
        return Err(bad("path must be absolute"));
    }
    if !Path::new(&path).is_dir() {
        return Err(bad(format!("{path} is not a directory")));
    }
    let (root, repo) = main_root(&mut Git::default(), &path)
        .ok_or_else(|| bad(format!("{path} is not in a git repository")))?;
    let reg = db(ctx)?;
    let n = sql(reg.conn.execute(
        "INSERT OR IGNORE INTO projects (path, added_ms) VALUES (?1, ?2)",
        params![root, crate::procs::now_ms()],
    ))?;
    Ok((n > 0, project_json(&root, &repo, false)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Fix the Login page!"), "fix-the-login-page");
        assert_eq!(slug("  ##  "), "task");
        assert_eq!(slug("Été 2026 — élan"), "t-2026-lan");
        assert!(slug(&"word ".repeat(30)).len() <= 40);
    }

    #[test]
    fn attachments_are_listed_after_the_description() {
        let f = [std::path::PathBuf::from("/h/a.png"), "/h/b.pdf".into()];
        assert_eq!(
            with_attachments("Fix it\n", &f),
            "Fix it\n\nAttachments:\n- /h/a.png\n- /h/b.pdf"
        );
        assert_eq!(with_attachments("  ", &f[..1]), "Attachments:\n- /h/a.png");
    }

    #[test]
    fn pr_followup_asks_only_for_the_checked_steps() {
        assert_eq!(pr_followup(false, false), None);
        let approve = pr_followup(false, true).unwrap();
        assert!(approve.contains("gh pr review --approve") && !approve.contains("gh pr merge"));
        let merge = pr_followup(true, false).unwrap();
        assert!(merge.contains("gh pr merge") && !merge.contains("--approve"));
        let both = pr_followup(true, true).unwrap();
        assert!(both.find("--approve").unwrap() < both.find("gh pr merge").unwrap());
    }

    fn dep(id: &str, status: &str, deps: &[&str]) -> Task {
        Task {
            id: id.into(),
            status: status.into(),
            depends_on: deps.iter().map(|d| d.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn dependencies_reject_self_unknown_and_cycles() {
        let tasks = [
            dep("a", "pending", &[]),
            dep("b", "pending", &["a"]),
            dep("c", "pending", &["b"]),
        ];
        let ids = |v: &[&str]| v.iter().map(|d| d.to_string()).collect::<Vec<_>>();
        assert_eq!(
            check_deps("c", &ids(&["b", "a", "b"]), &tasks).unwrap(),
            ["b", "a"]
        );
        assert_eq!(check_deps("", &ids(&["c"]), &tasks).unwrap(), ["c"]);
        assert!(check_deps("a", &ids(&["a"]), &tasks)
            .unwrap_err()
            .1
            .contains("itself"));
        assert!(check_deps("a", &ids(&["zz"]), &tasks)
            .unwrap_err()
            .1
            .contains("not a task"));
        let e = check_deps("a", &ids(&["c"]), &tasks).unwrap_err();
        assert_eq!(e.0, 400);
        assert!(e.1.contains("cycle"));
        assert!(check_deps("b", &ids(&["c"]), &tasks).is_err());
    }

    #[test]
    fn removing_a_task_cleans_its_dependents_in_one_transaction() {
        let dir = std::env::temp_dir().join(format!("sb-task-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let reg = Registry::open(&dir).unwrap();
        let mut ids = vec![];
        for (n, deps) in [(1, vec![]), (2, vec![]), (3, vec![0, 1]), (4, vec![0])] {
            let mut t = Task {
                id: format!("t{n}"),
                title: format!("t{n}"),
                project: "/p".into(),
                status: "pending".into(),
                depends_on: deps.iter().map(|i: &usize| format!("t{}", i + 1)).collect(),
                ..Default::default()
            };
            insert(&reg, &mut t, "st").unwrap();
            ids.push(t.id);
        }
        reg.conn
            .execute_batch(
                "CREATE TEMP TRIGGER no_delete BEFORE DELETE ON tasks BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .unwrap();
        assert!(remove(&reg, "t1").is_err());
        assert_eq!(get(&reg, "t3").unwrap().depends_on, ["t1", "t2"]);
        assert!(get(&reg, "t1").is_ok());
        reg.conn.execute_batch("DROP TRIGGER no_delete;").unwrap();
        let changed = remove(&reg, "t1").unwrap();
        assert_eq!(
            changed.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["t3", "t4"]
        );
        assert_eq!(get(&reg, "t3").unwrap().depends_on, ["t2"]);
        assert!(get(&reg, "t4").unwrap().depends_on.is_empty());
        assert_eq!(get(&reg, "t1").unwrap_err().0, 404);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dependency_refs_take_ids_or_numbers() {
        assert_eq!(dep_ref(&json!("01ABC")), Some(DepRef::Id("01ABC".into())));
        assert_eq!(dep_ref(&json!(12)), Some(DepRef::Number(12)));
        assert_eq!(dep_ref(&json!("#7")), Some(DepRef::Number(7)));
        assert_eq!(dep_ref(&json!(" 8 ")), Some(DepRef::Number(8)));
        assert_eq!(dep_ref(&json!(true)), None);
    }

    #[test]
    fn a_task_is_blocked_until_every_present_dependency_completes() {
        let mut tasks = vec![
            dep("a", "in_testing", &[]),
            dep("b", "completed", &[]),
            dep("c", "auto", &["a", "b", "gone"]),
        ];
        let ids = |t: &[Task]| {
            blockers(&t[2], t)
                .iter()
                .map(|x| x.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&tasks), ["a"]);
        tasks[0].status = "completed".into();
        assert!(ids(&tasks).is_empty());
    }

    #[test]
    fn only_review_tasks_not_yet_auto_completed_are_merge_candidates() {
        let mut t = Task {
            status: "in_testing".into(),
            ..Default::default()
        };
        assert!(merge_candidate(&t));
        t.session_state = Some("merged".into());
        assert!(!merge_candidate(&t));
        t.session_state = None;
        t.origin = Some(crate::session_tasks::ORIGIN.into());
        assert!(!merge_candidate(&t));
        t.origin = None;
        for s in ["pending", "auto", "in_progress", "completed"] {
            t.status = s.into();
            assert!(!merge_candidate(&t), "{s}");
        }
    }
}
