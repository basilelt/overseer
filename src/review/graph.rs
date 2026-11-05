use super::diff::{File, Review};
use super::lang::{self, Lang};
use super::symbols::{self, Sym};
use super::triage::{self, Triage};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

pub const NODES_CAP: usize = 400;
const FILE_BYTES_CAP: usize = 2 << 20;
const GRAPHS_CAP: usize = 8;
const STOP: &[&str] = &[
    "new",
    "from",
    "into",
    "default",
    "get",
    "set",
    "len",
    "map",
    "run",
    "main",
    "init",
    "clone",
    "fmt",
    "test",
    "to_string",
    "unwrap",
    "push",
    "iter",
    "then",
    "call",
    "apply",
    "bind",
    "self",
    "this",
    "super",
    "Self",
    "Some",
    "None",
    "Ok",
    "Err",
    "String",
    "Vec",
    "Option",
    "Result",
    "Box",
    "Arc",
    "Promise",
    "Array",
    "Object",
    "Map",
    "Set",
    "JSON",
    "Math",
    "console",
];

static GRAPHS: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

#[derive(Debug, Default)]
struct Node {
    id: String,
    kind: &'static str,
    name: String,
    path: String,
    file: usize,
    line: u32,
    end: u32,
    live: bool,
    hunks: BTreeSet<usize>,
    add: usize,
    del: usize,
    kinds: BTreeMap<&'static str, usize>,
    residual: usize,
    sig: usize,
    raw: f64,
    risk: u32,
}

impl Node {
    fn changed(&self) -> usize {
        self.add + self.del
    }

    fn tag(&self) -> &'static str {
        let rank = |k: &str| {
            ["logic", "test", "unknown"]
                .iter()
                .position(|x| *x == k)
                .map_or(0, |p| 3 - p)
        };
        self.kinds
            .iter()
            .max_by_key(|(k, n)| (**n, rank(k)))
            .map_or("unknown", |(k, _)| *k)
    }
}

pub struct Sources {
    pub new: Vec<Option<Vec<String>>>,
    pub old: Vec<Option<Vec<String>>>,
}

fn blobs(wt: &str, specs: &[String]) -> Vec<Option<String>> {
    let none = || vec![None; specs.len()];
    if specs.is_empty() {
        return vec![];
    }
    let Ok(mut child) = crate::git::Watchdog::guard(
        Command::new("git")
            .arg("-C")
            .arg(wt)
            .args(["cat-file", "--batch"])
            .env("GIT_TERMINAL_PROMPT", "0"),
    )
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn() else {
        return none();
    };
    let dog = crate::git::Watchdog::start(child.id(), crate::git::TIMEOUT);
    let input: String = specs.iter().map(|s| format!("{s}\n")).collect();
    let mut stdin = child.stdin.take();
    let writer = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(input.as_bytes());
        }
    });
    let mut res = Vec::with_capacity(specs.len());
    if let Some(so) = child.stdout.take() {
        let mut r = BufReader::new(so);
        while res.len() < specs.len() {
            match next_blob(&mut r) {
                Some(b) => res.push(b),
                None => break,
            }
        }
    }
    if res.len() < specs.len() {
        let _ = child.kill();
    }
    let _ = writer.join();
    let _ = child.wait();
    if dog.finish() {
        return none();
    }
    res.resize(specs.len(), None);
    res
}

fn text(body: &[u8]) -> Option<String> {
    (body.len() <= FILE_BYTES_CAP && !body[..body.len().min(8000)].contains(&0))
        .then(|| String::from_utf8_lossy(body).into_owned())
}

fn next_blob(r: &mut impl BufRead) -> Option<Option<String>> {
    let mut head = Vec::new();
    r.take(4096).read_until(b'\n', &mut head).ok()?;
    if head.last() != Some(&b'\n') {
        return None;
    }
    let head = String::from_utf8_lossy(&head);
    let mut f = head.trim_end().rsplit(' ');
    let Some(n) = f.next().and_then(|s| s.parse::<u64>().ok()) else {
        return Some(None);
    };
    let kind = f.next();
    if kind != Some("blob") || n > FILE_BYTES_CAP as u64 {
        let skip = n.checked_add(1)?;
        let got = std::io::copy(&mut r.take(skip), &mut std::io::sink()).ok()?;
        return (got == skip).then_some(None);
    }
    let mut body = vec![0; n as usize + 1];
    r.read_exact(&mut body).ok()?;
    body.pop();
    Some(text(&body))
}

fn read_capped(p: &Path) -> Option<Vec<String>> {
    let mut b = Vec::new();
    std::fs::File::open(p)
        .ok()?
        .take(FILE_BYTES_CAP as u64 + 1)
        .read_to_end(&mut b)
        .ok()?;
    text(&b).map(|s| split(&s))
}

fn split(s: &str) -> Vec<String> {
    s.split_terminator('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect()
}

fn from_hunks(f: &File, add: bool) -> Option<Vec<String>> {
    let want = if add { "add" } else { "del" };
    let lines: Vec<String> = f
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind == want)
        .map(|l| l.text.clone())
        .collect();
    (!f.truncated && !f.binary).then_some(lines)
}

fn needs_old(f: &File, syms: &[Sym]) -> bool {
    let lg = lang::of(&f.path);
    f.hunks.iter().flat_map(|h| &h.lines).any(|l| {
        l.kind == "del"
            && lg
                .def(&l.text)
                .is_some_and(|(_, n)| !syms.iter().any(|s| s.name == n))
    })
}

fn fetch(rv: &Review, want: Vec<(usize, bool, &str)>, out: &mut [Option<Vec<String>>]) {
    let mut by_dir: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    for (i, old, path) in want {
        if let Some((dir, rev, rest)) = super::diff::blob_at(rv, old, path) {
            by_dir
                .entry(dir)
                .or_default()
                .push((i, format!("{rev}:{rest}")));
        }
    }
    for (dir, want) in by_dir {
        let specs: Vec<String> = want.iter().map(|w| w.1.clone()).collect();
        for ((i, _), b) in want.iter().zip(blobs(&dir, &specs)) {
            out[*i] = b.as_deref().map(split);
        }
    }
}

pub fn sources(rv: &Review, live: bool) -> Sources {
    let n = rv.files.len();
    let mut new: Vec<Option<Vec<String>>> = vec![None; n];
    let mut want = vec![];
    for (i, f) in rv.files.iter().enumerate() {
        if f.truncated || f.binary || f.hunks.is_empty() || lang::of(&f.path) == Lang::Other {
            continue;
        }
        match f.status {
            "deleted" => {}
            "added" | "untracked" => new[i] = from_hunks(f, true),
            _ if live => new[i] = read_capped(&Path::new(&rv.worktree).join(&f.path)),
            _ => want.push((i, false, f.path.as_str())),
        }
    }
    fetch(rv, want, &mut new);
    let mut old: Vec<Option<Vec<String>>> = vec![None; n];
    let mut want = vec![];
    for (i, f) in rv.files.iter().enumerate() {
        if f.truncated || f.binary || f.status == "submodule" || lang::of(&f.path) == Lang::Other {
            continue;
        }
        if f.status == "deleted" {
            old[i] = from_hunks(f, false);
            continue;
        }
        let lg = lang::of(&f.path);
        let lines = new[i].as_deref().unwrap_or_default();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        if needs_old(f, &symbols::scan(lg, &f.path, &refs)) {
            want.push((i, true, f.old_path.as_str()));
        }
    }
    fetch(rv, want, &mut old);
    Sources { new, old }
}

fn line_kinds(t: &Triage, fi: usize, hi: usize, len: usize) -> Vec<(&'static str, bool)> {
    let mut v = vec![("", false); len];
    for g in t.files[fi].hunks.get(hi).into_iter().flatten() {
        for x in v.get_mut(g.from..g.to.min(len)).into_iter().flatten() {
            *x = (g.kind, g.sig);
        }
    }
    v
}

fn followed(line: &str, at: usize) -> bool {
    let rest = line[at..].trim_start();
    rest.starts_with('(')
        || rest.starts_with("::")
        || rest.starts_with('.')
        || rest.starts_with('<')
}

fn idents(line: &str) -> impl Iterator<Item = (&str, usize)> {
    let mut start = None;
    let mut out = vec![];
    for (i, c) in line
        .char_indices()
        .chain(std::iter::once((line.len(), ' ')))
    {
        match (lang::is_ident(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((&line[s..i], i));
                start = None;
            }
            _ => {}
        }
    }
    out.into_iter()
}

fn dir(p: &str) -> &str {
    p.rsplit_once('/').map_or("", |(d, _)| d)
}

fn normal(p: &str) -> String {
    let mut out: Vec<&str> = vec![];
    for s in p.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

fn quoted(line: &str) -> Option<&str> {
    let i = line.find(['\'', '"'])?;
    let q = &line[i..i + 1];
    let rest = &line[i + 1..];
    Some(&rest[..rest.find(q)?])
}

fn resolve(from: &str, line: &str, paths: &[&str]) -> Vec<usize> {
    let lg = lang::of(from);
    let t = line.trim();
    let mut cands: Vec<String> = vec![];
    let mut suffix: Vec<String> = vec![];
    match lg {
        Lang::Ts | Lang::Svelte => {
            let Some(spec) = quoted(t) else { return vec![] };
            let base = if spec.starts_with('.') {
                normal(&format!("{}/{spec}", dir(from)))
            } else if let Some(r) = spec.strip_prefix("$lib/") {
                suffix.push(format!("lib/{r}"));
                return paths
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| {
                        let stem = p.rsplit_once('.').map_or(**p, |(s, _)| s);
                        let stem = stem.strip_suffix(".svelte").unwrap_or(stem);
                        stem.ends_with(&suffix[0]) || p.ends_with(&suffix[0])
                    })
                    .map(|(i, _)| i)
                    .collect();
            } else {
                return vec![];
            };
            for ext in [
                "",
                ".ts",
                ".js",
                ".svelte",
                ".svelte.ts",
                ".tsx",
                ".jsx",
                ".mjs",
                "/index.ts",
                "/index.js",
            ] {
                cands.push(format!("{base}{ext}"));
            }
        }
        Lang::Rust => {
            let body = t.trim_start_matches("pub ").trim_end_matches(';');
            if let Some(m) = body.strip_prefix("mod ") {
                let m = m.trim();
                let stem = from
                    .rsplit('/')
                    .next()
                    .unwrap_or(from)
                    .trim_end_matches(".rs");
                let d = if matches!(stem, "mod" | "lib" | "main") {
                    dir(from).to_string()
                } else {
                    normal(&format!("{}/{stem}", dir(from)))
                };
                cands.push(normal(&format!("{d}/{m}.rs")));
                cands.push(normal(&format!("{d}/{m}/mod.rs")));
            } else if let Some(u) = body.strip_prefix("use ") {
                let segs: Vec<&str> = u
                    .split("::")
                    .map(str::trim)
                    .take_while(|s| lang::ident(s).is_some_and(|n| n.len() == s.len()))
                    .collect();
                let segs: Vec<&str> = segs
                    .into_iter()
                    .skip_while(|s| matches!(*s, "crate" | "super" | "self"))
                    .collect();
                for k in (1..=segs.len()).rev() {
                    let j = segs[..k].join("/");
                    suffix.push(format!("/{j}.rs"));
                    suffix.push(format!("/{j}/mod.rs"));
                }
            }
        }
        Lang::Dart => {
            let Some(spec) = quoted(t) else { return vec![] };
            if let Some(p) = spec.strip_prefix("package:") {
                let (pkg, rest) = p.split_once('/').unwrap_or((p, ""));
                suffix.push(format!("/{pkg}/lib/{rest}"));
                suffix.push(format!("/lib/{rest}"));
            } else if !spec.contains(':') {
                cands.push(normal(&format!("{}/{spec}", dir(from))));
            }
        }
        Lang::Python => {
            let m = t
                .strip_prefix("from ")
                .or_else(|| t.strip_prefix("import "))
                .and_then(|r| r.split_whitespace().next())
                .unwrap_or("");
            let dots = m.len() - m.trim_start_matches('.').len();
            let rel = m.trim_start_matches('.').replace('.', "/");
            if dots > 0 {
                let mut d = dir(from).to_string();
                for _ in 1..dots {
                    d = dir(&d).to_string();
                }
                cands.push(normal(&format!("{d}/{rel}.py")));
                cands.push(normal(&format!("{d}/{rel}/__init__.py")));
            } else if !rel.is_empty() {
                suffix.push(format!("{rel}.py"));
                suffix.push(format!("{rel}/__init__.py"));
            }
        }
        _ => {}
    }
    for c in &cands {
        if let Some(i) = paths.iter().position(|p| p == c) {
            return vec![i];
        }
    }
    for s in &suffix {
        let hits: Vec<usize> = paths
            .iter()
            .enumerate()
            .filter(|(_, p)| p.ends_with(s.as_str()) || s.strip_prefix('/') == Some(**p))
            .map(|(i, _)| i)
            .collect();
        if !hits.is_empty() {
            return hits;
        }
    }
    vec![]
}

#[derive(Default)]
struct Nodes {
    list: Vec<Node>,
    index: HashMap<String, usize>,
}

impl Nodes {
    fn get(&mut self, id: String, mk: impl FnOnce() -> Node) -> usize {
        let list = &mut self.list;
        *self.index.entry(id).or_insert_with(|| {
            list.push(mk());
            list.len() - 1
        })
    }

    fn file(&mut self, f: &File, fi: usize) -> usize {
        self.get(f.path.clone(), || Node {
            id: f.path.clone(),
            kind: "file",
            name: f.path.rsplit('/').next().unwrap_or(&f.path).to_string(),
            path: f.path.clone(),
            file: fi,
            line: 1,
            end: 1,
            live: f.status != "deleted",
            ..Default::default()
        })
    }

    fn sym(&mut self, f: &File, fi: usize, syms: &[Sym], i: usize, live: bool) -> usize {
        let id = format!("{}::{}", f.path, symbols::qualified(syms, i));
        let s = &syms[i];
        self.get(id.clone(), || Node {
            id,
            kind: s.kind,
            name: s.name.clone(),
            path: f.path.clone(),
            file: fi,
            line: s.line,
            end: s.end,
            live,
            ..Default::default()
        })
    }
}

pub fn build(rv: &Review, t: &Triage, src: &Sources) -> Value {
    let mut ns = Nodes::default();
    for (fi, f) in rv.files.iter().enumerate() {
        if f.status == "submodule" {
            continue;
        }
        let lg = lang::of(&f.path);
        let scan = |v: &Option<Vec<String>>| -> Vec<Sym> {
            let lines: Vec<&str> = v.iter().flatten().map(String::as_str).collect();
            symbols::scan(lg, &f.path, &lines)
        };
        let new = scan(&src.new[fi]);
        let old = scan(&src.old[fi]);
        if f.hunks.is_empty() {
            let k = ns.file(f, fi);
            let n = &mut ns.list[k];
            n.add = f.additions;
            n.del = f.deletions;
            let kind = if f.truncated || f.binary {
                "unknown"
            } else {
                "rename"
            };
            n.kinds.insert(kind, 1);
            n.residual = t.files[fi].residual;
        }
        for (hi, h) in f.hunks.iter().enumerate() {
            let kinds = line_kinds(t, fi, hi, h.lines.len());
            for (li, l) in h.lines.iter().enumerate() {
                if l.kind == "ctx" || l.text.trim().is_empty() {
                    continue;
                }
                let at_new = l.new.or_else(|| {
                    h.lines[li..]
                        .iter()
                        .find_map(|x| x.new)
                        .or(Some(h.new_start + h.new_lines))
                });
                let gone = (l.kind == "del")
                    .then(|| l.old.and_then(|o| symbols::innermost(&old, o)))
                    .flatten()
                    .filter(|&i| {
                        !new.iter()
                            .any(|s| s.name == old[i].name && s.kind == old[i].kind)
                    });
                let k = match (gone, at_new.and_then(|n| symbols::innermost(&new, n))) {
                    (Some(i), _) => ns.sym(f, fi, &old, i, false),
                    (None, Some(i)) => ns.sym(f, fi, &new, i, true),
                    (None, None) => ns.file(f, fi),
                };
                let n = &mut ns.list[k];
                n.hunks.insert(hi);
                if l.kind == "add" {
                    n.add += 1;
                } else {
                    n.del += 1;
                }
                let (kind, sig) = kinds[li];
                let kind = if kind.is_empty() { "logic" } else { kind };
                *n.kinds.entry(kind).or_default() += 1;
                if matches!(kind, "logic" | "test" | "unknown") {
                    n.residual += 1;
                }
                if sig {
                    n.sig += 1;
                }
            }
        }
    }
    let mut nodes = ns.list;
    for n in &mut nodes {
        let f = &rv.files[n.file];
        let w = if n.kinds.contains_key("generated") {
            0.0
        } else if lang::test_path(&f.path) {
            0.2
        } else {
            1.0
        };
        n.raw = n.residual as f64 * (1.0 + 0.5 * n.sig as f64) * w;
    }
    let total = nodes.len();
    nodes.sort_by(|a, b| {
        b.raw
            .total_cmp(&a.raw)
            .then(b.changed().cmp(&a.changed()))
            .then(a.id.cmp(&b.id))
    });
    nodes.truncate(NODES_CAP);
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let max = nodes.iter().map(|n| n.raw).fold(0.0, f64::max);
    for n in &mut nodes {
        n.risk = if max > 0.0 {
            (100.0 * n.raw / max).ceil() as u32
        } else {
            0
        };
    }
    let id_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, n) in nodes.iter().enumerate() {
        if n.kind != "file" && n.name.len() >= 3 && !STOP.contains(&n.name.as_str()) {
            by_name.entry(n.name.as_str()).or_default().push(i);
        }
    }
    let mut edges: BTreeSet<(usize, usize, &'static str)> = BTreeSet::new();
    for (a, n) in nodes.iter().enumerate() {
        if !n.live || n.kind == "file" {
            continue;
        }
        let Some(lines) = src.new[n.file].as_ref() else {
            continue;
        };
        let s = (n.line as usize).saturating_sub(1).min(lines.len());
        let e = (n.end as usize).min(lines.len()).max(s);
        for line in &lines[s..e] {
            for (tok, end) in idents(line) {
                let Some(targets) = by_name.get(tok) else {
                    continue;
                };
                if !followed(line, end) {
                    continue;
                }
                let local: Vec<usize> = targets
                    .iter()
                    .copied()
                    .filter(|&b| nodes[b].path == n.path)
                    .collect();
                let pick = if !local.is_empty() {
                    local
                } else {
                    targets.clone()
                };
                if pick.len() <= 2 {
                    for b in pick.into_iter().filter(|&b| b != a) {
                        edges.insert((a, b, "uses"));
                    }
                }
            }
        }
    }
    let paths: Vec<&str> = rv.files.iter().map(|f| f.path.as_str()).collect();
    let rep = |fi: usize| -> Option<usize> {
        id_of.get(rv.files[fi].path.as_str()).copied().or_else(|| {
            nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.file == fi)
                .max_by(|(_, x), (_, y)| x.risk.cmp(&y.risk).then(y.id.cmp(&x.id)))
                .map(|(i, _)| i)
        })
    };
    for (fi, f) in rv.files.iter().enumerate() {
        let lg = lang::of(&f.path);
        let Some(lines) = src.new[fi].as_ref() else {
            continue;
        };
        let Some(a) = rep(fi) else {
            continue;
        };
        for l in lines.iter().filter(|l| lg.is_import(l)) {
            for g in resolve(&f.path, l, &paths) {
                if g == fi {
                    continue;
                }
                if let Some(b) = rep(g) {
                    edges.insert((a, b, "imports"));
                }
            }
        }
    }
    let order = order(&nodes, &edges);
    json!({
        "revision": rv.revision(),
        "truncated": total > nodes.len(),
        "nodes": nodes.iter().map(|n| json!({
            "id": n.id, "kind": n.kind, "name": n.name, "path": n.path, "line": n.line, "endLine": n.end,
            "hunks": n.hunks.iter().map(|h| json!({"path": n.path, "hunk": h})).collect::<Vec<_>>(),
            "add": n.add, "del": n.del, "tag": n.tag(), "risk": n.risk,
        })).collect::<Vec<_>>(),
        "edges": edges.iter().map(|(a, b, k)| json!({"from": nodes[*a].id, "to": nodes[*b].id, "kind": k})).collect::<Vec<_>>(),
        "order": order.iter().map(|&i| nodes[i].id.as_str()).collect::<Vec<_>>(),
    })
}

fn order(nodes: &[Node], edges: &BTreeSet<(usize, usize, &'static str)>) -> Vec<usize> {
    let n = nodes.len();
    let mut deps: BTreeSet<(usize, usize)> = edges
        .iter()
        .filter(|e| e.0 != e.1)
        .map(|e| (e.0, e.1))
        .collect();
    let mut indeg = vec![0usize; n];
    for &(a, _) in &deps {
        indeg[a] += 1;
    }
    let key = |i: usize| {
        (
            std::cmp::Reverse(nodes[i].risk),
            std::cmp::Reverse(nodes[i].changed()),
            nodes[i].id.as_str(),
        )
    };
    let mut done = vec![false; n];
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let ready = (0..n)
            .filter(|&i| !done[i] && indeg[i] == 0)
            .min_by_key(|&i| key(i));
        let Some(i) = ready else {
            let cut = deps
                .iter()
                .filter(|(a, b)| !done[*a] && !done[*b])
                .min_by_key(|(a, b)| {
                    (
                        nodes[*a].risk.min(nodes[*b].risk),
                        nodes[*a].risk + nodes[*b].risk,
                        nodes[*a].id.as_str(),
                        nodes[*b].id.as_str(),
                    )
                })
                .copied();
            match cut {
                Some(e) => {
                    deps.remove(&e);
                    indeg[e.0] -= 1;
                }
                None => break,
            }
            continue;
        };
        done[i] = true;
        out.push(i);
        let users: Vec<(usize, usize)> = deps.iter().filter(|(_, b)| *b == i).copied().collect();
        for e in users {
            deps.remove(&e);
            indeg[e.0] -= 1;
        }
    }
    out
}

pub fn cached(key: &str, make: impl FnOnce() -> Value) -> Value {
    if let Some(v) = GRAPHS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(key))
    {
        return v.clone();
    }
    let v = make();
    let mut g = GRAPHS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() >= GRAPHS_CAP {
        m.clear();
    }
    m.insert(key.to_string(), v.clone());
    v
}

pub fn of_review(rv: &Review, live: bool) -> Value {
    let key = format!(
        "{}\0{}\0{}\0{}",
        rv.worktree,
        rv.branch.as_deref().unwrap_or(""),
        rv.merge_base.as_deref().unwrap_or(""),
        rv.revision()
    );
    cached(&key, || {
        let t = triage::analyze(&rv.files);
        build(rv, &t, &sources(rv, live))
    })
}

pub fn route(ctx: &crate::Ctx, t: &super::Target, base: Option<&str>) -> super::Res<Value> {
    super::watch::touch(&t.worktree);
    let rv = match &t.branch {
        Some(b) => super::diff::assemble_branch(&t.worktree, b, base),
        None => {
            let hint = base
                .map(str::to_string)
                .or_else(|| super::hint_of(ctx, &t.worktree).0);
            super::diff::assemble(&t.worktree, hint.as_deref())
        }
    }
    .map_err(|e| (409, e))?;
    Ok(of_review(&rv, t.branch.is_none()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::diff::parse_patch;

    fn file(path: &str, status: &'static str, body: &str) -> File {
        let p = format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{body}");
        let (_, hunks) = parse_patch(&p).remove(0);
        File {
            path: path.into(),
            old_path: path.into(),
            status,
            hunks,
            ..Default::default()
        }
    }

    fn lines(s: &str) -> Option<Vec<String>> {
        Some(s.lines().map(str::to_string).collect())
    }

    fn ids(v: &Value, k: &str) -> Vec<String> {
        v[k].as_array()
            .unwrap()
            .iter()
            .map(|x| {
                x.as_str()
                    .map_or_else(|| x["id"].as_str().unwrap().to_string(), str::to_string)
            })
            .collect()
    }

    #[test]
    fn maps_hunks_to_symbols_with_uses_imports_and_deleted_definitions() {
        let a = file(
            "src/a.rs",
            "added",
            "@@ -0,0 +1,7 @@\n+fn helper() -> u32 {\n+    2\n+}\n+\n+fn parse() -> u32 {\n+    helper() + 1\n+}\n",
        );
        let b = file(
            "src/b.rs",
            "modified",
            "@@ -1,8 +1,5 @@\n-use crate::x;\n+use crate::a;\n \n-fn old_thing() {\n-    x();\n-}\n-\n fn run() {\n+    parse();\n }\n",
        );
        let rv = Review {
            files: vec![a, b],
            ..Default::default()
        };
        let src = Sources {
            new: vec![
                lines(
                    "fn helper() -> u32 {\n    2\n}\n\nfn parse() -> u32 {\n    helper() + 1\n}\n",
                ),
                lines("use crate::a;\n\nfn run() {\n    parse();\n}\n"),
            ],
            old: vec![
                None,
                lines("use crate::x;\n\nfn old_thing() {\n    x();\n}\n\nfn run() {\n}\n"),
            ],
        };
        let g = build(&rv, &triage::analyze(&rv.files), &src);
        assert_eq!(
            ids(&g, "nodes"),
            [
                "src/a.rs::helper",
                "src/a.rs::parse",
                "src/b.rs",
                "src/b.rs::old_thing",
                "src/b.rs::run"
            ]
        );
        let edges: Vec<(String, String, String)> = g["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["from"].as_str().unwrap().into(),
                    e["to"].as_str().unwrap().into(),
                    e["kind"].as_str().unwrap().into(),
                )
            })
            .collect();
        let e = |a: &str, b: &str, k: &str| (a.to_string(), b.to_string(), k.to_string());
        assert_eq!(
            edges,
            [
                e("src/a.rs::parse", "src/a.rs::helper", "uses"),
                e("src/b.rs", "src/a.rs::helper", "imports"),
                e("src/b.rs::run", "src/a.rs::parse", "uses"),
            ]
        );
        assert_eq!(
            ids(&g, "order"),
            [
                "src/a.rs::helper",
                "src/a.rs::parse",
                "src/b.rs::old_thing",
                "src/b.rs",
                "src/b.rs::run"
            ]
        );
        let gone = &g["nodes"][3];
        assert_eq!(
            (
                gone["line"].as_u64(),
                gone["del"].as_u64(),
                gone["tag"].as_str()
            ),
            (Some(3), Some(3), Some("logic"))
        );
        assert_eq!(
            g["nodes"][0]["hunks"],
            json!([{"path": "src/a.rs", "hunk": 0}])
        );
        assert_eq!(g["truncated"], json!(false));
    }

    #[test]
    fn links_dart_files_through_package_and_relative_imports() {
        let a = file(
            "app/lib/src/price.dart",
            "added",
            "@@ -0,0 +1,3 @@\n+int total(int a) {\n+  return a * 2;\n+}\n",
        );
        let b = file(
            "app/lib/main.dart",
            "modified",
            "@@ -1,4 +1,5 @@\n+import 'package:app/src/price.dart';\n \n void main() {\n+  total(1);\n }\n",
        );
        let c = file(
            "app/lib/src/cart.dart",
            "modified",
            "@@ -1,3 +1,4 @@\n+import 'price.dart';\n class Cart {\n+  int sum() => total(3);\n }\n",
        );
        let rv = Review {
            files: vec![a, b, c],
            ..Default::default()
        };
        let src = Sources {
            new: vec![
                lines("int total(int a) {\n  return a * 2;\n}\n"),
                lines("import 'package:app/src/price.dart';\n\nvoid main() {\n  total(1);\n}\n"),
                lines("import 'price.dart';\nclass Cart {\n  int sum() => total(3);\n}\n"),
            ],
            old: vec![None, None, None],
        };
        let g = build(&rv, &triage::analyze(&rv.files), &src);
        let edges: Vec<(String, String, String)> = g["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["from"].as_str().unwrap().into(),
                    e["to"].as_str().unwrap().into(),
                    e["kind"].as_str().unwrap().into(),
                )
            })
            .collect();
        let e = |a: &str, b: &str, k: &str| (a.to_string(), b.to_string(), k.to_string());
        for want in [
            e(
                "app/lib/main.dart",
                "app/lib/src/price.dart::total",
                "imports",
            ),
            e(
                "app/lib/main.dart::main",
                "app/lib/src/price.dart::total",
                "uses",
            ),
            e(
                "app/lib/src/cart.dart",
                "app/lib/src/price.dart::total",
                "imports",
            ),
            e(
                "app/lib/src/cart.dart::Cart::sum",
                "app/lib/src/price.dart::total",
                "uses",
            ),
        ] {
            assert!(edges.contains(&want), "{want:?} not in {edges:?}");
        }
    }

    fn node(id: &str, risk: u32) -> Node {
        Node {
            id: id.into(),
            name: id.into(),
            risk,
            add: 1,
            ..Default::default()
        }
    }

    #[test]
    fn order_breaks_cycles_at_the_lowest_risk_edge_deterministically() {
        let nodes = vec![node("a", 90), node("b", 10), node("c", 50), node("d", 70)];
        let edges: BTreeSet<(usize, usize, &str)> = [
            (0, 1, "uses"),
            (1, 2, "uses"),
            (2, 0, "uses"),
            (3, 0, "uses"),
            (3, 3, "uses"),
        ]
        .into_iter()
        .collect();
        let names = |v: Vec<usize>| {
            v.into_iter()
                .map(|i| nodes[i].id.clone())
                .collect::<Vec<_>>()
        };
        let first = names(order(&nodes, &edges));
        assert_eq!(first, ["b", "a", "d", "c"]);
        for _ in 0..5 {
            assert_eq!(names(order(&nodes, &edges)), first);
        }
    }

    #[test]
    fn caps_nodes_and_keeps_the_riskiest() {
        let body: String = (0..NODES_CAP + 5)
            .map(|i| format!("+fn f{i}() {{ {} }}\n", "x();".repeat(1 + i % 3)))
            .collect();
        let f = file(
            "src/many.rs",
            "added",
            &format!("@@ -0,0 +1,{} @@\n{body}", NODES_CAP + 5),
        );
        let src = Sources {
            new: vec![from_hunks(&f, true)],
            old: vec![None],
        };
        let rv = Review {
            files: vec![f],
            ..Default::default()
        };
        let g = build(&rv, &triage::analyze(&rv.files), &src);
        assert_eq!(g["nodes"].as_array().unwrap().len(), NODES_CAP);
        assert_eq!(g["order"].as_array().unwrap().len(), NODES_CAP);
        assert_eq!(g["truncated"], json!(true));
    }

    #[test]
    fn skips_submodule_entries() {
        let sub = File {
            path: "vendor/lib.rs".into(),
            old_path: "vendor/lib.rs".into(),
            status: "submodule",
            submodule: Some((Some("c1".into()), Some("c2".into()))),
            ..Default::default()
        };
        let rv = Review {
            files: vec![sub],
            ..Default::default()
        };
        let src = sources(&rv, false);
        assert!(src.new[0].is_none() && src.old[0].is_none());
        let g = build(&rv, &triage::analyze(&rv.files), &src);
        assert!(g["nodes"].as_array().unwrap().is_empty());
    }

    #[test]
    fn reads_blobs_in_one_batch() {
        let d = std::env::temp_dir().join(format!("sb-graph-blobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new("git")
                .current_dir(&d)
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
        };
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(d.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(d.join("b.bin"), [0u8, 1, 2]).unwrap();
        std::fs::write(d.join("big.rs"), "x\n".repeat(FILE_BYTES_CAP / 2 + 1)).unwrap();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/c d.rs"), "fn c() {}\n").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-q", "-m", "one"]);
        let wt = d.to_string_lossy().into_owned();
        let got = blobs(
            &wt,
            &[
                "HEAD:a.rs".into(),
                "HEAD:nope.rs".into(),
                "HEAD:b.bin".into(),
                "HEAD:big.rs".into(),
                "HEAD:sub".into(),
                "HEAD:x y 7 z.rs".into(),
                "HEAD:sub/c d.rs".into(),
                "HEAD:a.rs".into(),
            ],
        );
        assert_eq!(
            got,
            [
                Some("fn a() {}\n".into()),
                None,
                None,
                None,
                None,
                None,
                Some("fn c() {}\n".into()),
                Some("fn a() {}\n".into())
            ]
        );
        assert!(blobs(&wt, &[]).is_empty());
        assert!(read_capped(&d.join("big.rs")).is_none());
        assert_eq!(read_capped(&d.join("a.rs")), Some(vec!["fn a() {}".into()]));
        std::fs::remove_dir_all(d).unwrap();
    }
}
