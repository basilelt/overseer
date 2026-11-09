use crate::board::{decode, git_out, publish};
use crate::daemon::{q, Request};
use crate::git::git;
use crate::worktrees::{self, valid_sha, Res};
use crate::Ctx;
use serde_json::{json, Value};
use std::path::{Component, Path};

const MAX_LOG: &str = "50";

fn bad(m: impl Into<String>) -> (u16, String) {
    (400, m.into())
}

fn conflict(m: impl Into<String>) -> (u16, String) {
    (409, m.into())
}

#[derive(Debug, Default, PartialEq)]
pub struct Sub {
    pub commit: bool,
    pub modified: bool,
    pub untracked: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Entry {
    pub path: String,
    pub old: Option<String>,
    pub x: char,
    pub y: char,
    pub sub: Option<Sub>,
}

fn xy(f: &str) -> (char, char) {
    let mut c = f.chars().map(|c| if c == '.' { ' ' } else { c });
    (c.next().unwrap_or(' '), c.next().unwrap_or(' '))
}

fn sub(f: &str) -> Option<Sub> {
    let c: Vec<char> = f.chars().collect();
    (c.first() == Some(&'S') && c.len() == 4).then(|| Sub {
        commit: c[1] == 'C',
        modified: c[2] == 'M',
        untracked: c[3] == 'U',
    })
}

pub fn parse_porcelain(s: &str) -> Vec<Entry> {
    let mut out = vec![];
    let mut it = s.split('\0');
    while let Some(rec) = it.next() {
        let Some((kind, rest)) = rec.split_once(' ') else {
            continue;
        };
        let e = match kind {
            "?" | "!" => {
                let c = kind.chars().next().unwrap_or('?');
                Entry {
                    path: rest.to_string(),
                    x: c,
                    y: c,
                    ..Entry::default()
                }
            }
            "1" | "2" | "u" => {
                let n = match kind {
                    "1" => 8,
                    "2" => 9,
                    _ => 10,
                };
                let f: Vec<&str> = rest.splitn(n, ' ').collect();
                if f.len() < n {
                    continue;
                }
                let (x, y) = xy(f[0]);
                let old = if kind == "2" {
                    it.next().map(str::to_string)
                } else {
                    None
                };
                Entry {
                    path: f[n - 1].to_string(),
                    old,
                    x,
                    y,
                    sub: sub(f[1]),
                }
            }
            _ => continue,
        };
        out.push(e);
    }
    out
}

fn unmerged(e: &Entry) -> bool {
    matches!(
        (e.x, e.y),
        ('D', 'D') | ('A', 'U') | ('U', 'D') | ('U', 'A') | ('D', 'U') | ('A', 'A') | ('U', 'U')
    )
}

fn file(e: &Entry, status: char, old: Option<&str>) -> Value {
    let mut v = json!({"path": e.path, "status": status.to_string()});
    if let Some(o) = old {
        v["oldPath"] = json!(o);
    }
    if let Some(s) = &e.sub {
        v["submodule"] =
            json!({"commit": s.commit, "modified": s.modified, "untracked": s.untracked});
    }
    v
}

pub fn split(entries: &[Entry]) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let (mut staged, mut changes, mut conflicts) = (vec![], vec![], vec![]);
    for e in entries {
        if unmerged(e) {
            conflicts.push(file(e, 'U', None));
            continue;
        }
        match (e.x, e.y) {
            ('?', '?') => {
                changes.push(file(e, '?', None));
                continue;
            }
            ('!', '!') => continue,
            _ => {}
        }
        match e.x {
            'M' | 'A' | 'D' | 'T' => staged.push(file(e, e.x, None)),
            'R' => staged.push(file(e, 'R', e.old.as_deref())),
            'C' => staged.push(file(e, 'A', None)),
            _ => {}
        }
        match e.y {
            'M' | 'D' | 'T' | 'A' => changes.push(file(e, e.y, None)),
            'R' => changes.push(file(e, 'R', e.old.as_deref())),
            _ => {}
        }
    }
    (staged, changes, conflicts)
}

pub fn valid_path(p: &str) -> bool {
    !p.is_empty()
        && !p.contains('\0')
        && !p.starts_with('/')
        && !p.starts_with('\\')
        && !Path::new(p).is_absolute()
        && !Path::new(p)
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
}

#[derive(Debug, PartialEq)]
pub enum Files {
    All,
    Some(Vec<String>),
}

pub fn files(b: &Value) -> Res<Files> {
    match &b["files"] {
        Value::String(s) if s == "all" => Ok(Files::All),
        Value::Array(a) if !a.is_empty() => a
            .iter()
            .map(|v| match v.as_str() {
                Some(p) if valid_path(p) => Ok(p.to_string()),
                Some(p) => Err(bad(format!("invalid path {p}"))),
                None => Err(bad(format!("invalid path {v}"))),
            })
            .collect::<Res<Vec<_>>>()
            .map(Files::Some),
        _ => Err(bad("files required")),
    }
}

fn target(ctx: &Ctx, worktree: Option<String>, line: Option<String>) -> Res<String> {
    let t = match (worktree, line) {
        (Some(w), _) => crate::review::resolve(ctx, Some(&w), None, None)?,
        (None, Some(l)) => crate::review::resolve(ctx, None, Some(&l), None)?,
        _ => return Err(bad("worktree or lineId required")),
    };
    Ok(t.worktree)
}

fn target_q(ctx: &Ctx, r: &Request) -> Res<String> {
    let d = |k| q(r, k).map(decode).filter(|s| !s.is_empty());
    target(ctx, d("worktree"), d("line"))
}

fn target_b(ctx: &Ctx, b: &Value) -> Res<String> {
    let s = |k: &str| b[k].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    target(ctx, s("worktree"), s("lineId"))
}

fn run(wt: &str, args: &[&str]) -> Res<String> {
    git_out(wt, args).map_err(conflict)
}

fn entries(wt: &str) -> Res<Vec<Entry>> {
    let args = ["status", "--porcelain=v2", "-z", "--untracked-files=all"];
    let out = crate::git::run_in("git", wt, &args, crate::git::limit_for(&args))
        .map_err(|e| conflict(format!("git status: {e}")))?;
    if !out.status.success() {
        return Err(conflict(format!(
            "git status: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(parse_porcelain(&String::from_utf8_lossy(&out.stdout)))
}

fn log(wt: &str, range: &str) -> Vec<Value> {
    git(
        wt,
        &["log", "--format=%H%x1f%s", "-n", MAX_LOG, range, "--"],
    )
    .map(|s| {
        s.lines()
            .filter_map(|l| l.split_once('\x1f'))
            .map(|(sha, subject)| json!({"sha": sha, "subject": subject}))
            .collect()
    })
    .unwrap_or_default()
}

fn operation(wt: &str) -> Value {
    let Some(dir) = git(wt, &["rev-parse", "--absolute-git-dir"]) else {
        return Value::Null;
    };
    let d = Path::new(&dir);
    if d.join("rebase-merge").exists() || d.join("rebase-apply").exists() {
        json!("rebase")
    } else if d.join("MERGE_HEAD").exists() {
        json!("merge")
    } else if d.join("CHERRY_PICK_HEAD").exists() {
        json!("cherry-pick")
    } else if d.join("REVERT_HEAD").exists() {
        json!("revert")
    } else {
        Value::Null
    }
}

fn project_of(wt: &str) -> String {
    worktrees::project(wt)
        .map(|(root, _)| root)
        .unwrap_or_else(|_| wt.to_string())
}

fn branch(wt: &str) -> Option<String> {
    git(wt, &["symbolic-ref", "-q", "--short", "HEAD"]).filter(|b| !b.is_empty())
}

fn head(wt: &str) -> Option<String> {
    git(wt, &["rev-parse", "-q", "--verify", "HEAD"]).filter(|s| !s.is_empty())
}

fn upstream(wt: &str) -> Option<String> {
    git(
        wt,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .filter(|s| !s.is_empty())
}

fn resolves(wt: &str, rev: &str) -> Option<String> {
    git(
        wt,
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
    .filter(|s| !s.is_empty())
}

fn submodules(wt: &str) -> Vec<Value> {
    git(
        wt,
        &[
            "config",
            "--file",
            ".gitmodules",
            "--get-regexp",
            r"^submodule\..*\.path$",
        ],
    )
    .map(|s| {
        s.lines()
            .filter_map(|l| l.split_once(' ').map(|(_, p)| p.to_string()))
            .filter_map(|p| {
                let dir = Path::new(wt).join(&p);
                if !dir.join(".git").exists() {
                    return None;
                }
                let top = git(dir.to_str()?, &["rev-parse", "--show-toplevel"])?;
                Some(json!({"path": p, "worktree": top}))
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn status(wt: &str) -> Res<Value> {
    let list = entries(wt)?;
    let (staged, changes, conflicts) = split(&list);
    let project = project_of(wt);
    let base = worktrees::default_branch_cached(&project);
    let head = head(wt);
    let up = upstream(wt);
    let (mut ahead, mut behind) = (0u64, 0u64);
    if let (Some(u), Some(_)) = (&up, &head) {
        if let Some(s) = git(
            wt,
            &[
                "rev-list",
                "--left-right",
                "--count",
                &format!("HEAD...{u}"),
            ],
        ) {
            let mut n = s.split_whitespace().map(|x| x.parse().unwrap_or(0));
            ahead = n.next().unwrap_or(0);
            behind = n.next().unwrap_or(0);
        }
    }
    let (outgoing, incoming) = match (&head, &up) {
        (None, _) => (vec![], vec![]),
        (Some(_), Some(u)) => (
            log(wt, &format!("{u}..HEAD")),
            log(wt, &format!("HEAD..{u}")),
        ),
        (Some(_), None) => {
            let sp = worktrees::start_point(&project, &base);
            let out = if sp != "HEAD" && resolves(wt, &sp).is_some() {
                log(wt, &format!("{sp}..HEAD"))
            } else {
                vec![]
            };
            (out, vec![])
        }
    };
    let stashes = git(wt, &["stash", "list"])
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);
    let branches: Vec<String> = git(
        wt,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "--sort=refname",
            "refs/heads",
        ],
    )
    .map(|s| s.lines().map(str::to_string).collect())
    .unwrap_or_default();
    Ok(json!({
        "worktree": wt,
        "project": project,
        "branch": branch(wt),
        "head": head,
        "upstream": up,
        "base": base,
        "ahead": ahead,
        "behind": behind,
        "staged": staged,
        "changes": changes,
        "conflicts": conflicts,
        "outgoing": outgoing,
        "incoming": incoming,
        "stashes": stashes,
        "branches": branches,
        "submodules": submodules(wt),
        "operation": operation(wt),
    }))
}

fn reply(wt: &str, commit: Option<String>) -> Res<Value> {
    let s = status(wt)?;
    let project = s["project"].clone();
    publish("git.changed", json!({ "project": project }));
    publish("worktree.updated", json!({ "project": project }));
    let mut v = json!({ "status": s });
    if let Some(c) = commit {
        v["commit"] = json!(c);
    }
    Ok(v)
}

fn with_files<'a>(base: &[&'a str], f: &'a [String]) -> Vec<&'a str> {
    let mut args = base.to_vec();
    args.push("--");
    args.extend(f.iter().map(String::as_str));
    args
}

fn conflicted(wt: &str) -> String {
    git(wt, &["diff", "--name-only", "--diff-filter=U"])
        .map(|s| s.lines().collect::<Vec<_>>().join(", "))
        .unwrap_or_default()
}

fn clean_tree(wt: &str) -> Res<()> {
    let n = entries(wt)?.len();
    if n > 0 {
        return Err(conflict(format!(
            "{n} uncommitted change(s): commit or stash them first"
        )));
    }
    Ok(())
}

pub fn stage(wt: &str, f: &Files) -> Res<Value> {
    if let Files::Some(paths) = f {
        let list = entries(wt)?;
        let inner = list.iter().find(|e| {
            paths.iter().any(|p| p.trim_end_matches('/') == e.path)
                && e.sub.as_ref().is_some_and(|s| !s.commit)
                && e.x == ' '
        });
        if let Some(e) = inner {
            return Err(conflict(format!(
                "{} is a submodule with uncommitted changes inside: commit them in the submodule first",
                e.path
            )));
        }
    }
    match f {
        Files::All => run(wt, &["add", "-A"])?,
        Files::Some(f) => run(wt, &with_files(&["add", "-A"], f))?,
    };
    reply(wt, None)
}

pub fn unstage(wt: &str, f: &Files) -> Res<Value> {
    let all = vec![".".to_string()];
    let unborn = head(wt).is_none();
    match (f, unborn) {
        (Files::All, false) => run(wt, &["reset", "-q"])?,
        (Files::Some(f), false) => run(wt, &with_files(&["reset", "-q"], f))?,
        (Files::All, true) => run(
            wt,
            &with_files(&["rm", "--cached", "-r", "-q", "--ignore-unmatch"], &all),
        )?,
        (Files::Some(f), true) => run(
            wt,
            &with_files(&["rm", "--cached", "-r", "-q", "--ignore-unmatch"], f),
        )?,
    };
    reply(wt, None)
}

pub fn discard(wt: &str, f: &Files) -> Res<Value> {
    let unborn = head(wt).is_none();
    let list = match f {
        Files::All => {
            if unborn {
                run(wt, &["rm", "-r", "-f", "-q", "--ignore-unmatch", "--", "."])?;
            } else {
                run(wt, &["reset", "-q", "--hard"])?;
            }
            run(wt, &["clean", "-f", "-d", "-q"])?;
            return reply(wt, None);
        }
        Files::Some(f) => f,
    };
    let known = entries(wt)?;
    for p in list {
        let e = known.iter().find(|e| &e.path == p);
        if e.is_some_and(|e| e.x == '?' && e.y == '?') {
            run(wt, &["clean", "-f", "-q", "--", p])?;
            continue;
        }
        let mut paths = vec![p.clone()];
        if let Some(o) = e.and_then(|e| e.old.clone()) {
            paths.push(o);
        }
        for one in paths {
            let in_head = !unborn && git(wt, &["cat-file", "-e", &format!("HEAD:{one}")]).is_some();
            if in_head {
                run(
                    wt,
                    &[
                        "restore",
                        "--staged",
                        "--worktree",
                        "--source=HEAD",
                        "--",
                        &one,
                    ],
                )?;
            } else {
                run(wt, &["rm", "-f", "-q", "--ignore-unmatch", "--", &one])?;
            }
        }
    }
    reply(wt, None)
}

fn unpushed_submodules(e: (u16, String)) -> (u16, String) {
    if !e.1.contains("not be found on any remote") {
        return e;
    }
    let paths: Vec<&str> =
        e.1.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.contains(' ') && !l.ends_with(':'))
            .collect();
    conflict(format!(
        "{} has commits that are not pushed: push the submodule first",
        if paths.is_empty() {
            "a submodule".to_string()
        } else {
            paths.join(", ")
        }
    ))
}

fn push_now(wt: &str) -> Result<(), (u16, String)> {
    let b = branch(wt).ok_or_else(|| conflict("HEAD is detached"))?;
    if upstream(wt).is_some() {
        run(wt, &["push", "-q", "--recurse-submodules=check"]).map_err(unpushed_submodules)?;
        return Ok(());
    }
    let remotes = git(wt, &["remote"]).unwrap_or_default();
    if !remotes.lines().any(|r| r == "origin") {
        return Err(conflict("no remote configured"));
    }
    run(
        wt,
        &[
            "push",
            "-q",
            "--recurse-submodules=check",
            "-u",
            "origin",
            &b,
        ],
    )
    .map_err(unpushed_submodules)?;
    Ok(())
}

pub fn commit(wt: &str, b: &Value) -> Res<Value> {
    let msg = b["message"].as_str().unwrap_or_default().trim().to_string();
    let amend = b["amend"] == true;
    if msg.is_empty() && !amend {
        return Err(bad("commit message required"));
    }
    if b["all"] == true {
        run(wt, &["add", "-A"])?;
    }
    if !amend {
        let (staged, _, _) = split(&entries(wt)?);
        if staged.is_empty() {
            return Err(conflict("nothing to commit"));
        }
    }
    let mut args = vec!["commit", "-q"];
    if amend {
        args.push("--amend");
    }
    if msg.is_empty() {
        args.push("--no-edit");
    } else {
        args.extend(["-m", &msg]);
    }
    run(wt, &args)?;
    let sha = head(wt).unwrap_or_default();
    if b["push"] == true {
        if let Err((_, e)) = push_now(wt) {
            let _ = reply(wt, None);
            return Err(conflict(format!(
                "committed {} but the push failed: {e}",
                &sha[..sha.len().min(7)]
            )));
        }
    }
    reply(wt, Some(sha))
}

pub fn fetch(wt: &str) -> Res<Value> {
    if git(wt, &["remote"]).unwrap_or_default().trim().is_empty() {
        return Err(conflict("no remote configured"));
    }
    run(wt, &["fetch", "--all", "--prune", "--quiet"])?;
    reply(wt, None)
}

pub fn pull(wt: &str) -> Res<Value> {
    let b = branch(wt).ok_or_else(|| conflict("HEAD is detached"))?;
    if upstream(wt).is_none() {
        return Err(conflict(format!("{b} has no upstream branch")));
    }
    if let Err(e) = run(wt, &["pull", "--no-rebase", "--no-edit", "-q"]) {
        let files = conflicted(wt);
        if files.is_empty() {
            return Err(e);
        }
        let _ = git_out(wt, &["merge", "--abort"]);
        return Err(conflict(format!(
            "merge conflicts in {files}; the pull was aborted"
        )));
    }
    reply(wt, None)
}

pub fn push(wt: &str) -> Res<Value> {
    push_now(wt)?;
    reply(wt, None)
}

pub fn switch(wt: &str, b: &Value) -> Res<Value> {
    let name = b["branch"].as_str().unwrap_or_default();
    let ok = !name.is_empty()
        && !name.starts_with('-')
        && git(wt, &["check-ref-format", "--branch", name]).is_some();
    if !ok {
        return Err(bad(format!("invalid branch name {name}")));
    }
    if b["create"] == true {
        run(wt, &["switch", "-q", "-c", name])?;
    } else {
        run(wt, &["switch", "-q", name])?;
    }
    reply(wt, None)
}

pub fn merge(wt: &str, b: &Value) -> Res<Value> {
    let rev = b["rev"].as_str().unwrap_or_default().trim();
    if rev.is_empty() || rev.starts_with('-') || rev.contains('\0') {
        return Err(bad(format!("invalid revision {rev}")));
    }
    if resolves(wt, rev).is_none() {
        return Err((404, format!("unknown revision {rev}")));
    }
    clean_tree(wt)?;
    if let Err(e) = run(wt, &["merge", "--no-edit", rev]) {
        let files = conflicted(wt);
        let _ = git_out(wt, &["merge", "--abort"]);
        if files.is_empty() {
            return Err(e);
        }
        return Err(conflict(format!(
            "merge conflicts in {files}; the merge was aborted"
        )));
    }
    reply(wt, None)
}

pub fn stash(wt: &str, b: &Value) -> Res<Value> {
    if b["pop"] == true {
        if git(wt, &["stash", "list"])
            .unwrap_or_default()
            .trim()
            .is_empty()
        {
            return Err(conflict("no stash to pop"));
        }
        run(wt, &["stash", "pop", "-q"])?;
    } else {
        if entries(wt)?.is_empty() {
            return Err(conflict("nothing to stash"));
        }
        run(wt, &["stash", "push", "-u", "-q"])?;
    }
    reply(wt, None)
}

pub fn revert(wt: &str, b: &Value) -> Res<Value> {
    let sha = b["sha"].as_str().unwrap_or_default();
    if !valid_sha(sha) {
        return Err(bad("sha must be 4-64 hex digits"));
    }
    let full = resolves(wt, sha).ok_or_else(|| (404, format!("unknown commit {sha}")))?;
    clean_tree(wt)?;
    let parents = git(wt, &["rev-list", "--parents", "-n", "1", &full])
        .map(|s| s.split_whitespace().count().saturating_sub(1))
        .unwrap_or(1);
    let mut args = vec!["revert", "--no-edit"];
    if parents > 1 {
        args.extend(["-m", "1"]);
    }
    args.push(&full);
    if let Err(e) = run(wt, &args) {
        let files = conflicted(wt);
        let _ = git_out(wt, &["revert", "--abort"]);
        if files.is_empty() {
            return Err(e);
        }
        return Err(conflict(format!(
            "conflicts in {files}; the revert was aborted"
        )));
    }
    reply(wt, head(wt))
}

fn res(r: Res<Value>) -> (u16, Value) {
    match r {
        Ok(v) => (200, v),
        Err((c, e)) => (c, json!({ "error": e })),
    }
}

pub fn routes(ctx: &Ctx, r: &Request, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    let post = |f: &dyn Fn(&str) -> Res<Value>| res(target_b(ctx, body).and_then(|wt| f(&wt)));
    let with =
        |f: &dyn Fn(&str, &Files) -> Res<Value>| post(&|wt| files(body).and_then(|fs| f(wt, &fs)));
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "git", "status"]) => res(target_q(ctx, r).and_then(|wt| {
            crate::review::watch::touch(&wt);
            status(&wt)
        })),
        ("POST", ["v1", "git", "stage"]) => with(&stage),
        ("POST", ["v1", "git", "unstage"]) => with(&unstage),
        ("POST", ["v1", "git", "discard"]) => with(&discard),
        ("POST", ["v1", "git", "commit"]) => post(&|wt| commit(wt, body)),
        ("POST", ["v1", "git", "fetch"]) => post(&fetch),
        ("POST", ["v1", "git", "pull"]) => post(&pull),
        ("POST", ["v1", "git", "push"]) => post(&push),
        ("POST", ["v1", "git", "switch"]) => post(&|wt| switch(wt, body)),
        ("POST", ["v1", "git", "merge"]) => post(&|wt| merge(wt, body)),
        ("POST", ["v1", "git", "stash"]) => post(&|wt| stash(wt, body)),
        ("POST", ["v1", "git", "revert"]) => post(&|wt| revert(wt, body)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "N... 100644 100644 100644 aaaa bbbb";
    const SUB: &str = "160000 160000 160000 aaaa bbbb";

    #[test]
    fn parses_porcelain_v2_with_renames_untracked_and_unmerged() {
        let s = format!(
            "1 M. {H} a.rs\01 .M {H} b.rs\02 R. {H} R100 new.rs\0old.rs\0? n.txt\0u UU N... 100644 100644 100644 100644 a b c c.rs\01 AM {H} d.rs\0! t\0"
        );
        let e = parse_porcelain(&s);
        assert_eq!(e.len(), 7);
        assert_eq!(e[2].path, "new.rs");
        assert_eq!(e[2].old.as_deref(), Some("old.rs"));
        let (staged, changes, conflicts) = split(&e);
        assert_eq!(
            staged,
            vec![
                json!({"path": "a.rs", "status": "M"}),
                json!({"path": "new.rs", "status": "R", "oldPath": "old.rs"}),
                json!({"path": "d.rs", "status": "A"}),
            ]
        );
        assert_eq!(
            changes,
            vec![
                json!({"path": "b.rs", "status": "M"}),
                json!({"path": "n.txt", "status": "?"}),
                json!({"path": "d.rs", "status": "M"}),
            ]
        );
        assert_eq!(conflicts, vec![json!({"path": "c.rs", "status": "U"})]);
    }

    #[test]
    fn keeps_spaces_in_paths_and_skips_garbage() {
        let e = parse_porcelain(&format!("1 .M {H} with space.txt\0x\0\0? a b\0"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].path, "with space.txt");
        assert_eq!(e[1].path, "a b");
    }

    #[test]
    fn every_unmerged_pair_is_a_conflict() {
        for xy in ["DD", "AU", "UD", "UA", "DU", "AA", "UU"] {
            let rec = format!("u {xy} N... 100644 100644 100644 100644 a b c f\0");
            let (s, c, k) = split(&parse_porcelain(&rec));
            assert!(s.is_empty() && c.is_empty() && k.len() == 1, "{xy}");
        }
    }

    #[test]
    fn flags_submodules_by_kind_of_change() {
        let case = |flags: &str, xy: &str| {
            let (s, c, _) = split(&parse_porcelain(&format!("1 {xy} {flags} {SUB} lib\0")));
            (s, c)
        };
        let (s, c) = case("S.M.", ".M");
        assert!(s.is_empty());
        assert_eq!(
            c,
            vec![
                json!({"path": "lib", "status": "M", "submodule": {"commit": false, "modified": true, "untracked": false}})
            ]
        );
        let (_, c) = case("SC..", ".M");
        assert_eq!(
            c[0]["submodule"],
            json!({"commit": true, "modified": false, "untracked": false})
        );
        let (_, c) = case("SCM.", ".M");
        assert_eq!(
            c[0]["submodule"],
            json!({"commit": true, "modified": true, "untracked": false})
        );
        let (s, _) = case("SC..", "M.");
        assert_eq!(s[0]["submodule"]["commit"], json!(true));
        let (_, c) = case("S..U", ".M");
        assert_eq!(c[0]["submodule"]["untracked"], json!(true));
    }

    #[test]
    fn validates_paths() {
        for ok in ["a", "src/a.rs", "dir/", "a b", ".hidden", "a..b"] {
            assert!(valid_path(ok), "{ok}");
        }
        for no in ["", "/etc/passwd", "../x", "a/../../b", "a\0b", "\\x"] {
            assert!(!valid_path(no), "{no}");
        }
    }

    #[test]
    fn reads_files_field() {
        assert_eq!(files(&json!({"files": "all"})), Ok(Files::All));
        assert_eq!(
            files(&json!({"files": ["a", "b/c"]})),
            Ok(Files::Some(vec!["a".into(), "b/c".into()]))
        );
        assert_eq!(files(&json!({})), Err(bad("files required")));
        assert_eq!(files(&json!({"files": []})), Err(bad("files required")));
        assert_eq!(files(&json!({"files": "some"})), Err(bad("files required")));
        assert_eq!(
            files(&json!({"files": ["../x"]})),
            Err(bad("invalid path ../x"))
        );
        assert_eq!(files(&json!({"files": [1]})), Err(bad("invalid path 1")));
    }
}
