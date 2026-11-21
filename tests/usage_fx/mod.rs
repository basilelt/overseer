#![allow(dead_code)]
use rusqlite::{params, Connection};
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn varint(mut v: u64, out: &mut Vec<u8>) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn int_field(f: u64, v: u64, out: &mut Vec<u8>) {
    varint(f << 3, out);
    varint(v, out);
}

fn bytes_field(f: u64, b: &[u8], out: &mut Vec<u8>) {
    varint(f << 3 | 2, out);
    varint(b.len() as u64, out);
    out.extend_from_slice(b);
}

pub fn agy_blob(model: &str, input: u64, output: u64, read: u64, secs: u64) -> Vec<u8> {
    let mut stats = vec![];
    int_field(1, 1132, &mut stats);
    int_field(2, input, &mut stats);
    int_field(3, output, &mut stats);
    if read > 0 {
        int_field(5, read, &mut stats);
    }
    int_field(6, 24, &mut stats);
    int_field(10, 49, &mut stats);
    bytes_field(11, b"anon", &mut stats);
    let mut ts = vec![];
    int_field(1, secs, &mut ts);
    int_field(2, 956_181_649, &mut ts);
    let mut at = vec![];
    int_field(2, u64::MAX, &mut at);
    bytes_field(4, &ts, &mut at);
    let mut head = vec![];
    int_field(3, 1132, &mut head);
    bytes_field(4, &stats, &mut head);
    bytes_field(9, &at, &mut head);
    bytes_field(19, model.as_bytes(), &mut head);
    let mut blob = vec![];
    bytes_field(2, &[2, 3], &mut blob);
    bytes_field(1, &head, &mut blob);
    blob
}

pub fn agy_db(dir: &Path, id: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(format!("{id}.db"));
    Connection::open(&p)
        .unwrap()
        .execute_batch(
            "CREATE TABLE gen_metadata (idx integer, data blob, size integer NOT NULL DEFAULT 0, PRIMARY KEY (idx));
             CREATE TABLE steps (idx integer, step_type integer NOT NULL DEFAULT 0);",
        )
        .unwrap();
    p
}

pub fn agy_add(p: &Path, idx: i64, blob: &[u8]) {
    Connection::open(p)
        .unwrap()
        .execute(
            "INSERT INTO gen_metadata (idx, data, size) VALUES (?1, ?2, ?3)",
            params![idx, blob, blob.len() as i64],
        )
        .unwrap();
}

pub fn copilot_record(
    ts: &str,
    model: &str,
    input: u64,
    output: u64,
    read: u64,
    write: u64,
) -> String {
    format!(
        "{}\n",
        json!({"type": "session.usage_record", "timestamp": ts,
            "data": {"usage": {"model": model, "inputTokens": input, "outputTokens": output,
                "cacheReadTokens": read, "cacheWriteTokens": write}}})
    )
}

pub fn copilot_start(cwd: &str) -> String {
    format!(
        "{}\n",
        json!({"type": "session.start", "timestamp": "2026-11-12T15:45:38.738Z",
            "data": {"sessionId": "s", "context": {"cwd": cwd}}})
    )
}

pub fn opencode_db(dir: &Path) -> (PathBuf, Connection) {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join("opencode.db");
    let c = Connection::open(&p).unwrap();
    c.execute_batch(
        "CREATE TABLE session (id text PRIMARY KEY, project_id text NOT NULL, parent_id text,
           directory text NOT NULL, title text NOT NULL, time_created integer NOT NULL,
           time_updated integer NOT NULL, model text, cost real DEFAULT 0 NOT NULL,
           tokens_input integer DEFAULT 0 NOT NULL, tokens_output integer DEFAULT 0 NOT NULL,
           tokens_reasoning integer DEFAULT 0 NOT NULL, tokens_cache_read integer DEFAULT 0 NOT NULL,
           tokens_cache_write integer DEFAULT 0 NOT NULL);",
    )
    .unwrap();
    (p, c)
}

pub fn opencode_session(c: &Connection, id: &str, model: Option<&str>, t: [i64; 5]) {
    let now = overseer::util::now_ms();
    c.execute(
        "INSERT OR REPLACE INTO session (id, project_id, parent_id, directory, title, time_created,
           time_updated, model, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read,
           tokens_cache_write)
         VALUES (?1, 'p', NULL, '/w/app', 't', ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, now - 1000, model, t[0], t[1], t[2], t[3], t[4]],
    )
    .unwrap();
}
