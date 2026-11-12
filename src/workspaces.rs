use crate::board::publish;
use crate::http::Request;
use crate::procs::now_ms;
use crate::registry::{col_at, columns, db, sql, update_row, Registry};
use crate::tasks::Task;
use crate::util::{format_rfc3339, ulid};
use crate::{Ctx, Line};
use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use std::path::Path;

pub type Res<T> = Result<T, (u16, String)>;

columns!(
    WS_COLUMNS,
    WS_COLS = id,
    name,
    path,
    folder_id,
    pinned,
    archived,
    auto,
    rank,
    tabs,
    notify_idle,
    created_ms,
    updated_ms,
    kind,
    cwd,
    parent_id
);
columns!(
    FOLDER_COLUMNS,
    FOLDER_COLS = id,
    name,
    parent_id,
    rank,
    collapsed,
    created_ms,
    updated_ms
);
columns!(
    GROUP_COLUMNS,
    GROUP_COLS = id,
    name,
    rank,
    members,
    notify_idle,
    created_ms,
    updated_ms
);

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// Empty for a subject: it has no directory of its own.
    pub path: String,
    pub folder_id: Option<String>,
    pub pinned: bool,
    pub archived: bool,
    pub auto: bool,
    pub rank: f64,
    pub tabs: Vec<String>,
    pub notify_idle: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub subject: bool,
    /// Where its sessions start: required for a subject, optional for a nested dir
    /// (none starts in its own folder).
    pub cwd: Option<String>,
    /// The top-level dir workspace this one is nested in (one level only).
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub rank: f64,
    pub collapsed: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub rank: f64,
    pub members: Vec<String>,
    pub notify_idle: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
}

fn strings(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}

// The NOT NULL UNIQUE path column holds a placeholder for subjects.
fn subject_path(id: &str) -> String {
    format!("subject:{id}")
}

fn ws_row(r: &Row) -> rusqlite::Result<Workspace> {
    let subject = r.get::<_, String>(col_at!(WS_COLUMNS, "kind"))? == "subject";
    Ok(Workspace {
        id: r.get(col_at!(WS_COLUMNS, "id"))?,
        name: r.get(col_at!(WS_COLUMNS, "name"))?,
        path: if subject {
            String::new()
        } else {
            r.get(col_at!(WS_COLUMNS, "path"))?
        },
        subject,
        cwd: r.get(col_at!(WS_COLUMNS, "cwd"))?,
        parent_id: r.get(col_at!(WS_COLUMNS, "parent_id"))?,
        folder_id: r.get(col_at!(WS_COLUMNS, "folder_id"))?,
        pinned: r.get(col_at!(WS_COLUMNS, "pinned"))?,
        archived: r.get(col_at!(WS_COLUMNS, "archived"))?,
        auto: r.get(col_at!(WS_COLUMNS, "auto"))?,
        rank: r.get(col_at!(WS_COLUMNS, "rank"))?,
        tabs: strings(r.get(col_at!(WS_COLUMNS, "tabs"))?),
        notify_idle: r.get(col_at!(WS_COLUMNS, "notify_idle"))?,
        created_ms: r.get(col_at!(WS_COLUMNS, "created_ms"))?,
        updated_ms: r.get(col_at!(WS_COLUMNS, "updated_ms"))?,
    })
}

fn folder_row(r: &Row) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: r.get(col_at!(FOLDER_COLUMNS, "id"))?,
        name: r.get(col_at!(FOLDER_COLUMNS, "name"))?,
        parent_id: r.get(col_at!(FOLDER_COLUMNS, "parent_id"))?,
        rank: r.get(col_at!(FOLDER_COLUMNS, "rank"))?,
        collapsed: r.get(col_at!(FOLDER_COLUMNS, "collapsed"))?,
        created_ms: r.get(col_at!(FOLDER_COLUMNS, "created_ms"))?,
        updated_ms: r.get(col_at!(FOLDER_COLUMNS, "updated_ms"))?,
    })
}

fn group_row(r: &Row) -> rusqlite::Result<Group> {
    Ok(Group {
        id: r.get(col_at!(GROUP_COLUMNS, "id"))?,
        name: r.get(col_at!(GROUP_COLUMNS, "name"))?,
        rank: r.get(col_at!(GROUP_COLUMNS, "rank"))?,
        members: strings(r.get(col_at!(GROUP_COLUMNS, "members"))?),
        notify_idle: r.get(col_at!(GROUP_COLUMNS, "notify_idle"))?,
        created_ms: r.get(col_at!(GROUP_COLUMNS, "created_ms"))?,
        updated_ms: r.get(col_at!(GROUP_COLUMNS, "updated_ms"))?,
    })
}

impl Workspace {
    pub fn start_dir(&self) -> &str {
        self.cwd.as_deref().unwrap_or(&self.path)
    }

    pub fn missing(&self) -> bool {
        let dir = if self.subject {
            self.start_dir()
        } else {
            &self.path
        };
        !Path::new(dir).is_dir()
    }

    pub fn to_json(&self, station: &str) -> Value {
        json!({
            "id": self.id, "name": self.name, "station": station,
            "path": (!self.subject).then_some(&self.path),
            "kind": if self.subject { "subject" } else { "dir" }, "cwd": self.cwd,
            "parentId": self.parent_id, "folderId": self.folder_id, "pinned": self.pinned, "archived": self.archived,
            "auto": self.auto, "rank": self.rank, "tabs": self.tabs, "notifyIdle": self.notify_idle,
            "missing": self.missing(),
            "createdAt": format_rfc3339(self.created_ms), "updatedAt": format_rfc3339(self.updated_ms),
        })
    }
}

impl Folder {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "parentId": self.parent_id, "rank": self.rank,
            "collapsed": self.collapsed,
            "createdAt": format_rfc3339(self.created_ms), "updatedAt": format_rfc3339(self.updated_ms),
        })
    }
}

impl Group {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "rank": self.rank, "members": self.members,
            "notifyIdle": self.notify_idle,
            "createdAt": format_rfc3339(self.created_ms), "updatedAt": format_rfc3339(self.updated_ms),
        })
    }
}

pub fn bad(msg: impl Into<String>) -> (u16, String) {
    (400, msg.into())
}

fn object(b: &Value) -> Res<&serde_json::Map<String, Value>> {
    b.as_object().ok_or_else(|| bad("JSON object expected"))
}

fn name_of(v: &Value) -> Res<String> {
    let n = v.as_str().map(str::trim).unwrap_or_default();
    if (1..=80).contains(&n.chars().count()) {
        Ok(n.to_string())
    } else {
        Err(bad("name must be 1 to 80 characters"))
    }
}

fn bool_of(v: &Value, k: &str) -> Res<bool> {
    v.as_bool()
        .ok_or_else(|| bad(format!("{k} must be a boolean")))
}

fn rank_of(v: &Value) -> Res<f64> {
    v.as_f64()
        .filter(|r| r.is_finite())
        .ok_or_else(|| bad("rank must be a finite number"))
}

fn strs_of(v: &Value, k: &str) -> Res<Vec<String>> {
    v.as_array()
        .and_then(|a| {
            a.iter()
                .map(|x| x.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| bad(format!("{k} must be an array of strings")))
}

fn under(p: &str, root: &str) -> bool {
    p == root
        || (root == "/" && p.starts_with('/'))
        || (p.starts_with(root) && p.as_bytes().get(root.len()) == Some(&b'/'))
}

pub fn all(reg: &Registry) -> Res<Vec<Workspace>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "SELECT {WS_COLS} FROM workspaces ORDER BY pinned DESC, rank, name"
    )))?;
    let rows = sql(st.query_map([], ws_row))?;
    sql(rows.collect())
}

pub fn get(reg: &Registry, id: &str) -> Res<Workspace> {
    sql(reg
        .conn
        .query_row(
            &format!("SELECT {WS_COLS} FROM workspaces WHERE id = ?1"),
            [id],
            ws_row,
        )
        .optional())?
    .ok_or_else(|| (404, format!("no workspace {id}")))
}

fn by_path(reg: &Registry, path: &str) -> Res<Option<Workspace>> {
    sql(reg
        .conn
        .query_row(
            &format!("SELECT {WS_COLS} FROM workspaces WHERE path = ?1"),
            [path],
            ws_row,
        )
        .optional())
}

fn save(ctx: &Ctx, reg: &Registry, w: &mut Workspace, kind: &str) -> Res<()> {
    w.updated_ms = now_ms();
    sql(update_row!(
        reg.conn, "workspaces", id = w.id;
        name = w.name,
        folder_id = w.folder_id,
        pinned = w.pinned,
        archived = w.archived,
        rank = w.rank,
        tabs = json!(w.tabs).to_string(),
        notify_idle = w.notify_idle,
        updated_ms = w.updated_ms,
        cwd = w.cwd,
    ))?;
    publish(
        kind,
        json!({"workspace": w.to_json(&ctx.config.station_id)}),
    );
    Ok(())
}

fn ensure_auto(ctx: &Ctx, reg: &Registry, projects: &[Value]) -> Res<()> {
    let now = now_ms();
    for (i, p) in projects.iter().enumerate() {
        let (Some(path), Some(name)) = (p["path"].as_str(), p["name"].as_str()) else {
            continue;
        };
        if p["missing"] == true {
            continue;
        }
        let n = sql(reg.conn.execute(
            "INSERT OR IGNORE INTO workspaces (id, name, path, auto, rank, created_ms, updated_ms)
             SELECT ?1, ?2, ?3, 1, ?4, ?5, ?5
             WHERE NOT EXISTS (SELECT 1 FROM workspace_tombstones WHERE path = ?3)",
            params![ulid(now), name, path, (now + i as i64) as f64, now],
        ))?;
        if n > 0 {
            if let Some(w) = by_path(reg, path)? {
                publish_ws(ctx, "workspace.created", &w);
                ensure_submodules(ctx, reg, std::slice::from_ref(&w))?;
            }
        }
    }
    Ok(())
}

pub fn list(ctx: &Ctx) -> Res<Vec<Workspace>> {
    let lines = crate::daemon::cached_lines(ctx, None).map_err(|e| (500, e))?;
    let projects = crate::tasks::projects(ctx, &lines)?;
    let reg = db(ctx)?;
    ensure_auto(ctx, &reg, &projects)?;
    all(&reg)
}

fn folder_exists(reg: &Registry, id: &str) -> Res<Folder> {
    get_folder(reg, id).map_err(|(c, e)| if c == 404 { bad(e) } else { (c, e) })
}

fn folder_ref(reg: &Registry, v: &Value) -> Res<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(id) => folder_exists(reg, id).map(|f| Some(f.id)),
        _ => Err(bad("folderId must be a string or null")),
    }
}

fn canonical(path: &str) -> Res<String> {
    canonical_as(path, "path")
}

fn canonical_as(path: &str, field: &str) -> Res<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err(bad(format!("{field} required")));
    }
    let path = match path.strip_prefix("~/") {
        Some(rest) => crate::home().join(rest),
        None if path == "~" => crate::home(),
        None => Path::new(path).to_path_buf(),
    };
    if !path.is_absolute() {
        return Err(bad(format!("{field} must be absolute")));
    }
    if !path.is_dir() {
        return Err(bad(format!("{} is not a directory", path.display())));
    }
    let p = path
        .canonicalize()
        .map_err(|e| bad(format!("{}: {e}", path.display())))?;
    Ok(p.to_string_lossy().into_owned())
}

struct Placement {
    parent: Option<Workspace>,
    folder_id: Option<String>,
    pinned: bool,
}

/// A nested workspace lives in its parent's block: it has no folder and is never pinned.
fn placement(reg: &Registry, b: &Value) -> Res<Placement> {
    let parent = match &b["parentId"] {
        Value::Null => None,
        Value::String(id) => Some(parent_ws(reg, id)?),
        _ => return Err(bad("parentId must be a string or null")),
    };
    if parent.is_some() && !(b["folderId"].is_null() && b["pinned"].is_null()) {
        return Err(bad("a nested workspace has no folder and cannot be pinned"));
    }
    Ok(Placement {
        folder_id: folder_ref(reg, &b["folderId"])?,
        pinned: match &b["pinned"] {
            Value::Null => false,
            v => bool_of(v, "pinned")?,
        },
        parent,
    })
}

fn parent_ws(reg: &Registry, id: &str) -> Res<Workspace> {
    let p = get(reg, id).map_err(|(c, e)| if c == 404 { bad(e) } else { (c, e) })?;
    if p.subject || p.parent_id.is_some() {
        return Err(bad("the parent must be a top-level folder workspace"));
    }
    Ok(p)
}

fn publish_ws(ctx: &Ctx, kind: &str, w: &Workspace) {
    publish(
        kind,
        json!({"workspace": w.to_json(&ctx.config.station_id)}),
    );
}

fn create_subject(ctx: &Ctx, b: &Value) -> Res<(bool, Workspace)> {
    if !b["path"].is_null() {
        return Err(bad("a subject has no path, give cwd"));
    }
    let name = match &b["name"] {
        Value::Null => return Err(bad("name required")),
        v => name_of(v)?,
    };
    let reg = db(ctx)?;
    let at = placement(&reg, b)?;
    let cwd = match (&b["cwd"], &at.parent) {
        (Value::Null, Some(p)) => p.path.clone(),
        (v, _) => canonical_as(v.as_str().unwrap_or_default(), "cwd")?,
    };
    let now = now_ms();
    let id = ulid(now);
    sql(reg.conn.execute(
        "INSERT INTO workspaces (id, name, path, folder_id, pinned, auto, rank, created_ms, updated_ms, kind, cwd, parent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?7, 'subject', ?8, ?9)",
        params![id, name, subject_path(&id), at.folder_id, at.pinned, now as f64, now, cwd, at.parent.map(|p| p.id)],
    ))?;
    let w = get(&reg, &id)?;
    publish_ws(ctx, "workspace.created", &w);
    Ok((true, w))
}

fn nested_cwd(v: &Value) -> Res<Option<String>> {
    match v {
        Value::Null => Ok(None),
        v => canonical_as(v.as_str().unwrap_or_default(), "cwd").map(Some),
    }
}

pub fn create(ctx: &Ctx, b: &Value) -> Res<(bool, Workspace)> {
    object(b)?;
    match &b["kind"] {
        Value::Null => {}
        Value::String(k) if k == "dir" => {}
        Value::String(k) if k == "subject" => return create_subject(ctx, b),
        _ => return Err(bad("kind must be \"dir\" or \"subject\"")),
    }
    let path = canonical(b["path"].as_str().unwrap_or_default())?;
    let reg = db(ctx)?;
    let at = placement(&reg, b)?;
    if at.parent.is_none() && !b["cwd"].is_null() {
        return Err(bad("cwd is only for subjects and nested workspaces"));
    }
    if let Some(p) = &at.parent {
        if p.path == path || !under(&path, &p.path) {
            return Err(bad(format!("{path} is not inside {}", p.path)));
        }
    }
    let cwd = nested_cwd(&b["cwd"])?;
    let name = match &b["name"] {
        Value::Null => Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone()),
        v => name_of(v)?,
    };
    sql(reg
        .conn
        .execute("DELETE FROM workspace_tombstones WHERE path = ?1", [&path]))?;
    if let Some(mut w) = by_path(&reg, &path)? {
        if let Some(p) = &at.parent {
            if w.parent_id.is_none() && !is_parent(&reg, &w.id)? {
                adopt(&reg, &w.id, &p.id)?;
                w = get(&reg, &w.id)?;
                if !b["name"].is_null() {
                    w.name = name;
                }
                w.cwd = cwd;
                w.archived = false;
                save(ctx, &reg, &mut w, "workspace.updated")?;
                return Ok((false, w));
            }
        }
        if w.archived {
            w.archived = false;
            save(ctx, &reg, &mut w, "workspace.updated")?;
        }
        return Ok((false, w));
    }
    let now = now_ms();
    let id = ulid(now);
    sql(reg.conn.execute(
        "INSERT INTO workspaces (id, name, path, folder_id, pinned, auto, rank, created_ms, updated_ms, cwd, parent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?7, ?8, ?9)",
        params![id, name, path, at.folder_id, at.pinned, now as f64, now, cwd, at.parent.as_ref().map(|p| &p.id)],
    ))?;
    let w = get(&reg, &id)?;
    publish_ws(ctx, "workspace.created", &w);
    if at.parent.is_none() {
        ensure_submodules(ctx, &reg, std::slice::from_ref(&w))?;
    }
    Ok((true, w))
}

fn is_parent(reg: &Registry, id: &str) -> Res<bool> {
    sql(reg.conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM workspaces WHERE parent_id = ?1)",
        [id],
        |r| r.get(0),
    ))
}

fn adopt(reg: &Registry, id: &str, parent: &str) -> Res<()> {
    sql(reg.conn.execute(
        "UPDATE workspaces SET parent_id = ?2, folder_id = NULL, pinned = 0 WHERE id = ?1",
        params![id, parent],
    ))
    .map(drop)
}

/// The `path = ...` entries of a `.gitmodules` file, relative to the repository.
pub fn submodule_paths(gitmodules: &str) -> Vec<String> {
    gitmodules
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == "path").then(|| v.trim().trim_matches('"').to_string())
        })
        .filter(|p| !p.is_empty() && !Path::new(p).is_absolute())
        .collect()
}

/// Nests a workspace for every checked-out submodule of the given top-level workspaces:
/// creates it, or adopts the top-level workspace already at that path (keeping its id,
/// tabs and sessions). A submodule workspace the user deleted stays deleted.
pub fn ensure_submodules(ctx: &Ctx, reg: &Registry, parents: &[Workspace]) -> Res<()> {
    for p in parents
        .iter()
        .filter(|p| !p.subject && p.parent_id.is_none())
    {
        let Ok(text) = std::fs::read_to_string(Path::new(&p.path).join(".gitmodules")) else {
            continue;
        };
        for rel in submodule_paths(&text) {
            let dir = Path::new(&p.path).join(&rel);
            if !dir.join(".git").exists() {
                continue;
            }
            let Ok(dir) = dir.canonicalize() else {
                continue;
            };
            let path = dir.to_string_lossy().into_owned();
            if path == p.path || !under(&path, &p.path) {
                continue;
            }
            let gone: bool = sql(reg.conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM workspace_tombstones WHERE path = ?1)",
                [&path],
                |r| r.get(0),
            ))?;
            if gone {
                continue;
            }
            match by_path(reg, &path)? {
                Some(w) if w.parent_id.is_some() || is_parent(reg, &w.id)? => {}
                Some(w) => {
                    adopt(reg, &w.id, &p.id)?;
                    publish_ws(ctx, "workspace.updated", &get(reg, &w.id)?);
                }
                None => {
                    let now = now_ms();
                    let id = ulid(now);
                    let name = dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| rel.clone());
                    sql(reg.conn.execute(
                        "INSERT INTO workspaces (id, name, path, auto, rank, created_ms, updated_ms, parent_id)
                         VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5, ?6)",
                        params![id, name, path, now as f64, now, p.id],
                    ))?;
                    publish_ws(ctx, "workspace.created", &get(reg, &id)?);
                }
            }
        }
    }
    Ok(())
}

pub fn scan_submodules(ctx: &Ctx) -> Res<()> {
    let reg = db(ctx)?;
    let top = all(&reg)?;
    ensure_submodules(ctx, &reg, &top)
}

pub fn patch(ctx: &Ctx, id: &str, b: &Value) -> Res<Workspace> {
    let obj = object(b)?;
    let reg = db(ctx)?;
    let mut w = get(&reg, id)?;
    let nested = w.parent_id.is_some();
    for (k, v) in obj {
        match k.as_str() {
            "name" => w.name = name_of(v)?,
            "folderId" | "pinned" if nested && !v.is_null() && v != &json!(false) => {
                return Err(bad("a nested workspace has no folder and cannot be pinned"))
            }
            "folderId" => w.folder_id = folder_ref(&reg, v)?,
            "pinned" => w.pinned = bool_of(v, k)?,
            "archived" => w.archived = bool_of(v, k)?,
            "notifyIdle" => w.notify_idle = bool_of(v, k)?,
            "rank" => w.rank = rank_of(v)?,
            "tabs" => w.tabs = strs_of(v, k)?,
            "cwd" if w.subject => {
                w.cwd = Some(canonical_as(v.as_str().unwrap_or_default(), "cwd")?)
            }
            "cwd" if nested => w.cwd = nested_cwd(v)?,
            "cwd" => return Err(bad("cwd is only for subjects and nested workspaces")),
            _ => {}
        }
    }
    save(ctx, &reg, &mut w, "workspace.updated")?;
    Ok(w)
}

pub fn delete(reg: &Registry, id: &str) -> Res<Value> {
    let w = get(reg, id)?;
    let children: Vec<Workspace> = all(reg)?
        .into_iter()
        .filter(|c| c.parent_id.as_deref() == Some(id))
        .collect();
    for c in children.iter().chain(std::iter::once(&w)) {
        drop_one(reg, c)?;
    }
    let mut out = json!({ "deleted": id });
    if !children.is_empty() {
        out["children"] = json!(children.iter().map(|c| &c.id).collect::<Vec<_>>());
    }
    Ok(out)
}

fn drop_one(reg: &Registry, w: &Workspace) -> Res<()> {
    let id = w.id.as_str();
    if !w.subject {
        sql(reg.conn.execute(
            "INSERT OR REPLACE INTO workspace_tombstones (path, at_ms) VALUES (?1, ?2)",
            params![w.path, now_ms()],
        ))?;
    }
    sql(reg
        .conn
        .execute("DELETE FROM workspaces WHERE id = ?1", [id]))?;
    sql(reg.conn.execute(
        "UPDATE lines SET workspace_id = NULL WHERE workspace_id = ?1",
        [id],
    ))?;
    for t in crate::term::all() {
        if t.info().workspace_id.as_deref() == Some(id) {
            t.set_workspace(None);
        }
    }
    publish("workspace.deleted", json!({"id": id}));
    Ok(())
}

pub fn folders(reg: &Registry) -> Res<Vec<Folder>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "SELECT {FOLDER_COLS} FROM folders ORDER BY rank, name"
    )))?;
    let rows = sql(st.query_map([], folder_row))?;
    sql(rows.collect())
}

fn get_folder(reg: &Registry, id: &str) -> Res<Folder> {
    sql(reg
        .conn
        .query_row(
            &format!("SELECT {FOLDER_COLS} FROM folders WHERE id = ?1"),
            [id],
            folder_row,
        )
        .optional())?
    .ok_or_else(|| (404, format!("no folder {id}")))
}

fn save_folder(reg: &Registry, f: &mut Folder, kind: &str) -> Res<()> {
    f.updated_ms = now_ms();
    sql(reg.conn.execute(
        "UPDATE folders SET name = ?2, parent_id = ?3, rank = ?4, collapsed = ?5, updated_ms = ?6 WHERE id = ?1",
        params![f.id, f.name, f.parent_id, f.rank, f.collapsed, f.updated_ms],
    ))?;
    publish(kind, json!({"folder": f.to_json()}));
    Ok(())
}

fn parent_of(reg: &Registry, v: &Value, own: Option<&str>) -> Res<Option<String>> {
    let id = match v {
        Value::Null => return Ok(None),
        Value::String(id) => id,
        _ => return Err(bad("parentId must be a string or null")),
    };
    if own == Some(id.as_str()) {
        return Err(bad("a folder cannot be its own parent"));
    }
    let p = folder_exists(reg, id)?;
    if p.parent_id.is_some() {
        return Err(bad(
            "parentId must be a top-level folder: folders nest one level",
        ));
    }
    Ok(Some(p.id))
}

pub fn create_folder(b: &Value, reg: &Registry) -> Res<Folder> {
    object(b)?;
    let name = name_of(&b["name"])?;
    let parent_id = parent_of(reg, &b["parentId"], None)?;
    let now = now_ms();
    let rank = match &b["rank"] {
        Value::Null => now as f64,
        v => rank_of(v)?,
    };
    let id = ulid(now);
    sql(reg.conn.execute(
        "INSERT INTO folders (id, name, parent_id, rank, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, parent_id, rank, now],
    ))?;
    let f = get_folder(reg, &id)?;
    publish("folder.created", json!({"folder": f.to_json()}));
    Ok(f)
}

fn children(reg: &Registry, id: &str) -> Res<Vec<Folder>> {
    Ok(folders(reg)?
        .into_iter()
        .filter(|f| f.parent_id.as_deref() == Some(id))
        .collect())
}

pub fn patch_folder(reg: &Registry, id: &str, b: &Value) -> Res<Folder> {
    let obj = object(b)?;
    let mut f = get_folder(reg, id)?;
    for (k, v) in obj {
        match k.as_str() {
            "name" => f.name = name_of(v)?,
            "rank" => f.rank = rank_of(v)?,
            "collapsed" => f.collapsed = bool_of(v, k)?,
            "parentId" => {
                f.parent_id = parent_of(reg, v, Some(id))?;
                if f.parent_id.is_some() && !children(reg, id)?.is_empty() {
                    return Err(bad("a folder with sub-folders cannot get a parent"));
                }
            }
            _ => {}
        }
    }
    save_folder(reg, &mut f, "folder.updated")?;
    Ok(f)
}

pub fn delete_folder(ctx: &Ctx, reg: &Registry, id: &str) -> Res<Value> {
    let f = get_folder(reg, id)?;
    for mut w in all(reg)?
        .into_iter()
        .filter(|w| w.folder_id.as_deref() == Some(id))
    {
        w.folder_id = f.parent_id.clone();
        save(ctx, reg, &mut w, "workspace.updated")?;
    }
    for mut c in children(reg, id)? {
        c.parent_id = f.parent_id.clone();
        save_folder(reg, &mut c, "folder.updated")?;
    }
    sql(reg.conn.execute("DELETE FROM folders WHERE id = ?1", [id]))?;
    publish("folder.deleted", json!({"id": id}));
    Ok(json!({"deleted": id}))
}

pub fn groups(reg: &Registry) -> Res<Vec<Group>> {
    let mut st = sql(reg.conn.prepare(&format!(
        "SELECT {GROUP_COLS} FROM groups ORDER BY rank, name"
    )))?;
    let rows = sql(st.query_map([], group_row))?;
    sql(rows.collect())
}

pub fn get_group(reg: &Registry, id: &str) -> Res<Group> {
    sql(reg
        .conn
        .query_row(
            &format!("SELECT {GROUP_COLS} FROM groups WHERE id = ?1"),
            [id],
            group_row,
        )
        .optional())?
    .ok_or_else(|| (404, format!("no group {id}")))
}

fn keys_of(v: &Value, k: &str) -> Res<Vec<String>> {
    let keys = strs_of(v, k)?;
    for key in &keys {
        let ok = matches!(key.split_once(':'), Some(("line" | "term", id)) if !id.is_empty());
        if !ok {
            return Err(bad(format!(
                "{k}: {key:?} is not a line:<id> or term:<id> key"
            )));
        }
    }
    Ok(keys)
}

fn add_keys(members: &mut Vec<String>, keys: Vec<String>) {
    for k in keys {
        if !members.contains(&k) {
            members.push(k);
        }
    }
}

fn save_group(reg: &Registry, g: &mut Group, kind: &str) -> Res<()> {
    g.updated_ms = now_ms();
    sql(reg.conn.execute(
        "UPDATE groups SET name = ?2, rank = ?3, members = ?4, notify_idle = ?5, updated_ms = ?6 WHERE id = ?1",
        params![g.id, g.name, g.rank, json!(g.members).to_string(), g.notify_idle, g.updated_ms],
    ))?;
    publish(kind, json!({"group": g.to_json()}));
    Ok(())
}

pub fn create_group(reg: &Registry, b: &Value) -> Res<Group> {
    object(b)?;
    let name = name_of(&b["name"])?;
    let mut members = vec![];
    if !b["members"].is_null() {
        add_keys(&mut members, keys_of(&b["members"], "members")?);
    }
    let now = now_ms();
    let rank = match &b["rank"] {
        Value::Null => now as f64,
        v => rank_of(v)?,
    };
    let id = ulid(now);
    sql(reg.conn.execute(
        "INSERT INTO groups (id, name, rank, members, created_ms, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, name, rank, json!(members).to_string(), now],
    ))?;
    let g = get_group(reg, &id)?;
    publish("group.created", json!({"group": g.to_json()}));
    Ok(g)
}

pub fn patch_group(reg: &Registry, id: &str, b: &Value) -> Res<Group> {
    let obj = object(b)?;
    let mut g = get_group(reg, id)?;
    for (k, v) in obj {
        match k.as_str() {
            "name" => g.name = name_of(v)?,
            "rank" => g.rank = rank_of(v)?,
            "notifyIdle" => g.notify_idle = bool_of(v, k)?,
            _ => {}
        }
    }
    if let Some(v) = obj.get("members") {
        g.members.clear();
        add_keys(&mut g.members, keys_of(v, "members")?);
    }
    if let Some(v) = obj.get("add") {
        add_keys(&mut g.members, keys_of(v, "add")?);
    }
    if let Some(v) = obj.get("remove") {
        let gone = keys_of(v, "remove")?;
        g.members.retain(|m| !gone.contains(m));
    }
    save_group(reg, &mut g, "group.updated")?;
    Ok(g)
}

pub fn delete_group(reg: &Registry, id: &str) -> Res<Value> {
    get_group(reg, id)?;
    sql(reg.conn.execute("DELETE FROM groups WHERE id = ?1", [id]))?;
    publish("group.deleted", json!({"id": id}));
    Ok(json!({"deleted": id}))
}

pub fn repo_root<'a>(projects: &'a [(String, String)], repo: &str, path: &str) -> Option<&'a str> {
    let clones: Vec<&str> = projects
        .iter()
        .filter(|(_, r)| !repo.is_empty() && r == repo)
        .map(|(p, _)| p.as_str())
        .collect();
    clones
        .iter()
        .copied()
        .filter(|p| under(path, p))
        .max_by_key(|p| p.len())
        .or(match clones[..] {
            [only] => Some(only),
            _ => None,
        })
}

pub struct Index {
    pub workspaces: Vec<Workspace>,
    pub tasks: Vec<Task>,
    pub projects: Vec<(String, String)>,
}

impl Index {
    pub fn new(ctx: &Ctx, lines: &[Line]) -> Res<Index> {
        let projects = crate::tasks::projects(ctx, lines)?
            .into_iter()
            .filter_map(|p| Some((p["path"].as_str()?.into(), p["repo"].as_str()?.into())))
            .collect();
        let reg = db(ctx)?;
        Ok(Index {
            workspaces: all(&reg)?,
            tasks: crate::tasks::all(&reg)?,
            projects,
        })
    }

    fn of_task(&self, id: &str) -> Option<&Workspace> {
        self.tasks
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| self.longest(&t.project))
    }

    fn assigned(&self, id: Option<&str>) -> Option<&Workspace> {
        id.and_then(|id| self.workspaces.iter().find(|w| w.id == id))
    }

    fn longest(&self, p: &str) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .filter(|w| !w.subject && !p.is_empty() && under(p, &w.path))
            .max_by_key(|w| w.path.len())
    }

    pub fn of_line(&self, l: &Line) -> Option<&Workspace> {
        if let Some(w) = self.assigned(l.workspace_id.as_deref()) {
            return Some(w);
        }
        if let Some(w) = self
            .tasks
            .iter()
            .find(|t| !t.from_session() && t.line_id.as_deref() == Some(l.id.as_str()))
            .and_then(|t| self.longest(&t.project))
        {
            return Some(w);
        }
        let path = if l.worktree_path.is_empty()
            || (!l.cwd.is_empty() && under(&l.cwd, &l.worktree_path))
        {
            &l.cwd
        } else {
            &l.worktree_path
        };
        let prefix = self.longest(path);
        let repo = repo_root(&self.projects, &l.repo, path).and_then(|p| self.longest(p));
        match (prefix, repo) {
            (p, Some(r)) if p.is_none_or(|p| !under(&p.path, &r.path)) => Some(r),
            (p, _) => p,
        }
    }

    pub fn of_term(&self, t: &crate::term::Info) -> Option<&Workspace> {
        self.assigned(t.workspace_id.as_deref())
            .or_else(|| t.task_id.as_deref().and_then(|id| self.of_task(id)))
            .or_else(|| self.longest(&t.cwd))
    }
}

pub fn start_body(ctx: &Ctx, body: &Value) -> Res<Value> {
    let Some(id) = body["workspaceId"].as_str() else {
        return Ok(body.clone());
    };
    let w = get(&db(ctx)?, id)?;
    let mut b = body.clone();
    if b["cwd"].is_null() {
        b["cwd"] = json!(w.start_dir());
    }
    Ok(b)
}

fn assignment(ctx: &Ctx, b: &Value) -> Res<Option<String>> {
    match object(b)?.get("workspaceId") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(id)) => get(&db(ctx)?, id).map(|w| Some(w.id)),
        _ => Err(bad("workspaceId must be a string or null")),
    }
}

fn assign_line(ctx: &Ctx, q: &str, b: &Value) -> Res<Value> {
    let wid = assignment(ctx, b)?;
    let lines = crate::scan(ctx).map_err(|e| (500, e))?;
    let id = crate::actions::find_line(&lines, q)
        .map_err(|e| (404, e))?
        .id
        .clone();
    let reg = db(ctx)?;
    let mark = || reg.set_mark(&id, "workspace_id", wid.as_deref());
    if wid.is_none() {
        let term = lines
            .iter()
            .find(|l| l.id == id)
            .and_then(|l| l.term_id.as_deref())
            .and_then(crate::term::get);
        unassign_in_order(|| term.iter().for_each(|t| t.set_workspace(None)), mark)
    } else {
        mark()
    }
    .map_err(|e| (500, e))?;
    Ok(json!({"lineId": id, "workspaceId": wid}))
}

/// Clears the term before the line mark: a scan between the two would otherwise
/// hand the line its term's assignment back.
fn unassign_in_order(
    clear_term: impl FnOnce(),
    clear_mark: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    clear_term();
    clear_mark()
}

fn assign_term(ctx: &Ctx, id: &str, b: &Value) -> Res<Value> {
    let t = crate::term::get(id).ok_or_else(|| (404, format!("no term {id}")))?;
    t.set_workspace(assignment(ctx, b)?);
    Ok(t.to_json())
}

fn out<T>(code: u16, r: Res<T>, f: impl FnOnce(T) -> Value) -> (u16, Value) {
    match r {
        Ok(v) => (code, f(v)),
        Err((c, e)) => (c, json!({ "error": e })),
    }
}

pub fn route(r: &Request, segs: &[&str], body: &Value, ctx: &Ctx) -> Option<(u16, Value)> {
    let st = ctx.config.station_id.as_str();
    let ws = |w: Workspace| w.to_json(st);
    let reg = || db(ctx);
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "workspaces"]) => out(200, list(ctx), |v| {
            Value::Array(
                v.iter()
                    .filter(|w| !(w.auto && w.missing()))
                    .map(|w| w.to_json(st))
                    .collect(),
            )
        }),
        ("POST", ["v1", "workspaces"]) => match create(ctx, body) {
            Ok((created, w)) => (if created { 201 } else { 200 }, w.to_json(st)),
            Err((c, e)) => (c, json!({ "error": e })),
        },
        ("PATCH", ["v1", "workspaces", id]) => out(200, patch(ctx, id, body), ws),
        ("DELETE", ["v1", "workspaces", id]) => {
            crate::daemon::mark(crate::daemon::Effect::Rescan);
            out(200, reg().and_then(|g| delete(&g, id)), |v| v)
        }
        ("GET", ["v1", "folders"]) => out(200, reg().and_then(|g| folders(&g)), |v| {
            Value::Array(v.iter().map(Folder::to_json).collect())
        }),
        ("POST", ["v1", "folders"]) => out(201, reg().and_then(|g| create_folder(body, &g)), |f| {
            f.to_json()
        }),
        ("PATCH", ["v1", "folders", id]) => {
            out(200, reg().and_then(|g| patch_folder(&g, id, body)), |f| {
                f.to_json()
            })
        }
        ("DELETE", ["v1", "folders", id]) => {
            out(200, reg().and_then(|g| delete_folder(ctx, &g, id)), |v| v)
        }
        ("GET", ["v1", "groups"]) => out(200, reg().and_then(|g| groups(&g)), |v| {
            Value::Array(v.iter().map(Group::to_json).collect())
        }),
        ("POST", ["v1", "groups"]) => out(201, reg().and_then(|g| create_group(&g, body)), |g| {
            g.to_json()
        }),
        ("PATCH", ["v1", "groups", id]) => {
            out(200, reg().and_then(|g| patch_group(&g, id, body)), |g| {
                g.to_json()
            })
        }
        ("DELETE", ["v1", "groups", id]) => {
            out(200, reg().and_then(|g| delete_group(&g, id)), |v| v)
        }
        ("POST", ["v1", "broadcast"]) => out(200, crate::broadcast::post(ctx, body), |v| v),
        ("PUT", ["v1", "lines", id, "workspace"]) => {
            crate::daemon::mark(crate::daemon::Effect::Persist);
            out(200, assign_line(ctx, id, body), |v| v)
        }
        ("PUT", ["v1", "terms", id, "workspace"]) => {
            crate::daemon::mark(crate::daemon::Effect::Persist);
            out(200, assign_term(ctx, id, body), |v| v)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {

    #[test]
    fn submodule_paths_reads_path_entries_and_skips_absolute_ones() {
        let text = "[submodule \"backend\"]\n\tpath = src/backend/document-service\n\turl = git@x:o/d.git\n\tignore = all\n[submodule \"web\"]\n  path=src/webplatform\n[submodule \"bad\"]\n\tpath = /etc\n\tpath =\n";
        assert_eq!(
            submodule_paths(text),
            vec!["src/backend/document-service", "src/webplatform"]
        );
        assert!(submodule_paths("").is_empty());
    }

    #[test]
    fn a_nested_dir_starts_in_its_cwd_but_is_missing_only_when_its_own_folder_is_gone() {
        let mut n = ws("/nonexistent-nested");
        n.parent_id = Some("p".into());
        n.cwd = Some("/tmp".into());
        assert_eq!(n.start_dir(), "/tmp");
        assert!(n.missing());
        assert_eq!(n.to_json("st")["parentId"], "p");
        n.cwd = None;
        assert_eq!(n.start_dir(), "/nonexistent-nested");
    }
    use super::*;

    fn ws(path: &str) -> Workspace {
        Workspace {
            id: path.into(),
            name: path.into(),
            path: path.into(),
            folder_id: None,
            pinned: false,
            archived: false,
            auto: false,
            rank: 0.0,
            tabs: vec![],
            notify_idle: false,
            created_ms: 0,
            updated_ms: 0,
            subject: false,
            cwd: None,
            parent_id: None,
        }
    }

    fn line(id: &str, wt: &str, repo: &str) -> Line {
        Line {
            id: id.into(),
            worktree_path: wt.into(),
            repo: repo.into(),
            ..Default::default()
        }
    }

    #[test]
    fn membership_prefers_task_then_prefix_inside_the_repo_then_repo() {
        let ix = Index {
            workspaces: vec![
                ws("/home/u"),
                ws("/home/u/src/app"),
                ws("/home/u/src/app/web"),
                ws("/home/u/src/lib"),
            ],
            tasks: vec![Task {
                id: "t1".into(),
                project: "/home/u/src/lib".into(),
                line_id: Some("l-task".into()),
                ..Default::default()
            }],
            projects: vec![
                ("/home/u/src/app".into(), "github.com/a/app".into()),
                ("/home/u/src/lib".into(), "github.com/a/lib".into()),
            ],
        };
        let of = |l: Line| ix.of_line(&l).map(|w| w.path.clone());
        assert_eq!(
            of(line("l-task", "/home/u/src/app", "github.com/a/app")).as_deref(),
            Some("/home/u/src/lib")
        );
        assert_eq!(
            of(line("a", "/home/u/src/app/web/x", "github.com/a/app")).as_deref(),
            Some("/home/u/src/app/web")
        );
        assert_eq!(
            of(line("b", "/home/u/wt/app-1", "github.com/a/app")).as_deref(),
            Some("/home/u/src/app")
        );
        assert_eq!(
            of(line("c", "/home/u/notes", "")).as_deref(),
            Some("/home/u")
        );
        assert_eq!(of(line("d", "/srv/x", "github.com/z/z")), None);
        assert_eq!(
            of(line("e", "/home/u/src/applet", "")).as_deref(),
            Some("/home/u")
        );
        let mut cwd = line("f", "", "");
        cwd.cwd = "/home/u/src/lib/sub".into();
        assert_eq!(of(cwd).as_deref(), Some("/home/u/src/lib"));
    }

    #[test]
    fn a_session_in_a_second_clone_of_a_repo_stays_in_that_clone_workspace() {
        let ix = Index {
            workspaces: vec![
                ws("/home/u/work/atk"),
                ws("/home/u/work/atk2"),
                ws("/home/u/.overseer/worktrees"),
            ],
            tasks: vec![],
            projects: vec![
                ("/home/u/work/atk".into(), "github.com/o/pkg".into()),
                ("/home/u/work/atk2".into(), "github.com/o/pkg".into()),
            ],
        };
        let of = |p: &str| {
            ix.of_line(&line("l", p, "github.com/o/pkg"))
                .map(|w| w.path.clone())
        };
        assert_eq!(of("/home/u/work/atk").as_deref(), Some("/home/u/work/atk"));
        assert_eq!(
            of("/home/u/work/atk2/src").as_deref(),
            Some("/home/u/work/atk2")
        );
        assert_eq!(
            of("/home/u/.overseer/worktrees/atk-1/feat").as_deref(),
            Some("/home/u/.overseer/worktrees")
        );
        assert_eq!(of("/srv/feat"), None);
    }

    #[test]
    fn repo_root_picks_the_clone_holding_the_path_or_the_only_clone() {
        let two = [
            ("/a/atk".to_string(), "r".to_string()),
            ("/a/atk2".to_string(), "r".to_string()),
        ];
        assert_eq!(repo_root(&two, "r", "/a/atk2/src"), Some("/a/atk2"));
        assert_eq!(repo_root(&two, "r", "/w/atk-1"), None);
        assert_eq!(repo_root(&two[..1], "r", "/w/atk-1"), Some("/a/atk"));
        assert_eq!(repo_root(&two, "", "/a/atk"), None);
    }

    #[test]
    fn a_session_in_a_child_workspace_folder_belongs_to_the_child_not_the_repo_root() {
        let ix = Index {
            workspaces: vec![ws("/home/u/src/app"), ws("/home/u/src/app/web")],
            tasks: vec![],
            projects: vec![("/home/u/src/app".into(), "github.com/a/app".into())],
        };
        let of = |cwd: &str| {
            let mut l = line("l", "/home/u/src/app", "github.com/a/app");
            l.cwd = cwd.into();
            ix.of_line(&l).map(|w| w.path.clone())
        };
        assert_eq!(
            of("/home/u/src/app/web/src").as_deref(),
            Some("/home/u/src/app/web")
        );
        assert_eq!(of("/home/u/src/app").as_deref(), Some("/home/u/src/app"));
        assert_eq!(of("/srv/elsewhere").as_deref(), Some("/home/u/src/app"));
    }

    #[test]
    fn sessions_of_a_project_inside_a_parent_workspace_join_it() {
        let ix = Index {
            workspaces: vec![ws("/home/u/work")],
            tasks: vec![
                Task {
                    id: "t1".into(),
                    project: "/home/u/work/webshop".into(),
                    line_id: Some("l-task".into()),
                    ..Default::default()
                },
                Task {
                    id: "t2".into(),
                    project: "/home/u/work/webshop".into(),
                    ..Default::default()
                },
            ],
            projects: vec![
                ("/home/u/work/webshop".into(), "github.com/a/webshop".into()),
                ("/home/u/work/x".into(), "local:/home/u/work/x".into()),
            ],
        };
        let wt = "/home/u/.overseer/worktrees/webshop-1/task-1";
        let of = |l: Line| ix.of_line(&l).map(|w| w.path.clone());
        assert_eq!(of(line("l-task", wt, "")).as_deref(), Some("/home/u/work"));
        assert_eq!(
            of(line("r", wt, "github.com/a/webshop")).as_deref(),
            Some("/home/u/work")
        );
        assert_eq!(
            of(line("l", wt, "local:/home/u/work/x")).as_deref(),
            Some("/home/u/work")
        );
        let t = crate::term::Info {
            cwd: "/srv".into(),
            task_id: Some("t2".into()),
            ..Default::default()
        };
        assert_eq!(
            ix.of_term(&t).map(|w| w.path.as_str()),
            Some("/home/u/work")
        );
    }

    #[test]
    fn an_explicit_assignment_beats_derived_membership_and_unknown_ids_fall_back() {
        let mut ix = Index {
            workspaces: vec![ws("/home/u"), ws("/home/u/src/app"), ws("/home/u/src/lib")],
            tasks: vec![Task {
                id: "t1".into(),
                project: "/home/u/src/lib".into(),
                line_id: Some("l-task".into()),
                ..Default::default()
            }],
            projects: vec![],
        };
        let mut l = line("l-task", "/home/u/src/app", "");
        l.workspace_id = Some("/home/u".into());
        let t = crate::term::Info {
            cwd: "/home/u/src/app/x".into(),
            workspace_id: Some("/home/u".into()),
            ..Default::default()
        };
        assert_eq!(ix.of_line(&l).map(|w| w.path.as_str()), Some("/home/u"));
        assert_eq!(ix.of_term(&t).map(|w| w.path.as_str()), Some("/home/u"));
        ix.workspaces.retain(|w| w.path != "/home/u");
        assert_eq!(
            ix.of_line(&l).map(|w| w.path.as_str()),
            Some("/home/u/src/lib")
        );
        assert_eq!(
            ix.of_term(&t).map(|w| w.path.as_str()),
            Some("/home/u/src/app")
        );
    }

    #[test]
    fn a_subject_never_matches_by_path_but_wins_when_assigned() {
        let mut subject = ws("subject:s1");
        subject.id = "s1".into();
        subject.path = String::new();
        subject.subject = true;
        subject.cwd = Some("/home/u/src/app".into());
        let ix = Index {
            workspaces: vec![ws("/home/u/src/app"), subject],
            tasks: vec![],
            projects: vec![],
        };
        let mut l = line("l1", "/home/u/src/app", "");
        let mut t = crate::term::Info {
            cwd: "/home/u/src/app".into(),
            ..Default::default()
        };
        assert_eq!(
            ix.of_line(&l).map(|w| w.id.as_str()),
            Some("/home/u/src/app")
        );
        assert_eq!(
            ix.of_term(&t).map(|w| w.id.as_str()),
            Some("/home/u/src/app")
        );
        l.workspace_id = Some("s1".into());
        t.workspace_id = Some("s1".into());
        assert_eq!(ix.of_line(&l).map(|w| w.id.as_str()), Some("s1"));
        assert_eq!(ix.of_term(&t).map(|w| w.id.as_str()), Some("s1"));
    }

    #[test]
    fn a_subject_starts_in_its_cwd_and_a_dir_in_its_path() {
        let mut s = ws("");
        s.subject = true;
        s.cwd = Some("/tmp".into());
        assert_eq!(s.start_dir(), "/tmp");
        assert!(!s.missing());
        assert!(s.to_json("st")["path"].is_null());
        assert_eq!(s.to_json("st")["kind"], "subject");
        assert_eq!(ws("/home/u").start_dir(), "/home/u");
        assert_eq!(ws("/home/u").to_json("st")["kind"], "dir");
    }

    #[test]
    fn unassigning_a_line_clears_its_term_before_the_mark() {
        let order = std::cell::RefCell::new(vec![]);
        unassign_in_order(
            || order.borrow_mut().push("term"),
            || {
                order.borrow_mut().push("mark");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*order.borrow(), ["term", "mark"]);
    }
}
