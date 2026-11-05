use crate::sys::OpenOptionsExt;
use crate::util::{format_rfc3339, ulid};
use crate::Line;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

pub struct Registry {
    pub conn: Connection,
}

#[derive(Default, Clone)]
pub struct Marks {
    pub id: Option<String>,
    pub ticket_override: Option<String>,
    pub title_override: Option<String>,
    pub pinned: bool,
    pub held_pid: i64,
    pub closed: bool,
    pub launch_args: Option<Vec<String>>,
    pub owner: Option<String>,
    pub origin: Option<String>,
    pub env_ref: Option<String>,
    pub workspace_id: Option<String>,
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct HookState {
    pub event: String,
    pub at_ms: i64,
    pub busy: bool,
    pub needs_input: bool,
    pub subagents: i64,
    pub tasks: i64,
    pub pid: Option<u32>,
    pub pane: Option<String>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS lines (
  id TEXT PRIMARY KEY,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  owner TEXT NOT NULL,
  station_id TEXT NOT NULL,
  repo TEXT NOT NULL,
  branch TEXT NOT NULL,
  worktree_path TEXT NOT NULL,
  cwd TEXT NOT NULL,
  ticket_key TEXT,
  state TEXT NOT NULL,
  held_reason TEXT,
  last_activity_at TEXT NOT NULL,
  launch_args TEXT,
  first_seen_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (agent, session_id)
);
CREATE TABLE IF NOT EXISTS worktrees (
  path TEXT PRIMARY KEY,
  repo TEXT NOT NULL,
  branch TEXT,
  ticket_key TEXT,
  volatile INTEGER NOT NULL,
  seen_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS hook_state (
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  event TEXT NOT NULL,
  at_ms INTEGER NOT NULL,
  busy INTEGER NOT NULL,
  needs_input INTEGER NOT NULL,
  subagents INTEGER NOT NULL,
  tasks INTEGER NOT NULL,
  pid INTEGER,
  pane TEXT,
  PRIMARY KEY (agent, session_id)
);
CREATE TABLE IF NOT EXISTS spool_offsets (
  file TEXT PRIMARY KEY,
  offset INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  project TEXT NOT NULL,
  status TEXT NOT NULL,
  agent TEXT NOT NULL,
  model TEXT NOT NULL,
  effort TEXT NOT NULL,
  ticket_key TEXT,
  line_id TEXT,
  term_id TEXT,
  branch TEXT,
  worktree_path TEXT,
  progress TEXT,
  error TEXT,
  started_ms INTEGER,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL,
  UNIQUE (project, number)
);
CREATE TABLE IF NOT EXISTS task_messages (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  from_id TEXT,
  body TEXT NOT NULL,
  created_ms INTEGER NOT NULL,
  delivered INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS task_messages_task ON task_messages (task_id, created_ms);
CREATE TABLE IF NOT EXISTS task_numbers (
  project TEXT PRIMARY KEY,
  last INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
  path TEXT PRIMARY KEY,
  added_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS task_sources (
  id TEXT PRIMARY KEY,
  project TEXT NOT NULL,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  created_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS workspaces (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  path TEXT NOT NULL UNIQUE,
  folder_id TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0,
  auto INTEGER NOT NULL DEFAULT 0,
  rank REAL NOT NULL,
  tabs TEXT NOT NULL DEFAULT '[]',
  notify_idle INTEGER NOT NULL DEFAULT 0,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS workspace_tombstones (
  path TEXT PRIMARY KEY,
  at_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS folders (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  parent_id TEXT,
  rank REAL NOT NULL,
  collapsed INTEGER NOT NULL DEFAULT 0,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS kv (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS groups (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  rank REAL NOT NULL,
  members TEXT NOT NULL DEFAULT '[]',
  notify_idle INTEGER NOT NULL DEFAULT 0,
  created_ms INTEGER NOT NULL,
  updated_ms INTEGER NOT NULL
);
";

const ADDED_COLUMNS: &[(&str, &str, &str)] = &[
    ("lines", "ticket_override", "TEXT"),
    ("lines", "pinned", "INTEGER NOT NULL DEFAULT 0"),
    ("lines", "held_by_user", "INTEGER NOT NULL DEFAULT 0"),
    ("lines", "closed_at", "TEXT"),
    ("lines", "owner_override", "TEXT"),
    ("lines", "origin", "TEXT"),
    ("lines", "env_ref", "TEXT"),
    ("lines", "session_now", "TEXT"),
    ("lines", "title_override", "TEXT"),
    ("lines", "workspace_id", "TEXT"),
    ("tasks", "rank", "REAL"),
    ("tasks", "origin", "TEXT"),
    ("tasks", "manual_status", "TEXT"),
    ("tasks", "session_state", "TEXT"),
    ("tasks", "badge", "TEXT"),
    ("tasks", "profile", "TEXT"),
    ("tasks", "title_source", "TEXT"),
    ("tasks", "urgent", "INTEGER"),
    ("tasks", "important", "INTEGER"),
    ("tasks", "parent_id", "TEXT"),
    ("tasks", "depends_on", "TEXT"),
    ("tasks", "inherit_branch", "INTEGER"),
];

impl Registry {
    pub fn open(state_dir: &Path) -> Result<Self, String> {
        let mut tries = 0;
        loop {
            match Self::open_once(state_dir) {
                Err(e) if e.contains("database is locked") && tries < 40 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(25 * tries.min(8)));
                }
                r => return r,
            }
        }
    }

    fn open_once(state_dir: &Path) -> Result<Self, String> {
        let path = state_dir.join("overseer.db");
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        for table in ["lines", "tasks"] {
            let have: Vec<String> = conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
                .map_err(|e| e.to_string())?;
            for (_, col, ty) in ADDED_COLUMNS.iter().filter(|c| c.0 == table) {
                if !have.iter().any(|h| h == col) {
                    match conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {col} {ty}"))
                    {
                        Err(e) if !e.to_string().contains("duplicate column name") => {
                            return Err(e.to_string())
                        }
                        _ => {}
                    }
                }
            }
        }
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS tasks_session_line ON tasks (line_id) WHERE origin = 'session'",
        )
        .map_err(|e| e.to_string())?;
        Ok(Registry { conn })
    }

    pub fn marks(&self, agent: &str, session_id: &str) -> Result<Marks, String> {
        self.conn
            .query_row(
                "SELECT id, ticket_override, pinned, held_by_user, closed_at IS NOT NULL, launch_args,
                   owner_override, origin, env_ref, title_override, workspace_id
                 FROM lines WHERE agent = ?1 AND session_id = ?2",
                params![agent, session_id],
                |r| {
                    Ok(Marks {
                        id: r.get(0)?,
                        ticket_override: r.get(1)?,
                        pinned: r.get(2)?,
                        held_pid: r.get(3)?,
                        closed: r.get(4)?,
                        launch_args: r
                            .get::<_, Option<String>>(5)?
                            .and_then(|s| serde_json::from_str(&s).ok()),
                        owner: r.get(6)?,
                        origin: r.get(7)?,
                        env_ref: r.get(8)?,
                        title_override: r.get(9)?,
                        workspace_id: r.get(10)?,
                    })
                },
            )
            .optional()
            .map(Option::unwrap_or_default)
            .map_err(|e| e.to_string())
    }

    pub fn divergent(&self, agent: &str) -> Result<HashMap<String, String>, String> {
        let mut st = self
            .conn
            .prepare(
                "SELECT session_id, session_now FROM lines
                 WHERE agent = ?1 AND session_now IS NOT NULL AND session_now != session_id",
            )
            .map_err(|e| e.to_string())?;
        let rows = st
            .query_map(params![agent], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    pub fn upsert_line(
        &self,
        l: &Line,
        alive_args: Option<&Vec<String>>,
        fallback_args: Option<&Vec<String>>,
        now_ms: i64,
    ) -> Result<(String, Vec<String>), String> {
        let now = format_rfc3339(now_ms);
        let enc = |a: Option<&Vec<String>>| a.map(|a| serde_json::to_string(a).unwrap_or_default());
        self.conn
            .query_row(
                "INSERT INTO lines (id, agent, session_id, owner, station_id, repo, branch, worktree_path, cwd,
                   ticket_key, state, held_reason, last_activity_at, launch_args, first_seen_at, updated_at,
                   session_now)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, COALESCE(?14, ?16), ?15, ?15, ?17)
                 ON CONFLICT (agent, session_id) DO UPDATE SET
                   owner = excluded.owner, station_id = excluded.station_id, repo = excluded.repo,
                   branch = excluded.branch, worktree_path = excluded.worktree_path, cwd = excluded.cwd,
                   ticket_key = excluded.ticket_key, state = excluded.state, held_reason = excluded.held_reason,
                   last_activity_at = excluded.last_activity_at,
                   launch_args = COALESCE(?14, lines.launch_args, ?16),
                   session_now = COALESCE(?17, lines.session_now),
                   updated_at = excluded.updated_at
                 RETURNING id, launch_args",
                params![
                    format!("{}-{}", l.station_id, ulid(now_ms)),
                    l.agent,
                    l.registry_key(),
                    l.owner,
                    l.station_id,
                    l.repo,
                    l.branch,
                    l.worktree_path,
                    l.cwd,
                    l.ticket,
                    l.state,
                    l.held_reason,
                    format_rfc3339(l.last_activity_ms),
                    enc(alive_args),
                    now,
                    enc(fallback_args),
                    alive_args.map(|_| l.session_id.as_str()),
                ],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .map(|(id, a)| (id, a.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()))
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT value FROM kv WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn put(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn set_mark(&self, id: &str, col: &str, value: Option<&str>) -> Result<(), String> {
        let sql = match col {
            "ticket_override" => "UPDATE lines SET ticket_override = ?2 WHERE id = ?1",
            "pinned" => "UPDATE lines SET pinned = COALESCE(?2, 0) WHERE id = ?1",
            "held_by_user" => "UPDATE lines SET held_by_user = COALESCE(?2, 0) WHERE id = ?1",
            "closed_at" => "UPDATE lines SET closed_at = ?2 WHERE id = ?1",
            "env_ref" => "UPDATE lines SET env_ref = ?2 WHERE id = ?1",
            "title_override" => "UPDATE lines SET title_override = ?2 WHERE id = ?1",
            "workspace_id" => "UPDATE lines SET workspace_id = ?2 WHERE id = ?1",
            "state" => "UPDATE lines SET state = ?2 WHERE id = ?1",
            _ => return Err(format!("unknown mark {col}")),
        };
        self.conn
            .execute(sql, params![id, value])
            .map_err(|e| e.to_string())
            .and_then(|n| match n {
                0 => Err(format!("no line {id}")),
                _ => Ok(()),
            })
    }

    pub fn reserve(&self, l: &Line, origin: &str, now_ms: i64) -> Result<(), String> {
        let now = format_rfc3339(now_ms);
        self.conn
            .execute(
                "INSERT INTO lines (id, agent, session_id, owner, station_id, repo, branch, worktree_path, cwd,
                   ticket_key, state, last_activity_at, first_seen_at, updated_at, owner_override, origin, ticket_override)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, 'active', ?10, ?10, ?10, ?4, ?11, ?9)",
                params![
                    l.id,
                    l.agent,
                    l.registry_key(),
                    l.owner,
                    l.station_id,
                    l.repo,
                    l.branch,
                    l.worktree_path,
                    l.ticket,
                    now,
                    origin
                ],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn upsert_worktree(
        &self,
        path: &str,
        repo: &str,
        branch: Option<&str>,
        ticket: Option<&str>,
        volatile: bool,
        now_ms: i64,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO worktrees (path, repo, branch, ticket_key, volatile, seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT (path) DO UPDATE SET repo = excluded.repo, branch = excluded.branch,
                   ticket_key = excluded.ticket_key, volatile = excluded.volatile, seen_at = excluded.seen_at",
                params![path, repo, branch, ticket, volatile, format_rfc3339(now_ms)],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn hook_state(&self, agent: &str, session_id: &str) -> Result<Option<HookState>, String> {
        self.conn
            .query_row(
                "SELECT event, at_ms, busy, needs_input, subagents, tasks, pid, pane FROM hook_state
                 WHERE agent = ?1 AND session_id = ?2",
                params![agent, session_id],
                |r| {
                    Ok(HookState {
                        event: r.get(0)?,
                        at_ms: r.get(1)?,
                        busy: r.get(2)?,
                        needs_input: r.get(3)?,
                        subagents: r.get(4)?,
                        tasks: r.get(5)?,
                        pid: r.get(6)?,
                        pane: r.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn hook_sessions(&self, agent: &str) -> Result<Vec<(String, HookState)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT session_id FROM hook_state WHERE agent = ?1")
            .map_err(|e| e.to_string())?;
        let ids: Vec<String> = stmt
            .query_map([agent], |r| r.get(0))
            .and_then(|rows| rows.collect())
            .map_err(|e| e.to_string())?;
        ids.into_iter()
            .filter_map(|id| match self.hook_state(agent, &id) {
                Ok(Some(h)) => Some(Ok((id, h))),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }

    pub fn put_hook_state(
        &self,
        agent: &str,
        session_id: &str,
        h: &HookState,
    ) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO hook_state (agent, session_id, event, at_ms, busy, needs_input, subagents, tasks, pid, pane)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![agent, session_id, h.event, h.at_ms, h.busy, h.needs_input, h.subagents, h.tasks, h.pid, h.pane],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn spool_offset(&self, file: &str) -> Result<u64, String> {
        self.conn
            .query_row(
                "SELECT offset FROM spool_offsets WHERE file = ?1",
                [file],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|o| o.unwrap_or(0) as u64)
            .map_err(|e| e.to_string())
    }

    pub fn set_spool_offset(&self, file: &str, offset: Option<u64>) -> Result<(), String> {
        match offset {
            Some(o) => self.conn.execute(
                "INSERT OR REPLACE INTO spool_offsets (file, offset) VALUES (?1, ?2)",
                params![file, o as i64],
            ),
            None => self
                .conn
                .execute("DELETE FROM spool_offsets WHERE file = ?1", [file]),
        }
        .map(|_| ())
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase1_database_upgrades_in_place() {
        let dir = std::env::temp_dir().join(format!("sb-reg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = Connection::open(dir.join("overseer.db")).unwrap();
        old.execute_batch(
            "CREATE TABLE lines (id TEXT PRIMARY KEY, agent TEXT NOT NULL, session_id TEXT NOT NULL,
               owner TEXT NOT NULL, station_id TEXT NOT NULL, repo TEXT NOT NULL, branch TEXT NOT NULL,
               worktree_path TEXT NOT NULL, cwd TEXT NOT NULL, ticket_key TEXT, state TEXT NOT NULL,
               held_reason TEXT, last_activity_at TEXT NOT NULL, launch_args TEXT,
               first_seen_at TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE (agent, session_id));
             INSERT INTO lines VALUES ('st-1', 'claude', 's1', 'o', 'st', 'r', 'b', '/w', '/w', NULL,
               'idle', NULL, 't', '[\"--model\",\"opus\"]', 't', 't');",
        )
        .unwrap();
        drop(old);
        let reg = Registry::open(&dir).unwrap();
        let m = reg.marks("claude", "s1").unwrap();
        assert_eq!(m.id.as_deref(), Some("st-1"));
        assert_eq!(m.launch_args.unwrap(), ["--model", "opus"]);
        assert!(!m.pinned && m.held_pid == 0 && !m.closed);
        reg.set_mark("st-1", "pinned", Some("1")).unwrap();
        drop(reg);
        assert!(
            Registry::open(&dir)
                .unwrap()
                .marks("claude", "s1")
                .unwrap()
                .pinned
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tasks_gain_dependencies_on_an_existing_database() {
        let dir = std::env::temp_dir().join(format!("sb-reg-deps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = Connection::open(dir.join("overseer.db")).unwrap();
        old.execute_batch(
            "CREATE TABLE tasks (id TEXT PRIMARY KEY, number INTEGER NOT NULL, title TEXT NOT NULL,
               description TEXT NOT NULL, project TEXT NOT NULL, status TEXT NOT NULL, agent TEXT NOT NULL,
               model TEXT NOT NULL, effort TEXT NOT NULL, ticket_key TEXT, line_id TEXT, term_id TEXT,
               branch TEXT, worktree_path TEXT, progress TEXT, error TEXT, started_ms INTEGER,
               created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL, UNIQUE (project, number));
             INSERT INTO tasks (id, number, title, description, project, status, agent, model, effort, created_ms, updated_ms)
               VALUES ('t1', 1, 'old', '', '/p', 'auto', 'claude', '', 'high', 5, 5);",
        )
        .unwrap();
        drop(old);
        let reg = Registry::open(&dir).unwrap();
        let t = crate::tasks::get(&reg, "t1").unwrap();
        assert!(t.depends_on.is_empty() && t.inherit_branch.is_none());
        assert_eq!(t.to_json("st")["dependsOn"], serde_json::json!([]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn title_override_mark_survives_reopen_and_clears() {
        // Given a database created before the title_override column existed
        // When a custom title is set, the registry reopened, then the title cleared
        // Then the title persists across the reopen and reads back as none once cleared
        let dir = std::env::temp_dir().join(format!("sb-reg-title-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = Connection::open(dir.join("overseer.db")).unwrap();
        old.execute_batch(
            "CREATE TABLE lines (id TEXT PRIMARY KEY, agent TEXT NOT NULL, session_id TEXT NOT NULL,
               owner TEXT NOT NULL, station_id TEXT NOT NULL, repo TEXT NOT NULL, branch TEXT NOT NULL,
               worktree_path TEXT NOT NULL, cwd TEXT NOT NULL, ticket_key TEXT, state TEXT NOT NULL,
               held_reason TEXT, last_activity_at TEXT NOT NULL, launch_args TEXT,
               first_seen_at TEXT NOT NULL, updated_at TEXT NOT NULL, UNIQUE (agent, session_id));
             INSERT INTO lines VALUES ('st-1', 'claude', 's1', 'o', 'st', 'r', 'b', '/w', '/w', NULL,
               'idle', NULL, 't', NULL, 't', 't');",
        )
        .unwrap();
        drop(old);
        let reg = Registry::open(&dir).unwrap();
        assert_eq!(reg.marks("claude", "s1").unwrap().title_override, None);
        reg.set_mark("st-1", "title_override", Some("Payments refactor"))
            .unwrap();
        drop(reg);
        let reg = Registry::open(&dir).unwrap();
        assert_eq!(
            reg.marks("claude", "s1").unwrap().title_override.as_deref(),
            Some("Payments refactor")
        );
        reg.set_mark("st-1", "title_override", None).unwrap();
        assert_eq!(reg.marks("claude", "s1").unwrap().title_override, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_line_workspace_assignment_persists_until_cleared() {
        // Given a registry holding one line
        // When the line is assigned to a workspace, the registry reopened, then the mark cleared
        // Then the assignment survives the reopen and the clear removes it
        let dir = std::env::temp_dir().join(format!("sb-reg-wsid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let reg = Registry::open(&dir).unwrap();
        let l = Line {
            id: "st-9".into(),
            agent: "claude",
            session_id: "s9".into(),
            ..Default::default()
        };
        reg.reserve(&l, "dev", 0).unwrap();
        assert_eq!(reg.marks("claude", "s9").unwrap().workspace_id, None);
        reg.set_mark("st-9", "workspace_id", Some("w1")).unwrap();
        drop(reg);
        let reg = Registry::open(&dir).unwrap();
        assert_eq!(
            reg.marks("claude", "s9").unwrap().workspace_id.as_deref(),
            Some("w1")
        );
        reg.set_mark("st-9", "workspace_id", None).unwrap();
        assert_eq!(reg.marks("claude", "s9").unwrap().workspace_id, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_opens_create_the_workspace_tables_once() {
        let dir = std::env::temp_dir().join(format!("sb-reg-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let opens: Vec<_> = (0..8)
            .map(|_| {
                let d = dir.clone();
                std::thread::spawn(move || Registry::open(&d).map(|_| ()))
            })
            .collect();
        for o in opens {
            o.join().unwrap().unwrap();
        }
        let reg = Registry::open(&dir).unwrap();
        reg.conn
            .execute(
                "INSERT INTO groups (id, name, rank, created_ms, updated_ms) VALUES ('g', 'n', 1, 0, 0)",
                [],
            )
            .unwrap();
        let n: i64 = reg
            .conn
            .query_row(
                "SELECT COUNT(*) FROM groups g JOIN workspaces w ON w.id = g.id
                 UNION ALL SELECT COUNT(*) FROM folders
                 UNION ALL SELECT COUNT(*) FROM workspace_tombstones",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
