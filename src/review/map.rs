use super::{diff, graph, lang, triage, Res, Target};
use crate::{Ctx, Line};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

const TREE_CAP: usize = 40_000;
const TREES_CAP: usize = 64;
const PER_NAME: usize = 2;
const STOP: &[&str] = &[
    "new", "default", "from", "into", "main", "test", "tests", "init", "run", "get", "set", "len",
    "clone", "drop", "render", "load", "start", "stop", "update", "build", "props", "state",
    "index", "value", "error", "result", "data", "config", "options",
];

type LinkKey = (usize, String, usize, String);
type LinkVal = (&'static str, usize, bool);
type Tree = Arc<(Vec<(String, u64)>, bool)>;
static TREES: Mutex<Option<HashMap<String, Tree>>> = Mutex::new(None);

#[derive(Clone, Debug, Default)]
pub struct Changed {
    pub path: String,
    pub status: &'static str,
    pub size: u64,
    pub add: usize,
    pub del: usize,
    pub logic: usize,
    pub noise: usize,
    pub sig: usize,
    pub edits: u32,
    pub fails: u32,
    pub viewed: bool,
    pub threads: usize,
    pub test: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Region {
    pub worktree: String,
    pub name: String,
    pub branch: Option<String>,
    pub parent: Option<usize>,
    pub error: Option<String>,
    pub tree: Tree,
    pub changed: Vec<Changed>,
    pub edges: Vec<(String, String, &'static str)>,
    pub symbols: Vec<(String, String)>,
}

pub fn parse_tree(out: &str) -> (Vec<(String, u64)>, bool) {
    let mut files = vec![];
    for rec in out.split('\0').filter(|r| !r.is_empty()) {
        let Some((meta, path)) = rec.split_once('\t') else {
            continue;
        };
        let mut f = meta.split_whitespace();
        let (_, kind, _, size) = (f.next(), f.next(), f.next(), f.next());
        if kind != Some("blob") {
            continue;
        }
        if files.len() >= TREE_CAP {
            return (files, true);
        }
        files.push((
            path.to_string(),
            size.and_then(|s| s.parse().ok()).unwrap_or(0),
        ));
    }
    (files, false)
}

fn tree(wt: &str) -> Tree {
    let head = crate::git::git(wt, &["rev-parse", "-q", "--verify", "HEAD"]).unwrap_or_default();
    let key = format!("{wt}\0{head}");
    if let Some(t) = TREES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&key))
    {
        return t.clone();
    }
    let t: Tree = Arc::new(if head.is_empty() {
        (vec![], false)
    } else {
        parse_tree(&crate::git::git(wt, &["ls-tree", "-r", "-l", "-z", "HEAD"]).unwrap_or_default())
    });
    let mut g = TREES.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() >= TREES_CAP {
        m.clear();
    }
    m.insert(key, t.clone());
    t
}

fn submodules(wt: &str) -> Vec<String> {
    crate::git::git(
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
            .filter_map(|l| l.split_once(' ').map(|(_, p)| Path::new(wt).join(p)))
            .filter(|d| d.join(".git").exists())
            .filter_map(|d| crate::git::git(d.to_str()?, &["rev-parse", "--show-toplevel"]))
            .collect()
    })
    .unwrap_or_default()
}

fn name_of(wt: &str) -> String {
    Path::new(wt)
        .file_name()
        .map_or_else(|| wt.to_string(), |n| n.to_string_lossy().into_owned())
}

pub fn struggle(ctx: &Ctx, lines: &[&Line]) -> HashMap<String, (u32, u32)> {
    let mut out: HashMap<String, (u32, u32)> = HashMap::new();
    let mut seen = BTreeSet::new();
    for l in lines {
        if !seen.insert(l.id.clone()) {
            continue;
        }
        let Some(t) = crate::activity::find_transcript(
            &ctx.claude_dir,
            &ctx.codex_dir,
            &ctx.gemini_dir,
            l.agent,
            &l.session_id,
        ) else {
            continue;
        };
        let dir = super::line_dir(l).unwrap_or("");
        for (p, (e, f)) in crate::activity::edit_stats(l.agent, &t) {
            let abs = if Path::new(&p).is_absolute() {
                p
            } else {
                Path::new(dir).join(&p).to_string_lossy().into_owned()
            };
            let slot = out.entry(abs).or_default();
            slot.0 += e;
            slot.1 += f;
        }
    }
    out
}

fn region(
    ctx: &Ctx,
    t: &Target,
    wt: &str,
    parent: Option<usize>,
    primary: bool,
    stats: &HashMap<String, (u32, u32)>,
    reg: &crate::registry::Registry,
) -> Region {
    let mut r = Region {
        worktree: wt.to_string(),
        name: name_of(wt),
        branch: crate::git::git(wt, &["symbolic-ref", "--short", "-q", "HEAD"]),
        parent,
        tree: tree(wt),
        ..Default::default()
    };
    let rv = match (&t.branch, primary) {
        (Some(b), true) => diff::assemble_branch(wt, b, None),
        _ => diff::assemble(wt, super::hint_of(ctx, wt).0.as_deref()),
    };
    let rv = match rv {
        Ok(rv) => rv,
        Err(e) => {
            r.error = Some(e);
            return r;
        }
    };
    let tr = triage::analyze(&rv.files);
    let g = graph::of_review(&rv, t.branch.is_none() || !primary);
    let seen = super::viewed(reg, wt).unwrap_or_default();
    let open: Vec<super::Thread> = super::threads_where(
        reg,
        "WHERE worktree = ?1 AND status IN ('pending', 'sent')",
        &[&wt],
    )
    .unwrap_or_default();
    let sizes: HashMap<&str, u64> = r.tree.0.iter().map(|(p, s)| (p.as_str(), *s)).collect();
    for (f, ft) in rv.files.iter().zip(&tr.files) {
        let changed = f.additions + f.deletions;
        let abs = Path::new(wt).join(&f.path).to_string_lossy().into_owned();
        let (edits, fails) = stats.get(&abs).copied().unwrap_or_default();
        let size = sizes.get(f.path.as_str()).copied().unwrap_or_else(|| {
            std::fs::metadata(Path::new(wt).join(&f.path)).map_or(0, |m| m.len())
        });
        r.changed.push(Changed {
            path: f.path.clone(),
            status: f.status,
            size,
            add: f.additions,
            del: f.deletions,
            logic: ft.residual,
            noise: changed.saturating_sub(ft.residual),
            sig: ft.sig,
            edits,
            fails,
            viewed: seen.get(&f.path) == Some(&f.hash()),
            threads: open.iter().filter(|x| x.path == f.path).count(),
            test: lang::test_path(&f.path),
        });
    }
    let path_of: HashMap<&str, &str> = g["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| Some((n["id"].as_str()?, n["path"].as_str()?)))
        .collect();
    for e in g["edges"].as_array().into_iter().flatten() {
        let (Some(a), Some(b)) = (
            e["from"].as_str().and_then(|x| path_of.get(x)),
            e["to"].as_str().and_then(|x| path_of.get(x)),
        ) else {
            continue;
        };
        if a != b {
            let kind = if e["kind"] == "imports" {
                "imports"
            } else {
                "uses"
            };
            r.edges.push((a.to_string(), b.to_string(), kind));
        }
    }
    for n in g["nodes"].as_array().into_iter().flatten() {
        if n["kind"] == "file" {
            continue;
        }
        if let (Some(p), Some(name)) = (n["path"].as_str(), n["name"].as_str()) {
            r.symbols.push((p.to_string(), name.to_string()));
        }
    }
    r
}

fn file_json(c: &Changed) -> Value {
    json!({
        "path": c.path, "status": c.status, "size": c.size, "add": c.add, "del": c.del,
        "logic": c.logic, "noise": c.noise, "sig": c.sig, "edits": c.edits, "fails": c.fails,
        "viewed": c.viewed, "threads": c.threads, "test": c.test,
    })
}

pub fn build(regions: &[Region]) -> (Value, usize) {
    let mut work = 0usize;
    let mut out = vec![];
    for r in regions {
        let changed: BTreeSet<&str> = r.changed.iter().map(|c| c.path.as_str()).collect();
        let mut tree = vec![];
        for (p, s) in &r.tree.0 {
            work += 1;
            if !changed.contains(p.as_str()) {
                tree.push(json!([p, s]));
            }
        }
        work += r.changed.len();
        out.push(json!({
            "worktree": r.worktree, "name": r.name, "branch": r.branch, "parent": r.parent,
            "error": r.error, "truncated": r.tree.1,
            "changed": r.changed.iter().map(file_json).collect::<Vec<_>>(),
            "tree": tree,
        }));
    }
    let mut links: BTreeMap<LinkKey, LinkVal> = BTreeMap::new();
    for (i, r) in regions.iter().enumerate() {
        for (a, b, kind) in &r.edges {
            work += 1;
            let e = links
                .entry((i, a.clone(), i, b.clone()))
                .or_insert((kind, 0, false));
            e.1 += 1;
        }
    }
    let mut names: HashMap<&str, Vec<(usize, &str)>> = HashMap::new();
    for (i, r) in regions.iter().enumerate() {
        for (p, n) in &r.symbols {
            work += 1;
            if n.len() < 4 || STOP.contains(&n.as_str()) {
                continue;
            }
            let v = names.entry(n.as_str()).or_default();
            let mine = v.iter().filter(|(w, _)| *w == i).count();
            if mine < PER_NAME && !v.iter().any(|(w, q)| *w == i && *q == p.as_str()) {
                v.push((i, p.as_str()));
            }
        }
    }
    for list in names.values() {
        for (k, (wa, pa)) in list.iter().enumerate() {
            for (wb, pb) in &list[k + 1..] {
                work += 1;
                if wa == wb {
                    continue;
                }
                let (from, to) = if wa > wb {
                    ((wa, pa), (wb, pb))
                } else {
                    ((wb, pb), (wa, pa))
                };
                links
                    .entry((*from.0, from.1.to_string(), *to.0, to.1.to_string()))
                    .or_insert(("uses", 0, true))
                    .1 += 1;
            }
        }
    }
    let links: Vec<Value> = links
        .into_iter()
        .map(|((fw, fp, tw, tp), (kind, count, probable))| {
            let mut v = json!({"from": {"w": fw, "path": fp}, "to": {"w": tw, "path": tp}, "kind": kind, "count": count, "cross": fw != tw});
            if probable {
                v["probable"] = json!(true);
            }
            v
        })
        .collect();
    (json!({"regions": out, "links": links}), work)
}

pub fn route(ctx: &Ctx, t: &Target, with: &[String]) -> Res<Value> {
    super::watch::touch(&t.worktree);
    let mut roots = vec![t.worktree.clone()];
    for w in with {
        let w = super::top(w)?;
        if !roots.contains(&w) {
            roots.push(w);
        }
    }
    if t.branch.is_none() {
        for w in super::links(ctx, t) {
            if !roots.contains(&w) {
                roots.push(w);
            }
        }
    }
    let mut plan: Vec<(String, Option<usize>)> = vec![];
    for root in &roots {
        if plan.iter().any(|(w, _)| w == root) {
            continue;
        }
        let at = plan.len();
        plan.push((root.clone(), None));
        for s in submodules(root) {
            if !plan.iter().any(|(w, _)| *w == s) && !roots.contains(&s) {
                plan.push((s, Some(at)));
            }
        }
    }
    let all = super::cached_lines(ctx);
    let mut lines: Vec<&Line> = vec![];
    for (w, _) in &plan {
        lines.extend(super::agents_at(ctx, w, &all));
    }
    if let Some(l) = t
        .line_id
        .as_ref()
        .and_then(|id| all.iter().find(|l| &l.id == id))
    {
        lines.push(l);
    }
    let stats = struggle(ctx, &lines);
    let reg = super::open(ctx)?;
    let regions: Vec<Region> = plan
        .iter()
        .enumerate()
        .map(|(i, (w, parent))| {
            super::watch::touch(w);
            region(ctx, t, w, *parent, i == 0, &stats, &reg)
        })
        .collect();
    Ok(build(&regions).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(name: &str, tree: &[(&str, u64)], changed: &[&str]) -> Region {
        Region {
            worktree: format!("/w/{name}"),
            name: name.into(),
            tree: Arc::new((
                tree.iter().map(|(p, s)| (p.to_string(), *s)).collect(),
                false,
            )),
            changed: changed
                .iter()
                .map(|p| Changed {
                    path: p.to_string(),
                    add: 3,
                    logic: 3,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn parses_ls_tree_blobs_and_skips_submodule_links() {
        let out = [
            "100644 blob aa 12\tsrc/a.rs",
            "160000 commit bb -\tlib",
            "100755 blob cc 7\tbin/run sh",
            "",
        ]
        .join("\0");
        let out = out.as_str();
        let (files, cut) = parse_tree(out);
        assert_eq!(
            files,
            vec![("src/a.rs".to_string(), 12), ("bin/run sh".to_string(), 7)]
        );
        assert!(!cut);
    }

    #[test]
    fn keeps_unchanged_files_in_the_tree_and_changed_ones_with_their_signals() {
        let r = reg("web", &[("a.rs", 10), ("b.rs", 20)], &["a.rs"]);
        let (v, _) = build(&[r]);
        let w = &v["regions"][0];
        assert_eq!(w["tree"], json!([["b.rs", 20]]));
        assert_eq!(w["changed"][0]["path"], "a.rs");
        assert_eq!(w["changed"][0]["logic"], 3);
    }

    #[test]
    fn collapses_symbol_edges_to_file_links_with_counts() {
        let mut r = reg("web", &[], &["a.rs", "b.rs"]);
        r.edges = vec![
            ("a.rs".into(), "b.rs".into(), "uses"),
            ("a.rs".into(), "b.rs".into(), "uses"),
            ("b.rs".into(), "a.rs".into(), "imports"),
        ];
        let (v, _) = build(&[r]);
        let l = v["links"].as_array().unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0]["count"], 2);
        assert_eq!(l[0]["cross"], false);
        assert!(l[0].get("probable").is_none());
    }

    #[test]
    fn links_worktrees_through_a_shared_symbol_name_as_probable() {
        let mut a = reg("web", &[], &["src/m.rs"]);
        a.symbols = vec![
            ("src/m.rs".into(), "pushManifest".into()),
            ("src/m.rs".into(), "new".into()),
        ];
        let mut b = reg("api", &[], &["push.ts"]);
        b.symbols = vec![
            ("push.ts".into(), "pushManifest".into()),
            ("push.ts".into(), "new".into()),
        ];
        let (v, _) = build(&[a, b]);
        let l = v["links"].as_array().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0]["from"], json!({"w": 1, "path": "push.ts"}));
        assert_eq!(l[0]["to"], json!({"w": 0, "path": "src/m.rs"}));
        assert_eq!(
            (l[0]["cross"].clone(), l[0]["probable"].clone()),
            (json!(true), json!(true))
        );
    }
}
