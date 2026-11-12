use crate::git::git_out;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const FILE_LINES_CAP: usize = 3000;
pub const TOTAL_CAP: usize = 4 << 20;
const UNTRACKED_CAP: u64 = 2 << 20;

#[derive(Clone, Debug, PartialEq)]
pub struct DLine {
    pub kind: &'static str,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DLine>,
}

#[derive(Clone, Debug, Default)]
pub struct File {
    pub path: String,
    pub old_path: String,
    pub status: &'static str,
    pub binary: bool,
    pub additions: usize,
    pub deletions: usize,
    pub truncated: bool,
    pub hunks: Vec<Hunk>,
    pub submodule: Option<(Option<String>, Option<String>)>,
    pub sub_base: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub at_ms: i64,
}

impl Commit {
    pub fn to_json(&self) -> Value {
        json!({"sha": self.sha, "subject": self.subject, "author": self.author, "at": crate::util::format_rfc3339(self.at_ms)})
    }
}

#[derive(Clone, Debug, Default)]
pub struct Review {
    pub worktree: String,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub merge_base: Option<String>,
    pub head: Option<String>,
    pub base_source: Option<&'static str>,
    pub truncated: bool,
    pub files: Vec<File>,
    pub commits: Vec<Commit>,
    pub recent: Vec<Commit>,
}

pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn hex(h: u64) -> String {
    format!("{h:016x}")
}

impl File {
    pub fn hash(&self) -> String {
        let mut s = format!(
            "{}\0{}\0{}\0{}",
            self.path, self.old_path, self.status, self.binary
        );
        for h in &self.hunks {
            s.push_str(&h.header);
            for l in &h.lines {
                s.push(match l.kind {
                    "add" => '+',
                    "del" => '-',
                    _ => ' ',
                });
                s.push_str(&l.text);
                s.push('\n');
            }
        }
        if self.truncated {
            s.push_str(&format!("{}+{}-", self.additions, self.deletions));
        }
        if let Some((a, b)) = &self.submodule {
            s.push_str(&format!("{a:?}{b:?}"));
        }
        hex(fnv(s.as_bytes()))
    }

    pub fn to_json(&self, viewed: bool) -> Value {
        let mut v = json!({
            "path": self.path, "oldPath": self.old_path, "status": self.status,
            "binary": self.binary, "additions": self.additions, "deletions": self.deletions,
            "hash": self.hash(), "viewed": viewed, "truncated": self.truncated,
            "hunks": self.hunks.iter().map(|h| json!({
                "header": h.header, "oldStart": h.old_start, "oldLines": h.old_lines,
                "newStart": h.new_start, "newLines": h.new_lines,
                "lines": h.lines.iter().map(|l| {
                    let mut v = json!({"kind": l.kind, "text": l.text});
                    if let Some(o) = l.old { v["old"] = json!(o); }
                    if let Some(n) = l.new { v["new"] = json!(n); }
                    v
                }).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        });
        if let Some((from, to)) = &self.submodule {
            v["submodule"] = json!({"from": from, "to": to});
        }
        v
    }
}

impl Review {
    pub fn revision(&self) -> String {
        let s: String = self
            .files
            .iter()
            .map(|f| f.hash())
            .chain(self.head.clone())
            .collect::<Vec<_>>()
            .join(",");
        hex(fnv(s.as_bytes()))
    }

    pub fn file(&self, path: &str) -> Option<&File> {
        self.files.iter().find(|f| f.path == path)
    }
}

fn range(s: &str) -> (u32, u32) {
    let (a, b) = s.split_once(',').unwrap_or((s, "1"));
    (a.parse().unwrap_or(0), b.parse().unwrap_or(1))
}

pub fn parse_header(h: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = h.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let (os, ol) = range(old);
    let (ns, nl) = range(new);
    Some((os, ol, ns, nl))
}

pub fn parse_patch(patch: &str) -> Vec<(bool, Vec<Hunk>)> {
    let mut out: Vec<(bool, Vec<Hunk>)> = vec![];
    let (mut old, mut new) = (0u32, 0u32);
    let (mut used_old, mut used_new) = (0u32, 0u32);
    for raw in patch.split('\n') {
        if raw.starts_with("diff --git ") {
            out.push((false, vec![]));
            continue;
        }
        let Some((binary, hunks)) = out.last_mut() else {
            continue;
        };
        if hunks.is_empty() && raw.starts_with("Binary files ") {
            *binary = true;
            continue;
        }
        if raw.starts_with("@@ ") {
            if let Some((os, ol, ns, nl)) = parse_header(raw) {
                hunks.push(Hunk {
                    header: raw.to_string(),
                    old_start: os,
                    old_lines: ol,
                    new_start: ns,
                    new_lines: nl,
                    lines: vec![],
                });
                (old, new) = (os, ns);
                (used_old, used_new) = (0, 0);
                continue;
            }
        }
        let Some(h) = hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match raw.as_bytes().first() {
            Some(b'+') => ("add", &raw[1..]),
            Some(b'-') => ("del", &raw[1..]),
            Some(b' ') => ("ctx", &raw[1..]),
            _ => continue,
        };
        if (kind != "add" && used_old >= h.old_lines) || (kind != "del" && used_new >= h.new_lines)
        {
            continue;
        }
        let line = DLine {
            kind,
            old: (kind != "add").then_some(old),
            new: (kind != "del").then_some(new),
            text: text.to_string(),
        };
        if kind != "add" {
            old += 1;
            used_old += 1;
        }
        if kind != "del" {
            new += 1;
            used_new += 1;
        }
        h.lines.push(line);
    }
    out
}

fn status_of(code: &str) -> &'static str {
    match code.as_bytes().first() {
        Some(b'A') => "added",
        Some(b'D') => "deleted",
        Some(b'R') => "renamed",
        Some(b'C') => "copied",
        _ => "modified",
    }
}

type Sub = Option<(Option<String>, Option<String>)>;

fn sha(s: &str) -> Option<String> {
    Some(s.to_string()).filter(|s| !s.is_empty() && s.bytes().any(|b| b != b'0'))
}

fn raw_status(z: &str) -> Vec<(&'static str, String, String, Sub)> {
    let mut it = z.split('\0').filter(|s| !s.is_empty());
    let mut out = vec![];
    while let Some(meta) = it.next() {
        let f: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
        let code = f.get(4).copied().unwrap_or("M");
        let st = status_of(code);
        let Some(a) = it.next() else { break };
        let b = if code.starts_with('R') || code.starts_with('C') {
            it.next().unwrap_or(a)
        } else {
            a
        };
        let sub = (f.first() == Some(&"160000") || f.get(1) == Some(&"160000"))
            .then(|| (f.get(2).and_then(|s| sha(s)), f.get(3).and_then(|s| sha(s))));
        out.push((st, a.to_string(), b.to_string(), sub));
    }
    out
}

fn sub_head(hunks: &[Hunk]) -> Option<String> {
    hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind == "add")
        .find_map(|l| l.text.strip_prefix("Subproject commit "))
        .and_then(|s| sha(s.trim_end_matches("-dirty")))
}

fn untracked(wt: &str, path: &str) -> File {
    let mut f = File {
        path: path.into(),
        old_path: path.into(),
        status: "untracked",
        ..Default::default()
    };
    let p = Path::new(wt).join(path);
    let Ok(meta) = std::fs::metadata(&p) else {
        return f;
    };
    if !meta.is_file() {
        return f;
    }
    if meta.len() > UNTRACKED_CAP {
        f.truncated = true;
        return f;
    }
    let Ok(bytes) = std::fs::read(&p) else {
        return f;
    };
    if bytes[..bytes.len().min(8000)].contains(&0) {
        f.binary = true;
        return f;
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split_terminator('\n').collect();
    f.additions = lines.len();
    if lines.len() > FILE_LINES_CAP {
        f.truncated = true;
        return f;
    }
    if !lines.is_empty() {
        f.hunks.push(Hunk {
            header: format!("@@ -0,0 +1,{} @@", lines.len()),
            old_start: 0,
            old_lines: 0,
            new_start: 1,
            new_lines: lines.len() as u32,
            lines: lines
                .iter()
                .enumerate()
                .map(|(i, l)| DLine {
                    kind: "add",
                    old: None,
                    new: Some(i as u32 + 1),
                    text: l.strip_suffix('\r').unwrap_or(l).to_string(),
                })
                .collect(),
        });
    }
    f
}

fn exists(wt: &str, r: &str) -> bool {
    git_out(
        wt,
        &["rev-parse", "--verify", "-q", &format!("{r}^{{commit}}")],
    )
    .is_ok()
}

fn upstream(wt: &str, branch: Option<&str>) -> Option<String> {
    git_out(
        wt,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty() && exists(wt, s))
    .or_else(|| {
        branch
            .map(|b| format!("origin/{b}"))
            .filter(|r| exists(wt, r))
    })
}

pub struct Defaults {
    pub name: String,
    pub start: String,
}

impl Defaults {
    pub fn of(wt: &str) -> Defaults {
        let name = crate::worktrees::default_branch(wt);
        let start = if name == "HEAD" {
            name.clone()
        } else {
            crate::worktrees::start_point(wt, &name)
        };
        Defaults { name, start }
    }
}

pub const BASE_KEY: &str = "overseer-base";

#[derive(Clone, Copy, PartialEq)]
pub enum Infer {
    Default,
    Created,
    Full,
}

pub fn base_ref(
    wt: &str,
    branch: Option<&str>,
    hint: Option<&str>,
    infer: Infer,
) -> (Option<String>, &'static str) {
    if let Some(h) = hint.filter(|h| !h.is_empty() && Some(*h) != branch && exists(wt, h)) {
        return (Some(h.to_string()), "hint");
    }
    let key = (wt.to_string(), branch.map(str::to_string), infer as u8);
    let stamp = crate::git::ref_stamp(wt, branch);
    let hit = BASES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&key))
        .filter(|c| stamp.as_ref() == Some(&c.0) && c.1.elapsed() < BASE_TTL)
        .map(|c| c.2.clone());
    if let Some(r) = hit {
        return r;
    }
    let r = detect(wt, branch, &Defaults::of(wt), infer);
    if let Some(st) = stamp {
        let mut g = BASES.lock().unwrap_or_else(|e| e.into_inner());
        let m = g.get_or_insert_with(HashMap::new);
        if m.len() >= 512 {
            m.clear();
        }
        m.insert(key, (st, Instant::now(), r.clone()));
    }
    r
}

const BASE_TTL: Duration = Duration::from_secs(60);

type Detected = (Option<String>, &'static str);
type BaseKey = (String, Option<String>, u8);
type Bases = HashMap<BaseKey, (crate::git::RefStamp, Instant, Detected)>;

static BASES: Mutex<Option<Bases>> = Mutex::new(None);

pub fn base_with(wt: &str, branch: Option<&str>, d: &Defaults) -> Option<String> {
    detect(wt, branch, d, Infer::Created).0
}

pub fn created_from(wt: &str, branch: &str) -> Option<String> {
    let line = |s: String| s.lines().next().map(str::trim).map(str::to_string);
    let stored = git_out(
        wt,
        &["config", "--get", &format!("branch.{branch}.{BASE_KEY}")],
    )
    .ok()
    .and_then(line);
    let from = stored.or_else(|| {
        git_out(
            wt,
            &[
                "reflog",
                "show",
                "--format=%gs",
                &format!("refs/heads/{branch}"),
                "--",
            ],
        )
        .ok()?
        .lines()
        .last()?
        .strip_prefix("branch: Created from ")
        .map(str::to_string)
    })?;
    Some(from).filter(|f| !f.is_empty() && f != "HEAD" && f != branch && exists(wt, f))
}

fn checked_out_from(wt: &str, branch: &str) -> Option<String> {
    let log = git_out(
        wt,
        &["reflog", "show", "--format=%gs", "-n", "2000", "HEAD", "--"],
    )
    .ok()?;
    let to = format!(" to {branch}");
    log.lines()
        .rev()
        .find_map(|l| {
            l.strip_prefix("checkout: moving from ")?
                .strip_suffix(to.as_str())
                .map(str::to_string)
        })
        .filter(|f| f != branch && exists(wt, &format!("refs/heads/{f}")))
}

fn ahead(wt: &str, base: &str, tip: &str) -> usize {
    git_out(
        wt,
        &["rev-list", "--count", &format!("{base}..{tip}"), "--"],
    )
    .ok()
    .and_then(|s| s.trim().parse().ok())
    .unwrap_or(usize::MAX)
}

pub fn detect(
    wt: &str,
    branch: Option<&str>,
    d: &Defaults,
    deep: Infer,
) -> (Option<String>, &'static str) {
    let on_default = branch == Some(d.name.as_str());
    let own = branch.filter(|_| !on_default && deep != Infer::Default);
    if on_default {
        if let Some(u) = upstream(wt, branch) {
            return (Some(u), "upstream");
        }
    }
    if let Some(c) = own.and_then(|b| created_from(wt, b)) {
        return (Some(c), "created");
    }
    let def = if !on_default
        && d.start != "HEAD"
        && Some(d.start.as_str()) != branch
        && exists(wt, &d.start)
    {
        Some(d.start.clone())
    } else {
        ["main", "master", "develop", "trunk"]
            .into_iter()
            .find(|b| Some(*b) != branch && exists(wt, b))
            .map(str::to_string)
    };
    if let Some(b) = own.filter(|_| deep == Infer::Full) {
        if let Some(c) = checked_out_from(wt, b).filter(|c| Some(c) != def.as_ref()) {
            let tip = format!("refs/heads/{b}");
            if def
                .as_deref()
                .is_none_or(|x| ahead(wt, &c, &tip) <= ahead(wt, x, &tip))
            {
                return (Some(c), "forkpoint");
            }
        }
    }
    (def, "default")
}

pub fn log(wt: &str, range: &str, n: usize) -> Vec<Commit> {
    let Ok(out) = git_out(
        wt,
        &[
            "log",
            "--no-color",
            &format!("-n{n}"),
            "--format=%H%x1f%s%x1f%an%x1f%ct%x1e",
            range,
            "--",
        ],
    ) else {
        return vec![];
    };
    out.split('\x1e')
        .filter_map(|r| {
            let mut f = r.trim_start_matches('\n').split('\x1f');
            let sha = f.next().filter(|s| !s.is_empty())?.to_string();
            Some(Commit {
                sha,
                subject: f.next().unwrap_or_default().to_string(),
                author: f.next().unwrap_or_default().to_string(),
                at_ms: f
                    .next()
                    .and_then(|t| t.trim().parse::<i64>().ok())
                    .unwrap_or(0)
                    * 1000,
            })
        })
        .collect()
}

pub const COMMITS_CAP: usize = 50;
pub const RECENT: usize = 10;

fn renames(wt: &str) -> &'static str {
    match git_out(wt, &["config", "--get", "diff.renames"]) {
        Ok(v) if matches!(v.trim(), "copies" | "copy") => "-C",
        _ => "-M",
    }
}

fn tracked(wt: &str, range: &[&str]) -> Result<Vec<File>, String> {
    let detect = renames(wt);
    let mut ns_args = vec![
        "diff",
        "--no-ext-diff",
        detect,
        "--raw",
        "--no-abbrev",
        "-z",
        "--ignore-submodules=none",
    ];
    ns_args.extend_from_slice(range);
    ns_args.push("--");
    let ns = git_out(wt, &ns_args)?;
    let mut p_args = vec![
        "diff",
        "--no-ext-diff",
        "--no-color",
        detect,
        "--ignore-submodules=none",
        "--submodule=short",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    p_args.extend_from_slice(range);
    p_args.push("--");
    let patch = git_out(wt, &p_args)?;
    let parsed = parse_patch(&patch);
    let mut files = vec![];
    for ((status, old, new, sub), (binary, hunks)) in raw_status(&ns).into_iter().zip(parsed) {
        if let Some((from, to)) = sub {
            let to = to.or_else(|| (status != "deleted").then(|| sub_head(&hunks)).flatten());
            let (inner, base) = match (&from, range.len() > 1, &to) {
                (Some(f), false, _) => expand(wt, &new, f, None),
                (Some(f), true, Some(t)) => expand(wt, &new, f, Some(t)),
                _ => (vec![], None),
            };
            if from.is_some() && to.is_some() && inner.is_empty() {
                continue;
            }
            files.extend(inner);
            files.push(File {
                path: new,
                old_path: old,
                status: "submodule",
                submodule: Some((from, to)),
                sub_base: base,
                ..Default::default()
            });
            continue;
        }
        let count = |k: &str| {
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == k)
                .count()
        };
        let mut f = File {
            path: new,
            old_path: old,
            status,
            binary,
            additions: count("add"),
            deletions: count("del"),
            truncated: false,
            hunks,
            submodule: None,
            sub_base: None,
        };
        if f.hunks.iter().map(|h| h.lines.len()).sum::<usize>() > FILE_LINES_CAP {
            f.hunks.clear();
            f.truncated = true;
        }
        files.push(f);
    }
    Ok(files)
}

fn expand(wt: &str, sub: &str, from: &str, to: Option<&str>) -> (Vec<File>, Option<String>) {
    let dir = Path::new(wt).join(sub);
    let dir = dir.to_string_lossy();
    if git_out(&dir, &["rev-parse", "--show-prefix"]).map_or(true, |p| !p.trim().is_empty()) {
        return (vec![], None);
    }
    let branch = git_out(&dir, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && *s != crate::worktrees::default_branch_cached(&dir));
    if branch.is_none() {
        return (vec![], None);
    }
    let (mut files, base) = match to {
        None => match assemble(&dir, None) {
            Ok(r) => (r.files, r.merge_base),
            Err(_) => return (vec![], None),
        },
        Some(to) => {
            let (b, _) = base_ref(&dir, branch.as_deref(), None, Infer::Default);
            let mb = merge_base_of(&dir, to, b.as_ref())
                .or_else(|| exists(&dir, &format!("{from}^{{commit}}")).then(|| from.to_string()));
            let Some(mb) = mb else { return (vec![], None) };
            (tracked(&dir, &[&mb, to]).unwrap_or_default(), Some(mb))
        }
    };
    for f in &mut files {
        f.path = format!("{sub}/{}", f.path);
        f.old_path = format!("{sub}/{}", f.old_path);
    }
    (files, base)
}

fn in_submodule<'a>(r: &'a Review, path: &'a str) -> Option<(&'a File, &'a str)> {
    r.files
        .iter()
        .filter(|f| f.submodule.is_some())
        .filter_map(|f| Some((f, path.strip_prefix(f.path.as_str())?.strip_prefix('/')?)))
        .max_by_key(|(f, _)| f.path.len())
}

pub fn blob_at(r: &Review, old: bool, path: &str) -> Option<(String, String, String)> {
    match in_submodule(r, path) {
        Some((f, rest)) => {
            let (_, to) = f.submodule.as_ref()?;
            let rev = if old {
                f.sub_base.clone()?
            } else {
                to.clone()?
            };
            let dir = Path::new(&r.worktree).join(&f.path);
            Some((dir.to_string_lossy().into_owned(), rev, rest.to_string()))
        }
        None => {
            let rev = if old {
                r.merge_base.clone()?
            } else {
                r.head.clone().unwrap_or_else(|| "HEAD".into())
            };
            Some((r.worktree.clone(), rev, path.to_string()))
        }
    }
}

fn cap(files: &mut [File]) -> bool {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut size = 0usize;
    let mut truncated = false;
    for f in files {
        size += f
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| l.text.len() + 16)
            .sum::<usize>();
        if size > TOTAL_CAP {
            truncated = true;
            if !f.hunks.is_empty() {
                f.hunks.clear();
                f.truncated = true;
            }
        }
    }
    truncated
}

fn merge_base_of(wt: &str, rev: &str, base: Option<&String>) -> Option<String> {
    base.and_then(|b| git_out(wt, &["merge-base", rev, b]).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn history(r: &mut Review, wt: &str, tip: &str) {
    if let (Some(mb), Some(h)) = (&r.merge_base, &r.head) {
        if mb != h {
            r.commits = log(wt, &format!("{mb}..{tip}"), COMMITS_CAP);
        }
    }
    if r.files.is_empty() && r.commits.is_empty() && r.head.is_some() {
        r.recent = log(wt, tip, RECENT);
    }
}

pub fn assemble(wt: &str, base_hint: Option<&str>) -> Result<Review, String> {
    let head = git_out(wt, &["rev-parse", "--verify", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string());
    let branch = git_out(wt, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let (base, source) = match head {
        Some(_) => base_ref(wt, branch.as_deref(), base_hint, Infer::Full),
        None => (None, "default"),
    };
    let merge_base = merge_base_of(wt, "HEAD", base.as_ref()).or_else(|| head.clone());
    let mut files = match &merge_base {
        Some(mb) => tracked(wt, &[mb])?,
        None => vec![],
    };
    let others = git_out(wt, &["ls-files", "--others", "--exclude-standard", "-z"])?;
    for p in others.split('\0').filter(|s| !s.is_empty()) {
        files.push(untracked(wt, p));
    }
    let truncated = cap(&mut files);
    let mut r = Review {
        worktree: wt.to_string(),
        branch,
        base_source: base.as_ref().map(|_| source),
        base,
        merge_base,
        head,
        truncated,
        files,
        ..Default::default()
    };
    history(&mut r, wt, "HEAD");
    Ok(r)
}

pub fn assemble_branch(wt: &str, branch: &str, base_hint: Option<&str>) -> Result<Review, String> {
    let rev = format!("refs/heads/{branch}");
    let head = git_out(
        wt,
        &["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")],
    )
    .map(|s| s.trim().to_string())
    .map_err(|_| format!("no local branch {branch}"))?;
    let (base, source) = base_ref(wt, Some(branch), base_hint, Infer::Default);
    let merge_base = merge_base_of(wt, &rev, base.as_ref()).unwrap_or_else(|| head.clone());
    let mut files = tracked(wt, &[&merge_base, &head])?;
    let truncated = cap(&mut files);
    let mut r = Review {
        worktree: wt.to_string(),
        branch: Some(branch.to_string()),
        base_source: base.as_ref().map(|_| source),
        base,
        merge_base: Some(merge_base),
        head: Some(head.clone()),
        truncated,
        files,
        ..Default::default()
    };
    history(&mut r, wt, &head);
    Ok(r)
}

pub fn lines_at(wt: &str, rev: &str, path: &str) -> Option<Vec<String>> {
    if let Some(l) = base_lines(wt, rev, path) {
        return Some(l);
    }
    let mut at = 0;
    while let Some(i) = path[at..].find('/') {
        let (sub, rest) = (&path[..at + i], &path[at + i + 1..]);
        at += i + 1;
        let tree = git_out(wt, &["ls-tree", rev, "--", sub]).ok()?;
        if let Some(sha) = tree
            .strip_prefix("160000 commit ")
            .and_then(|t| t.split('\t').next())
        {
            let dir = Path::new(wt).join(sub);
            return lines_at(&dir.to_string_lossy(), sha, rest);
        }
    }
    None
}

pub fn base_lines(wt: &str, merge_base: &str, path: &str) -> Option<Vec<String>> {
    git_out(wt, &["show", &format!("{merge_base}:{path}")])
        .ok()
        .map(|s| s.split_terminator('\n').map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunks_and_numbers() {
        let p = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1,3 +1,4 @@ fn\n a\n-b\n+B\n+C\n c\ndiff --git a/y b/y\nBinary files a/y and b/y differ\n";
        let v = parse_patch(p);
        assert_eq!(v.len(), 2);
        let h = &v[0].1[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 3, 1, 4)
        );
        let k: Vec<_> = h.lines.iter().map(|l| (l.kind, l.old, l.new)).collect();
        assert_eq!(
            k,
            [
                ("ctx", Some(1), Some(1)),
                ("del", Some(2), None),
                ("add", None, Some(2)),
                ("add", None, Some(3)),
                ("ctx", Some(3), Some(4))
            ]
        );
        assert!(v[1].0 && v[1].1.is_empty());
        assert_eq!(parse_header("@@ -0,0 +1 @@"), Some((0, 0, 1, 1)));
    }

    fn repo(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sb-review-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let d = d.canonicalize().unwrap();
        run(&d, &["init", "-q", "-b", "main"]);
        d
    }

    fn run(d: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .current_dir(d)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=T",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn cached_base_follows_ref_and_config_changes() {
        let d = repo("basecache");
        std::fs::write(d.join("a"), "a\n").unwrap();
        run(&d, &["add", "-A"]);
        run(&d, &["commit", "-q", "-m", "a"]);
        run(&d, &["branch", "other"]);
        run(&d, &["checkout", "-q", "-b", "feat"]);
        let wt = d.to_string_lossy().into_owned();
        let base = || base_ref(&wt, Some("feat"), None, Infer::Created);
        assert_eq!(base(), (Some("main".into()), "default"));
        assert_eq!(base(), (Some("main".into()), "default"));
        std::thread::sleep(Duration::from_millis(10));
        run(&d, &["config", &format!("branch.feat.{BASE_KEY}"), "other"]);
        assert_eq!(base(), (Some("other".into()), "created"));
        std::thread::sleep(Duration::from_millis(10));
        run(
            &d,
            &["config", "--unset", &format!("branch.feat.{BASE_KEY}")],
        );
        run(&d, &["branch", "-m", "main", "trunk"]);
        assert_eq!(base(), (Some("trunk".into()), "default"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn assembles_branch_uncommitted_and_untracked_against_merge_base() {
        let d = repo("asm");
        let w = |p: &str, s: &str| std::fs::write(d.join(p), s).unwrap();
        w("a.rs", "fn a() {\n    one();\n}\n");
        w("gone.txt", "bye\n");
        w("old.txt", "keep me\nas is\nplease\n");
        run(&d, &["add", "-A"]);
        run(&d, &["commit", "-q", "-m", "base"]);
        run(&d, &["checkout", "-q", "-b", "feature"]);
        w("a.rs", "fn a() {\n    two();\n}\n");
        run(&d, &["mv", "old.txt", "new.txt"]);
        run(&d, &["commit", "-q", "-am", "on branch"]);
        run(&d, &["checkout", "-q", "main"]);
        w("main_only.txt", "later on main\n");
        run(&d, &["add", "-A"]);
        run(&d, &["commit", "-q", "-m", "main moves on"]);
        run(&d, &["checkout", "-q", "feature"]);
        std::fs::remove_file(d.join("gone.txt")).unwrap();
        w("a.rs", "fn a() {\n    two();\n    three();\n}\n");
        w("fresh.md", "# new\n\nfile\n");
        std::fs::write(d.join("blob.bin"), [0u8, 1, 2, 3]).unwrap();
        let wt = d.to_string_lossy().into_owned();

        let r = assemble(&wt, None).unwrap();
        assert_eq!(r.branch.as_deref(), Some("feature"));
        assert_eq!(r.base.as_deref(), Some("main"));
        assert_ne!(r.merge_base, r.head);
        let st: Vec<(&str, &str)> = r
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect();
        assert_eq!(
            st,
            [
                ("a.rs", "modified"),
                ("blob.bin", "untracked"),
                ("fresh.md", "untracked"),
                ("gone.txt", "deleted"),
                ("new.txt", "renamed")
            ]
        );
        assert!(r.file("main_only.txt").is_none());
        let a = r.file("a.rs").unwrap();
        assert_eq!((a.additions, a.deletions), (2, 1));
        let added: Vec<(Option<u32>, &str)> = a.hunks[0]
            .lines
            .iter()
            .filter(|l| l.kind == "add")
            .map(|l| (l.new, l.text.as_str()))
            .collect();
        assert_eq!(added, [(Some(2), "    two();"), (Some(3), "    three();")]);
        assert_eq!(r.file("new.txt").unwrap().old_path, "old.txt");
        let fresh = r.file("fresh.md").unwrap();
        assert_eq!(fresh.hunks[0].header, "@@ -0,0 +1,3 @@");
        assert_eq!(fresh.additions, 3);
        assert!(r.file("blob.bin").unwrap().binary);
        let rev = r.revision();
        assert_eq!(assemble(&wt, None).unwrap().revision(), rev);
        w("fresh.md", "# new\n\nfile!\n");
        let r2 = assemble(&wt, None).unwrap();
        assert_ne!(r2.revision(), rev);
        assert_ne!(r2.file("fresh.md").unwrap().hash(), fresh.hash());
        assert_eq!(r2.file("a.rs").unwrap().hash(), a.hash());

        let only_head = assemble(&wt, Some("feature")).unwrap();
        assert_eq!(only_head.base.as_deref(), Some("main"));
        assert_eq!(
            base_lines(&wt, r.merge_base.as_deref().unwrap(), "a.rs").unwrap(),
            ["fn a() {", "    one();", "}"]
        );
        std::fs::remove_dir_all(d).unwrap();

        let m = repo("main");
        std::fs::write(m.join("x.txt"), "1\n").unwrap();
        run(&m, &["add", "-A"]);
        run(&m, &["commit", "-q", "-m", "one"]);
        std::fs::write(m.join("x.txt"), "2\n").unwrap();
        let mw = m.to_string_lossy().into_owned();
        let on_main = assemble(&mw, None).unwrap();
        assert_eq!(on_main.base, None);
        assert_eq!(on_main.merge_base, on_main.head);
        assert_eq!(on_main.files.len(), 1);
        assert_eq!(on_main.files[0].status, "modified");
        std::fs::remove_dir_all(m).unwrap();
    }

    #[test]
    fn default_branch_reviews_unpushed_work_against_its_upstream_then_recent_commits() {
        let origin = repo("origin");
        std::fs::write(origin.join("x.txt"), "1\n").unwrap();
        run(&origin, &["add", "-A"]);
        run(&origin, &["commit", "-q", "-m", "first"]);
        let d = std::env::temp_dir().join(format!("sb-review-clone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        run(
            &origin,
            &["clone", "-q", origin.to_str().unwrap(), d.to_str().unwrap()],
        );
        let wt = d.canonicalize().unwrap().to_string_lossy().into_owned();
        run(Path::new(&wt), &["remote", "set-head", "origin", "-d"]);

        let clean = assemble(&wt, None).unwrap();
        assert_eq!(clean.base.as_deref(), Some("origin/main"));
        assert!(clean.files.is_empty() && clean.commits.is_empty());
        let subjects: Vec<&str> = clean.recent.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["first"]);
        assert!(clean.recent[0].at_ms > 0);

        std::fs::write(Path::new(&wt).join("y.txt"), "2\n").unwrap();
        run(Path::new(&wt), &["add", "-A"]);
        run(Path::new(&wt), &["commit", "-q", "-m", "local only"]);
        std::fs::write(Path::new(&wt).join("x.txt"), "1\n1b\n").unwrap();
        std::fs::write(Path::new(&wt).join("z.txt"), "3\n").unwrap();
        let r = assemble(&wt, None).unwrap();
        assert_eq!(r.branch.as_deref(), Some("main"));
        assert_eq!(r.base.as_deref(), Some("origin/main"));
        assert_ne!(r.merge_base, r.head);
        let st: Vec<(&str, &str)> = r
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect();
        assert_eq!(
            st,
            [
                ("x.txt", "modified"),
                ("y.txt", "added"),
                ("z.txt", "untracked")
            ]
        );
        assert_eq!(r.commits.len(), 1);
        assert_eq!(r.commits[0].subject, "local only");
        assert!(r.recent.is_empty());

        let since = assemble(&wt, Some(&format!("{}~1", r.commits[0].sha))).unwrap();
        assert!(since.file("y.txt").is_some());
        std::fs::remove_dir_all(&d).unwrap();
        std::fs::remove_dir_all(&origin).unwrap();
    }

    #[test]
    fn base_is_where_the_branch_was_created_then_its_fork_point_then_the_default() {
        let d = repo("base");
        let wt = d.to_string_lossy().into_owned();
        let commit = |f: &str, m: &str| {
            std::fs::write(d.join(f), m).unwrap();
            run(&d, &["add", "-A"]);
            run(&d, &["commit", "-q", "-m", m]);
        };
        commit("a.txt", "base");
        run(&d, &["checkout", "-q", "-b", "dev"]);
        commit("dev.txt", "dev work");
        run(&d, &["checkout", "-q", "-b", "feat", "dev"]);
        commit("feat.txt", "feat work");

        let r = assemble(&wt, None).unwrap();
        assert_eq!(
            (r.base.as_deref(), r.base_source),
            (Some("dev"), Some("created"))
        );
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["feat.txt"]);

        let explicit = assemble(&wt, Some("main")).unwrap();
        assert_eq!(
            (explicit.base.as_deref(), explicit.base_source),
            (Some("main"), Some("hint"))
        );
        assert_eq!(explicit.files.len(), 2);
        let other = assemble_branch(&wt, "feat", None).unwrap();
        assert_eq!(other.base.as_deref(), Some("main"));

        run(&d, &["checkout", "-q", "dev"]);
        run(&d, &["checkout", "-q", "-b", "feat2"]);
        commit("f2.txt", "feat2 work");
        let r = assemble(&wt, None).unwrap();
        assert_eq!(
            (r.base.as_deref(), r.base_source),
            (Some("dev"), Some("forkpoint"))
        );

        run(&d, &["branch", "feat3", "main"]);
        run(&d, &["config", &format!("branch.feat3.{BASE_KEY}"), "dev"]);
        run(&d, &["checkout", "-q", "feat3"]);
        let r = assemble(&wt, None).unwrap();
        assert_eq!(
            (r.base.as_deref(), r.base_source),
            (Some("dev"), Some("created"))
        );

        run(&d, &["checkout", "-q", "main"]);
        run(&d, &["checkout", "-q", "-b", "feat4"]);
        let r = assemble(&wt, None).unwrap();
        assert_eq!(
            (r.base.as_deref(), r.base_source),
            (Some("main"), Some("default"))
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn copies_are_detected_when_the_repo_opts_in() {
        let d = repo("copies");
        let body = "fn shared() {\n    let a = 1;\n    let b = 2;\n    a + b;\n}\n";
        std::fs::write(d.join("a.rs"), body).unwrap();
        run(&d, &["add", "-A"]);
        run(&d, &["commit", "-q", "-m", "base"]);
        run(&d, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(d.join("a.rs"), format!("{body}fn more() {{}}\n")).unwrap();
        std::fs::write(d.join("b.rs"), body).unwrap();
        run(&d, &["add", "-A"]);
        run(&d, &["commit", "-q", "-m", "copy"]);
        let wt = d.to_string_lossy().into_owned();
        let plain = assemble(&wt, None).unwrap();
        assert_eq!(plain.file("b.rs").unwrap().status, "added");
        run(&d, &["config", "diff.renames", "copies"]);
        let r = assemble(&wt, None).unwrap();
        let b = r.file("b.rs").unwrap();
        assert_eq!((b.status, b.old_path.as_str()), ("copied", "a.rs"));
        assert!(b.hunks.is_empty());
        let t = super::super::triage::analyze(&r.files);
        assert_eq!(t.by_kind.get("rename"), Some(&1));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn raw_status_handles_renames_and_submodules() {
        let z = ":100644 100644 aa bb M\0a.rs\0:100644 100644 aa aa R087\0old.rs\0new.rs\0:100644 000000 aa 0000 D\0gone.rs\0:160000 160000 c1 0000000 M\0vendor/lib\0";
        let v = raw_status(z);
        assert_eq!(v[1].0, "renamed");
        assert_eq!((v[1].1.as_str(), v[1].2.as_str()), ("old.rs", "new.rs"));
        assert_eq!(v[2].0, "deleted");
        assert_eq!(v[0].3, None);
        assert_eq!(v[3].3, Some((Some("c1".into()), None)));
    }
}
