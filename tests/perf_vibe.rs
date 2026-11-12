use overseer::vibe::{load, Change};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

fn entry(i: usize) -> Value {
    json!({"type": "message", "id": format!("m{i}"), "createdAt": i, "role": "user",
        "content": [{"type": "text", "text": format!("prompt {i}")}]})
}

fn store(tag: &str, n: usize) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("ov-perf-vibe-{tag}-{}", std::process::id()))
        .join("unified/s1");
    let _ = std::fs::remove_dir_all(&dir);
    let g = dir.join("generations/0000000000000001");
    for d in [&g, &dir.join("chunks"), &dir.join("journal")] {
        std::fs::create_dir_all(d).unwrap();
    }
    let all: Vec<Value> = (0..n).map(entry).collect();
    let digests: Vec<String> = all
        .chunks(50)
        .enumerate()
        .map(|(i, c)| {
            let d = format!("{i:064x}");
            std::fs::write(dir.join(format!("chunks/{d}.json")), json!(c).to_string()).unwrap();
            d
        })
        .collect();
    std::fs::write(
        g.join("projection-state.json"),
        json!({"snapshot": {"history": {"entries": []}}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        g.join("manifest.json"),
        json!({
        "projection_state": {"path": "projection-state.json", "chunks": digests},
        "recovery_journal_segment": {"path": "journal/0000000000000001.jsonl"}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("journal/0000000000000001.jsonl"), "").unwrap();
    std::fs::write(
        dir.join("CURRENT"),
        json!({"store_format": "mistral.vibe.unified-session-store/v1",
        "store_format_minor": 10, "session_id": "s1", "generation": "0000000000000001"})
        .to_string(),
    )
    .unwrap();
    dir
}

fn append(dir: &Path, i: usize) {
    let rec = json!({"type": "projection_delta", "sequence": 1,
        "payload": {"delta": [{"op": "append_entry", "entry": entry(i)}]}});
    std::fs::OpenOptions::new()
        .append(true)
        .open(dir.join("journal/0000000000000001.jsonl"))
        .unwrap()
        .write_all(format!("{rec}\n").as_bytes())
        .unwrap();
}

fn refresh_work(n: usize) -> (usize, usize) {
    let dir = store(&n.to_string(), n);
    let (mut s, all) = load(&dir).unwrap();
    assert_eq!(all.len(), n);
    let cold = s.parsed();
    append(&dir, n);
    match s.refresh(&dir) {
        Some(Change::Append(v)) => assert_eq!(v.len(), 1),
        _ => panic!("expected one appended entry"),
    }
    (cold, s.parsed())
}

#[test]
fn a_journal_append_parses_one_record_whatever_the_history() {
    let (small_cold, small) = refresh_work(100);
    let (big_cold, big) = refresh_work(1000);
    assert_eq!((small, big), (1, 1));
    let ratio = big_cold as f64 / small_cold as f64;
    assert!(ratio <= 11.0, "10x history did {ratio:.1}x the cold work");
}
