use crate::git::Git;
use crate::registry::Registry;
use crate::Ctx;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

type Res<T> = Result<T, (u16, String)>;

const DEPTH: usize = 3;
const SKIP: &[&str] = &["node_modules", "target", ".venv"];

static RUNNING: AtomicBool = AtomicBool::new(false);

pub fn walk(roots: &[PathBuf], depth: usize, skip: &[PathBuf]) -> Vec<PathBuf> {
    let skip: Vec<PathBuf> = skip
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();
    let mut out = vec![];
    for r in roots {
        if let Ok(r) = r.canonicalize() {
            visit(&r, depth, &skip, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn visit(dir: &Path, depth: usize, skip: &[PathBuf], out: &mut Vec<PathBuf>) {
    if skip.iter().any(|s| s == dir) {
        return;
    }
    if dir.join(".git").exists() {
        out.push(dir.to_path_buf());
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        if !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIP.contains(&&*name) {
            continue;
        }
        visit(&e.path(), depth - 1, skip, out);
    }
}

pub fn run(ctx: &Ctx) -> Res<Value> {
    let mut skip = vec![ctx.config.worktree_dir.clone()];
    skip.extend(crate::storage::legacy_worktree_dir());
    let found = walk(&ctx.config.discover_roots, DEPTH, &skip);
    let reg = Registry::open(&ctx.state_dir).map_err(|e| (500, e))?;
    let sql = |e: rusqlite::Error| -> (u16, String) { (500, e.to_string()) };
    let known: HashSet<String> = {
        let mut st = reg.conn.prepare("SELECT path FROM projects").map_err(sql)?;
        let rows = st.query_map([], |r| r.get(0)).map_err(sql)?;
        rows.collect::<rusqlite::Result<_>>().map_err(sql)?
    };
    let mut g = Git::default();
    let mut added = 0;
    for p in found {
        let p = p.to_string_lossy().into_owned();
        if known.contains(&p) {
            continue;
        }
        let Some((root, _)) = crate::tasks::main_root(&mut g, &p) else {
            continue;
        };
        added += reg
            .conn
            .execute(
                "INSERT OR IGNORE INTO projects (path, added_ms) VALUES (?1, ?2)",
                params![root, crate::procs::now_ms()],
            )
            .map_err(sql)?;
    }
    drop(reg);
    crate::workspaces::list(ctx)?;
    let lines = crate::daemon::cached_lines(ctx, None).map_err(|e| (500, e))?;
    let projects = crate::tasks::projects(ctx, &lines)?;
    Ok(json!({"added": added, "projects": projects}))
}

pub fn background() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let probe = crate::procs::System;
        let ctx = Ctx::from_env(&probe);
        if !ctx.config.discover_roots.is_empty() {
            if let Err((_, e)) = run(&ctx) {
                eprintln!("overseer daemon: discovery failed: {e}");
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_to_repos() {
        let base = std::env::temp_dir().join(format!(
            "sb-discover-{}-{}",
            std::process::id(),
            crate::procs::now_ms()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        let dir = |p: &str| std::fs::create_dir_all(root.join(p)).unwrap();
        dir("a/.git");
        dir("b");
        std::fs::write(root.join("b/.git"), "gitdir: /elsewhere\n").unwrap();
        dir("c/node_modules/x/.git");
        dir("d/e/f/g/.git");
        dir("d/e/f/.git");
        dir("a/sub/.git");
        dir(".hidden/r/.git");
        dir("target/t/.git");
        dir("s/r/.git");
        dir("plain/nothing");
        let found = walk(std::slice::from_ref(&root), 3, &[root.join("s")]);
        let root = root.canonicalize().unwrap();
        let want: Vec<PathBuf> = ["a", "b", "d/e/f"].iter().map(|p| root.join(p)).collect();
        assert_eq!(found, want);
        assert_eq!(walk(&[root.join("a")], 3, &[]), vec![root.join("a")]);
        assert!(walk(&[base.join("missing")], 3, &[]).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
