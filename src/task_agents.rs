use crate::board::publish;
use crate::deliver::{send, Channel};
use crate::git::git_out;
use crate::registry::Registry;
use crate::registry::{db, sql};
use crate::status;
use crate::tasks::{self, bad, Res, Task};
use crate::util::{format_rfc3339, ulid};
use crate::Ctx;
use rusqlite::{params, Row};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::Mutex;

pub const ORGANIZER: &str = "organizer";
pub const CONFIGURATOR: &str = "configurator";
pub const ORGANIZE_AFTER_MS: i64 = 30_000;
pub const ORGANIZE_BACKOFF_MS: i64 = 600_000;
const MAX_BODY: usize = 8000;
const OVERLAP_PREFIX: &str = "Overlap: task #";
const OVERLAP_FILES: usize = 20;
static ORGANIZE: Mutex<()> = Mutex::new(());
static FAILED: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn failed<T>(f: impl FnOnce(&mut HashMap<String, i64>) -> T) -> T {
    let mut g = FAILED.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

pub fn backed_off(at: Option<i64>, now_ms: i64) -> bool {
    at.is_some_and(|a| now_ms - a < ORGANIZE_BACKOFF_MS)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    pub task_id: String,
    pub from: Option<String>,
    pub from_number: Option<i64>,
    pub from_title: Option<String>,
    pub body: String,
    pub created_ms: i64,
    pub delivered: bool,
}

impl Message {
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "id": self.id, "taskId": self.task_id, "from": self.from, "body": self.body,
            "createdAt": format_rfc3339(self.created_ms), "delivered": self.delivered,
        });
        if let Some(n) = self.from_number {
            v["fromNumber"] = json!(n);
        }
        if let Some(t) = &self.from_title {
            v["fromTitle"] = json!(t);
        }
        v
    }
}

const MSG_SELECT: &str =
    "SELECT m.id, m.task_id, m.from_id, f.number, f.title, m.body, m.created_ms, m.delivered
     FROM task_messages m LEFT JOIN tasks f ON f.id = m.from_id";

fn msg_row(r: &Row) -> rusqlite::Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        task_id: r.get(1)?,
        from: r.get(2)?,
        from_number: r.get(3)?,
        from_title: r.get(4)?,
        body: r.get(5)?,
        created_ms: r.get(6)?,
        delivered: r.get::<_, i64>(7)? != 0,
    })
}

pub fn is_organizer(t: &Task) -> bool {
    matches!(t.origin.as_deref(), Some(ORGANIZER | CONFIGURATOR))
}

pub fn messages(reg: &Registry, id: &str) -> Res<Vec<Message>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "{MSG_SELECT} WHERE m.task_id = ?1 OR m.from_id = ?1 ORDER BY m.created_ms, m.rowid"
    )))?;
    let rows = sql(st.query_map([id], msg_row))?;
    sql(rows.collect())
}

pub fn undelivered(reg: &Registry, id: &str) -> Res<Vec<Message>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "{MSG_SELECT} WHERE m.task_id = ?1 AND m.delivered = 0 ORDER BY m.created_ms, m.rowid"
    )))?;
    let rows = sql(st.query_map([id], msg_row))?;
    sql(rows.collect())
}

pub fn mark_delivered(reg: &Registry, ms: &[Message]) {
    for m in ms {
        let _ = reg.conn.execute(
            "UPDATE task_messages SET delivered = 1 WHERE id = ?1",
            [&m.id],
        );
    }
}

pub fn delivery_text(m: &Message) -> String {
    match (&m.from, m.from_number) {
        (Some(from), Some(n)) => format!(
            "[Overseer] Message from task #{n} \"{}\" (id {from}): {}\nReply with the send_message tool (to: {from}) if needed.",
            m.from_title.as_deref().unwrap_or_default(),
            m.body
        ),
        (Some(from), None) => format!(
            "[Overseer] Message from task {from}: {}\nReply with the send_message tool (to: {from}) if needed.",
            m.body
        ),
        _ if m.body.starts_with(OVERLAP_PREFIX) || m.body.starts_with(crate::quorum::PREFIX) => {
            format!("[Overseer] {}", m.body)
        }
        _ => format!("[Overseer] Message from the user: {}", m.body),
    }
}

pub fn backlog(ms: &[Message]) -> String {
    ms.iter()
        .map(|m| format!("- {}", delivery_text(m).replace('\n', " ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn store_message(reg: &Registry, m: &Message) -> Res<()> {
    sql(reg.conn.execute(
        "INSERT INTO task_messages (id, task_id, from_id, body, created_ms, delivered)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![m.id, m.task_id, m.from, m.body, m.created_ms, m.delivered],
    ))?;
    Ok(())
}

pub fn send_message(ctx: &Ctx, id: &str, b: &Value) -> Res<Message> {
    let body = b["body"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("body required"))?;
    if body.chars().count() > MAX_BODY {
        return Err(bad(format!("body must be at most {MAX_BODY} characters")));
    }
    let from = match &b["from"] {
        Value::Null => None,
        Value::String(f) if f == id => return Err(bad("a task cannot message itself")),
        Value::String(f) => Some(f.clone()),
        _ => return Err(bad("from must be a task id")),
    };
    let reg = db(ctx)?;
    if let Some(f) = &from {
        let sender = tasks::get(&reg, f).map_err(|_| bad(format!("from {f} is not a task")))?;
        allow_agent_message(&reg, &sender, &tasks::get(&reg, id)?)?;
    }
    post(&reg, id, from, body)
}

const AGENT_MESSAGING_KEY: &str = "agentMessaging";

pub fn agent_messaging_enabled(reg: &Registry) -> bool {
    !matches!(reg.get(AGENT_MESSAGING_KEY), Ok(Some(v)) if v == "false")
}

pub fn set_agent_messaging(reg: &Registry, on: bool) -> Result<(), String> {
    reg.put(AGENT_MESSAGING_KEY, if on { "true" } else { "false" })
}

const AGENT_TASKS_KEY: &str = "agentTasks";

pub fn agent_tasks_enabled(reg: &Registry) -> bool {
    !matches!(reg.get(AGENT_TASKS_KEY), Ok(Some(v)) if v == "false")
}

pub fn set_agent_tasks(reg: &Registry, on: bool) -> Result<(), String> {
    reg.put(AGENT_TASKS_KEY, if on { "true" } else { "false" })
}

fn parent_and_child(a: &Task, b: &Task) -> bool {
    a.parent_id.as_deref() == Some(b.id.as_str()) || b.parent_id.as_deref() == Some(a.id.as_str())
}

pub fn allow_agent_message(reg: &Registry, from: &Task, to: &Task) -> Res<()> {
    if from.project != to.project {
        return Err((
            403,
            format!(
                "task {} belongs to another project ({}): agents may only message tasks of their own project. Do not retry.",
                to.id, to.project
            ),
        ));
    }
    if !agent_messaging_enabled(reg) && !parent_and_child(from, to) {
        return Err((
            403,
            "messages between agents are turned off in Overseer's settings. Do not retry; carry on without them.".into(),
        ));
    }
    Ok(())
}

fn post(reg: &Registry, id: &str, from: Option<String>, body: &str) -> Res<Message> {
    let target = tasks::get(reg, id)?;
    let now = crate::util::now_ms();
    let mut m = Message {
        id: ulid(now),
        task_id: id.into(),
        from,
        from_number: None,
        from_title: None,
        body: body.into(),
        created_ms: now,
        delivered: false,
    };
    store_message(reg, &m)?;
    if let Some(f) = &m.from {
        if let Ok(t) = tasks::get(reg, f) {
            m.from_number = Some(t.number);
            m.from_title = Some(t.title);
        }
    }
    if let Some(term) = target.live_term() {
        if send(&Channel::Term(term), &delivery_text(&m), None, "").is_ok() {
            mark_delivered(reg, std::slice::from_ref(&m));
            m.delivered = true;
        }
    }
    publish("task.message", json!({"message": m.to_json()}));
    Ok(m)
}

pub fn intersect(a: &BTreeSet<String>, b: &BTreeSet<String>) -> Vec<String> {
    a.intersection(b).cloned().collect()
}

pub fn porcelain_paths(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| l.len() > 3)
        .map(|l| {
            let p = &l[3..];
            let p = p.rsplit_once(" -> ").map_or(p, |(_, to)| to);
            p.trim_matches('"').to_string()
        })
        .collect()
}

fn base_branch(project: &str) -> String {
    crate::git::git(project, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "main".into())
}

fn changed_files(wt: &str, base: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let upstream = format!("{base}@{{upstream}}");
    let mut args = vec!["merge-base", "HEAD", base];
    if git_out(wt, &["rev-parse", "--verify", "-q", &upstream]).is_ok() {
        args.push(&upstream);
    }
    let mb = git_out(wt, &args)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !mb.is_empty() {
        if let Ok(d) = git_out(wt, &["diff", "--name-only", &mb]) {
            out.extend(d.lines().filter(|l| !l.is_empty()).map(str::to_string));
        }
    }
    if let Ok(s) = git_out(wt, &["status", "--porcelain"]) {
        out.extend(porcelain_paths(&s));
    }
    out
}

fn live_worktree(t: &Task) -> Option<&str> {
    t.worktree_path.as_deref().filter(|p| Path::new(p).is_dir())
}

pub fn overlaps(ctx: &Ctx, id: &str) -> Res<Vec<Value>> {
    overlaps_in(&db(ctx)?, id)
}

fn overlaps_in(reg: &Registry, id: &str) -> Res<Vec<Value>> {
    let me = tasks::get(reg, id)?;
    let Some(mine_wt) = live_worktree(&me) else {
        return Ok(vec![]);
    };
    let base = base_branch(&me.project);
    if is_organizer(&me) {
        return Ok(vec![]);
    }
    let mine = changed_files(mine_wt, &base);
    if mine.is_empty() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for t in tasks::all(reg)? {
        let Some(wt) = live_worktree(&t) else {
            continue;
        };
        if t.id == me.id
            || t.project != me.project
            || wt == mine_wt
            || is_organizer(&t)
            || !matches!(t.status.as_str(), status::IN_PROGRESS | status::IN_TESTING)
        {
            continue;
        }
        let files = intersect(&mine, &changed_files(wt, &base));
        if !files.is_empty() {
            out.push(json!({
                "task": {"id": t.id, "number": t.number, "title": t.title, "status": t.status,
                         "branch": t.branch},
                "files": files,
            }));
        }
    }
    Ok(out)
}

pub fn overlap_text(other: (&str, i64, &str), mine: (i64, &str), files: &[String]) -> String {
    let more = files.len().saturating_sub(OVERLAP_FILES);
    let mut list = files
        .iter()
        .take(OVERLAP_FILES)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if more > 0 {
        list.push_str(&format!(" (+{more} more)"));
    }
    format!(
        "{OVERLAP_PREFIX}{} \"{}\" and task #{} \"{}\" both change: {list}. Agree who changes what with send_message (to: {}).",
        mine.0, mine.1, other.1, other.2, other.0
    )
}

pub fn overlap_key(a: &str, b: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("overlap:{lo}:{hi}")
}

pub fn overlap_changed(stored: Option<&str>, files: &[String]) -> Option<String> {
    let mut sorted = files.to_vec();
    sorted.sort();
    let joined = sorted.join("\n");
    (!sorted.is_empty() && stored != Some(joined.as_str())).then_some(joined)
}

fn alert_overlaps(reg: &Registry, id: &str) -> Res<()> {
    if !agent_messaging_enabled(reg) {
        return Ok(());
    }
    let me = tasks::get(reg, id)?;
    for o in overlaps_in(reg, id)? {
        let files: Vec<String> = o["files"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let (oid, num, title) = (
            o["task"]["id"].as_str().unwrap_or_default(),
            o["task"]["number"].as_i64().unwrap_or_default(),
            o["task"]["title"].as_str().unwrap_or_default(),
        );
        let key = overlap_key(&me.id, oid);
        let stored = reg.get(&key).ok().flatten();
        let Some(joined) = overlap_changed(stored.as_deref(), &files) else {
            continue;
        };
        reg.put(&key, &joined).map_err(|e| (500, e))?;
        let _ = post(
            reg,
            &me.id,
            None,
            &overlap_text((oid, num, title), (me.number, &me.title), &files),
        );
        let _ = post(
            reg,
            oid,
            None,
            &overlap_text((&me.id, me.number, &me.title), (num, title), &files),
        );
    }
    Ok(())
}

pub fn spawn_overlap_alert(ctx: &Ctx, t: &Task) {
    if is_organizer(t) || t.from_session() {
        return;
    }
    let (dir, id) = (ctx.state_dir.clone(), t.id.clone());
    std::thread::spawn(move || {
        if let Ok(reg) = Registry::open(&dir) {
            let _ = alert_overlaps(&reg, &id);
        }
    });
}

pub fn organizer_prompt(project: &str, messaging: bool) -> String {
    let name = Path::new(project)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.to_string());
    let point_out = if messaging {
        "Point out duplicate or overlapping tasks with send_message to the agents of the tasks concerned when they are in_progress. "
    } else {
        ""
    };
    format!(
        "You are the Overseer board organizer for the project {name} ({project}).\n\
Call list_tasks (project {project}, status pending,auto,in_progress,in_testing). \
Classify every task whose origin is not organizer with prioritize_task (urgent, important) following the Eisenhower matrix: \
urgent and important = do, important only = plan, urgent only = delegate, neither = drop. \
Also set rank so that within a status column do comes before plan, plan before delegate, delegate before drop, \
keeping the current relative order inside a quadrant (ranks are floats, lower first). \
{point_out}Never edit files, never create tasks, never start tasks. Finish with a one-line summary."
    )
}

pub fn organizable(tasks: &[Task], project: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in tasks {
        if crate::status::is_done(&t.status)
            || is_organizer(t)
            || t.from_session()
            || project.is_some_and(|p| p != t.project)
            || out.contains(&t.project)
        {
            continue;
        }
        out.push(t.project.clone());
    }
    out
}

pub fn organize_due(tasks: &[Task], project: &str, live_organizer: bool, now_ms: i64) -> bool {
    if live_organizer {
        return false;
    }
    let newest = |f: &dyn Fn(&Task) -> bool| {
        tasks
            .iter()
            .filter(|t| t.project == project && f(t))
            .map(|t| t.created_ms)
            .max()
    };
    let Some(work) = newest(&|t| {
        !crate::status::is_done(&t.status)
            && matches!(t.origin.as_deref(), None | Some("user" | "agent"))
    }) else {
        return false;
    };
    newest(&is_organizer).is_none_or(|o| work > o) && now_ms - work >= ORGANIZE_AFTER_MS
}

fn live_organizer(tasks: &[Task], project: &str) -> bool {
    tasks
        .iter()
        .any(|t| t.project == project && is_organizer(t) && t.is_running())
}

fn organize_one(ctx: &Ctx, reg: &Registry, project: &str) -> Res<Task> {
    let _g = ORGANIZE.lock().unwrap_or_else(|e| e.into_inner());
    if tasks::all(reg).is_ok_and(|all| live_organizer(&all, project)) {
        return Err((409, "an organizer is already running".into()));
    }
    let claude =
        crate::agents::get(crate::agents::DEFAULT_ID).ok_or_else(|| bad("claude agent unknown"))?;
    if !crate::agents::is_installed(claude) {
        return Err((
            409,
            format!("{} is not installed ({})", claude.name, claude.bin),
        ));
    }
    let now = crate::util::now_ms();
    let mut t = Task {
        id: ulid(now),
        title: "Organize board".into(),
        description: organizer_prompt(project, agent_messaging_enabled(reg)),
        project: project.into(),
        status: "pending".into(),
        agent: crate::agents::DEFAULT_ID.into(),
        effort: "high".into(),
        origin: Some(ORGANIZER.into()),
        created_ms: now,
        updated_ms: now,
        rank: now as f64,
        ..Default::default()
    };
    tasks::insert(reg, &mut t, &ctx.config.station_id)?;
    match tasks::start_with(ctx, &t.id, None, true) {
        Ok(started) => Ok(started),
        Err(e) => {
            let _ = reg.conn.execute("DELETE FROM tasks WHERE id = ?1", [&t.id]);
            publish("task.deleted", json!({"id": t.id}));
            Err(e)
        }
    }
}

pub fn organize(ctx: &Ctx, project: Option<&str>) -> Res<Vec<Task>> {
    let reg = db(ctx)?;
    let all = tasks::all(&reg)?;
    let mut started = vec![];
    let mut last = None;
    for p in organizable(&all, project) {
        match organize_one(ctx, &reg, &p) {
            Ok(t) => started.push(t),
            Err(e) => last = Some(e),
        }
    }
    match (started.is_empty(), last) {
        (true, Some(e)) => Err(e),
        (true, None) => Err((409, "nothing to organize".into())),
        _ => Ok(started),
    }
}

const AUTO_ORGANIZE_KEY: &str = "autoOrganize";

pub fn auto_organize_enabled(reg: &Registry) -> bool {
    matches!(reg.get(AUTO_ORGANIZE_KEY), Ok(Some(v)) if v == "true")
}

pub fn auto_organize_on(ctx: &Ctx, reg: &Registry) -> bool {
    ctx.config.auto_organize || auto_organize_enabled(reg)
}

pub fn set_auto_organize(reg: &Registry, on: bool) -> Result<(), String> {
    reg.put(AUTO_ORGANIZE_KEY, if on { "true" } else { "false" })
}

pub fn auto_organize(ctx: &Ctx, reg: &Registry) {
    let Ok(all) = tasks::all(reg) else {
        return;
    };
    let now = crate::util::now_ms();
    for p in organizable(&all, None) {
        if failed(|m| backed_off(m.get(&p).copied(), now)) {
            continue;
        }
        if organize_due(&all, &p, live_organizer(&all, &p), now)
            && organize_one(ctx, reg, &p).is_err()
        {
            failed(|m| m.insert(p.clone(), now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(tag: &str) -> Registry {
        let dir = std::env::temp_dir().join(format!(
            "sb-task-agents-{tag}-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Registry::open(&dir).unwrap()
    }

    #[test]
    fn auto_organize_flag_roundtrips() {
        let r = reg("auto-flag");
        assert!(!auto_organize_enabled(&r));
        set_auto_organize(&r, true).unwrap();
        assert!(auto_organize_enabled(&r));
        set_auto_organize(&r, false).unwrap();
        assert!(!auto_organize_enabled(&r));
    }

    fn task(id: &str, project: &str, status: &str, origin: Option<&str>, created: i64) -> Task {
        Task {
            id: id.into(),
            title: format!("title {id}"),
            project: project.into(),
            status: status.into(),
            agent: "claude".into(),
            effort: "high".into(),
            origin: origin.map(str::to_string),
            created_ms: created,
            updated_ms: created,
            rank: created as f64,
            ..Default::default()
        }
    }

    fn msg(id: &str, to: &str, from: Option<&str>, ms: i64) -> Message {
        Message {
            id: id.into(),
            task_id: to.into(),
            from: from.map(str::to_string),
            from_number: None,
            from_title: None,
            body: format!("body {id}"),
            created_ms: ms,
            delivered: false,
        }
    }

    #[test]
    fn new_fields_roundtrip_and_patch_store() {
        let r = reg("fields");
        let mut parent = task("P", "/p", "pending", None, 1);
        tasks::insert(&r, &mut parent, "st").unwrap();
        let mut t = task("C", "/p", "pending", Some("agent"), 2);
        t.urgent = Some(true);
        t.important = Some(false);
        t.parent_id = Some("P".into());
        tasks::insert(&r, &mut t, "st").unwrap();
        let got = tasks::get(&r, "C").unwrap();
        assert_eq!(got.urgent, Some(true));
        assert_eq!(got.important, Some(false));
        assert_eq!(got.parent_id.as_deref(), Some("P"));
        let j = got.to_json("st");
        assert_eq!(j["urgent"], true);
        assert_eq!(j["important"], false);
        assert_eq!(j["parentId"], "P");
        assert_eq!(j["origin"], "agent");
        let j = tasks::get(&r, "P").unwrap().to_json("st");
        assert!(j.get("urgent").is_none() && j.get("important").is_none());
        assert!(j.get("parentId").is_none());
        assert_eq!(j["origin"], "user");
        let mut got = got;
        got.urgent = None;
        got.important = Some(true);
        tasks::store(&r, &mut got, "st").unwrap();
        let again = tasks::get(&r, "C").unwrap();
        assert_eq!((again.urgent, again.important), (None, Some(true)));
        assert_eq!(again.parent_id.as_deref(), Some("P"));
    }

    #[test]
    fn messages_store_and_list() {
        let r = reg("msgs");
        for (id, n) in [("A", 1), ("B", 2), ("C", 3)] {
            let mut t = task(id, "/p", "in_progress", None, n);
            tasks::insert(&r, &mut t, "st").unwrap();
        }
        store_message(&r, &msg("m1", "A", None, 10)).unwrap();
        store_message(&r, &msg("m2", "B", Some("A"), 20)).unwrap();
        store_message(&r, &msg("m3", "A", Some("B"), 30)).unwrap();
        store_message(&r, &msg("m4", "C", Some("B"), 40)).unwrap();
        let a = messages(&r, "A").unwrap();
        assert_eq!(
            a.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["m1", "m2", "m3"]
        );
        assert_eq!(a[0].from, None);
        assert_eq!(a[1].from_number, Some(1));
        assert_eq!(a[2].from_title.as_deref(), Some("title B"));
        assert_eq!(a[1].to_json()["fromNumber"], 1);
        assert!(a[0].to_json().get("fromNumber").is_none());
        assert_eq!(a[0].to_json()["from"], Value::Null);
        let pending = undelivered(&r, "A").unwrap();
        assert_eq!(pending.len(), 2);
        mark_delivered(&r, &pending[..1]);
        let left = undelivered(&r, "A").unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "m3");
        assert!(messages(&r, "A").unwrap()[0].delivered);
        assert!(messages(&r, "nope").unwrap().is_empty());
    }

    #[test]
    fn delivery_texts() {
        let mut m = msg("m", "A", None, 1);
        m.body = "hello".into();
        assert_eq!(delivery_text(&m), "[Overseer] Message from the user: hello");
        m.body = "Quorum: subtask #2 is accepted".into();
        assert_eq!(
            delivery_text(&m),
            "[Overseer] Quorum: subtask #2 is accepted"
        );
        m.body = "hello".into();
        m.from = Some("X1".into());
        m.from_number = Some(7);
        m.from_title = Some("Fix it".into());
        assert_eq!(
            delivery_text(&m),
            "[Overseer] Message from task #7 \"Fix it\" (id X1): hello\nReply with the send_message tool (to: X1) if needed."
        );
        assert!(!backlog(&[m]).contains('\n'));
    }

    #[test]
    fn overlap_files() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(
            intersect(&set(&["a", "b", "c"]), &set(&["c", "a", "z"])),
            ["a", "c"]
        );
        assert!(intersect(&set(&["a"]), &set(&["b"])).is_empty());
        assert_eq!(
            porcelain_paths(" M src/a.rs\n?? new.txt\nR  old.rs -> src/b.rs\nA  \"sp ace.rs\"\n"),
            ["src/a.rs", "new.txt", "src/b.rs", "sp ace.rs"]
        );
    }

    #[test]
    fn changed_files_ignore_commits_the_local_base_branch_is_behind_on() {
        let sh = |d: &Path, args: &[&str]| {
            crate::git::fixture::run(d, args);
        };
        let commit = |d: &Path, file: &str| {
            std::fs::write(d.join(file), file).unwrap();
            sh(d, &["add", "-A"]);
            sh(d, &["commit", "-q", "-m", file]);
        };
        let d = std::env::temp_dir().join(format!(
            "sb-overlap-base-{}-{}",
            std::process::id(),
            crate::util::now_ms()
        ));
        let up = d.join("up");
        std::fs::create_dir_all(&up).unwrap();
        sh(&up, &["init", "-q", "-b", "next"]);
        commit(&up, "a.txt");
        sh(&d, &["clone", "-q", "up", "work"]);
        let work = d.join("work");
        commit(&up, "landed.txt");
        sh(&work, &["fetch", "-q", "origin"]);
        sh(
            &work,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "task/1",
                "../wt",
                "origin/next",
            ],
        );
        let wt = d.join("wt");
        commit(&wt, "mine.txt");
        let files = changed_files(wt.to_str().unwrap(), "next");
        assert_eq!(files.into_iter().collect::<Vec<_>>(), ["mine.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn overlap_alert_text_and_dedupe() {
        let f = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let t = overlap_text(("B1", 5, "Other"), (3, "Mine"), &f(&["a.rs", "b.rs"]));
        assert_eq!(
            t,
            "Overlap: task #3 \"Mine\" and task #5 \"Other\" both change: a.rs, b.rs. Agree who changes what with send_message (to: B1)."
        );
        let mut m = msg("m", "A", None, 1);
        m.body = t.clone();
        assert_eq!(delivery_text(&m), format!("[Overseer] {t}"));
        let many: Vec<String> = (0..25).map(|i| format!("f{i}")).collect();
        assert!(overlap_text(("B1", 5, "O"), (3, "M"), &many).contains("(+5 more)"));
        assert_eq!(overlap_key("b", "a"), overlap_key("a", "b"));
        assert_eq!(overlap_key("b", "a"), "overlap:a:b");
        assert_eq!(
            overlap_changed(None, &f(&["b", "a"])).as_deref(),
            Some("a\nb")
        );
        assert_eq!(overlap_changed(Some("a\nb"), &f(&["b", "a"])), None);
        assert!(overlap_changed(Some("a"), &f(&["b", "a"])).is_some());
        assert!(overlap_changed(None, &[]).is_none());
    }

    #[test]
    fn overlap_dedupe_key_persists() {
        let r = reg("overlap-kv");
        let key = overlap_key("2", "1");
        let files = vec!["a.rs".to_string()];
        let first = overlap_changed(r.get(&key).unwrap().as_deref(), &files).unwrap();
        r.put(&key, &first).unwrap();
        assert!(overlap_changed(r.get(&key).unwrap().as_deref(), &files).is_none());
        let more = vec!["a.rs".to_string(), "b.rs".to_string()];
        assert!(overlap_changed(r.get(&key).unwrap().as_deref(), &more).is_some());
    }

    #[test]
    fn backoff_window() {
        assert!(!backed_off(None, 1_000_000));
        assert!(backed_off(
            Some(1_000_000),
            1_000_000 + ORGANIZE_BACKOFF_MS - 1
        ));
        assert!(!backed_off(
            Some(1_000_000),
            1_000_000 + ORGANIZE_BACKOFF_MS
        ));
    }

    #[test]
    fn organize_trigger() {
        let now = 1_000_000;
        let ts = vec![task("a", "/p", "pending", None, now - 40_000)];
        assert!(organize_due(&ts, "/p", false, now));
        assert!(!organize_due(&ts, "/p", true, now));
        assert!(!organize_due(&ts, "/q", false, now));
        assert!(!organize_due(&ts, "/p", false, now - 20_000));
        let mut ts = ts;
        ts.push(task("o", "/p", "completed", Some(ORGANIZER), now - 10_000));
        assert!(!organize_due(&ts, "/p", false, now));
        ts.push(task("b", "/p", "auto", Some("agent"), now - 15_000));
        assert!(!organize_due(&ts, "/p", false, now));
        ts[2].created_ms = now - 5_000;
        assert!(!organize_due(&ts, "/p", false, now));
        ts[2].created_ms = now - 9_000;
        let later = now + 40_000;
        assert!(organize_due(&ts, "/p", false, later));
        ts[2].status = "completed".into();
        ts[0].status = "completed".into();
        assert!(!organize_due(&ts, "/p", false, later));
    }

    #[test]
    fn organizable_projects() {
        let ts = vec![
            task("a", "/p", "pending", None, 1),
            task("b", "/p", "in_progress", None, 2),
            task("c", "/q", "completed", None, 3),
            task("d", "/r", "pending", Some(ORGANIZER), 4),
            task("e", "/s", "pending", Some("session"), 5),
            task("f", "/t", "auto", Some("agent"), 6),
        ];
        assert_eq!(organizable(&ts, None), ["/p", "/t"]);
        assert_eq!(organizable(&ts, Some("/t")), ["/t"]);
        assert!(organizable(&ts, Some("/q")).is_empty());
        assert!(organizer_prompt("/home/u/repo", true).contains("repo (/home/u/repo)"));
    }

    #[test]
    fn organizer_is_told_to_message_agents_only_when_messaging_is_on() {
        assert!(organizer_prompt("/p", true).contains("send_message"));
        assert!(!organizer_prompt("/p", false).contains("send_message"));
    }

    #[test]
    fn agent_messaging_is_on_by_default_and_roundtrips() {
        let r = reg("messaging-flag");
        assert!(agent_messaging_enabled(&r));
        set_agent_messaging(&r, false).unwrap();
        assert!(!agent_messaging_enabled(&r));
        set_agent_messaging(&r, true).unwrap();
        assert!(agent_messaging_enabled(&r));
    }

    #[test]
    fn agent_tasks_is_on_by_default_and_roundtrips() {
        let r = reg("agent-tasks-flag");
        assert!(agent_tasks_enabled(&r));
        set_agent_tasks(&r, false).unwrap();
        assert!(!agent_tasks_enabled(&r));
        set_agent_tasks(&r, true).unwrap();
        assert!(agent_tasks_enabled(&r));
    }

    #[test]
    fn agents_message_only_inside_their_project() {
        let r = reg("messaging-project");
        let (a, b) = (
            task("A", "/p", "in_progress", None, 1),
            task("B", "/p", "in_progress", None, 2),
        );
        let far = task("C", "/q", "in_progress", None, 3);
        assert!(allow_agent_message(&r, &a, &b).is_ok());
        let (code, e) = allow_agent_message(&r, &a, &far).unwrap_err();
        assert_eq!(code, 403);
        assert!(e.contains("another project (/q)"), "{e}");
    }

    #[test]
    fn messaging_off_refuses_agents_but_keeps_parent_and_subtask_talking() {
        let r = reg("messaging-off");
        set_agent_messaging(&r, false).unwrap();
        let root = task("R", "/p", "in_progress", None, 1);
        let mut sub = task("S", "/p", "in_progress", None, 2);
        sub.parent_id = Some("R".into());
        let other = task("O", "/p", "in_progress", None, 3);
        let (code, e) = allow_agent_message(&r, &root, &other).unwrap_err();
        assert_eq!(code, 403);
        assert!(e.contains("turned off"), "{e}");
        assert!(allow_agent_message(&r, &root, &sub).is_ok());
        assert!(allow_agent_message(&r, &sub, &root).is_ok());
        let mut far_sub = sub.clone();
        far_sub.project = "/q".into();
        assert!(allow_agent_message(&r, &root, &far_sub).is_err());
    }
}
