use crate::claude::mtime_ms;
use crate::util::parse_rfc3339;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone)]
pub struct Session {
    pub id: String,
    pub cwd: Option<String>,
    pub started_ms: Option<i64>,
    pub activity_ms: i64,
    pub path: PathBuf,
}

#[derive(Clone)]
struct Head {
    id: String,
    started_ms: Option<i64>,
    main: bool,
}

type Heads = HashMap<PathBuf, (i64, Option<Head>)>;

static HEADS: Mutex<Option<Heads>> = Mutex::new(None);

fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let i = s.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = s[i..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

fn head(path: &Path) -> Option<Head> {
    let mut buf = vec![0u8; 4096];
    let n = std::fs::File::open(path).ok()?.read(&mut buf).ok()?;
    let s = String::from_utf8_lossy(&buf[..n]);
    let id = field(&s, "sessionId")?.to_string();
    if !crate::util::safe_session_id(&id) {
        return None;
    }
    Some(Head {
        id,
        started_ms: field(&s, "startTime").and_then(parse_rfc3339),
        main: field(&s, "kind").is_none_or(|k| k == "main"),
    })
}

fn cached_head(path: &Path, mtime: i64) -> Option<Head> {
    let mut g = HEADS.lock().unwrap_or_else(|e| e.into_inner());
    let m = g.get_or_insert_with(HashMap::new);
    if let Some((t, h)) = m.get(path) {
        if *t == mtime {
            return h.clone();
        }
    }
    let h = head(path);
    m.insert(path.to_path_buf(), (mtime, h.clone()));
    h
}

fn project_roots(gemini_dir: &Path) -> HashMap<String, String> {
    let v: serde_json::Value = std::fs::read(gemini_dir.join("projects.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    v["projects"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(path, slug)| Some((slug.as_str()?.to_string(), path.clone())))
                .collect()
        })
        .unwrap_or_default()
}

fn is_session_file(name: &str) -> bool {
    name.starts_with("session-") && (name.ends_with(".json") || name.ends_with(".jsonl"))
}

pub fn sessions(gemini_dir: &Path) -> Vec<Session> {
    let Ok(projects) = std::fs::read_dir(gemini_dir.join("tmp")) else {
        return vec![];
    };
    let roots = project_roots(gemini_dir);
    let mut out: HashMap<String, Session> = HashMap::new();
    for p in projects.flatten() {
        let dir = p.path();
        let Ok(files) = std::fs::read_dir(dir.join("chats")) else {
            continue;
        };
        let slug = p.file_name().to_string_lossy().into_owned();
        let cwd = std::fs::read_to_string(dir.join(".project_root"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| roots.get(&slug).cloned());
        for f in files.flatten() {
            let path = f.path();
            if !f.file_name().to_str().is_some_and(is_session_file) {
                continue;
            }
            let mtime = mtime_ms(&path);
            let Some(h) = cached_head(&path, mtime).filter(|h| h.main) else {
                continue;
            };
            let s = Session {
                id: h.id.clone(),
                cwd: cwd.clone(),
                started_ms: h.started_ms,
                activity_ms: mtime,
                path,
            };
            match out.get(&h.id) {
                Some(o) if o.activity_ms >= s.activity_ms => {}
                _ => {
                    out.insert(h.id, s);
                }
            }
        }
    }
    out.into_values().collect()
}

pub fn find(gemini_dir: &Path, session_id: &str) -> Option<PathBuf> {
    sessions(gemini_dir)
        .into_iter()
        .find(|s| s.id == session_id)
        .map(|s| s.path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_heads() {
        let s = r#"{"sessionId": "abc-1", "projectHash":"x","startTime":"2026-10-29T10:00:00.000Z","kind":"main"}"#;
        assert_eq!(field(s, "sessionId"), Some("abc-1"));
        assert_eq!(field(s, "kind"), Some("main"));
        assert_eq!(field(s, "missing"), None);
    }
}
