use crate::board::publish;
use crate::http::{q, Request};
use crate::registry::{self, sql, Registry};
use crate::task_agents::CONFIGURATOR;
use crate::tasks::{self, Task};
use crate::util::{format_rfc3339, ticket, ulid};
use crate::workspaces::{self, Folder, Group, Workspace};
use crate::{Ctx, Line};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

type Res<T> = Result<T, (u16, String)>;

pub const VERSION: u64 = 1;
pub const MAX_ACTIONS: usize = 500;
pub const MAX_PICKUP: usize = 40;
pub const MAX_SESSIONS: usize = 300;
pub const STALE_DAYS: i64 = 30;
const DAY_MS: i64 = 86_400_000;
const KEEP: i64 = 20;
const MAX_TEXT: usize = 500;
const KINDS: &[&str] = &[
    "create_folder",
    "create_workspace",
    "move_workspace",
    "create_group",
    "hold",
    "close",
    "pin",
    "archive_workspace",
    "set_task_status",
    "track",
];
const WHY: &[&str] = &["dirty", "todos", "recent"];

pub const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS proposals (id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL,
           status TEXT NOT NULL, source TEXT NOT NULL, task_id TEXT, body TEXT NOT NULL, results TEXT)";

fn db(ctx: &Ctx) -> Res<Registry> {
    registry::db_with(ctx, SCHEMA)
}

fn trunc(s: &str, max: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

fn one_line(s: &str, max: usize) -> String {
    trunc(s.lines().find(|l| !l.trim().is_empty()).unwrap_or(""), max)
}

pub fn line_label(l: &Line) -> String {
    l.title
        .clone()
        .filter(|t| !t.is_empty())
        .or_else(|| (!l.branch.is_empty()).then(|| l.branch.clone()))
        .or_else(|| l.repo.rsplit('/').next().map(str::to_string))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| l.id.clone())
}

fn base_name(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}

pub struct World<'a> {
    pub lines: &'a [Line],
    pub workspaces: &'a [Workspace],
    pub folders: &'a [Folder],
    pub tasks: &'a [Task],
    pub remote: &'a dyn Fn(&str) -> bool,
}

impl World<'_> {
    fn line(&self, id: &str) -> Option<&Line> {
        self.lines.iter().find(|l| l.id == id)
    }
}

pub fn validate(plan: &Value, w: &World) -> Result<Value, Vec<String>> {
    let mut problems = vec![];
    let Some(obj) = plan.as_object() else {
        return Err(vec!["plan must be a JSON object".into()]);
    };
    if !matches!(obj.get("version"), None | Some(Value::Null))
        && obj["version"].as_u64() != Some(VERSION)
    {
        problems.push(format!("version must be {VERSION}"));
    }
    let empty = vec![];
    let actions = match obj.get("actions") {
        None | Some(Value::Null) => &empty,
        Some(Value::Array(a)) => a,
        _ => return Err(vec!["actions must be an array".into()]),
    };
    if actions.len() > MAX_ACTIONS {
        return Err(vec![format!("at most {MAX_ACTIONS} actions")]);
    }
    let mut ids: HashSet<String> = HashSet::new();
    let mut refs: HashMap<String, String> = HashMap::new();
    let mut out = vec![];
    for (i, a) in actions.iter().enumerate() {
        let s = |k: &str| a[k].as_str().map(str::trim).filter(|v| !v.is_empty());
        let at = format!("actions[{i}]");
        let Some(id) = s("id") else {
            problems.push(format!("{at}: id required"));
            continue;
        };
        if !ids.insert(id.to_string()) {
            problems.push(format!("{at}: duplicate id {id}"));
            continue;
        }
        let kind = s("kind").unwrap_or("");
        if !KINDS.contains(&kind) {
            problems.push(format!("{at}: unknown kind {kind:?}"));
            continue;
        }
        let mut n = json!({"id": id, "kind": kind});
        let mut fail = |m: String| problems.push(format!("{at} ({kind}): {m}"));
        let folder =
            |k: &str, refs: &HashMap<String, String>| -> Result<(String, String), String> {
                let v = s(k).ok_or(format!("{k} required"))?;
                if let Some(name) = refs.get(v) {
                    return Ok((v.to_string(), name.clone()));
                }
                w.folders
                    .iter()
                    .find(|f| f.id == v)
                    .map(|f| (v.to_string(), f.name.clone()))
                    .ok_or(format!(
                        "{k} {v:?} is neither an earlier create_folder ref nor a folder id"
                    ))
            };
        let line = |n: &mut Value| -> Result<(), String> {
            let id = s("lineId").ok_or("lineId required")?;
            n["lineId"] = json!(id);
            match w.line(id) {
                Some(l) => {
                    n["label"] = json!(line_label(l));
                    Ok(())
                }
                None if (w.remote)(id) => Ok(()),
                None => Err(format!("no line {id}")),
            }
        };
        let workspace = |n: &mut Value| -> Result<(), String> {
            let id = s("workspaceId").ok_or("workspaceId required")?;
            let ws = w
                .workspaces
                .iter()
                .find(|x| x.id == id)
                .ok_or(format!("no workspace {id}"))?;
            n["workspaceId"] = json!(id);
            n["label"] = json!(ws.name);
            Ok(())
        };
        let r: Result<(), String> = match kind {
            "create_folder" => (|| {
                let name = s("name").ok_or("name required")?;
                let r = s("ref").ok_or("ref required")?;
                if refs.contains_key(r) {
                    return Err(format!("ref {r} used twice"));
                }
                refs.insert(r.to_string(), name.to_string());
                n["name"] = json!(trunc(name, 80));
                n["ref"] = json!(r);
                n["label"] = json!(trunc(name, 80));
                Ok(())
            })(),
            "create_workspace" => (|| {
                let path = s("path").ok_or("path required")?;
                if !Path::new(path).is_absolute() {
                    return Err("path must be absolute".into());
                }
                n["path"] = json!(path);
                n["label"] = json!(base_name(path));
                if !a["folder"].is_null() {
                    let (f, name) = folder("folder", &refs)?;
                    n["folder"] = json!(f);
                    n["folderName"] = json!(name);
                }
                match &a["pinned"] {
                    Value::Null => {}
                    Value::Bool(b) => n["pinned"] = json!(b),
                    _ => return Err("pinned must be a boolean".into()),
                }
                Ok(())
            })(),
            "move_workspace" => (|| {
                workspace(&mut n)?;
                let (f, name) = folder("folder", &refs)?;
                n["folder"] = json!(f);
                n["folderName"] = json!(name);
                Ok(())
            })(),
            "archive_workspace" => workspace(&mut n),
            "create_group" => (|| {
                let name = s("name").ok_or("name required")?;
                let lines: Vec<&str> = a["lines"]
                    .as_array()
                    .map(|v| v.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if lines.is_empty() {
                    return Err("lines must be a non-empty array of line ids".into());
                }
                if let Some(x) = lines.iter().find(|x| w.line(x).is_none() && !(w.remote)(x)) {
                    return Err(format!("no line {x}"));
                }
                n["name"] = json!(trunc(name, 80));
                n["lines"] = json!(lines);
                n["label"] = json!(trunc(name, 80));
                Ok(())
            })(),
            "hold" | "pin" | "track" => line(&mut n),
            "close" => line(&mut n).and_then(|_| match w.line(s("lineId").unwrap_or("")) {
                Some(l) if l.busy => Err("the line is busy".into()),
                Some(l) if l.needs_input => Err("the line needs input".into()),
                _ => Ok(()),
            }),
            "set_task_status" => (|| {
                let id = s("taskId").ok_or("taskId required")?;
                let t = w
                    .tasks
                    .iter()
                    .find(|t| t.id == id)
                    .ok_or(format!("no task {id}"))?;
                let st = s("status").unwrap_or("");
                if !crate::status::PROPOSAL_SETTABLE.contains(&st) {
                    return Err(format!(
                        "status must be one of {}",
                        crate::status::PROPOSAL_SETTABLE.join(", ")
                    ));
                }
                n["taskId"] = json!(id);
                n["status"] = json!(st);
                n["label"] = json!(format!("#{} {}", t.number, t.title));
                Ok(())
            })(),
            _ => unreachable!(),
        };
        match r {
            Ok(()) => out.push(n),
            Err(m) => fail(m),
        }
    }
    let mut reasons = Map::new();
    if let Some(m) = obj.get("reasons").and_then(Value::as_object) {
        for (k, v) in m {
            if let (true, Some(t)) = (ids.contains(k), v.as_str()) {
                reasons.insert(k.clone(), json!(trunc(t, MAX_TEXT)));
            }
        }
    }
    let picks = obj
        .get("pickUp")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if picks.len() > MAX_PICKUP {
        problems.push(format!("at most {MAX_PICKUP} pickUp items"));
    }
    let mut pick_up = vec![];
    for (i, p) in picks.iter().take(MAX_PICKUP).enumerate() {
        let id = p["lineId"].as_str().unwrap_or("");
        let why = p["why"].as_str().unwrap_or("");
        let Some(l) = w.line(id) else {
            problems.push(format!("pickUp[{i}]: no line {id:?}"));
            continue;
        };
        if !WHY.contains(&why) {
            problems.push(format!(
                "pickUp[{i}]: why must be one of {}",
                WHY.join(", ")
            ));
            continue;
        }
        let mut v = json!({
            "lineId": id, "why": why, "title": line_label(l),
            "summary": one_line(p["summary"].as_str().unwrap_or(""), 200),
            "branch": l.branch, "repo": l.repo,
        });
        for k in ["edited", "todos"] {
            if let Some(x) = p[k].as_u64() {
                v[k] = json!(x);
            }
        }
        if let Some(x) = p["workspaceId"].as_str() {
            v["workspaceId"] = json!(x);
        }
        pick_up.push(v);
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(json!({
        "version": VERSION,
        "summary": trunc(obj.get("summary").and_then(Value::as_str).unwrap_or(""), 2000),
        "actions": out,
        "pickUp": pick_up,
        "reasons": reasons,
    }))
}

pub struct Input<'a> {
    pub lines: &'a [Line],
    pub workspaces: &'a [Workspace],
    pub folders: &'a [Folder],
    pub groups: &'a [Group],
    pub projects: &'a [(String, String)],
    pub tasks: &'a [Task],
    pub station: &'a str,
    pub now_ms: i64,
    pub stale_days: i64,
    pub member: &'a dyn Fn(&Line) -> Option<String>,
    pub landed: &'a dyn Fn(&Task) -> Option<bool>,
    pub activity: &'a dyn Fn(&Line) -> Option<Value>,
}

fn owner_of(repo: &str, path: &str) -> String {
    let parts: Vec<&str> = repo.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() >= 3 {
        return parts[1].to_string();
    }
    Path::new(path)
        .parent()
        .map(|p| base_name(&p.to_string_lossy()))
        .unwrap_or_default()
}

struct Plan {
    actions: Vec<Value>,
    reasons: Map<String, Value>,
}

impl Plan {
    fn add(&mut self, mut a: Value, why: String) -> String {
        let id = format!("a{}", self.actions.len() + 1);
        a["id"] = json!(id);
        self.reasons.insert(id.clone(), json!(why));
        self.actions.push(a);
        id
    }
}

pub fn deterministic(i: &Input) -> Value {
    let stale_ms = i.stale_days.max(1) * DAY_MS;
    let mut p = Plan {
        actions: vec![],
        reasons: Map::new(),
    };
    let local = |l: &Line| l.station_id.is_empty() || l.station_id == i.station;
    let live = |l: &&Line| l.state != crate::wire::STATE_HUNG_UP && !l.duplicate;
    let repo_root = |l: &Line| {
        let path = if l.worktree_path.is_empty() {
            &l.cwd
        } else {
            &l.worktree_path
        };
        workspaces::repo_root(i.projects, &l.repo, path)
            .unwrap_or(path)
            .to_string()
    };
    let mut roots: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for l in i.lines.iter().filter(live) {
        if !local(l) || l.cwd_missing || !l.recent(i.now_ms) || (i.member)(l).is_some() {
            continue;
        }
        let root = repo_root(l);
        if root.is_empty() || root == "/" {
            continue;
        }
        let e = roots
            .entry(root.clone())
            .or_insert((owner_of(&l.repo, &root), 0));
        e.1 += 1;
    }
    roots.retain(|r, _| !i.workspaces.iter().any(|w| w.path == *r && !w.archived));
    let repo_of = |path: &str| {
        i.projects
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, r)| r.as_str())
            .unwrap_or("")
    };
    let kept: Vec<&Workspace> = i.workspaces.iter().filter(|w| !w.archived).collect();
    let mut owners: BTreeMap<String, (Vec<&Workspace>, Vec<String>)> = BTreeMap::new();
    for w in &kept {
        let o = owners
            .entry(owner_of(repo_of(&w.path), &w.path))
            .or_default();
        if w.folder_id.is_none() && w.parent_id.is_none() {
            o.0.push(w);
        }
    }
    for (root, (owner, _)) in &roots {
        owners
            .entry(owner.clone())
            .or_default()
            .1
            .push(root.clone());
    }
    let mut folder_of: HashMap<String, String> = HashMap::new();
    if owners.len() >= 2 {
        for (owner, (ws, new)) in &owners {
            if owner.is_empty() || ws.len() + new.len() < 2 {
                continue;
            }
            let target = match i
                .folders
                .iter()
                .find(|f| f.parent_id.is_none() && f.name.eq_ignore_ascii_case(owner))
            {
                Some(f) => f.id.clone(),
                None => {
                    let r = format!("f-{owner}");
                    p.add(
                        json!({"kind": "create_folder", "name": owner, "ref": r}),
                        format!("{} projects belong to {owner}", ws.len() + new.len()),
                    );
                    r
                }
            };
            for w in ws {
                p.add(
                    json!({"kind": "move_workspace", "workspaceId": w.id, "folder": target}),
                    format!("{} belongs to {owner}", w.name),
                );
            }
            folder_of.insert(owner.clone(), target);
        }
    }
    for (root, (owner, n)) in &roots {
        let mut a = json!({"kind": "create_workspace", "path": root});
        if let Some(f) = folder_of.get(owner) {
            a["folder"] = json!(f);
        }
        p.add(a, format!("{n} recent session(s) here have no workspace"));
    }
    let mut tickets: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in i.lines.iter().filter(live).filter(|l| l.recent(i.now_ms)) {
        if let Some(k) = l.ticket.clone().or_else(|| ticket(&l.branch)) {
            tickets.entry(k).or_default().push(l.id.clone());
        }
    }
    let group_names: HashSet<String> = i.groups.iter().map(|g| g.name.to_lowercase()).collect();
    for (k, ids) in tickets {
        if ids.len() >= 2 && !group_names.contains(&k.to_lowercase()) {
            let n = ids.len();
            p.add(
                json!({"kind": "create_group", "name": k, "lines": ids}),
                format!("{n} sessions work on {k}"),
            );
        }
    }
    for l in i.lines.iter().filter(live) {
        let quiet = !l.busy && !l.needs_input && !l.pinned;
        if l.alive() && quiet && l.held_reason.is_none() && i.now_ms - l.last_activity_ms > stale_ms
        {
            let days = (i.now_ms - l.last_activity_ms) / DAY_MS;
            p.add(
                json!({"kind": "hold", "lineId": l.id}),
                format!("idle for {days} days: hold frees its memory, resume brings it back"),
            );
        } else if l.cwd_missing && quiet && local(l) {
            p.add(
                json!({"kind": "close", "lineId": l.id}),
                format!("its folder {} no longer exists", l.cwd),
            );
        }
    }
    let open =
        |t: &Task| !crate::status::is_done(&t.status) && !crate::task_agents::is_organizer(t);
    for t in i.tasks.iter().filter(|t| open(t)) {
        let running = t.status == "in_progress" && !t.from_session();
        if !running && t.branch.is_some() && (i.landed)(t) == Some(true) {
            p.add(
                json!({"kind": "set_task_status", "taskId": t.id, "status": "merged"}),
                format!(
                    "branch {} is merged into the default branch",
                    t.branch.as_deref().unwrap_or("")
                ),
            );
        }
    }
    for w in &kept {
        if w.pinned || i.now_ms - w.updated_ms <= stale_ms {
            continue;
        }
        let active = i.lines.iter().any(|l| {
            (l.alive() || i.now_ms - l.last_activity_ms <= stale_ms)
                && (i.member)(l).as_deref() == Some(w.id.as_str())
        });
        let tasks = i.tasks.iter().any(|t| open(t) && t.project == w.path);
        if !active && !tasks {
            p.add(
                json!({"kind": "archive_workspace", "workspaceId": w.id}),
                format!(
                    "no session or open task in {} days: archiving hides it and keeps it",
                    i.stale_days
                ),
            );
        }
    }
    p.actions.truncate(MAX_ACTIONS);
    let pick_up = pick_up(i);
    let count = |k: &str| p.actions.iter().filter(|a| a["kind"] == k).count();
    let summary = format!(
        "{} workspace(s) to create, {} folder(s), {} group(s), {} session(s) to hold, {} to close, {} task(s) done, {} workspace(s) to archive; {} session(s) to pick up.",
        count("create_workspace"),
        count("create_folder"),
        count("create_group"),
        count("hold"),
        count("close"),
        count("set_task_status"),
        count("archive_workspace"),
        pick_up.len()
    );
    json!({
        "version": VERSION, "summary": summary, "actions": p.actions,
        "pickUp": pick_up, "reasons": p.reasons,
    })
}

fn pick_up(i: &Input) -> Vec<Value> {
    let mut cands: Vec<&Line> = i
        .lines
        .iter()
        .filter(|l| {
            l.state != crate::wire::STATE_HUNG_UP
                && !l.duplicate
                && !l.busy
                && !l.cwd_missing
                && (l.station_id.is_empty() || l.station_id == i.station)
                && i.now_ms - l.last_activity_ms <= crate::RECENT_MS
        })
        .collect();
    cands.sort_by_key(|l| std::cmp::Reverse(l.last_activity_ms));
    cands.truncate(MAX_PICKUP + MAX_PICKUP / 2);
    let done = |l: &Line| {
        i.tasks.iter().any(|t| {
            t.line_id.as_deref() == Some(l.id.as_str())
                && (crate::status::is_done(&t.status) || (i.landed)(t) == Some(true))
        })
    };
    let mut out: Vec<(usize, i64, Value)> = cands
        .into_iter()
        .map(|l| {
            let a = (i.activity)(l).unwrap_or(Value::Null);
            let todos = a["todos"]
                .as_array()
                .map(|t| t.iter().filter(|x| x["status"] != "completed").count())
                .unwrap_or(0);
            let edited = a["editedFiles"].as_array().map_or(0, Vec::len);
            let (rank, why) = if edited > 0 && !done(l) {
                (0, "dirty")
            } else if todos > 0 {
                (1, "todos")
            } else {
                (2, "recent")
            };
            let summary = ["summary", "lastAssistantText", "lastPrompt", "goal"]
                .iter()
                .find_map(|k| a[*k].as_str().filter(|s| !s.trim().is_empty()))
                .map(|s| one_line(s, 160))
                .unwrap_or_default();
            let mut v = json!({"lineId": l.id, "why": why, "summary": summary,
                               "edited": edited, "todos": todos});
            if let Some(w) = (i.member)(l) {
                v["workspaceId"] = json!(w);
            }
            (rank, l.last_activity_ms, v)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    out.into_iter().take(MAX_PICKUP).map(|x| x.2).collect()
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let body: String = r.get(5)?;
    let mut v: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));
    v["id"] = json!(r.get::<_, String>(0)?);
    v["createdAt"] = json!(format_rfc3339(r.get(1)?));
    v["status"] = json!(r.get::<_, String>(2)?);
    v["source"] = json!(r.get::<_, String>(3)?);
    if let Some(t) = r.get::<_, Option<String>>(4)? {
        v["taskId"] = json!(t);
    }
    if let Some(res) = r.get::<_, Option<String>>(6)? {
        v["results"] = serde_json::from_str(&res).unwrap_or(Value::Null);
    }
    Ok(v)
}

const SELECT: &str = "SELECT id, created_ms, status, source, task_id, body, results FROM proposals";

pub fn get(reg: &Registry, id: &str) -> Res<Value> {
    sql(reg
        .conn
        .query_row(&format!("{SELECT} WHERE id = ?1"), [id], row)
        .optional())?
    .ok_or_else(|| (404, format!("no proposal {id}")))
}

pub fn list(reg: &Registry) -> Res<Vec<Value>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "{SELECT} ORDER BY created_ms DESC, id DESC LIMIT {KEEP}"
    )))?;
    let rows = sql(st.query_map([], row))?;
    sql(rows.collect())
}

pub fn store(reg: &Registry, plan: &Value, source: &str, task: Option<&str>) -> Res<Value> {
    let now = crate::util::now_ms();
    let id = ulid(now);
    sql(reg.conn.execute(
        "INSERT INTO proposals (id, created_ms, status, source, task_id, body) VALUES (?1, ?2, 'pending_review', ?3, ?4, ?5)",
        params![id, now, source, task, plan.to_string()],
    ))?;
    sql(reg.conn.execute(
        &format!("DELETE FROM proposals WHERE id NOT IN (SELECT id FROM proposals ORDER BY created_ms DESC, id DESC LIMIT {KEEP})"),
        [],
    ))?;
    let p = get(reg, &id)?;
    publish("organize.proposal", json!({"proposal": p}));
    Ok(p)
}

fn set_status(reg: &Registry, id: &str, status: &str, results: Option<&Value>) -> Res<Value> {
    sql(reg.conn.execute(
        "UPDATE proposals SET status = ?2, results = COALESCE(?3, results) WHERE id = ?1",
        params![id, status, results.map(Value::to_string)],
    ))?;
    let p = get(reg, id)?;
    publish("organize.proposal", json!({"proposal": p}));
    Ok(p)
}

fn index(ctx: &Ctx, lines: &[Line]) -> Res<workspaces::Index> {
    workspaces::list(ctx)?;
    workspaces::Index::new(ctx, lines)
}

fn world_check(ctx: &Ctx, reg: &Registry, plan: &Value) -> Result<Value, (u16, Value)> {
    let lines = crate::daemon::lines(ctx);
    let ws = workspaces::all(reg).map_err(|(c, e)| (c, json!({"error": e})))?;
    let folders = workspaces::folders(reg).map_err(|(c, e)| (c, json!({"error": e})))?;
    let tasks = tasks::all(reg).map_err(|(c, e)| (c, json!({"error": e})))?;
    let remote = |id: &str| crate::board::remote_station(ctx, id).is_some();
    validate(
        plan,
        &World {
            lines: &lines,
            workspaces: &ws,
            folders: &folders,
            tasks: &tasks,
            remote: &remote,
        },
    )
    .map_err(|p| {
        (
            400,
            json!({"error": format!("invalid plan: {}", p.join("; ")), "problems": p}),
        )
    })
}

pub fn build(ctx: &Ctx) -> Result<Value, (u16, Value)> {
    let e = |(c, e): (u16, String)| (c, json!({ "error": e }));
    let lines = crate::daemon::lines(ctx);
    let ix = index(ctx, &lines).map_err(e)?;
    let reg = db(ctx).map_err(e)?;
    let folders = workspaces::folders(&reg).map_err(e)?;
    let groups = workspaces::groups(&reg).map_err(e)?;
    let member = |l: &Line| ix.of_line(l).map(|w| w.id.clone());
    let landed = |t: &Task| crate::worktrees::branch_landed_cached(&t.project, t.branch.as_deref());
    let activity = |l: &Line| {
        crate::activity::find_transcript(
            &ctx.claude_dir,
            &ctx.codex_dir,
            &ctx.gemini_dir,
            l.agent,
            &l.session_id,
        )
        .and_then(|p| crate::activity::activity(l.agent, &p, Some(ctx.now_ms)))
    };
    // Subjects have no directory to organize or archive by.
    let dirs: Vec<Workspace> = ix
        .workspaces
        .iter()
        .filter(|w| !w.subject)
        .cloned()
        .collect();
    let plan = deterministic(&Input {
        lines: &lines,
        workspaces: &dirs,
        folders: &folders,
        groups: &groups,
        projects: &ix.projects,
        tasks: &ix.tasks,
        station: &ctx.config.station_id,
        now_ms: ctx.now_ms,
        stale_days: STALE_DAYS,
        member: &member,
        landed: &landed,
        activity: &activity,
    });
    let plan = world_check(ctx, &reg, &plan)?;
    store(&reg, &plan, "deterministic", None).map_err(e)
}

fn record(ctx: &Ctx, b: &Value) -> Result<Value, (u16, Value)> {
    let e = |(c, e): (u16, String)| (c, json!({ "error": e }));
    let reg = db(ctx).map_err(e)?;
    let task = match b["taskId"].as_str() {
        None => None,
        Some(id) => {
            let t = tasks::get(&reg, id).map_err(e)?;
            if t.origin.as_deref() != Some(CONFIGURATOR) {
                return Err((
                    403,
                    json!({"error": "only the session configurator may propose an organization"}),
                ));
            }
            Some(t.id)
        }
    };
    let plan = world_check(ctx, &reg, &b["plan"])?;
    store(&reg, &plan, "agent", task.as_deref()).map_err(e)
}

pub struct Outcome {
    pub status: &'static str,
    pub detail: Option<String>,
}

fn done(d: impl Into<String>) -> Outcome {
    Outcome {
        status: "done",
        detail: Some(d.into()).filter(|s: &String| !s.is_empty()),
    }
}

fn skipped(d: impl Into<String>) -> Outcome {
    Outcome {
        status: "skipped",
        detail: Some(d.into()),
    }
}

fn failed(d: impl Into<String>) -> Outcome {
    Outcome {
        status: "failed",
        detail: Some(d.into()),
    }
}

pub trait Ops {
    fn remote(&self, line: &str) -> Option<(String, bool)>;
    fn relay(&self, line: &str, kind: &str) -> Result<(), String>;
    fn hold(&self, l: &Line) -> Result<(), String>;
    fn close(&self, l: &Line) -> Result<(), String>;
    fn create_workspace(&self, b: &Value) -> Res<(bool, Workspace)>;
    fn patch_workspace(&self, id: &str, b: &Value) -> Res<Workspace>;
    fn patch_task(&self, id: &str, b: &Value) -> Res<Task>;
    fn project_of(&self, l: &Line) -> Option<String>;
    fn station(&self) -> String;
}

struct Live<'a, 'b>(&'a Ctx<'b>);

fn report(r: Result<crate::actions::Report, String>) -> Result<(), String> {
    let r = r?;
    if r.ok() {
        return Ok(());
    }
    let why: Vec<String> = r
        .checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .chain(r.error.clone())
        .collect();
    Err(why.join("; "))
}

impl Ops for Live<'_, '_> {
    fn remote(&self, line: &str) -> Option<(String, bool)> {
        crate::board::remote_station(self.0, line).map(|(s, online, _)| (s, online))
    }
    fn relay(&self, line: &str, kind: &str) -> Result<(), String> {
        match crate::remote::relay(self.0, line, kind, json!({"dryRun": false})) {
            (c, _) if c < 400 => Ok(()),
            (_, v) => Err(v["error"].as_str().unwrap_or("relay failed").to_string()),
        }
    }
    fn hold(&self, l: &Line) -> Result<(), String> {
        report(Ok(crate::actions::hold_line(
            self.0, l, true, true, false, false,
        )))
    }
    fn close(&self, l: &Line) -> Result<(), String> {
        report(crate::actions::close_line(self.0, l, true, false, false))
    }
    fn create_workspace(&self, b: &Value) -> Res<(bool, Workspace)> {
        workspaces::create(self.0, b)
    }
    fn patch_workspace(&self, id: &str, b: &Value) -> Res<Workspace> {
        workspaces::patch(self.0, id, b)
    }
    fn patch_task(&self, id: &str, b: &Value) -> Res<Task> {
        tasks::patch(self.0, id, b)
    }
    fn project_of(&self, l: &Line) -> Option<String> {
        let path = if l.worktree_path.is_empty() {
            &l.cwd
        } else {
            &l.worktree_path
        };
        tasks::main_root(&mut crate::git::Git::default(), path).map(|(r, _)| r)
    }
    fn station(&self) -> String {
        self.0.config.station_id.clone()
    }
}

pub fn run_action(
    ops: &dyn Ops,
    reg: &Registry,
    lines: &[Line],
    refs: &mut HashMap<String, String>,
    a: &Value,
) -> Outcome {
    let s = |k: &str| a[k].as_str().unwrap_or("");
    let line = || lines.iter().find(|l| l.id == s("lineId"));
    let folder = |refs: &HashMap<String, String>| -> Result<String, String> {
        let f = s("folder");
        refs.get(f).cloned().map(Ok).unwrap_or_else(|| {
            workspaces::folders(reg)
                .map_err(|e| e.1)?
                .into_iter()
                .find(|x| x.id == f)
                .map(|x| x.id)
                .ok_or(format!(
                    "folder {f} was not created: check its create_folder"
                ))
        })
    };
    let r: Result<Outcome, String> = (|| match s("kind") {
        "create_folder" => {
            let all = workspaces::folders(reg).map_err(|e| e.1)?;
            if let Some(f) = all
                .iter()
                .find(|f| f.parent_id.is_none() && f.name.eq_ignore_ascii_case(s("name")))
            {
                refs.insert(s("ref").into(), f.id.clone());
                return Ok(skipped(format!("folder {} exists", f.name)));
            }
            let f = workspaces::create_folder(&json!({"name": s("name")}), reg).map_err(|e| e.1)?;
            refs.insert(s("ref").into(), f.id);
            Ok(done(""))
        }
        "create_workspace" => {
            let f = match a["folder"].is_null() {
                true => None,
                false => Some(folder(refs)?),
            };
            let mut b = json!({"path": s("path")});
            if let Some(p) = a["pinned"].as_bool() {
                b["pinned"] = json!(p);
            }
            if let Some(f) = &f {
                b["folderId"] = json!(f);
            }
            let (created, w) = ops.create_workspace(&b).map_err(|e| e.1)?;
            if created {
                return Ok(done(""));
            }
            match f {
                Some(f) if w.folder_id.as_deref() != Some(f.as_str()) => {
                    ops.patch_workspace(&w.id, &json!({"folderId": f}))
                        .map_err(|e| e.1)?;
                    Ok(done("workspace existed: moved into the folder"))
                }
                _ => Ok(skipped("workspace exists")),
            }
        }
        "move_workspace" => {
            let f = folder(refs)?;
            let w = workspaces::get(reg, s("workspaceId")).map_err(|e| e.1)?;
            if w.parent_id.is_some() {
                return Ok(skipped("nested in another workspace"));
            }
            if w.folder_id.as_deref() == Some(f.as_str()) {
                return Ok(skipped("already in the folder"));
            }
            ops.patch_workspace(&w.id, &json!({"folderId": f}))
                .map_err(|e| e.1)?;
            Ok(done(""))
        }
        "archive_workspace" => {
            let w = workspaces::get(reg, s("workspaceId")).map_err(|e| e.1)?;
            if w.archived {
                return Ok(skipped("already archived"));
            }
            ops.patch_workspace(&w.id, &json!({"archived": true}))
                .map_err(|e| e.1)?;
            Ok(done(""))
        }
        "create_group" => {
            let keys: Vec<String> = a["lines"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|id| format!("line:{id}"))
                .collect();
            let all = workspaces::groups(reg).map_err(|e| e.1)?;
            match all.iter().find(|g| g.name.eq_ignore_ascii_case(s("name"))) {
                Some(g) => {
                    let add: Vec<&String> =
                        keys.iter().filter(|k| !g.members.contains(k)).collect();
                    if add.is_empty() {
                        return Ok(skipped("group exists with these sessions"));
                    }
                    workspaces::patch_group(reg, &g.id, &json!({"add": add})).map_err(|e| e.1)?;
                    Ok(done(format!(
                        "added {} session(s) to {}",
                        add.len(),
                        g.name
                    )))
                }
                None => {
                    workspaces::create_group(reg, &json!({"name": s("name"), "members": keys}))
                        .map_err(|e| e.1)?;
                    Ok(done(""))
                }
            }
        }
        k @ ("hold" | "close") => {
            if let Some((station, online)) = ops.remote(s("lineId")) {
                if !online {
                    return Err(format!("station {station} is offline"));
                }
                ops.relay(s("lineId"), k)?;
                return Ok(done(format!("relayed to {station}")));
            }
            let l = line().ok_or(format!("no line {}", s("lineId")))?;
            if k == "close" {
                if l.state == crate::wire::STATE_HUNG_UP {
                    return Ok(skipped("already closed"));
                }
                if l.busy || l.needs_input {
                    return Err("the session is busy or waits for input".into());
                }
                ops.close(l)?;
            } else {
                if l.held_reason.is_some() || l.state == crate::wire::STATE_HELD {
                    return Ok(skipped("already held"));
                }
                if !l.alive() {
                    return Ok(skipped("not running"));
                }
                ops.hold(l)?;
            }
            Ok(done(""))
        }
        "pin" => {
            if ops.remote(s("lineId")).is_some() {
                return Err("pin a session of another station on that station".into());
            }
            let l = line().ok_or(format!("no line {}", s("lineId")))?;
            if l.pinned {
                return Ok(skipped("already pinned"));
            }
            reg.set_mark(&l.id, "pinned", Some("1"))?;
            Ok(done(""))
        }
        "track" => {
            if ops.remote(s("lineId")).is_some() {
                return Err("track a session of another station on that station".into());
            }
            let l = line().ok_or(format!("no line {}", s("lineId")))?;
            let all = tasks::all(reg).map_err(|e| e.1)?;
            if all
                .iter()
                .any(|t| t.line_id.as_deref() == Some(l.id.as_str()))
            {
                return Ok(skipped("already has a task"));
            }
            let project = ops
                .project_of(l)
                .ok_or("the session is not in a git repository")?;
            let mut t = crate::session_tasks::new_task(l, project, None);
            let created = tasks::insert(reg, &mut t, &ops.station()).map_err(|e| e.1)?;
            Ok(match created {
                true => done(format!("task #{}", t.number)),
                false => skipped("already has a task"),
            })
        }
        "set_task_status" => {
            let t = tasks::get(reg, s("taskId")).map_err(|e| e.1)?;
            if t.status == s("status") {
                return Ok(skipped(format!("already {}", t.status)));
            }
            ops.patch_task(&t.id, &json!({"status": s("status")}))
                .map_err(|e| e.1)?;
            Ok(done(""))
        }
        k => Err(format!("unknown kind {k}")),
    })();
    r.unwrap_or_else(failed)
}

pub fn apply_with(
    ops: &dyn Ops,
    reg: &Registry,
    lines: &[Line],
    proposal: &Value,
    ids: &[String],
) -> Vec<Value> {
    let mut refs = HashMap::new();
    proposal["actions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["id"].as_str().is_some_and(|i| ids.iter().any(|x| x == i)))
        .map(|a| {
            let o = run_action(ops, reg, lines, &mut refs, a);
            let mut v = json!({"actionId": a["id"], "kind": a["kind"], "status": o.status});
            if let Some(d) = o.detail {
                v["detail"] = json!(d);
            }
            v
        })
        .collect()
}

fn apply(ctx: &Ctx, id: &str, b: &Value) -> Res<Value> {
    let ids: Vec<String> = match &b["actionIds"] {
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect(),
        _ => return Err((400, "actionIds must be an array of action ids".into())),
    };
    let reg = db(ctx)?;
    let p = get(&reg, id)?;
    if p["status"] == "dismissed" {
        return Err((409, "the proposal was dismissed".into()));
    }
    let acts = p["actions"].as_array().cloned().unwrap_or_default();
    if let Some(x) = ids
        .iter()
        .find(|x| !acts.iter().any(|a| a["id"] == x.as_str()))
    {
        return Err((400, format!("no action {x} in this proposal")));
    }
    let fresh = acts.iter().any(|a| {
        matches!(a["kind"].as_str(), Some("hold" | "close"))
            && ids.iter().any(|x| a["id"] == x.as_str())
    });
    let lines = match fresh {
        true => crate::scan(ctx).map_err(|e| (500, e))?,
        false => crate::daemon::lines(ctx),
    };
    let results = apply_with(&Live(ctx), &reg, &lines, &p, &ids);
    crate::actions::log_action(
        ctx,
        &json!({"action": "organize", "proposalId": id, "source": p["source"], "results": results}),
    );
    set_status(&reg, id, "applied", Some(&json!(results)))?;
    Ok(json!({"proposalId": id, "results": results}))
}

pub fn configurator_prompt() -> String {
    format!(
        "You are the Overseer session configurator. Propose how to organize the user's coding agent sessions; the user reviews your proposal and applies what they accept.\n\n\
Use the overseer MCP tools: list_layout (workspaces, folders, groups), list_projects, list_sessions (sinceDays 90), get_session_brief for sessions whose purpose is unclear (at most 40), list_worktrees for projects with many sessions.\n\n\
Heuristics: one workspace per project for sessions that have none, folders per owner or client when there are several, a group per ticket key or feature shared by several sessions, hold sessions idle for more than {STALE_DAYS} days (reversible), close only sessions whose folder is gone, set_task_status merged for tasks whose branch is merged, archive_workspace for projects without activity, track old sessions worth keeping, pin the few the user returns to daily, and up to {MAX_PICKUP} pickUp items (why dirty, todos or recent) for unfinished work. Give every action a short reason in reasons.\n\n\
Plan format (version 1): {{\"version\":1, \"summary\":\"…\", \"actions\":[{{\"id\":\"a1\", \"kind\":\"create_folder\", \"name\":\"…\", \"ref\":\"f1\"}}, {{\"id\":\"a2\", \"kind\":\"create_workspace\", \"path\":\"/abs/path\", \"folder\":\"f1\"}}, …], \"pickUp\":[{{\"lineId\":\"…\", \"summary\":\"…\", \"why\":\"dirty\"}}], \"reasons\":{{\"a1\":\"…\"}}}}. \
Kinds: create_folder{{name,ref}}, create_workspace{{path,folder?,pinned?}}, move_workspace{{workspaceId,folder}}, create_group{{name,lines}}, hold{{lineId}}, close{{lineId}}, pin{{lineId}}, archive_workspace{{workspaceId}}, set_task_status{{taskId,status}}, track{{lineId}}. folder is an earlier create_folder ref or an existing folder id; ids come from the tools. At most {MAX_ACTIONS} actions.\n\n\
Call propose_organization exactly once with the whole plan, then reply with a one-line summary and stop and wait. Never act yourself: never edit files, never run git write commands (commit, push, merge, rebase, reset, checkout, branch, worktree), never hold, close, resume or start anything, and never create or move tasks."
    )
}

fn start(ctx: &Ctx, b: &Value) -> Res<Value> {
    let agent = crate::feedback::agent_for(b)?;
    let reg = db(ctx)?;
    if tasks::all(&reg)?
        .iter()
        .any(|t| t.origin.as_deref() == Some(CONFIGURATOR) && t.is_running())
    {
        return Err((409, "a session configurator is already running".into()));
    }
    let dir = ctx.state_dir.join("organize");
    crate::mkdir_private(&dir).map_err(|e| (500, format!("{}: {e}", dir.display())))?;
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    let s = |k: &str| b[k].as_str().unwrap_or("").trim().to_string();
    let now = crate::util::now_ms();
    let mut t = Task {
        id: ulid(now),
        title: "Organize sessions".into(),
        description: configurator_prompt(),
        project: dir.to_string_lossy().into_owned(),
        status: "pending".into(),
        agent: agent.id.into(),
        model: s("model"),
        effort: Some(s("effort"))
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "high".into()),
        origin: Some(CONFIGURATOR.into()),
        created_ms: now,
        updated_ms: now,
        rank: now as f64,
        ..Default::default()
    };
    tasks::insert(&reg, &mut t, &ctx.config.station_id)?;
    let claude = agent.mcp == crate::agents::Mcp::Claude;
    match tasks::start_with(ctx, &t.id, None, true) {
        Ok(started) => Ok(json!({
            "task": started.to_json(&ctx.config.station_id),
            "mcp": if claude { "configured" } else { "manual" },
        })),
        Err(e) => {
            let _ = reg.conn.execute("DELETE FROM tasks WHERE id = ?1", [&t.id]);
            publish("task.deleted", json!({"id": t.id}));
            Err(e)
        }
    }
}

pub fn trim_session(l: &Line, ix: &workspaces::Index) -> Value {
    let task = ix
        .tasks
        .iter()
        .find(|t| t.line_id.as_deref() == Some(l.id.as_str()));
    let mut v = json!({
        "id": l.id, "agent": l.agent, "title": line_label(l), "repo": l.repo, "branch": l.branch,
        "worktreePath": if l.worktree_path.is_empty() { &l.cwd } else { &l.worktree_path },
        "cwdMissing": l.cwd_missing, "state": l.state, "busy": l.busy, "needsInput": l.needs_input,
        "lastActivity": format_rfc3339(l.last_activity_ms), "pinned": l.pinned,
    });
    if let Some(k) = &l.ticket {
        v["ticket"] = json!(k);
    }
    if let Some(w) = ix.of_line(l) {
        v["workspaceId"] = json!(w.id);
    }
    if let Some(t) = task {
        v["taskId"] = json!(t.id);
        v["taskStatus"] = json!(t.status);
    }
    v
}

fn sessions(ctx: &Ctx, r: &Request) -> Res<Value> {
    let dec = |k: &str| q(r, k).map(crate::util::percent_decode);
    let limit = dec("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100)
        .min(MAX_SESSIONS);
    let since = dec("sinceDays").and_then(|v| v.parse::<i64>().ok());
    let project = dec("project").filter(|p| !p.is_empty());
    let mut lines = crate::daemon::lines(ctx);
    lines.retain(|l| {
        since.is_none_or(|d| l.alive() || ctx.now_ms - l.last_activity_ms <= d * DAY_MS)
            && project.as_deref().is_none_or(|p| {
                l.worktree_path.starts_with(p) || l.cwd.starts_with(p) || l.repo.contains(p)
            })
    });
    lines.sort_by_key(|l| std::cmp::Reverse(l.last_activity_ms));
    let total = lines.len();
    lines.truncate(limit);
    let ix = index(ctx, &lines)?;
    Ok(json!({
        "total": total,
        "sessions": lines.iter().map(|l| trim_session(l, &ix)).collect::<Vec<_>>(),
    }))
}

fn layout(ctx: &Ctx) -> Res<Value> {
    let ws = workspaces::list(ctx)?;
    let reg = db(ctx)?;
    let st = &ctx.config.station_id;
    Ok(json!({
        "workspaces": ws.iter().map(|w| w.to_json(st)).collect::<Vec<_>>(),
        "folders": workspaces::folders(&reg)?.iter().map(Folder::to_json).collect::<Vec<_>>(),
        "groups": workspaces::groups(&reg)?.iter().map(Group::to_json).collect::<Vec<_>>(),
    }))
}

fn out(code: u16, r: Res<Value>) -> (u16, Value) {
    match r {
        Ok(v) => (code, v),
        Err((c, e)) => (c, json!({ "error": e })),
    }
}

pub fn routes(ctx: &Ctx, r: &Request, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    if segs.get(1) != Some(&"organize") {
        return None;
    }
    let both = |r: Result<Value, (u16, Value)>| r.map(|v| (201, v)).unwrap_or_else(|e| e);
    Some(match (r.method.as_str(), segs) {
        ("POST", ["v1", "organize", "deterministic"]) => both(build(ctx)),
        ("POST", ["v1", "organize", "proposals"]) => both(record(ctx, body)),
        ("GET", ["v1", "organize", "proposals"]) => {
            out(200, db(ctx).and_then(|g| list(&g)).map(Value::from))
        }
        ("GET", ["v1", "organize", "proposals", id]) => out(200, db(ctx).and_then(|g| get(&g, id))),
        ("DELETE", ["v1", "organize", "proposals", id]) => out(
            200,
            db(ctx).and_then(|g| {
                get(&g, id)?;
                set_status(&g, id, "dismissed", None)
            }),
        ),
        ("POST", ["v1", "organize", "proposals", id, "apply"]) => out(200, apply(ctx, id, body)),
        ("POST", ["v1", "organize", "start"]) => out(201, start(ctx, body)),
        ("GET", ["v1", "organize", "sessions"]) => out(200, sessions(ctx, r)),
        ("GET", ["v1", "organize", "projects"]) => out(
            200,
            tasks::projects(ctx, &crate::daemon::lines(ctx)).map(Value::from),
        ),
        ("GET", ["v1", "organize", "layout"]) => out(200, layout(ctx)),
        _ => (404, json!({"error": "not found"})),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const NOW: i64 = 1_800_000_000_000;

    fn reg(tag: &str) -> Registry {
        let dir = std::env::temp_dir().join(format!(
            "sb-organize-{tag}-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let r = Registry::open(&dir).unwrap();
        r.conn.execute_batch(SCHEMA).unwrap();
        r
    }

    fn line(id: &str, repo: &str, wt: &str, days: i64) -> Line {
        Line {
            id: id.into(),
            agent: "claude",
            session_id: format!("s-{id}"),
            station_id: "st".into(),
            repo: repo.into(),
            worktree_path: wt.into(),
            cwd: wt.into(),
            state: "idle",
            last_activity_ms: NOW - days * DAY_MS,
            ..Default::default()
        }
    }

    fn ws(id: &str, path: &str) -> Workspace {
        Workspace {
            id: id.into(),
            name: base_name(path),
            path: path.into(),
            folder_id: None,
            pinned: false,
            archived: false,
            auto: true,
            rank: 0.0,
            tabs: vec![],
            notify_idle: false,
            created_ms: NOW,
            updated_ms: NOW,
            subject: false,
            cwd: None,
            parent_id: None,
        }
    }

    fn task(id: &str, project: &str, status: &str, branch: Option<&str>) -> Task {
        Task {
            id: id.into(),
            number: 1,
            title: id.into(),
            project: project.into(),
            status: status.into(),
            branch: branch.map(str::to_string),
            ..Default::default()
        }
    }

    fn plan_of(
        lines: &[Line],
        workspaces: &[Workspace],
        tasks: &[Task],
        activity: &dyn Fn(&Line) -> Option<Value>,
    ) -> Value {
        let member = |l: &Line| {
            workspaces
                .iter()
                .find(|w| l.worktree_path.starts_with(&w.path))
                .map(|w| w.id.clone())
        };
        let landed = |t: &Task| Some(t.branch.as_deref() == Some("feat/done"));
        deterministic(&Input {
            lines,
            workspaces,
            folders: &[],
            groups: &[],
            projects: &[
                ("/src/a".into(), "github.com/acme/a".into()),
                ("/src/b".into(), "github.com/acme/b".into()),
                ("/src/c".into(), "github.com/other/c".into()),
            ],
            tasks,
            station: "st",
            now_ms: NOW,
            stale_days: STALE_DAYS,
            member: &member,
            landed: &landed,
            activity,
        })
    }

    #[test]
    fn sessions_in_a_second_clone_of_a_repo_propose_that_clone_as_workspace() {
        let lines = [
            line("a", "github.com/o/pkg", "/w/atk", 1),
            line("b", "github.com/o/pkg", "/w/atk2/src", 1),
        ];
        let projects = [
            ("/w/atk".to_string(), "github.com/o/pkg".to_string()),
            ("/w/atk2".to_string(), "github.com/o/pkg".to_string()),
        ];
        let p = deterministic(&Input {
            lines: &lines,
            workspaces: &[],
            folders: &[],
            groups: &[],
            projects: &projects,
            tasks: &[],
            station: "st",
            now_ms: NOW,
            stale_days: STALE_DAYS,
            member: &|_| None,
            landed: &|_| None,
            activity: &|_| None,
        });
        let created: Vec<Value> = kinds(&p, "create_workspace")
            .iter()
            .map(|a| a["path"].clone())
            .collect();
        assert_eq!(created, [json!("/w/atk"), json!("/w/atk2")]);
    }

    fn kinds(p: &Value, kind: &str) -> Vec<Value> {
        p["actions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["kind"] == kind)
            .cloned()
            .collect()
    }

    #[test]
    fn a_nested_workspace_is_never_moved_into_an_owner_folder() {
        let mut nested = ws("wb", "/src/b");
        nested.parent_id = Some("wa".into());
        let workspaces = vec![ws("wa", "/src/a"), nested, ws("wc", "/src/c")];
        let p = plan_of(&[], &workspaces, &[], &|_| None);
        assert!(
            kinds(&p, "move_workspace")
                .iter()
                .all(|a| a["workspaceId"] != "wb"),
            "{p}"
        );
        assert!(kinds(&p, "create_folder").is_empty(), "{p}");
    }

    #[test]
    fn deterministic_rules() {
        let mut stale = line("old", "github.com/acme/a", "/src/a", 40);
        stale.pid = Some(1);
        let mut gone = line("gone", "", "/tmp/x", 2);
        gone.cwd_missing = true;
        let mut busy_gone = line("busy", "", "/tmp/y", 2);
        busy_gone.cwd_missing = true;
        busy_gone.busy = true;
        let mut t1 = line("t1", "github.com/acme/b", "/src/b-wt1", 1);
        t1.ticket = Some("AS-12".into());
        let mut t2 = line("t2", "github.com/other/c", "/src/c", 1);
        t2.branch = "feat/AS-12-login".into();
        let mut pinned = line("pin", "github.com/acme/a", "/src/a", 50);
        pinned.pid = Some(2);
        pinned.pinned = true;
        let lines = vec![stale, gone, busy_gone, t1, t2, pinned];
        let workspaces = vec![ws("wa", "/src/a")];
        let tasks = vec![
            task("done", "/src/b", "in_testing", Some("feat/done")),
            task("open", "/src/b", "in_testing", Some("feat/open")),
            task("run", "/src/b", "in_progress", Some("feat/done")),
        ];
        let p = plan_of(&lines, &workspaces, &tasks, &|_| None);
        let holds = kinds(&p, "hold");
        assert_eq!(holds.len(), 1);
        assert_eq!(holds[0]["lineId"], "old");
        let closes = kinds(&p, "close");
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["lineId"], "gone");
        let groups = kinds(&p, "create_group");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["name"], "AS-12");
        assert_eq!(groups[0]["lines"], json!(["t1", "t2"]));
        let st = kinds(&p, "set_task_status");
        assert_eq!(st.len(), 1);
        assert_eq!(st[0]["taskId"], "done");
        assert_eq!(st[0]["status"], "merged");
        let created: Vec<Value> = kinds(&p, "create_workspace")
            .iter()
            .map(|a| a["path"].clone())
            .collect();
        assert_eq!(created, [json!("/src/b"), json!("/src/c")]);
        let folders = kinds(&p, "create_folder");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0]["name"], "acme");
        let moved = kinds(&p, "move_workspace");
        assert_eq!(moved[0]["workspaceId"], "wa");
        assert_eq!(moved[0]["folder"], folders[0]["ref"]);
        let b = kinds(&p, "create_workspace")
            .into_iter()
            .find(|a| a["path"] == "/src/b")
            .unwrap();
        assert_eq!(b["folder"], folders[0]["ref"]);
        for a in p["actions"].as_array().unwrap() {
            assert!(p["reasons"][a["id"].as_str().unwrap()].is_string());
        }
        let mut idle = vec![ws("wa", "/src/a"), ws("wb", "/src/b")];
        idle[0].updated_ms = NOW - 40 * DAY_MS;
        idle[1].updated_ms = NOW - 40 * DAY_MS;
        idle[0].pinned = true;
        let p = plan_of(&[line("x", "", "/elsewhere", 1)], &idle, &[], &|_| None);
        let arch = kinds(&p, "archive_workspace");
        assert_eq!(arch.len(), 1);
        assert_eq!(arch[0]["workspaceId"], "wb");
    }

    #[test]
    fn pick_up_ranking() {
        let mut lines: Vec<Line> = (0..60)
            .map(|i| line(&format!("l{i}"), "", "/w", 0))
            .collect();
        for (i, l) in lines.iter_mut().enumerate() {
            l.last_activity_ms = NOW - i as i64 * 1000;
        }
        lines.push(line("old", "", "/w", 9));
        lines[5].busy = true;
        let activity = |l: &Line| match l.id.as_str() {
            "l3" => Some(json!({"editedFiles": ["a.rs"], "summary": "fix login\nmore"})),
            "l2" => Some(
                json!({"todos": [{"status": "completed"}, {"status": "pending"}], "lastPrompt": "do x"}),
            ),
            "l4" => Some(json!({"todos": [{"status": "completed"}]})),
            _ => None,
        };
        let p = plan_of(&lines, &[], &[], &activity);
        let picks = p["pickUp"].as_array().unwrap();
        assert_eq!(picks.len(), MAX_PICKUP);
        assert_eq!(picks[0]["lineId"], "l3");
        assert_eq!(picks[0]["why"], "dirty");
        assert_eq!(picks[0]["summary"], "fix login");
        assert_eq!(picks[1]["lineId"], "l2");
        assert_eq!(picks[1]["why"], "todos");
        assert_eq!(picks[1]["todos"], 1);
        assert_eq!(picks[2]["lineId"], "l0");
        assert_eq!(picks[2]["why"], "recent");
        let ids: Vec<&str> = picks
            .iter()
            .map(|p| p["lineId"].as_str().unwrap())
            .collect();
        assert!(!ids.contains(&"l5"));
        assert!(!ids.contains(&"old"));
    }

    fn check(plan: Value) -> Result<Value, Vec<String>> {
        let mut busy = line("busy", "", "/w", 0);
        busy.busy = true;
        let mut asks = line("asks", "", "/w", 0);
        asks.needs_input = true;
        let lines = vec![line("idle", "", "/w", 0), busy, asks];
        let wss = vec![ws("w1", "/w")];
        let folders = vec![Folder {
            id: "F1".into(),
            name: "Clients".into(),
            parent_id: None,
            rank: 0.0,
            collapsed: false,
            created_ms: 0,
            updated_ms: 0,
        }];
        let tasks = vec![task("T1", "/w", "pending", None)];
        let remote = |id: &str| id == "far";
        validate(
            &plan,
            &World {
                lines: &lines,
                workspaces: &wss,
                folders: &folders,
                tasks: &tasks,
                remote: &remote,
            },
        )
    }

    #[test]
    fn validation() {
        let ok = check(json!({"version": 1, "summary": "s", "actions": [
            {"id": "a1", "kind": "create_folder", "name": "Acme", "ref": "f1", "extra": 1},
            {"id": "a2", "kind": "create_workspace", "path": "/w", "folder": "f1"},
            {"id": "a3", "kind": "move_workspace", "workspaceId": "w1", "folder": "F1"},
            {"id": "a4", "kind": "close", "lineId": "idle"},
            {"id": "a5", "kind": "hold", "lineId": "far"},
            {"id": "a6", "kind": "set_task_status", "taskId": "T1", "status": "completed"},
        ], "pickUp": [{"lineId": "idle", "why": "todos", "summary": "a\nb"}],
           "reasons": {"a1": "r", "zz": "dropped"}}))
        .unwrap();
        let acts = ok["actions"].as_array().unwrap();
        assert_eq!(acts.len(), 6);
        assert!(acts[0].get("extra").is_none());
        assert_eq!(acts[1]["folderName"], "Acme");
        assert_eq!(acts[2]["folderName"], "Clients");
        assert_eq!(acts[2]["label"], "w");
        assert_eq!(acts[5]["label"], "#1 T1");
        assert_eq!(ok["reasons"], json!({"a1": "r"}));
        assert_eq!(ok["pickUp"][0]["summary"], "a");
        let bad = |plan: Value, needle: &str| {
            let e = check(plan).unwrap_err().join("; ");
            assert!(e.contains(needle), "{e}");
        };
        bad(
            json!({"actions": [{"id": "a", "kind": "rm_rf", "lineId": "idle"}]}),
            "unknown kind",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "create_workspace", "path": "/w", "folder": "nope"}]}),
            "neither an earlier create_folder ref",
        );
        bad(
            json!({"actions": [
                {"id": "a", "kind": "move_workspace", "workspaceId": "w1", "folder": "f1"},
                {"id": "b", "kind": "create_folder", "name": "X", "ref": "f1"}]}),
            "neither an earlier create_folder ref",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "close", "lineId": "busy"}]}),
            "busy",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "close", "lineId": "asks"}]}),
            "needs input",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "hold", "lineId": "ghost"}]}),
            "no line ghost",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "pin", "lineId": "idle"}, {"id": "a", "kind": "pin", "lineId": "idle"}]}),
            "duplicate id",
        );
        bad(
            json!({"actions": [{"id": "a", "kind": "set_task_status", "taskId": "T1", "status": "in_progress"}]}),
            "status must be",
        );
        bad(json!({"version": 2}), "version must be 1");
        let many: Vec<Value> = (0..=MAX_ACTIONS)
            .map(|i| json!({"id": format!("a{i}"), "kind": "pin", "lineId": "idle"}))
            .collect();
        bad(json!({ "actions": many }), "at most 500");
    }

    struct Fake<'a> {
        reg: &'a Registry,
        calls: RefCell<Vec<String>>,
    }

    impl Ops for Fake<'_> {
        fn remote(&self, line: &str) -> Option<(String, bool)> {
            match line {
                "far" => Some(("st2".into(), true)),
                "off" => Some(("st3".into(), false)),
                _ => None,
            }
        }
        fn relay(&self, line: &str, kind: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("relay {kind} {line}"));
            Ok(())
        }
        fn hold(&self, l: &Line) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("hold {}", l.id));
            Ok(())
        }
        fn close(&self, l: &Line) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("close {}", l.id));
            Ok(())
        }
        fn create_workspace(&self, b: &Value) -> Res<(bool, Workspace)> {
            let path = b["path"].as_str().unwrap();
            if let Some(w) = workspaces::all(self.reg)?
                .into_iter()
                .find(|w| w.path == path)
            {
                return Ok((false, w));
            }
            let id = format!("W{}", path.replace('/', "_"));
            sql(self.reg.conn.execute(
                "INSERT INTO workspaces (id, name, path, folder_id, rank, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, 0, 0, 0)",
                params![id, base_name(path), path, b["folderId"].as_str()],
            ))?;
            Ok((true, workspaces::get(self.reg, &id)?))
        }
        fn patch_workspace(&self, id: &str, b: &Value) -> Res<Workspace> {
            if let Some(f) = b["folderId"].as_str() {
                sql(self.reg.conn.execute(
                    "UPDATE workspaces SET folder_id = ?2 WHERE id = ?1",
                    params![id, f],
                ))?;
            }
            if b["archived"] == true {
                sql(self
                    .reg
                    .conn
                    .execute("UPDATE workspaces SET archived = 1 WHERE id = ?1", [id]))?;
            }
            workspaces::get(self.reg, id)
        }
        fn patch_task(&self, id: &str, b: &Value) -> Res<Task> {
            let mut t = tasks::get(self.reg, id)?;
            t.status = b["status"].as_str().unwrap().into();
            tasks::store(self.reg, &mut t, "st")?;
            Ok(t)
        }
        fn project_of(&self, l: &Line) -> Option<String> {
            Some(l.worktree_path.clone())
        }
        fn station(&self) -> String {
            "st".into()
        }
    }

    fn statuses(rs: &[Value]) -> Vec<(String, String)> {
        rs.iter()
            .map(|r| {
                (
                    r["actionId"].as_str().unwrap().to_string(),
                    r["status"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn applying_a_folder_move_skips_a_nested_workspace() {
        let reg = reg("apply-nested");
        sql(reg.conn.execute_batch(
            "INSERT INTO workspaces (id, name, path, rank, created_ms, updated_ms) VALUES ('wp', 'p', '/p', 0, 0, 0);
             INSERT INTO workspaces (id, name, path, rank, created_ms, updated_ms, parent_id) VALUES ('wn', 'n', '/p/n', 0, 0, 0, 'wp');
             INSERT INTO folders (id, name, rank, created_ms, updated_ms) VALUES ('f1', 'F', 0, 0, 0);",
        ))
        .unwrap();
        let plan = json!({"actions": [{"id": "a1", "kind": "move_workspace", "workspaceId": "wn", "folder": "f1"}]});
        let fake = Fake {
            reg: &reg,
            calls: RefCell::new(vec![]),
        };
        let r = apply_with(&fake, &reg, &[], &plan, &["a1".to_string()]);
        assert_eq!(statuses(&r), [("a1".to_string(), "skipped".to_string())]);
        assert!(workspaces::get(&reg, "wn").unwrap().folder_id.is_none());
    }

    #[test]
    fn apply_runs_checked_actions_and_is_idempotent() {
        let reg = reg("apply");
        let mut t = task("", "/w", "in_testing", Some("feat/x"));
        t.id = ulid(NOW);
        tasks::insert(&reg, &mut t, "st").unwrap();
        let mut held = line("held", "", "/w", 0);
        held.state = "held";
        held.held_reason = Some("user");
        let mut alive = line("alive", "", "/w", 40);
        alive.pid = Some(7);
        let mut gone = line("gone", "", "/nope", 3);
        gone.cwd_missing = true;
        let lines = vec![line("idle", "", "/w", 0), held, alive, gone];
        reg.reserve(&lines[0], "dev", NOW).unwrap();
        let plan = json!({"actions": [
            {"id": "a1", "kind": "create_folder", "name": "Acme", "ref": "f1"},
            {"id": "a2", "kind": "create_workspace", "path": "/w", "folder": "f1"},
            {"id": "a3", "kind": "create_group", "name": "AS-1", "lines": ["idle", "alive"]},
            {"id": "a4", "kind": "hold", "lineId": "alive"},
            {"id": "a5", "kind": "hold", "lineId": "held"},
            {"id": "a6", "kind": "close", "lineId": "gone"},
            {"id": "a7", "kind": "pin", "lineId": "idle"},
            {"id": "a8", "kind": "set_task_status", "taskId": t.id, "status": "completed"},
            {"id": "a9", "kind": "track", "lineId": "idle"},
            {"id": "a10", "kind": "hold", "lineId": "far"},
            {"id": "a11", "kind": "close", "lineId": "off"},
            {"id": "a12", "kind": "archive_workspace", "workspaceId": "Wnone"},
            {"id": "a13", "kind": "move_workspace", "workspaceId": "W_w", "folder": "f9"},
        ]});
        let fake = Fake {
            reg: &reg,
            calls: RefCell::new(vec![]),
        };
        let all: Vec<String> = (1..=13).map(|i| format!("a{i}")).collect();
        let checked: Vec<String> = all.iter().filter(|a| *a != "a6").cloned().collect();
        let first = apply_with(&fake, &reg, &lines, &plan, &checked);
        let want: Vec<(String, String)> = [
            ("a1", "done"),
            ("a2", "done"),
            ("a3", "done"),
            ("a4", "done"),
            ("a5", "skipped"),
            ("a7", "done"),
            ("a8", "done"),
            ("a9", "done"),
            ("a10", "done"),
            ("a11", "failed"),
            ("a12", "failed"),
            ("a13", "failed"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(statuses(&first), want, "{first:?}");
        assert!(first[9]["detail"].as_str().unwrap().contains("offline"));
        assert_eq!(*fake.calls.borrow(), ["hold alive", "relay hold far"]);
        let w = workspaces::get(&reg, "W_w").unwrap();
        let f = workspaces::folders(&reg).unwrap();
        assert_eq!(w.folder_id.as_deref(), Some(f[0].id.as_str()));
        let g = workspaces::groups(&reg).unwrap();
        assert_eq!(g[0].members, ["line:idle", "line:alive"]);
        assert_eq!(tasks::get(&reg, &t.id).unwrap().status, "completed");
        let tracked = tasks::all(&reg)
            .unwrap()
            .into_iter()
            .filter(|t| t.line_id.as_deref() == Some("idle"))
            .count();
        assert_eq!(tracked, 1);

        let mut lines2 = lines.clone();
        lines2[0].pinned = true;
        lines2[2].state = "held";
        lines2[2].held_reason = Some("user");
        lines2[3].state = "hung-up";
        fake.calls.borrow_mut().clear();
        let again = apply_with(&fake, &reg, &lines2, &plan, &all[..9]);
        assert!(again.iter().all(|r| r["status"] == "skipped"), "{again:?}");
        assert!(fake.calls.borrow().is_empty());
        assert_eq!(workspaces::folders(&reg).unwrap().len(), 1);
        assert_eq!(workspaces::groups(&reg).unwrap().len(), 1);
    }

    #[test]
    fn proposals_store_and_list() {
        let reg = reg("store");
        let a = store(
            &reg,
            &json!({"summary": "one", "actions": []}),
            "deterministic",
            None,
        )
        .unwrap();
        assert_eq!(a["status"], "pending_review");
        assert_eq!(a["source"], "deterministic");
        for i in 0..KEEP + 3 {
            store(&reg, &json!({ "summary": i }), "agent", Some("T")).unwrap();
        }
        let all = list(&reg).unwrap();
        assert_eq!(all.len(), KEEP as usize);
        let id = all[0]["id"].as_str().unwrap();
        let p = set_status(&reg, id, "applied", Some(&json!([{"actionId": "a1"}]))).unwrap();
        assert_eq!(p["status"], "applied");
        assert_eq!(p["taskId"], "T");
        assert_eq!(p["results"][0]["actionId"], "a1");
    }

    #[test]
    fn prompt_keeps_the_agent_from_acting() {
        let p = configurator_prompt();
        for needle in [
            "propose_organization exactly once",
            "never edit files",
            "never run git write commands",
            "stop and wait",
        ] {
            assert!(p.contains(needle), "{needle}");
        }
    }
}
