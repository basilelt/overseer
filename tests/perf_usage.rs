mod usage_fx;

use overseer::usage::{decoded, Dirs, Index};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

fn setup(tag: &str) -> (PathBuf, Index) {
    let d = std::env::temp_dir().join(format!("ov-perf-usage-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let dirs = Dirs {
        claude: d.join("claude"),
        codex: d.join("codex"),
        gemini: d.join("gemini"),
        vibe: d.join("vibe"),
        copilot: d.join("copilot"),
        opencode: d.join("opencode"),
        agy: d.join("agy"),
    };
    std::fs::create_dir_all(&d).unwrap();
    let idx = Index::open(&d.join("usage.db"), dirs).unwrap();
    (d, idx)
}

fn bump(p: &Path) {
    let f = std::fs::File::options().append(true).open(p).unwrap();
    f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
        .unwrap();
}

fn now() -> i64 {
    overseer::util::now_ms()
}

fn ts() -> String {
    overseer::util::format_rfc3339(now() - 60_000)
}

fn append_work(n: usize, kind: &str) -> u64 {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (d, mut idx) = setup(&format!("{kind}-{n}"));
    let secs = now() as u64 / 1000 - 60;
    let touched: PathBuf;
    match kind {
        "copilot" => {
            let dir = d.join("copilot/session-state/s1");
            std::fs::create_dir_all(&dir).unwrap();
            touched = dir.join("events.jsonl");
            let mut body = usage_fx::copilot_start("/w/app");
            for _ in 0..n {
                body += &usage_fx::copilot_record(&ts(), "m", 100, 10, 20, 30);
            }
            std::fs::write(&touched, body).unwrap();
        }
        "agy" => {
            touched = usage_fx::agy_db(&d.join("agy/conversations"), "c1");
            for i in 0..n {
                usage_fx::agy_add(
                    &touched,
                    i as i64,
                    &usage_fx::agy_blob("m", 100, 10, 20, secs),
                );
            }
        }
        _ => unreachable!(),
    }
    idx.sweep(now(), true, None).unwrap();
    let cold = decoded();
    assert!(cold > 0);
    match kind {
        "copilot" => {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&touched)
                .unwrap()
                .write_all(usage_fx::copilot_record(&ts(), "m", 1, 1, 1, 1).as_bytes())
                .unwrap();
        }
        _ => usage_fx::agy_add(&touched, n as i64, &usage_fx::agy_blob("m", 1, 1, 1, secs)),
    }
    bump(&touched);
    idx.sweep(now(), true, None).unwrap();
    let work = decoded() - cold;
    idx.sweep(now(), true, None).unwrap();
    assert_eq!(
        decoded() - cold,
        work,
        "an unchanged source decodes nothing"
    );
    let _ = std::fs::remove_dir_all(&d);
    work
}

#[test]
fn an_appended_copilot_record_decodes_one_line_whatever_the_history() {
    assert_eq!(append_work(50, "copilot"), 1);
    assert_eq!(append_work(500, "copilot"), 1);
}

#[test]
fn an_appended_antigravity_row_decodes_one_row_whatever_the_history() {
    assert_eq!(append_work(50, "agy"), 1);
    assert_eq!(append_work(500, "agy"), 1);
}

fn opencode_work(n: usize) -> (u64, u64) {
    let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let (d, mut idx) = setup(&format!("oc-{n}"));
    let (p, c) = usage_fx::opencode_db(&d.join("opencode"));
    for i in 0..n {
        usage_fx::opencode_session(&c, &format!("ses_{i}"), None, [100, 10, 0, 20, 30]);
    }
    idx.sweep(now(), true, None).unwrap();
    let cold = decoded();
    idx.sweep(now(), true, None).unwrap();
    let idle = decoded() - cold;
    c.execute(
        "UPDATE session SET tokens_input = tokens_input + 1 WHERE id = 'ses_0'",
        [],
    )
    .unwrap();
    bump(&p);
    idx.sweep(now(), true, None).unwrap();
    let one = decoded() - cold;
    let _ = std::fs::remove_dir_all(&d);
    (idle, one)
}

#[test]
fn one_changed_opencode_session_is_ingested_once_whatever_the_session_count() {
    for n in [20, 200] {
        assert_eq!(opencode_work(n), (0, 1));
    }
}
