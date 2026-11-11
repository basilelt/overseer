use crate::install::{self, Format, Target};
use crate::registry::Registry;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const KEY: &str = "import.switchboard";
const OLD_HOOK: &str = install::OLD_MARKER;
const OLD_STATUSLINE: &str = "#switchboard-statusline";
const OLD_UNIT: &str = "switchboard-daemon.service";
const OLD_LABEL: &str = "dev.switchboard.daemon";
const OLD_APP: &str = "fr.armtek.switchboard";
const OLD_WORKTREES: &str = "~/.switchboard/worktrees";
const SKIP_TABLES: &[&str] = &["spool_offsets", "sqlite_sequence"];
const FIX_REGISTRY: &str = "
UPDATE hook_state SET pid = NULL, pane = NULL
  WHERE (agent, session_id) IN (SELECT agent, session_id FROM old.hook_state);
UPDATE lines SET held_by_user = 0 WHERE id IN (SELECT id FROM old.lines);
";
const COPY_FILES: &[&str] = &[
    "profiles.json",
    "board.json",
    "user.json",
    "sync.json",
    "projects.json",
];
const COPY_DIRS: &[&str] = &["stations", "feedback"];
const COPY_HOME_DIRS: &[&str] = &["u", "tasks", "reviews"];

fn locate(get: &dyn Fn(&str) -> Option<String>, windows: bool, macos: bool) -> PathBuf {
    if let Some(d) = get("SWITCHBOARD_STATE_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    if windows {
        if let Some(d) = get("LOCALAPPDATA").filter(|v| !v.is_empty()) {
            return PathBuf::from(d).join("switchboard");
        }
    }
    let home = PathBuf::from(get("HOME").unwrap_or_default());
    if macos {
        return home.join("Library/Application Support/switchboard");
    }
    get("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join("switchboard")
}

pub fn old_dir() -> PathBuf {
    locate(
        &|k| std::env::var(k).ok(),
        cfg!(windows),
        cfg!(target_os = "macos"),
    )
}

fn old_db(dir: &Path) -> PathBuf {
    dir.join("switchboard.db")
}

fn readonly(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .ok()
}

fn count(c: &Connection, sql: &str) -> i64 {
    c.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}

fn counts(dir: &Path) -> (i64, i64, i64) {
    match readonly(&old_db(dir)) {
        Some(c) => (
            count(&c, "SELECT COUNT(*) FROM lines WHERE closed_at IS NULL"),
            count(&c, "SELECT COUNT(*) FROM tasks"),
            count(&c, "SELECT COUNT(*) FROM workspaces"),
        ),
        None => (0, 0, 0),
    }
}

#[cfg(unix)]
fn alive(pid: i32) -> bool {
    if unsafe { libc::kill(pid, 0) } != 0 {
        return false;
    }
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => !s
            .rsplit(')')
            .next()
            .is_some_and(|r| r.trim_start().starts_with('Z')),
        // No /proc (macOS): a zombie shows as Z in ps.
        Err(_) => !std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim().starts_with('Z')),
    }
}

#[cfg(not(unix))]
fn alive(_: i32) -> bool {
    false
}

fn cmdline(pid: i32) -> String {
    if let Ok(b) = std::fs::read(format!("/proc/{pid}/cmdline")) {
        return String::from_utf8_lossy(&b).replace('\0', " ");
    }
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

fn old_pid(dir: &Path) -> Option<i32> {
    std::fs::read_to_string(dir.join("daemon.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()
        .filter(|p| *p > 1 && *p != std::process::id() as i32)
        .filter(|p| alive(*p) && cmdline(*p).contains("switchboard"))
}

fn marked(state_dir: &Path) -> bool {
    Registry::open(state_dir)
        .ok()
        .and_then(|r| {
            r.conn
                .query_row("SELECT 1 FROM kv WHERE key = ?1", [KEY], |_| Ok(()))
                .ok()
        })
        .is_some()
}

fn old_target_path(t: &Target) -> PathBuf {
    let name = t.path.file_name().unwrap_or_default().to_string_lossy();
    t.path
        .with_file_name(name.replace("overseer", "switchboard"))
}

fn has_marker(path: &Path, marker: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|s| s.contains(marker))
}

fn old_hooks(t: &Target) -> bool {
    match t.format {
        Format::File => has_marker(&old_target_path(t), OLD_HOOK),
        _ => {
            has_marker(&t.path, OLD_HOOK)
                || (t.agent == "claude" && has_marker(&t.path, OLD_STATUSLINE))
        }
    }
}

fn utf16(b: &[u8]) -> String {
    let u: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&u)
}

fn rename_key(k: &str) -> Option<String> {
    ["switchboard.", "switchboard:"].iter().find_map(|p| {
        k.strip_prefix(p)
            .map(|r| format!("overseer{}{r}", &p[11..]))
    })
}

fn read_local_storage(path: &Path, out: &mut Map<String, Value>) {
    let Some(c) = readonly(path) else { return };
    let Ok(mut s) = c.prepare("SELECT key, value FROM ItemTable") else {
        return;
    };
    let Ok(rows) = s.query_map([], |r| {
        let v: rusqlite::types::Value = r.get(1)?;
        Ok((r.get::<_, String>(0)?, v))
    }) else {
        return;
    };
    for (k, v) in rows.flatten() {
        let text = match v {
            rusqlite::types::Value::Blob(b) => utf16(&b),
            rusqlite::types::Value::Text(t) => t,
            _ => continue,
        };
        if let Some(k) = rename_key(&k) {
            out.insert(k, Value::String(text));
        }
    }
}

fn find_local_storage(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if depth < 8 {
                find_local_storage(&p, depth + 1, found);
            }
        } else if ft.is_file() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.ends_with(".localstorage") || n == "localstorage.sqlite3" {
                found.push(p);
            }
        }
    }
}

fn webview_root(get: &dyn Fn(&str) -> Option<String>, macos: bool) -> Option<PathBuf> {
    let home = PathBuf::from(get("HOME").filter(|v| !v.is_empty())?);
    if macos {
        return Some(home.join("Library/WebKit").join(OLD_APP));
    }
    Some(
        get("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join(OLD_APP),
    )
}

fn settings() -> Value {
    if cfg!(windows) {
        return json!({});
    }
    let get = |k: &str| std::env::var(k).ok();
    let Some(root) = webview_root(&get, cfg!(target_os = "macos")) else {
        return json!({});
    };
    let mut files = vec![];
    find_local_storage(&root, 0, &mut files);
    files.sort();
    let mut out = Map::new();
    for f in files {
        read_local_storage(&f, &mut out);
    }
    Value::Object(out)
}

fn status(state_dir: &Path) -> Value {
    let dir = old_dir();
    let found = old_db(&dir).is_file();
    let (sessions, tasks, workspaces) = if found { counts(&dir) } else { (0, 0, 0) };
    json!({
        "found": found,
        "dir": found.then(|| dir.to_string_lossy().into_owned()),
        "imported": marked(state_dir),
        "running": found && old_pid(&dir).is_some(),
        "sessions": sessions,
        "tasks": tasks,
        "workspaces": workspaces,
        "hooks": install::targets().iter().any(old_hooks),
        "settings": settings(),
    })
}

fn disable_autostart(run: crate::autostart::Run, state_dir: &Path) {
    let env = crate::autostart::Env::current(state_dir);
    match env.kind {
        crate::autostart::Kind::Systemd => {
            let unit = env.config_home.join("systemd/user").join(OLD_UNIT);
            if unit.exists() {
                let _ = run("systemctl", &["--user", "disable", "--now", OLD_UNIT]);
                let _ = std::fs::remove_file(&unit);
                let _ = run("systemctl", &["--user", "daemon-reload"]);
            }
        }
        crate::autostart::Kind::Launchd => {
            let plist = env
                .home
                .join("Library/LaunchAgents")
                .join(format!("{OLD_LABEL}.plist"));
            if plist.exists() {
                let uid = unsafe_uid();
                let _ = run("launchctl", &["bootout", &format!("gui/{uid}/{OLD_LABEL}")]);
                let _ = std::fs::remove_file(&plist);
            }
        }
        crate::autostart::Kind::Unsupported => {}
    }
}

#[cfg(unix)]
fn unsafe_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn unsafe_uid() -> u32 {
    0
}

#[cfg(unix)]
fn wait_dead(pid: i32, within: Duration) -> bool {
    let end = std::time::Instant::now() + within;
    while std::time::Instant::now() < end {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    !alive(pid)
}

#[cfg(unix)]
fn stop_old(dir: &Path, state_dir: &Path) -> Result<bool, String> {
    disable_autostart(&crate::autostart::run_cmd, state_dir);
    let Some(pid) = old_pid(dir) else {
        return Ok(false);
    };
    unsafe { libc::kill(pid, libc::SIGTERM) };
    if !wait_dead(pid, Duration::from_secs(3)) {
        unsafe { libc::kill(pid, libc::SIGKILL) };
        if !wait_dead(pid, Duration::from_secs(2)) {
            return Err(format!("the switchboard daemon (pid {pid}) did not stop"));
        }
    }
    Ok(true)
}

#[cfg(not(unix))]
fn stop_old(_: &Path, state_dir: &Path) -> Result<bool, String> {
    disable_autostart(&crate::autostart::run_cmd, state_dir);
    Ok(false)
}

fn quote(id: &str) -> String {
    format!("\"{}\"", id.replace('"', "\"\""))
}

fn tables(c: &Connection, schema: &str) -> Result<Vec<String>, String> {
    c.prepare(&format!(
        "SELECT name FROM {schema}.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
    ))
    .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
    .map_err(|e| e.to_string())
}

fn merge_db(dest: &Path, old: &Path, fix: &str) -> Result<(), String> {
    let mut attempt = 0;
    loop {
        match merge_once(dest, old, fix) {
            Err(e) if e.contains("database is locked") && attempt < 5 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(100 * attempt));
            }
            r => return r,
        }
    }
}

fn merge_once(dest: &Path, old: &Path, fix: &str) -> Result<(), String> {
    let mut c = Connection::open(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    c.busy_timeout(Duration::from_secs(10))
        .map_err(|e| e.to_string())?;
    c.execute("ATTACH DATABASE ?1 AS old", [old.to_string_lossy()])
        .map_err(|e| format!("{}: {e}", old.display()))?;
    let mine = tables(&c, "main")?;
    let theirs = tables(&c, "old")?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    for t in theirs {
        if !mine.contains(&t) || t.starts_with("sync_") || SKIP_TABLES.contains(&t.as_str()) {
            continue;
        }
        let cols: Vec<String> = tx
            .prepare(
                "SELECT m.name FROM pragma_table_info(?1, 'main') m
                 JOIN pragma_table_info(?1, 'old') o ON m.name = o.name",
            )
            .and_then(|mut s| s.query_map([&t], |r| r.get(0))?.collect())
            .map_err(|e| e.to_string())?;
        if cols.is_empty() {
            continue;
        }
        let list = cols.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
        let sql = if t == "task_numbers" {
            "INSERT INTO main.task_numbers (project, last) SELECT project, last FROM old.task_numbers WHERE true
                 ON CONFLICT (project) DO UPDATE SET last = MAX(last, excluded.last)".to_string()
        } else {
            let verb = if t == "kv" { "IGNORE" } else { "REPLACE" };
            format!(
                "INSERT OR {verb} INTO main.{q} ({list}) SELECT {list} FROM old.{q}",
                q = quote(&t)
            )
        };
        tx.execute_batch(&sql).map_err(|e| format!("{t}: {e}"))?;
    }
    if !fix.is_empty() {
        tx.execute_batch(fix).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

fn clone_db(old: &Path, dest: &Path) -> Result<(), String> {
    let c = Connection::open(old).map_err(|e| format!("{}: {e}", old.display()))?;
    c.execute("VACUUM INTO ?1", [dest.to_string_lossy()])
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", dest.display()))
}

fn merge_or_clone(state_dir: &Path, old_dir: &Path, name: &str, fix: &str) -> Result<(), String> {
    let (old, dest) = (old_dir.join(name), state_dir.join(name));
    if !old.is_file() {
        return Ok(());
    }
    if dest.exists() && name != "sync.db" {
        merge_db(&dest, &old, fix)
    } else if dest.exists() {
        Ok(())
    } else {
        clone_db(&old, &dest)
    }
}

fn merge_config(ours: &Value, old: &Value, worktrees_exist: bool) -> Value {
    let mut out = ours.as_object().cloned().unwrap_or_default();
    if let Some(o) = old.as_object() {
        for (k, v) in o {
            if k != "remoteCommand" && !out.contains_key(k) {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    if worktrees_exist && !out.contains_key("worktreeDir") {
        out.insert("worktreeDir".into(), json!(OLD_WORKTREES));
    }
    Value::Object(out)
}

fn read_json(p: &Path) -> Value {
    std::fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(Value::Null)
}

fn import_config(state_dir: &Path, old_dir: &Path) -> Result<(), String> {
    let old_path = old_dir.join("config.json");
    let old = read_json(&old_path);
    if old.is_null() && !old_path.exists() {
        return Ok(());
    }
    let ours_path = state_dir.join("config.json");
    let ours = read_json(&ours_path);
    let wt = crate::home().join(".switchboard/worktrees").is_dir();
    let merged = merge_config(&ours, &old, wt);
    if merged == ours {
        return Ok(());
    }
    install::atomic_write(&ours_path, &install::pretty(&merged))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&ours_path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn copy_missing(src: &Path, dst: &Path) -> Result<(), String> {
    let Ok(m) = std::fs::symlink_metadata(src) else {
        return Ok(());
    };
    if m.is_dir() {
        std::fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        let rd = std::fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?;
        for e in rd.flatten() {
            copy_missing(&e.path(), &dst.join(e.file_name()))?;
        }
    } else if m.is_file() && !dst.exists() {
        std::fs::copy(src, dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    }
    Ok(())
}

fn strip_target(t: &Target, stamp: &str) -> Result<bool, String> {
    if t.format == Format::File {
        let old = old_target_path(t);
        if !has_marker(&old, OLD_HOOK) {
            return Ok(false);
        }
        std::fs::remove_file(&old).map_err(|e| format!("{}: {e}", old.display()))?;
        return Ok(true);
    }
    if !old_hooks(t) {
        return Ok(false);
    }
    let text =
        std::fs::read_to_string(&t.path).map_err(|e| format!("{}: {e}", t.path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| {
        format!(
            "{} is not valid JSON ({e}); not touching it",
            t.path.display()
        )
    })?;
    if !v.is_object() {
        return Err(format!(
            "{} is not a JSON object; not touching it",
            t.path.display()
        ));
    }
    let mut after = install::unmerge_marker(&v, OLD_HOOK);
    if let Some(orig) = v["statusLine"]["command"]
        .as_str()
        .and_then(|c| crate::statusline::original_marker(c, OLD_STATUSLINE))
    {
        match orig {
            Some(o) => after["statusLine"]["command"] = json!(o),
            None => {
                after.as_object_mut().map(|m| m.remove("statusLine"));
            }
        }
    }
    if after == v {
        return Ok(false);
    }
    let out = install::pretty(&after);
    if serde_json::from_str::<Value>(&out).ok().as_ref() != Some(&after) {
        return Err("rewritten settings do not round-trip; not writing".into());
    }
    let bak = PathBuf::from(format!("{}.bak-{stamp}", t.path.display()));
    std::fs::copy(&t.path, &bak).map_err(|e| format!("backup {}: {e}", bak.display()))?;
    install::atomic_write(&t.path, &out)?;
    Ok(true)
}

fn import_hooks(state_dir: &Path) -> bool {
    let stamp = install::stamp(crate::procs::now_ms());
    let targets = install::targets();
    let mut agents: Vec<String> = vec![];
    let mut wrapped = false;
    for t in &targets {
        wrapped |= t.agent == "claude" && has_marker(&t.path, OLD_STATUSLINE);
        match strip_target(t, &stamp) {
            Ok(true) => agents.push(t.agent.to_string()),
            Ok(false) => {}
            Err(e) => crate::dlog::line("warn", &format!("overseer import: {}: {e}", t.agent)),
        }
    }
    if agents.is_empty() {
        return false;
    }
    let bin = install::hook_bin();
    let r = install::Run {
        mode: install::Mode::Install,
        apply: true,
        bin: &bin,
        state_dir,
        stamp: &stamp,
    };
    for o in install::run(&targets, Some(&agents), &r, |_| true) {
        if let Some(e) = o.error {
            crate::dlog::line(
                "warn",
                &format!("overseer import: install {}: {e}", o.agent),
            );
        }
    }
    if wrapped {
        if let Err(e) = crate::statusline::apply(
            &crate::statusline::settings_path(),
            true,
            false,
            &bin,
            state_dir,
            crate::procs::now_ms(),
        ) {
            crate::dlog::line("warn", &format!("overseer import: statusline: {e}"));
        }
    }
    true
}

fn run_import(state_dir: &Path, dir: &Path) -> Result<Value, String> {
    let (sessions, tasks, workspaces) = counts(dir);
    let stopped = stop_old(dir, state_dir)?;
    Registry::open(state_dir)?;
    merge_db(&state_dir.join("overseer.db"), &old_db(dir), FIX_REGISTRY)
        .map_err(|e| format!("merging the database: {e}"))?;
    merge_or_clone(state_dir, dir, "usage.db", "").map_err(|e| format!("merging usage: {e}"))?;
    import_config(state_dir, dir)?;
    for f in COPY_FILES.iter().chain(COPY_DIRS) {
        copy_missing(&dir.join(f), &state_dir.join(f))?;
    }
    merge_or_clone(state_dir, dir, "sync.db", "")?;
    let home = crate::home();
    for d in COPY_HOME_DIRS {
        copy_missing(
            &home.join(".switchboard").join(d),
            &home.join(".overseer").join(d),
        )?;
    }
    let hooks = import_hooks(state_dir);
    Registry::open(state_dir)?
        .conn
        .execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            rusqlite::params![KEY, crate::procs::now_ms().to_string()],
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "imported": true,
        "sessions": sessions,
        "tasks": tasks,
        "workspaces": workspaces,
        "stopped": stopped,
        "hooks": hooks,
    }))
}

pub fn routes(
    method: &str,
    segs: &[&str],
    _body: &Value,
    state_dir: &Path,
) -> Option<(u16, Value)> {
    match (method, segs) {
        ("GET", ["v1", "import", "switchboard"]) => Some((200, status(state_dir))),
        ("POST", ["v1", "import", "switchboard"]) => {
            let dir = old_dir();
            if !old_db(&dir).is_file() {
                return Some((404, json!({"error": "no switchboard data found"})));
            }
            Some(match run_import(state_dir, &dir) {
                Ok(v) => (200, v),
                Err(e) => (500, json!({"error": e})),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ov-import-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|p| p.0 == k).map(|p| p.1.to_string())
    }

    #[test]
    fn locate_prefers_env_override() {
        let g = env(&[("SWITCHBOARD_STATE_DIR", "/x/sb"), ("HOME", "/h")]);
        assert_eq!(locate(&g, false, false), PathBuf::from("/x/sb"));
    }

    #[test]
    fn locate_uses_xdg_then_home() {
        let g = env(&[("XDG_STATE_HOME", "/xdg"), ("HOME", "/h")]);
        assert_eq!(locate(&g, false, false), PathBuf::from("/xdg/switchboard"));
        let g = env(&[("HOME", "/h")]);
        assert_eq!(
            locate(&g, false, false),
            PathBuf::from("/h/.local/state/switchboard")
        );
    }

    #[test]
    fn locate_handles_macos_and_windows() {
        let g = env(&[("HOME", "/Users/u"), ("LOCALAPPDATA", "C:/L")]);
        assert_eq!(
            locate(&g, false, true),
            PathBuf::from("/Users/u/Library/Application Support/switchboard")
        );
        assert_eq!(locate(&g, true, false), PathBuf::from("C:/L/switchboard"));
    }

    #[test]
    fn webview_root_follows_platform() {
        let g = env(&[("HOME", "/h")]);
        assert_eq!(
            webview_root(&g, false).unwrap(),
            PathBuf::from("/h/.local/share/fr.armtek.switchboard")
        );
        assert_eq!(
            webview_root(&g, true).unwrap(),
            PathBuf::from("/h/Library/WebKit/fr.armtek.switchboard")
        );
        let g = env(&[("HOME", "/h"), ("XDG_DATA_HOME", "/d")]);
        assert_eq!(
            webview_root(&g, false).unwrap(),
            PathBuf::from("/d/fr.armtek.switchboard")
        );
        assert!(webview_root(&env(&[]), false).is_none());
    }

    fn utf16_bytes(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn local_storage_decodes_utf16_and_renames_keys() {
        let d = tmp("ls");
        let f = d.join("localstorage.sqlite3");
        let c = Connection::open(&f).unwrap();
        c.execute_batch("CREATE TABLE ItemTable (key TEXT UNIQUE, value BLOB NOT NULL)")
            .unwrap();
        for (k, v) in [
            ("switchboard.theme", "dark é"),
            ("switchboard:tabs", "[1]"),
            ("other", "x"),
        ] {
            c.execute(
                "INSERT INTO ItemTable VALUES (?1, ?2)",
                rusqlite::params![k, utf16_bytes(v)],
            )
            .unwrap();
        }
        drop(c);
        let mut out = Map::new();
        read_local_storage(&f, &mut out);
        assert_eq!(
            Value::Object(out),
            json!({"overseer.theme": "dark é", "overseer:tabs": "[1]"})
        );
        let mut found = vec![];
        find_local_storage(&d, 0, &mut found);
        assert_eq!(found, vec![f]);
    }

    #[test]
    fn local_storage_missing_or_broken_is_empty() {
        let d = tmp("lsbad");
        let f = d.join("x.localstorage");
        std::fs::write(&f, "not sqlite").unwrap();
        let mut out = Map::new();
        read_local_storage(&f, &mut out);
        read_local_storage(&d.join("none"), &mut out);
        assert!(out.is_empty());
    }

    fn db(path: &Path) -> Connection {
        let r = Registry::open(path.parent().unwrap()).unwrap();
        drop(r);
        Connection::open(path).unwrap()
    }

    fn line(c: &Connection, id: &str, sid: &str, branch: &str) {
        c.execute(
            "INSERT OR REPLACE INTO lines (id, agent, session_id, owner, station_id, repo, branch, worktree_path, cwd, state, last_activity_at, first_seen_at, updated_at, held_by_user)
             VALUES (?1, 'claude', ?2, 'o', 's', 'r', ?3, '/w', '/w', 'idle', 't', 't', 't', 4242)",
            rusqlite::params![id, sid, branch],
        )
        .unwrap();
    }

    #[test]
    fn merge_keeps_ours_old_wins_and_clears_pids() {
        let ours_dir = tmp("mo");
        let old_dir = tmp("mn");
        let ours = db(&ours_dir.join("overseer.db"));
        let old = db(&old_dir.join("overseer.db"));
        line(&ours, "mine", "s1", "ours");
        line(&ours, "both", "s2", "ours");
        line(&old, "both", "s2", "old");
        line(&old, "theirs", "s3", "old");
        for c in [&ours, &old] {
            c.execute(
                "INSERT INTO task_numbers VALUES ('p', ?1)",
                [if std::ptr::eq(c, &ours) { 9 } else { 3 }],
            )
            .unwrap();
        }
        old.execute("INSERT INTO task_numbers VALUES ('q', 5)", [])
            .unwrap();
        old.execute(
            "INSERT INTO hook_state VALUES ('claude','s3','Stop',1,0,0,0,0,77,'%1')",
            [],
        )
        .unwrap();
        old.execute("INSERT INTO spool_offsets VALUES ('f', 5)", [])
            .unwrap();
        drop(old);
        drop(ours);
        merge_db(
            &ours_dir.join("overseer.db"),
            &old_dir.join("overseer.db"),
            FIX_REGISTRY,
        )
        .unwrap();
        let c = Connection::open(ours_dir.join("overseer.db")).unwrap();
        let branch = |id: &str| -> String {
            c.query_row("SELECT branch FROM lines WHERE id = ?1", [id], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(branch("mine"), "ours");
        assert_eq!(branch("both"), "old");
        assert_eq!(branch("theirs"), "old");
        let held: i64 = c
            .query_row(
                "SELECT held_by_user FROM lines WHERE id = 'theirs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let kept: i64 = c
            .query_row(
                "SELECT held_by_user FROM lines WHERE id = 'mine'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((held, kept), (0, 4242));
        let pid: Option<i64> = c
            .query_row(
                "SELECT pid FROM hook_state WHERE session_id = 's3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pid, None);
        let n = |p: &str| -> i64 {
            c.query_row(
                "SELECT last FROM task_numbers WHERE project = ?1",
                [p],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!((n("p"), n("q")), (9, 5));
        let spool: i64 = c
            .query_row("SELECT COUNT(*) FROM spool_offsets", [], |r| r.get(0))
            .unwrap();
        assert_eq!(spool, 0);
    }

    #[test]
    fn merge_survives_concurrent_writes_to_the_destination() {
        let ours_dir = tmp("mc");
        let old_dir = tmp("mcn");
        let dest = ours_dir.join("overseer.db");
        let ours = db(&dest);
        let old = db(&old_dir.join("overseer.db"));
        line(&old, "theirs", "s3", "old");
        drop(old);
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut n = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    n += 1;
                    line(&ours, &format!("live{}", n % 50), "sx", "live");
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
        };
        for _ in 0..200 {
            merge_db(&dest, &old_dir.join("overseer.db"), FIX_REGISTRY).unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
    }

    #[test]
    fn merge_keeps_our_kv_and_fills_missing_keys() {
        let ours_dir = tmp("ko");
        let old_dir = tmp("kn");
        let ours = db(&ours_dir.join("overseer.db"));
        let old = db(&old_dir.join("overseer.db"));
        ours.execute("INSERT INTO kv VALUES ('saved_terms', '[\"ours\"]')", [])
            .unwrap();
        ours.execute("INSERT INTO kv VALUES ('boot_id', 'ours')", [])
            .unwrap();
        old.execute("INSERT INTO kv VALUES ('saved_terms', '[\"old\"]')", [])
            .unwrap();
        old.execute("INSERT INTO kv VALUES ('boot_id', 'old')", [])
            .unwrap();
        old.execute("INSERT INTO kv VALUES ('autoOrganize', 'true')", [])
            .unwrap();
        drop(old);
        drop(ours);
        merge_db(
            &ours_dir.join("overseer.db"),
            &old_dir.join("overseer.db"),
            "",
        )
        .unwrap();
        let c = Connection::open(ours_dir.join("overseer.db")).unwrap();
        let get = |k: &str| -> String {
            c.query_row("SELECT value FROM kv WHERE key = ?1", [k], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(get("saved_terms"), "[\"ours\"]");
        assert_eq!(get("boot_id"), "ours");
        assert_eq!(get("autoOrganize"), "true");
    }

    #[test]
    fn merge_skips_sync_tables_and_tolerates_extra_old_tables() {
        let ours_dir = tmp("so");
        let old_dir = tmp("sn");
        drop(db(&ours_dir.join("overseer.db")));
        let old = db(&old_dir.join("overseer.db"));
        old.execute_batch("CREATE TABLE sync_meta (k TEXT, v INTEGER); INSERT INTO sync_meta VALUES ('a', 1); CREATE TABLE legacy (x)").unwrap();
        drop(old);
        let ours = Connection::open(ours_dir.join("overseer.db")).unwrap();
        ours.execute_batch("CREATE TABLE sync_meta (k TEXT, v INTEGER)")
            .unwrap();
        drop(ours);
        merge_db(
            &ours_dir.join("overseer.db"),
            &old_dir.join("overseer.db"),
            "",
        )
        .unwrap();
        let c = Connection::open(ours_dir.join("overseer.db")).unwrap();
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM sync_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn merge_failure_leaves_destination_untouched() {
        let d = tmp("bad");
        drop(db(&d.join("overseer.db")));
        std::fs::write(d.join("junk.db"), "garbage garbage garbage").unwrap();
        assert!(merge_db(&d.join("overseer.db"), &d.join("junk.db"), "").is_err());
    }

    #[test]
    fn clone_db_copies_when_missing() {
        let o = tmp("co");
        let n = tmp("cn");
        let c = Connection::open(o.join("usage.db")).unwrap();
        c.execute_batch(
            "CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT); INSERT INTO kv VALUES ('a','b')",
        )
        .unwrap();
        drop(c);
        merge_or_clone(&n, &o, "usage.db", "").unwrap();
        let c = Connection::open(n.join("usage.db")).unwrap();
        let v: String = c.query_row("SELECT v FROM kv", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "b");
    }

    #[test]
    fn config_old_fills_missing_and_ours_wins() {
        let ours = json!({"stationId": "mine", "roots": ["a"]});
        let old =
            json!({"stationId": "old", "remoteCommand": "switchboard", "idleAfterSeconds": 5});
        let m = merge_config(&ours, &old, true);
        assert_eq!(
            m,
            json!({"stationId": "mine", "roots": ["a"], "idleAfterSeconds": 5, "worktreeDir": OLD_WORKTREES})
        );
    }

    #[test]
    fn config_keeps_explicit_worktree_dir_and_skips_when_absent() {
        let m = merge_config(
            &json!({"worktreeDir": "/w"}),
            &json!({"worktreeDir": "/old"}),
            true,
        );
        assert_eq!(m["worktreeDir"], "/w");
        assert!(merge_config(&Value::Null, &json!({}), false)
            .get("worktreeDir")
            .is_none());
        assert_eq!(
            merge_config(&Value::Null, &json!({"worktreeDir": "/old"}), true)["worktreeDir"],
            "/old"
        );
    }

    #[test]
    fn copy_missing_never_overwrites() {
        let s = tmp("cps");
        let d = tmp("cpd");
        std::fs::create_dir_all(s.join("sub")).unwrap();
        std::fs::write(s.join("a"), "old").unwrap();
        std::fs::write(s.join("sub/b"), "old").unwrap();
        std::fs::write(d.join("a"), "mine").unwrap();
        copy_missing(&s, &d).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("a")).unwrap(), "mine");
        assert_eq!(std::fs::read_to_string(d.join("sub/b")).unwrap(), "old");
        copy_missing(&s.join("nope"), &d.join("nope")).unwrap();
    }

    fn claude_target(dir: &Path) -> Target {
        Target {
            agent: "claude",
            path: dir.join("settings.json"),
            events: &[],
            format: Format::Groups(5),
            home: dir.to_path_buf(),
        }
    }

    #[test]
    fn unmerge_old_marker_keeps_user_and_overseer_hooks() {
        let v = json!({"hooks": {"Stop": [
            {"hooks": [{"type": "command", "command": "x #switchboard-hook"}]},
            {"hooks": [{"type": "command", "command": "mine"}, {"type": "command", "command": "y #switchboard-hook"}]},
            {"hooks": [{"type": "command", "command": "z #overseer-hook"}]}
        ], "Start": [{"hooks": [{"type": "command", "command": "q #switchboard-hook"}]}]}});
        let after = install::unmerge_marker(&v, OLD_HOOK);
        assert_eq!(
            after,
            json!({"hooks": {"Stop": [
                {"hooks": [{"type": "command", "command": "mine"}]},
                {"hooks": [{"type": "command", "command": "z #overseer-hook"}]}
            ]}})
        );
    }

    #[test]
    fn strip_target_rewrites_hooks_and_unwraps_statusline_with_backup() {
        let d = tmp("strip");
        let t = claude_target(&d);
        let wrapped = "[ -x '/sb' ] && exec '/sb' statusline-tap --state '/s' -- 'echo hi' || exec sh -c 'echo hi' #switchboard-statusline";
        let v = json!({
            "model": "x",
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "a #switchboard-hook"}, {"type": "command", "command": "keep"}]}]},
            "statusLine": {"type": "command", "command": wrapped}
        });
        std::fs::write(&t.path, v.to_string()).unwrap();
        assert!(strip_target(&t, "202601010000").unwrap());
        let after = read_json(&t.path);
        assert_eq!(after["statusLine"]["command"], "echo hi");
        assert_eq!(after["model"], "x");
        assert_eq!(
            after["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(),
            1
        );
        assert!(d.join("settings.json.bak-202601010000").exists());
        assert!(!strip_target(&t, "202601010001").unwrap());
    }

    #[test]
    fn strip_target_removes_statusline_that_wrapped_nothing() {
        let d = tmp("strip2");
        let t = claude_target(&d);
        let cmd =
            "[ -x '/sb' ] && exec '/sb' statusline-tap --state '/s' || : #switchboard-statusline";
        std::fs::write(
            &t.path,
            json!({"hooks": {}, "statusLine": {"type": "command", "command": cmd}}).to_string(),
        )
        .unwrap();
        assert!(strip_target(&t, "s").unwrap());
        assert!(read_json(&t.path).get("statusLine").is_none());
    }

    #[test]
    fn strip_target_refuses_invalid_json_and_removes_old_owned_file() {
        let d = tmp("strip3");
        let t = claude_target(&d);
        std::fs::write(&t.path, "{ #switchboard-hook").unwrap();
        assert!(strip_target(&t, "s").is_err());
        assert_eq!(
            std::fs::read_to_string(&t.path).unwrap(),
            "{ #switchboard-hook"
        );
        let f = Target {
            agent: "opencode",
            path: d.join("overseer.js"),
            events: &[],
            format: Format::File,
            home: d.clone(),
        };
        std::fs::write(
            d.join("switchboard.js"),
            "const MARKER = \"#switchboard-hook\";",
        )
        .unwrap();
        assert!(old_hooks(&f));
        assert!(strip_target(&f, "s").unwrap());
        assert!(!d.join("switchboard.js").exists());
        assert!(!strip_target(&f, "s").unwrap());
    }

    #[test]
    fn counts_and_marker_read_from_dbs() {
        let o = tmp("cnt");
        let c = db(&o.join("overseer.db"));
        line(&c, "a", "s1", "b");
        line(&c, "b", "s2", "b");
        c.execute("UPDATE lines SET closed_at = 't' WHERE id = 'b'", [])
            .unwrap();
        drop(c);
        std::fs::rename(o.join("overseer.db"), old_db(&o)).unwrap();
        assert_eq!(counts(&o), (1, 0, 0));
        assert_eq!(counts(&tmp("nodb")), (0, 0, 0));
        let s = tmp("mark");
        assert!(!marked(&s));
        Registry::open(&s)
            .unwrap()
            .conn
            .execute("INSERT INTO kv VALUES (?1, '1')", [KEY])
            .unwrap();
        assert!(marked(&s));
    }

    #[cfg(unix)]
    #[test]
    fn old_pid_needs_a_live_switchboard_process() {
        let d = tmp("pid");
        assert!(old_pid(&d).is_none());
        std::fs::write(d.join("daemon.pid"), "999999").unwrap();
        assert!(old_pid(&d).is_none());
        std::fs::write(d.join("daemon.pid"), std::process::id().to_string()).unwrap();
        assert!(old_pid(&d).is_none());
    }
}
