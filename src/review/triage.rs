use super::diff::File;
use super::lang::{self, Lang};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const LOW_RISK: &[&str] = &[
    "move",
    "format",
    "imports",
    "ident",
    "rename",
    "generated",
    "comment",
];
const RESIDUAL: &[&str] = &["logic", "test", "unknown"];
const CACHE_CAP: usize = 512;
const CACHE_LINES: usize = 100_000;

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub kind: &'static str,
    pub from: usize,
    pub to: usize,
    pub refer: Option<(String, usize)>,
    pub sig: bool,
}

impl Tag {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"kind": self.kind, "from": self.from, "to": self.to});
        if let Some((path, hunk)) = &self.refer {
            v["ref"] = json!({"path": path, "hunk": hunk});
        }
        if self.sig {
            v["sig"] = json!(true);
        }
        v
    }
}

#[derive(Clone, Debug, Default)]
pub struct FileTriage {
    pub risk: u32,
    pub residual: usize,
    pub sig: usize,
    pub hunks: Vec<Vec<Tag>>,
}

impl FileTriage {
    pub fn annotate(&self, mut v: Value) -> Value {
        v["risk"] = json!(self.risk);
        v["residual"] = json!(self.residual);
        if let Some(hs) = v["hunks"].as_array_mut() {
            for (h, tags) in hs.iter_mut().zip(&self.hunks) {
                h["tags"] = tags.iter().map(Tag::to_json).collect();
            }
        }
        v
    }
}

#[derive(Clone, Debug, Default)]
pub struct Triage {
    pub files: Vec<FileTriage>,
    pub changed: usize,
    pub by_kind: BTreeMap<&'static str, usize>,
    pub low_risk_pct: u32,
}

impl Triage {
    pub fn to_json(&self) -> Value {
        json!({"changed": self.changed, "byKind": self.by_kind, "lowRiskPct": self.low_risk_pct})
    }
}

#[derive(Clone, Debug, Default)]
struct L {
    change: bool,
    add: bool,
    block: usize,
    norm: u64,
    alnum: usize,
    early: Option<&'static str>,
    late: &'static str,
    sig: bool,
    pair: Option<Box<(String, String)>>,
}

#[derive(Debug, Default)]
struct Local {
    generated: bool,
    hunks: Vec<Vec<L>>,
}

type Cache = HashMap<String, Arc<Local>>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn norm(s: &str) -> (u64, usize) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let (mut any, mut alnum) = (false, 0);
    for b in s.bytes().filter(|b| !b.is_ascii_whitespace()) {
        any = true;
        alnum += (b.is_ascii_alphanumeric() || b >= 0x80) as usize;
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    (if any { h } else { 0 }, alnum)
}

fn tokens(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c.is_whitespace() {
            continue;
        }
        let mut end = i + c.len_utf8();
        if lang::is_ident(c) {
            while let Some(&(j, d)) = it.peek() {
                if !lang::is_ident(d) {
                    break;
                }
                end = j + d.len_utf8();
                it.next();
            }
        }
        out.push(&s[i..end]);
    }
    out
}

fn renamed_ident(old: &str, new: &str) -> Option<(String, String)> {
    let (a, b) = (tokens(old), tokens(new));
    if a.len() != b.len() {
        return None;
    }
    let mut diff = a.iter().zip(&b).filter(|(x, y)| x != y);
    let (x, y) = diff.next()?;
    let word = |t: &str| lang::ident(t).is_some_and(|n| n.len() == t.len());
    (diff.next().is_none() && word(x) && word(y)).then(|| (x.to_string(), y.to_string()))
}

fn local(f: &File) -> Local {
    let lg = lang::of(&f.path);
    let head: Vec<&str> = f
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .filter(|l| l.new.or(l.old).is_some_and(|n| n <= 5))
        .map(|l| l.text.as_str())
        .collect();
    let generated = lang::generated(&f.path, &head);
    let test = lang::test_path(&f.path);
    let mut hunks = vec![];
    let mut block = 0;
    for h in &f.hunks {
        let mut ls: Vec<L> = Vec::with_capacity(h.lines.len());
        let mut prev_ctx = true;
        for l in &h.lines {
            let change = l.kind != "ctx";
            if change && prev_ctx {
                block += 1;
            }
            prev_ctx = !change;
            let (n, alnum) = norm(&l.text);
            let late = if !change {
                ""
            } else if generated {
                "generated"
            } else if test {
                "test"
            } else if lg == Lang::Doc || lg.is_comment(&l.text) {
                "comment"
            } else {
                "logic"
            };
            ls.push(L {
                change,
                add: l.kind == "add",
                block,
                norm: n,
                alnum,
                early: (change && n == 0).then_some("format"),
                late,
                sig: late == "logic" && lg.def(&l.text).is_some(),
                pair: None,
            });
        }
        let changed: Vec<usize> = (0..ls.len()).filter(|&i| ls[i].change).collect();
        let only_imports = !changed.is_empty()
            && changed
                .iter()
                .all(|&i| ls[i].norm == 0 || lg.is_import(&h.lines[i].text));
        if only_imports {
            let set = |add: bool| {
                let mut v: Vec<u64> = changed
                    .iter()
                    .filter(|&&i| ls[i].add == add && ls[i].norm != 0)
                    .map(|&i| ls[i].norm)
                    .collect();
                v.sort_unstable();
                v
            };
            let dels = set(false);
            if !dels.is_empty() && dels == set(true) {
                for &i in &changed {
                    ls[i].early = Some("imports");
                }
            }
        }
        let mut i = 0;
        while i < ls.len() {
            if !ls[i].change {
                i += 1;
                continue;
            }
            let start = i;
            while i < ls.len() && ls[i].change {
                i += 1;
            }
            let dels: Vec<usize> = (start..i).filter(|&k| !ls[k].add).collect();
            let adds: Vec<usize> = (start..i).filter(|&k| ls[k].add).collect();
            let joined = |v: &[usize]| -> String {
                v.iter()
                    .flat_map(|&k| h.lines[k].text.chars())
                    .filter(|c| !c.is_whitespace())
                    .collect()
            };
            if !dels.is_empty() && !adds.is_empty() && joined(&dels) == joined(&adds) {
                for l in &mut ls[start..i] {
                    l.early.get_or_insert("format");
                }
                continue;
            }
            if dels.len() == adds.len() {
                for (&d, &a) in dels.iter().zip(&adds) {
                    if ls[d].norm == ls[a].norm {
                        ls[d].early.get_or_insert("format");
                        ls[a].early.get_or_insert("format");
                    } else if let Some(p) = renamed_ident(&h.lines[d].text, &h.lines[a].text) {
                        ls[d].pair = Some(Box::new(p.clone()));
                        ls[a].pair = Some(Box::new(p));
                    }
                }
            }
        }
        hunks.push(ls);
    }
    Local { generated, hunks }
}

fn locals(files: &[File]) -> Vec<Arc<Local>> {
    let keys: Vec<String> = files.iter().map(File::hash).collect();
    let hits: Vec<Option<Arc<Local>>> = {
        let g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        keys.iter()
            .map(|k| g.as_ref().and_then(|m| m.get(k).cloned()))
            .collect()
    };
    let out: Vec<Arc<Local>> = files
        .iter()
        .zip(hits)
        .map(|(f, h)| h.unwrap_or_else(|| Arc::new(local(f))))
        .collect();
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    let fresh: Vec<(&String, &Arc<Local>)> = keys
        .iter()
        .zip(&out)
        .filter(|(k, _)| !m.contains_key(*k))
        .collect();
    let need = fresh.iter().map(|(_, l)| l.weight()).sum::<usize>();
    let mut total = m.values().map(|l| l.weight()).sum::<usize>();
    if m.len() + fresh.len() > CACHE_CAP || total + need > CACHE_LINES {
        let live: HashSet<&String> = keys.iter().collect();
        let stale: Vec<String> = m.keys().filter(|k| !live.contains(k)).cloned().collect();
        for k in stale {
            if m.len() + fresh.len() <= CACHE_CAP && total + need <= CACHE_LINES {
                break;
            }
            if let Some(l) = m.remove(&k) {
                total -= l.weight();
            }
        }
    }
    for (k, l) in fresh {
        let w = l.weight();
        if m.len() < CACHE_CAP && total + w <= CACHE_LINES {
            m.insert(k.clone(), l.clone());
            total += w;
        }
    }
    out
}

impl Local {
    fn weight(&self) -> usize {
        1 + self.hunks.iter().map(Vec::len).sum::<usize>()
    }
}

#[cfg(test)]
fn cache_len() -> usize {
    CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or(0, HashMap::len)
}

type Loc = (usize, usize, usize);

fn moves(ls: &[Arc<Local>]) -> HashMap<Loc, Loc> {
    let neutral = |l: &L| l.norm == 0 || l.alnum == 0;
    let mut seen: [HashMap<u64, Vec<(Loc, usize)>>; 2] = Default::default();
    for (fi, f) in ls.iter().enumerate().filter(|(_, f)| !f.generated) {
        for (hi, h) in f.hunks.iter().enumerate() {
            for (li, l) in h
                .iter()
                .enumerate()
                .filter(|(_, l)| l.change && !neutral(l))
            {
                let v = seen[l.add as usize].entry(l.norm).or_default();
                if v.len() < 2 && v.iter().all(|&((f2, _, _), b)| (f2, b) != (fi, l.block)) {
                    v.push(((fi, hi, li), l.block));
                }
            }
        }
    }
    let mut out = HashMap::new();
    for (fi, f) in ls.iter().enumerate() {
        for (hi, h) in f.hunks.iter().enumerate() {
            let other = |li: usize| -> Option<Loc> {
                let l = &h[li];
                seen[!l.add as usize]
                    .get(&l.norm)?
                    .iter()
                    .find(|&&((f2, _, _), b)| (f2, b) != (fi, l.block))
                    .map(|&(loc, _)| loc)
            };
            let mut close = |run: &mut Option<(usize, usize, usize, usize, Loc)>, upto: usize| {
                if let Some((start, mut last, lines, alnum, loc)) = run.take() {
                    if lines >= 3 || alnum >= 20 {
                        while last + 1 < upto && neutral(&h[last + 1]) {
                            last += 1;
                        }
                        for k in start..=last {
                            out.insert((fi, hi, k), loc);
                        }
                    }
                }
            };
            let mut run = None;
            let mut seg_end = 0;
            for k in 0..h.len() {
                let l = &h[k];
                if k >= seg_end {
                    close(&mut run, seg_end);
                    seg_end = k + 1;
                    while seg_end < h.len()
                        && h[seg_end].change == l.change
                        && h[seg_end].add == l.add
                    {
                        seg_end += 1;
                    }
                }
                if !l.change {
                    continue;
                }
                match (neutral(l), other(k)) {
                    (true, _) => {}
                    (false, Some(loc)) => {
                        let r = run.get_or_insert((k, k, 0, 0, loc));
                        r.1 = k;
                        r.2 += 1;
                        r.3 += l.alnum;
                    }
                    (false, None) => close(&mut run, seg_end),
                }
            }
            close(&mut run, seg_end);
        }
    }
    out
}

pub fn analyze(files: &[File]) -> Triage {
    let ls = locals(files);
    let moved = moves(&ls);
    let mut pairs: HashMap<&(String, String), Vec<(usize, usize)>> = HashMap::new();
    for (fi, f) in ls.iter().enumerate() {
        for (hi, h) in f.hunks.iter().enumerate() {
            for p in h.iter().filter_map(|l| l.pair.as_ref()) {
                let v = pairs.entry(p).or_default();
                if !v.contains(&(fi, hi)) {
                    v.push((fi, hi));
                }
            }
        }
    }
    let mut t = Triage::default();
    let mut raw = vec![];
    for (fi, (f, l)) in files.iter().zip(&ls).enumerate() {
        let mut ft = FileTriage::default();
        let mut bump = |kind: &'static str, n: usize, ft: &mut FileTriage| {
            *t.by_kind.entry(kind).or_default() += n;
            t.changed += n;
            if RESIDUAL.contains(&kind) {
                ft.residual += n;
            }
        };
        if f.truncated || f.binary {
            bump("unknown", (f.additions + f.deletions).max(1), &mut ft);
        } else if f.hunks.is_empty() && matches!(f.status, "renamed" | "copied") {
            bump("rename", 1, &mut ft);
        }
        for (hi, (h, lh)) in f.hunks.iter().zip(&l.hunks).enumerate() {
            let mut tags: Vec<Tag> = vec![];
            for (li, x) in lh.iter().enumerate() {
                if !x.change {
                    continue;
                }
                let mv = moved.get(&(fi, hi, li));
                let kind = if mv.is_some() {
                    "move"
                } else if let Some(k) = x.early {
                    k
                } else if x.pair.as_ref().is_some_and(|p| pairs[&**p].len() >= 2) {
                    "ident"
                } else {
                    x.late
                };
                let refer = mv.map(|&(f2, h2, _)| (files[f2].path.clone(), h2));
                bump(kind, 1, &mut ft);
                if x.sig && kind == "logic" {
                    ft.sig += 1;
                }
                match tags.last_mut() {
                    Some(g) if g.to == li && g.kind == kind && g.refer == refer => {
                        g.to = li + 1;
                        g.sig |= x.sig && kind == "logic";
                    }
                    _ => tags.push(Tag {
                        kind,
                        from: li,
                        to: li + 1,
                        refer,
                        sig: x.sig && kind == "logic",
                    }),
                }
            }
            debug_assert_eq!(h.lines.len(), lh.len());
            ft.hunks.push(tags);
        }
        let weight = if l.generated {
            0.0
        } else if lang::test_path(&f.path) {
            0.2
        } else {
            1.0
        };
        raw.push(ft.residual as f64 * (1.0 + 0.5 * ft.sig as f64) * weight);
        t.files.push(ft);
    }
    let max = raw.iter().copied().fold(0.0, f64::max);
    for (ft, r) in t.files.iter_mut().zip(raw) {
        ft.risk = if max > 0.0 {
            (100.0 * r / max).ceil() as u32
        } else {
            0
        };
    }
    let low: usize = LOW_RISK.iter().filter_map(|k| t.by_kind.get(k)).sum();
    t.low_risk_pct = if t.changed > 0 {
        ((100 * low) as f64 / t.changed as f64).round() as u32
    } else {
        0
    };
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::diff::parse_patch;

    fn file(path: &str, body: &str) -> File {
        let p = format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{body}");
        let (binary, hunks) = parse_patch(&p).remove(0);
        let count = |k: &str| {
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == k)
                .count()
        };
        File {
            path: path.into(),
            old_path: path.into(),
            status: "modified",
            binary,
            additions: count("add"),
            deletions: count("del"),
            hunks,
            ..Default::default()
        }
    }

    fn kinds(t: &Triage, f: usize) -> Vec<Vec<(&'static str, usize, usize)>> {
        t.files[f]
            .hunks
            .iter()
            .map(|h| h.iter().map(|g| (g.kind, g.from, g.to)).collect())
            .collect()
    }

    #[test]
    fn detects_blocks_moved_across_files() {
        let a = file(
            "src/a.rs",
            "@@ -1,6 +1,1 @@\n fn keep() {}\n-fn helper(x: u32) -> u32 {\n-    let y = x * 2;\n-    y + 1\n-}\n-\n",
        );
        let b = file(
            "src/b.rs",
            "@@ -1,1 +1,7 @@\n fn other() {}\n+\n+fn helper(x: u32) -> u32 {\n+    let y = x * 2;\n+    y + 1\n+}\n+fn fresh() {}\n",
        );
        let t = analyze(&[a, b]);
        assert_eq!(kinds(&t, 0), [vec![("move", 1, 6)]]);
        assert_eq!(t.files[0].hunks[0][0].refer, Some(("src/b.rs".into(), 0)));
        assert_eq!(
            kinds(&t, 1),
            [vec![("format", 1, 2), ("move", 2, 6), ("logic", 6, 7)]]
        );
        assert!(t.files[1].hunks[0][2].sig);
        assert_eq!(t.files[1].residual, 1);
        assert_eq!(t.files[0].risk, 0);
        assert_eq!(t.files[1].risk, 100);
    }

    #[test]
    fn an_edited_move_stays_logic_and_short_lines_do_not_move() {
        let a = file("a.ts", "@@ -1,3 +1,0 @@\n-}\n-return x;\n-}\n");
        let b = file("b.ts", "@@ -1,0 +1,3 @@\n+}\n+return x;\n+}\n");
        let t = analyze(&[a, b]);
        assert_eq!(kinds(&t, 1), [vec![("logic", 0, 3)]]);
        let a = file(
            "a.ts",
            "@@ -1,2 +1,0 @@\n-const total = items.reduce(sum, 0);\n-return total * rate;\n",
        );
        let b = file(
            "b.ts",
            "@@ -1,0 +1,2 @@\n+const total = items.reduce(sum, 0);\n+return total * rate * 2;\n",
        );
        let t = analyze(&[a, b]);
        assert_eq!(kinds(&t, 1), [vec![("move", 0, 1), ("logic", 1, 2)]]);
    }

    #[test]
    fn whitespace_and_rewraps_are_format() {
        let f = file(
            "x.ts",
            "@@ -1,3 +1,5 @@\n a\n-call(one, two, three);\n+call(\n+  one, two,\n+  three);\n b\n",
        );
        let g = file("y.py", "@@ -1,2 +1,2 @@\n-x=1\n+x = 1\n y\n");
        let t = analyze(&[f, g]);
        assert_eq!(kinds(&t, 0), [vec![("format", 1, 5)]]);
        assert_eq!(kinds(&t, 1), [vec![("format", 0, 2)]]);
        assert_eq!(t.low_risk_pct, 100);
    }

    #[test]
    fn import_reorder_needs_the_same_set() {
        let same = file(
            "a.ts",
            "@@ -1,2 +1,2 @@\n-import { b } from './b';\n-import { a } from './a';\n+import { a } from './a';\n+import { b } from './b';\n",
        );
        let added = file(
            "b.rs",
            "@@ -1,1 +1,2 @@\n-use std::fs;\n+use std::io;\n+use std::fs;\n",
        );
        let t = analyze(&[same, added]);
        assert_eq!(kinds(&t, 0), [vec![("imports", 0, 4)]]);
        assert_eq!(kinds(&t, 1), [vec![("logic", 0, 3)]]);
    }

    #[test]
    fn identifier_renames_count_only_when_repeated() {
        let a = file(
            "a.go",
            "@@ -1,3 +1,3 @@\n-x := oldName(1)\n+x := newName(1)\n z\n@@ -9,1 +9,1 @@\n-return oldName\n+return newName\n",
        );
        let b = file(
            "b.go",
            "@@ -1,1 +1,1 @@\n-y := count + 1\n+y := total + 1\n",
        );
        let t = analyze(&[a, b]);
        assert_eq!(kinds(&t, 0), [vec![("ident", 0, 2)], vec![("ident", 0, 2)]]);
        assert_eq!(kinds(&t, 1), [vec![("logic", 0, 2)]]);
    }

    #[test]
    fn generated_tests_and_comments_by_path_and_content() {
        let lock = file("app/pnpm-lock.yaml", "@@ -1,1 +1,1 @@\n-a: 1\n+a: 2\n");
        let gen = file(
            "src/api.rs",
            "@@ -1,1 +1,2 @@\n // @generated\n+pub fn x() {}\n",
        );
        let test = file("app/x.test.ts", "@@ -1,0 +1,1 @@\n+expect(a).toBe(1);\n");
        let doc = file(
            "src/m.rs",
            "@@ -1,1 +1,2 @@\n-// old note\n+/// new doc\n+// more\n",
        );
        let md = file("README.md", "@@ -1,0 +1,1 @@\n+Some prose\n");
        let code = file("src/c.rs", "@@ -1,0 +1,1 @@\n+let a = 1;\n");
        let t = analyze(&[lock, gen, test, doc, md, code]);
        assert_eq!(kinds(&t, 0), [vec![("generated", 0, 2)]]);
        assert_eq!(kinds(&t, 1), [vec![("generated", 1, 2)]]);
        assert_eq!(kinds(&t, 2), [vec![("test", 0, 1)]]);
        assert_eq!(kinds(&t, 3), [vec![("comment", 0, 3)]]);
        assert_eq!(kinds(&t, 4), [vec![("comment", 0, 1)]]);
        let risks: Vec<u32> = t.files.iter().map(|f| f.risk).collect();
        assert_eq!(risks, [0, 0, 20, 0, 0, 100]);
        assert_eq!(t.files[2].residual, 1);
    }

    #[test]
    fn truncated_binary_and_pure_renames() {
        let big = File {
            path: "big.rs".into(),
            status: "modified",
            additions: 4000,
            truncated: true,
            ..Default::default()
        };
        let mv = File {
            path: "new.rs".into(),
            old_path: "old.rs".into(),
            status: "renamed",
            ..Default::default()
        };
        let cp = File {
            path: "copy.rs".into(),
            status: "copied",
            ..Default::default()
        };
        let t = analyze(&[big, mv, cp]);
        assert_eq!(t.by_kind.get("unknown"), Some(&4000));
        assert_eq!(t.by_kind.get("rename"), Some(&2));
        assert_eq!(t.changed, 4002);
        assert_eq!(t.files[0].risk, 100);
        assert_eq!((t.files[1].risk, t.files[1].residual), (0, 0));
        assert_eq!(t.low_risk_pct, 0);
        assert!(analyze(&[]).files.is_empty());
    }

    #[test]
    fn local_pass_is_cached_by_hash_and_bounded() {
        let f = file(
            "cache/a.rs",
            "@@ -1,0 +1,1 @@\n+let unique_cache_probe = 1;\n",
        );
        let a = locals(std::slice::from_ref(&f));
        assert!(Arc::ptr_eq(&a[0], &locals(&[f])[0]));
        let many: Vec<File> = (0..CACHE_CAP + 5)
            .map(|i| {
                file(
                    &format!("cache/{i}.rs"),
                    &format!("@@ -1,0 +1,1 @@\n+x{i}\n"),
                )
            })
            .collect();
        let first = locals(&many);
        assert!(cache_len() <= CACHE_CAP);
        let again = locals(&many);
        let reused = first
            .iter()
            .zip(&again)
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count();
        assert!(reused >= CACHE_CAP - 64, "{reused}");
        assert!(cache_len() <= CACHE_CAP);

        let body: String = (0..3000)
            .map(|i| format!("+let big_{i} = {i};\n"))
            .collect();
        let big: Vec<File> = (0..40)
            .map(|i| {
                file(
                    &format!("cache/big{i}.rs"),
                    &format!("@@ -1,0 +1,3000 @@\n{body}"),
                )
            })
            .collect();
        let got = locals(&big);
        assert_eq!(got.len(), 40);
        assert_eq!(got[0].hunks[0].len(), 3000);
        let weight: usize = CACHE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map_or(0, |m| m.values().map(|l| l.weight()).sum());
        assert!(weight <= CACHE_LINES, "{weight}");
        assert!(weight >= CACHE_LINES / 2, "{weight}");
    }
}
