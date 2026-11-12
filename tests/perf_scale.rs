use overseer::chat::{self, Cursor, Query};
use overseer::{activity, gitgraph, screen};
use serde_json::{json, Value};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const LINEAR: f64 = 30.0;

fn per(f: &mut dyn FnMut(), n: u32) -> f64 {
    let t = Instant::now();
    for _ in 0..n {
        f();
    }
    t.elapsed().as_secs_f64() / f64::from(n)
}

fn reps(f: &mut dyn FnMut()) -> u32 {
    let mut n = 1;
    loop {
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        if t.elapsed().as_secs_f64() >= 0.002 || n >= 4096 {
            return n;
        }
        n *= 2;
    }
}

fn scaling(small: &mut dyn FnMut(), big: &mut dyn FnMut(), bound: f64) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        if best < bound {
            break;
        }
        let (ns, nb) = (reps(small), reps(big));
        let (mut a, mut b) = (f64::INFINITY, f64::INFINITY);
        for _ in 0..7 {
            a = a.min(per(small, ns));
            b = b.min(per(big, nb));
        }
        best = best.min(b / a);
    }
    best
}

fn dir(tag: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("perf-scale-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn append(p: &Path, s: &str) {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .unwrap()
        .write_all(s.as_bytes())
        .unwrap();
}

fn rewrite_at(p: &Path, offset: u64, s: &str) {
    let mut f = std::fs::OpenOptions::new().write(true).open(p).unwrap();
    f.seek(SeekFrom::Start(offset)).unwrap();
    f.write_all(s.as_bytes()).unwrap();
}

fn texts(v: &Value) -> Vec<String> {
    v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn chat_pages_read_only_the_appended_bytes() {
    let d = dir("chat");
    let p = d.join("s.jsonl");
    let user = |id: &str, text: &str| {
        format!(
            "{}\n",
            json!({"type": "user", "uuid": id, "timestamp": "2026-10-29T10:00:00Z", "message": {"content": text}})
        )
    };
    let mut body = String::new();
    for i in 0..5000 {
        body.push_str(&user(&format!("u{i:05}"), &format!("m{i:05}")));
    }
    std::fs::write(&p, &body).unwrap();
    let q = |cursor| Query {
        cursor,
        limit: chat::DEFAULT_LIMIT,
        live: None,
    };
    let tail = chat::page("claude", &p, q(Cursor::Tail)).unwrap();
    assert_eq!(texts(&tail).last().unwrap(), "m04999");
    let cursor = tail["cursor"].as_u64().unwrap();

    let last = body.rfind("m04999").unwrap() as u64;
    rewrite_at(&p, last, "x04999");
    append(&p, &user("u05000", "m05000"));

    assert!(chat::changed(&p).unwrap() > cursor);
    let delta = chat::page("claude", &p, q(Cursor::After(cursor))).unwrap();
    assert_eq!(texts(&delta), ["m05000"]);
    assert_eq!(delta["reset"], false);
    let again = chat::page("claude", &p, q(Cursor::Tail)).unwrap();
    let t = texts(&again);
    assert_eq!(t[t.len() - 2], "m04999");
    assert!(delta.to_string().len() * 50 < tail.to_string().len());
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn activity_reads_only_the_appended_bytes() {
    let d = dir("activity");
    let p = d.join("t.jsonl");
    let tool = |id: &str| {
        format!(
            "{}\n",
            json!({"type": "assistant", "timestamp": "2026-10-29T10:00:01Z", "message": {"content": [{"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "ls"}}]}})
        )
    };
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&tool(&format!("a{i:05}")));
    }
    std::fs::write(&p, &body).unwrap();
    let ids = |v: &Value| -> Vec<String> {
        v["workLog"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect()
    };
    let first = activity::activity("claude", &p, None).unwrap();
    assert_eq!(ids(&first).last().unwrap(), "a02999");

    let last = body.rfind("a02999").unwrap() as u64;
    rewrite_at(&p, last, "z02999");
    append(&p, &tool("b00000"));

    let next = ids(&activity::activity("claude", &p, None).unwrap());
    assert_eq!(next[next.len() - 2..], ["a02999", "b00000"]);
    assert!(!next.iter().any(|i| i.starts_with('z')));
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn screen_render_cost_is_bounded_by_its_tail() {
    let block = |n: usize| {
        let mut s = String::new();
        for i in 0..n {
            s.push_str(&format!(
                "\x1b[2m{i:07}\x1b[0m \x1b[3{}mbuild\x1b[0m compiling crate_{} ... ok\r\n",
                i % 6 + 1,
                i % 97
            ));
        }
        s.into_bytes()
    };
    let small = block(20_000);
    let big = block(200_000);
    assert!(small.len() > screen::TAIL);
    let a = screen::render(&small, 120, 40).text();
    let b = screen::render(&big, 120, 40).text();
    assert!(a.contains("0019999") && b.contains("0199999"));
    let r = scaling(
        &mut || {
            screen::render(&small, 120, 40);
        },
        &mut || {
            screen::render(&big, 120, 40);
        },
        4.0,
    );
    assert!(r < 4.0, "render ratio {r:.1}");
}

#[test]
fn git_log_and_refs_parse_in_linear_time() {
    let log = |n: usize| {
        (0..n)
            .map(|i| {
                format!(
                    "{i:040x}\x1f{:040x}\x1fDev\x1fdev@example.com\x1f1700000000\x1fcommit {i}\x1e\n",
                    i + 1
                )
            })
            .collect::<String>()
    };
    let refs = |n: usize| {
        (0..n)
            .map(|i| {
                format!("refs/heads/b{i}\x1f{i:040x}\x1f\x1forigin/b{i}\x1fahead 1, behind 2\n")
            })
            .collect::<String>()
    };
    let (ls, lb) = (log(2_000), log(20_000));
    let (rs, rb) = (refs(2_000), refs(20_000));
    assert_eq!(gitgraph::parse_log(&lb).len(), 20_000);
    assert_eq!(gitgraph::parse_refs(&rb).len(), 20_000);
    let r = scaling(
        &mut || {
            gitgraph::parse_log(&ls);
        },
        &mut || {
            gitgraph::parse_log(&lb);
        },
        LINEAR,
    );
    assert!(r < LINEAR, "parse_log ratio {r:.1}");
    let r = scaling(
        &mut || {
            gitgraph::parse_refs(&rs);
        },
        &mut || {
            gitgraph::parse_refs(&rb);
        },
        LINEAR,
    );
    assert!(r < LINEAR, "parse_refs ratio {r:.1}");
}

#[test]
fn commit_file_lists_parse_in_linear_time() {
    let lists = |n: usize| {
        let ns: String = (0..n).map(|i| format!("M\0src/f{i}.rs\0")).collect();
        let num: String = (0..n)
            .map(|i| format!("{}\t1\tsrc/f{i}.rs\0", i % 9))
            .collect();
        (ns, num)
    };
    let (sn, sm) = lists(500);
    let (bn, bm) = lists(5_000);
    let files = gitgraph::parse_files(&bn, &bm);
    assert_eq!(files.len(), 5_000);
    assert_eq!(files[4_999].added, 4_999 % 9);
    let r = scaling(
        &mut || {
            gitgraph::parse_files(&sn, &sm);
        },
        &mut || {
            gitgraph::parse_files(&bn, &bm);
        },
        LINEAR,
    );
    assert!(r < LINEAR, "parse_files ratio {r:.1}");
}

struct Killed(std::sync::Arc<overseer::term::Term>);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn term_titled(len: usize) -> Killed {
    Killed(
        overseer::term::spawn(overseer::term::Spec {
            cwd: "/".into(),
            cmd: vec!["/bin/sh".into()],
            title: Some("t".repeat(len)),
            workspace_id: Some("w1".into()),
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap(),
    )
}

#[test]
fn inheriting_a_term_workspace_does_not_depend_on_the_term_state() {
    let (small_t, big_t) = (term_titled(1), term_titled(100_000));
    let (a, b) = (small_t.0.info().id, big_t.0.info().id);
    assert_eq!(overseer::term::workspace_of(&a).as_deref(), Some("w1"));
    assert_eq!(overseer::term::workspace_of(&b).as_deref(), Some("w1"));
    let mut small = || {
        std::hint::black_box(overseer::term::workspace_of(&a));
    };
    let mut big = || {
        std::hint::black_box(overseer::term::workspace_of(&b));
    };
    let r = scaling(&mut small, &mut big, 3.0);
    assert!(
        r < 3.0,
        "lookup cost grows with the term state: ratio {r:.1}"
    );
}
