use crate::board::{decode, publish};
use crate::daemon::{q, Request};
use crate::git::{git, git_out};
use crate::util::{format_rfc3339, ticket};
use crate::worktrees::{self, valid_sha, Res};
use crate::Ctx;
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant, UNIX_EPOCH};

const DIFF_CAP: usize = 1 << 20;
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
const WATCH_FOR: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub at_ms: i64,
    pub subject: String,
}

impl Commit {
    pub fn to_json(&self) -> Value {
        json!({"sha": self.sha, "parents": self.parents, "author": self.author, "email": self.email,
               "at": format_rfc3339(self.at_ms), "subject": self.subject})
    }
}

pub const LOG_FORMAT: &str = "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%s%x1e";

pub fn parse_log(s: &str) -> Vec<Commit> {
    s.split('\x1e')
        .filter_map(|rec| {
            let mut f = rec.trim_start_matches('\n').split('\x1f');
            let sha = f.next().filter(|x| !x.is_empty())?.to_string();
            let parents = f.next()?.split_whitespace().map(str::to_string).collect();
            Some(Commit {
                sha,
                parents,
                author: f.next()?.to_string(),
                email: f.next()?.to_string(),
                at_ms: f.next()?.trim().parse::<i64>().unwrap_or(0) * 1000,
                subject: f.next().unwrap_or_default().to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ref {
    pub name: String,
    pub kind: &'static str,
    pub sha: String,
    pub upstream: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
}

pub fn parse_refs(s: &str) -> Vec<Ref> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\x1f').collect();
            let full = *f.first()?;
            let (kind, name) = if let Some(n) = full.strip_prefix("refs/heads/") {
                ("branch", n)
            } else if let Some(n) = full.strip_prefix("refs/remotes/") {
                if n.ends_with("/HEAD") {
                    return None;
                }
                ("remote", n)
            } else {
                ("tag", full.strip_prefix("refs/tags/")?)
            };
            let peeled = f.get(2).filter(|x| !x.is_empty());
            let sha = peeled.or(f.get(1))?.to_string();
            let upstream = f.get(3).filter(|x| !x.is_empty()).map(|x| x.to_string());
            let track = f.get(4).copied().unwrap_or_default();
            let num = |k: &str| {
                track
                    .split(',')
                    .find_map(|p| p.trim().strip_prefix(k)?.trim().parse().ok())
            };
            let known = upstream.is_some() && track != "gone";
            Some(Ref {
                name: name.to_string(),
                kind,
                sha,
                upstream,
                ahead: known.then(|| num("ahead").unwrap_or(0)),
                behind: known.then(|| num("behind").unwrap_or(0)),
            })
        })
        .collect()
}

pub fn refs(root: &str) -> Vec<Ref> {
    git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)%1f%(objectname)%1f%(*objectname)%1f%(upstream:short)%1f%(upstream:track,nobracket)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )
    .map(|s| parse_refs(&s))
    .unwrap_or_default()
}

fn overlays(ctx: &Ctx, root: &str, repo: &str, all: &[Ref]) -> Vec<Value> {
    let known = worktrees::Known::load(ctx, root);
    let entries = worktrees::entries(root);
    let paths: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    let owner = |p: &str| {
        paths
            .iter()
            .filter(|w| p == w.as_str() || p.starts_with(&format!("{w}/")))
            .max_by_key(|w| w.len())
            .cloned()
    };
    let mut by: Vec<(String, Map<String, Value>)> = vec![];
    fn slot(by: &mut Vec<(String, Map<String, Value>)>, b: &str) -> usize {
        match by.iter().position(|(n, _)| n == b) {
            Some(i) => i,
            None => {
                by.push((b.to_string(), Map::new()));
                by.len() - 1
            }
        }
    }
    let mut lines: Vec<&crate::Line> = known
        .lines
        .iter()
        .filter(|l| !l.branch.is_empty() && owner(&l.worktree_path).is_some())
        .collect();
    lines.sort_by_key(|l| std::cmp::Reverse(l.last_activity_ms));
    for l in lines {
        let i = slot(&mut by, &l.branch);
        if by[i].1.contains_key("lineId") {
            continue;
        }
        let m = &mut by[i].1;
        m.insert("lineId".into(), json!(l.id));
        m.insert("agent".into(), json!(l.agent));
        m.insert("state".into(), json!(l.state));
        m.insert("busy".into(), json!(l.busy));
        m.insert("needsInput".into(), json!(l.needs_input));
        m.insert("worktree".into(), json!(owner(&l.worktree_path)));
        if let Some(t) = &l.ticket {
            m.insert("ticketKey".into(), json!(t));
        }
    }
    let mut tasks: Vec<&crate::tasks::Task> = known
        .tasks
        .iter()
        .filter(|t| t.branch.is_some() && !t.from_session())
        .collect();
    tasks.sort_by_key(|t| std::cmp::Reverse(t.updated_ms));
    for t in tasks {
        let i = slot(&mut by, t.branch.as_deref().unwrap_or_default());
        let m = &mut by[i].1;
        if m.contains_key("taskId") {
            continue;
        }
        m.insert("taskId".into(), json!(t.id));
        m.insert("taskNumber".into(), json!(t.number));
        m.insert("taskStatus".into(), json!(t.status));
        if let Some(k) = &t.ticket_key {
            m.entry("ticketKey").or_insert(json!(k));
        }
        if let Some(w) = &t.worktree_path {
            m.entry("worktree").or_insert(json!(w));
        }
    }
    let managed_root = crate::layout::ManagedRoot::new(&ctx.config.worktree_dir);
    for e in &entries {
        if let (true, Some(b)) = (managed_root.holds_entry(&e.path), &e.branch) {
            let i = slot(&mut by, b);
            by[i].1.entry("worktree").or_insert(json!(e.path));
        }
    }
    by.into_iter()
        .map(|(b, mut m)| {
            if let Some(r) = all.iter().find(|r| r.kind == "branch" && r.name == b) {
                m.insert("sha".into(), json!(r.sha));
            }
            if !m.contains_key("ticketKey") {
                if let Some(k) = ticket(&b) {
                    m.insert("ticketKey".into(), json!(k));
                }
            }
            let project = root.to_string();
            let pr = crate::gh::cached_or_fetch(repo, &b, move || {
                publish("git.changed", json!({ "project": project }))
            })["pr"]
                .clone();
            if !pr.is_null() {
                m.insert(
                    "pr".into(),
                    json!({"number": pr["number"], "state": pr["state"], "isDraft": pr["isDraft"], "url": pr["url"]}),
                );
            }
            m.insert("branch".into(), json!(b));
            Value::Object(m)
        })
        .collect()
}

fn revs(root: &str, sel: &str, base: &str, ov: &[Value]) -> Vec<String> {
    match sel {
        "" | "all" => ["--branches", "--remotes", "--tags", "HEAD"]
            .map(String::from)
            .to_vec(),
        "current" => vec!["HEAD".into()],
        "agents" => {
            let mut v = vec!["HEAD".to_string(), base.to_string()];
            v.extend(
                ov.iter()
                    .filter_map(|o| o["branch"].as_str())
                    .map(|b| format!("refs/heads/{b}")),
            );
            v.retain(|r| git(root, &["rev-parse", "--verify", "-q", r]).is_some());
            v
        }
        list => list
            .split(',')
            .map(str::trim)
            .filter(|r| !r.is_empty() && !r.starts_with('-'))
            .filter(|r| git(root, &["rev-parse", "--verify", "-q", r]).is_some())
            .map(String::from)
            .collect(),
    }
}

pub fn log(root: &str, revs: &[String], skip: usize, n: usize) -> Vec<Commit> {
    if revs.is_empty() {
        return vec![];
    }
    let skip = format!("--skip={skip}");
    let n = format!("-n{n}");
    let mut args = vec!["log", "--date-order", LOG_FORMAT, &skip, &n];
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    git(root, &args).map(|s| parse_log(&s)).unwrap_or_default()
}

pub fn graph(ctx: &Ctx, path: &str, r: &Request) -> Res<Value> {
    let (root, repo) = worktrees::project(path)?;
    touch(&root);
    let limit = q(r, "limit")
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| (400, "limit must be a number".to_string()))
        })
        .transpose()?
        .unwrap_or(300)
        .clamp(1, 2000);
    let skip = q(r, "before")
        .map(|s| {
            decode(s)
                .parse::<usize>()
                .map_err(|_| (400, "bad before cursor".to_string()))
        })
        .transpose()?
        .unwrap_or(0);
    let sel = q(r, "refs").map(decode).unwrap_or_default();
    let all = refs(&root);
    let base = worktrees::default_branch(&root);
    let ov = overlays(ctx, &root, &repo, &all);
    let revs = revs(&root, &sel, &worktrees::start_point(&root, &base), &ov);
    let mut commits = log(&root, &revs, skip, limit + 1);
    let more = commits.len() > limit;
    commits.truncate(limit);
    let checked: HashMap<String, String> = worktrees::entries(&root)
        .into_iter()
        .filter_map(|e| Some((e.branch?, e.path)))
        .collect();
    let refs: Vec<Value> = all
        .iter()
        .map(|x| {
            let mut v = json!({"name": x.name, "kind": x.kind, "sha": x.sha});
            if let Some(u) = &x.upstream {
                v["upstream"] = json!(u);
            }
            if let (Some(a), Some(b)) = (x.ahead, x.behind) {
                v["ahead"] = json!(a);
                v["behind"] = json!(b);
            }
            if x.kind == "branch" {
                if let Some(p) = checked.get(&x.name) {
                    v["worktree"] = json!(p);
                }
            }
            v
        })
        .collect();
    let head = git(&root, &["rev-parse", "-q", "--verify", "HEAD"]).map(
        |sha| json!({"sha": sha, "branch": git(&root, &["symbolic-ref", "-q", "--short", "HEAD"])}),
    );
    let mut v = json!({
        "project": root, "repo": repo, "base": base, "head": head, "refs": refs, "overlays": ov,
        "commits": commits.iter().map(Commit::to_json).collect::<Vec<_>>(),
    });
    if more {
        v["next"] = json!((skip + limit).to_string());
    }
    Ok(v)
}

#[derive(Debug, PartialEq)]
pub struct File {
    pub path: String,
    pub old_path: Option<String>,
    pub status: String,
    pub added: u64,
    pub removed: u64,
    pub binary: bool,
}

pub fn parse_files(name_status: &str, numstat: &str) -> Vec<File> {
    let mut files = vec![];
    let mut it = name_status.split('\0').filter(|s| !s.is_empty());
    while let Some(st) = it.next() {
        let letter = st.chars().next().unwrap_or('M');
        let (old, path) = if matches!(letter, 'R' | 'C') {
            let o = it.next().unwrap_or_default().to_string();
            (Some(o), it.next().unwrap_or_default().to_string())
        } else {
            (None, it.next().unwrap_or_default().to_string())
        };
        files.push(File {
            path,
            old_path: old,
            status: letter.to_string(),
            added: 0,
            removed: 0,
            binary: false,
        });
    }
    let mut at: HashMap<String, usize> = HashMap::with_capacity(files.len());
    for (i, f) in files.iter().enumerate() {
        at.entry(f.path.clone()).or_insert(i);
    }
    let mut toks = numstat.split('\0');
    while let Some(t) = toks.next() {
        let mut f = t.splitn(3, '\t');
        let (Some(a), Some(d), Some(p)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        let path = if p.is_empty() {
            let _old = toks.next();
            toks.next().unwrap_or_default().to_string()
        } else {
            p.to_string()
        };
        if let Some(x) = at.get(&path).map(|&i| &mut files[i]) {
            x.binary = a == "-" && d == "-";
            x.added = a.parse().unwrap_or(0);
            x.removed = d.parse().unwrap_or(0);
        }
    }
    files
}

pub fn commit(path: &str, sha: &str) -> Res<Value> {
    if !valid_sha(sha) {
        return Err((400, "sha must be 4-64 hex digits".into()));
    }
    let (root, _) = worktrees::project(path)?;
    let full = git(
        &root,
        &["rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")],
    )
    .ok_or_else(|| (404, format!("unknown commit {sha}")))?;
    let meta = git(
        &root,
        &[
            "show",
            "-s",
            "--format=%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ct%x1f%s%x1f%b",
            &full,
        ],
    )
    .ok_or_else(|| (404, format!("unknown commit {sha}")))?;
    let f: Vec<&str> = meta.splitn(8, '\x1f').collect();
    let get = |i: usize| f.get(i).copied().unwrap_or_default();
    let secs = |i: usize| get(i).trim().parse::<i64>().unwrap_or(0) * 1000;
    let parents: Vec<&str> = get(0).split_whitespace().collect();
    let from = parents.first().copied().unwrap_or(EMPTY_TREE);
    let ns =
        git_out(&root, &["diff", "--name-status", "-z", "-M", from, &full]).unwrap_or_default();
    let num = git_out(&root, &["diff", "--numstat", "-z", "-M", from, &full]).unwrap_or_default();
    let mut diff =
        git_out(&root, &["diff", "-M", "--no-ext-diff", from, &full]).unwrap_or_default();
    let truncated = diff.len() > DIFF_CAP;
    if truncated {
        let mut cut = DIFF_CAP;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        let cut = diff[..cut].rfind('\n').map_or(cut, |i| i + 1);
        diff.truncate(cut);
    }
    let names: Vec<String> = git(
        &root,
        &[
            "for-each-ref",
            "--points-at",
            &full,
            "--format=%(refname:short)",
        ],
    )
    .map(|s| s.lines().map(str::to_string).collect())
    .unwrap_or_default();
    let files: Vec<Value> = parse_files(&ns, &num)
        .into_iter()
        .map(|x| {
            let mut v = json!({"path": x.path, "status": x.status, "added": x.added,
                               "removed": x.removed, "binary": x.binary});
            if let Some(o) = x.old_path {
                v["oldPath"] = json!(o);
            }
            v
        })
        .collect();
    Ok(json!({
        "sha": full, "parents": parents, "author": get(1), "email": get(2), "at": format_rfc3339(secs(3)),
        "committer": get(4), "committedAt": format_rfc3339(secs(5)), "subject": get(6),
        "body": get(7).trim_end(), "refs": names, "files": files, "diff": diff, "truncated": truncated,
    }))
}

pub fn branch(path: &str, b: &Value) -> Res<Value> {
    let (root, _) = worktrees::project(path)?;
    let name = b["name"].as_str().map(str::trim).unwrap_or_default();
    let sha = b["sha"].as_str().unwrap_or_default();
    if name.is_empty()
        || name.starts_with('-')
        || git(&root, &["check-ref-format", "--branch", name]).is_none()
    {
        return Err((400, format!("'{name}' is not a valid branch name")));
    }
    if !valid_sha(sha) {
        return Err((400, "sha must be 4-64 hex digits".into()));
    }
    let full = git(
        &root,
        &["rev-parse", "--verify", "-q", &format!("{sha}^{{commit}}")],
    )
    .ok_or_else(|| (400, format!("unknown commit {sha}")))?;
    if git(
        &root,
        &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")],
    )
    .is_some()
    {
        return Err((409, format!("branch {name} already exists")));
    }
    git_out(&root, &["branch", "--no-track", name, &full]).map_err(|e| (409, e))?;
    publish("git.changed", json!({ "project": root }));
    Ok(json!({"name": name, "sha": full}))
}

type Watched = HashMap<String, (Instant, String, u64)>;
static WATCHED: Mutex<Option<Watched>> = Mutex::new(None);

fn walk(dir: &Path, h: &mut DefaultHasher) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut items: Vec<_> = rd.flatten().collect();
    items.sort_by_key(|e| e.file_name());
    for e in items {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => walk(&p, h),
            Ok(_) => stamp(&p, h),
            Err(_) => {}
        }
    }
}

fn stamp(p: &Path, h: &mut DefaultHasher) {
    p.hash(h);
    if let Ok(m) = std::fs::metadata(p) {
        m.len().hash(h);
        if let Ok(t) = m.modified() {
            t.duration_since(UNIX_EPOCH).unwrap_or_default().hash(h);
        }
    }
}

pub fn fingerprint(common: &str) -> u64 {
    let c = Path::new(common);
    let mut h = DefaultHasher::new();
    stamp(&c.join("HEAD"), &mut h);
    stamp(&c.join("packed-refs"), &mut h);
    walk(&c.join("refs"), &mut h);
    if let Ok(rd) = std::fs::read_dir(c.join("worktrees")) {
        let mut ws: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        ws.sort();
        for w in ws {
            stamp(&w.join("HEAD"), &mut h);
        }
    }
    h.finish()
}

pub fn touch(root: &str) {
    let mut g = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some(e) = m.get_mut(root) {
        e.0 = Instant::now();
        return;
    }
    let Some(common) = git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ) else {
        return;
    };
    let fp = fingerprint(&common);
    m.insert(root.to_string(), (Instant::now(), common, fp));
}

pub fn poll() -> Vec<String> {
    let items: Vec<(String, String, u64)> = {
        let mut g = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        m.retain(|_, (at, _, _)| at.elapsed() < WATCH_FOR);
        m.iter()
            .map(|(r, (_, c, f))| (r.clone(), c.clone(), *f))
            .collect()
    };
    let mut changed = vec![];
    for (root, common, old) in items {
        let fp = fingerprint(&common);
        if fp != old {
            if let Some(e) = WATCHED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_or_insert_with(HashMap::new)
                .get_mut(&root)
            {
                e.2 = fp;
            }
            changed.push(root);
        }
    }
    changed
}

fn watching() -> bool {
    WATCHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|m| !m.is_empty())
}

pub fn watch_loop() {
    loop {
        std::thread::sleep(Duration::from_secs(if watching() { 1 } else { 5 }));
        for project in poll() {
            publish("git.changed", json!({ "project": project }));
        }
    }
}

fn res(code: u16, r: Res<Value>) -> (u16, Value) {
    match r {
        Ok(v) => (code, v),
        Err((c, e)) => (c, json!({ "error": e })),
    }
}

pub fn routes(ctx: &Ctx, r: &Request, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    Some(match (r.method.as_str(), segs) {
        ("GET", ["v1", "projects", p, "graph"]) => res(200, graph(ctx, &decode(p), r)),
        ("GET", ["v1", "projects", p, "commits", sha]) => res(200, commit(&decode(p), sha)),
        ("POST", ["v1", "projects", p, "branches"]) => res(201, branch(&decode(p), body)),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_log_records() {
        let s = "a1\x1fp1 p2\x1fAda\x1fada@x\x1f1700000000\x1fMerge x\x1e\nb2\x1f\x1fBob\x1fb@x\x1f1\x1fRoot \x1f weird\x1e\n";
        let c = parse_log(s);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].parents, vec!["p1", "p2"]);
        assert_eq!(c[0].at_ms, 1_700_000_000_000);
        assert!(c[1].parents.is_empty());
        assert_eq!(c[1].subject, "Root ");
    }

    #[test]
    fn parses_refs_and_tracking() {
        let s = "refs/heads/main\x1fa\x1f\x1forigin/main\x1fahead 1, behind 2\n\
                 refs/heads/x\x1fb\x1f\x1forigin/x\x1fgone\n\
                 refs/remotes/origin/HEAD\x1fa\x1f\x1f\x1f\n\
                 refs/remotes/origin/main\x1fc\x1f\x1f\x1f\n\
                 refs/tags/v1\x1ftagobj\x1fpeeled\x1f\x1f\n";
        let r = parse_refs(s);
        assert_eq!(r.len(), 4);
        assert_eq!((r[0].ahead, r[0].behind), (Some(1), Some(2)));
        assert_eq!(r[1].ahead, None);
        assert_eq!(r[2].kind, "remote");
        assert_eq!((r[3].kind, r[3].sha.as_str()), ("tag", "peeled"));
    }

    #[test]
    fn parses_changed_files() {
        let ns = "M\0src/a.rs\0R087\0old.rs\0new.rs\0A\0img.png\0";
        let num = "3\t1\tsrc/a.rs\x002\t0\t\0old.rs\0new.rs\0-\t-\timg.png\0";
        let f = parse_files(ns, num);
        assert_eq!(f.len(), 3);
        assert_eq!((f[0].added, f[0].removed), (3, 1));
        assert_eq!(f[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!((f[1].status.as_str(), f[1].added), ("R", 2));
        assert!(f[2].binary);
    }
}
