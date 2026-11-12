use crate::registry::Registry;
use crate::util::{format_rfc3339, ulid};
use serde_json::{json, Value};

pub const NOTE_MAX: usize = 4000;
pub const NOTES_MAX: usize = 50;

pub type Err = (u16, String);

/// A note as stored: newlines kept, other control characters dropped, trimmed and capped; blank is none.
pub fn clean_note(raw: Option<&str>) -> Option<String> {
    let s: String = raw?
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    let t: String = s.trim().chars().take(NOTE_MAX).collect();
    Some(t.trim_end().to_string()).filter(|t| !t.is_empty())
}

pub fn parse(raw: Option<&str>) -> Vec<Value> {
    raw.and_then(|s| serde_json::from_str::<Vec<Value>>(s).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|n| n["id"].is_string() && n["text"].is_string())
        .collect()
}

pub fn add(notes: &mut Vec<Value>, text: &str, now_ms: i64) -> Result<Value, Err> {
    if notes.len() >= NOTES_MAX {
        return Err((409, format!("at most {NOTES_MAX} notes per session")));
    }
    let at = format_rfc3339(now_ms);
    let n = json!({"id": ulid(now_ms), "text": text, "createdAt": at, "updatedAt": at});
    notes.push(n.clone());
    Ok(n)
}

pub fn edit(notes: &mut [Value], id: &str, text: &str, now_ms: i64) -> Result<Value, Err> {
    let n = notes
        .iter_mut()
        .find(|n| n["id"] == id)
        .ok_or_else(|| (404, format!("no note {id}")))?;
    if n["text"] != text {
        n["text"] = json!(text);
        n["updatedAt"] = json!(format_rfc3339(now_ms));
    }
    Ok(n.clone())
}

pub fn remove(notes: &mut Vec<Value>, id: &str) -> Result<(), Err> {
    let before = notes.len();
    notes.retain(|n| n["id"] != id);
    if notes.len() == before {
        return Err((404, format!("no note {id}")));
    }
    Ok(())
}

/// Read-modify-write of one line's notes in a single immediate transaction, so two clients never drop each other's notes.
pub fn update<T>(
    reg: &mut Registry,
    line_id: &str,
    f: impl FnOnce(&mut Vec<Value>) -> Result<T, Err>,
) -> Result<(T, Vec<Value>), Err> {
    let tx = reg
        .conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| (500, e.to_string()))?;
    let raw: Option<Option<String>> = tx
        .query_row("SELECT notes FROM lines WHERE id = ?1", [line_id], |r| {
            r.get(0)
        })
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })
        .map_err(|e| (500, e.to_string()))?;
    let Some(raw) = raw else {
        return Err((404, format!("no line {line_id}")));
    };
    let mut notes = parse(raw.as_deref());
    let out = f(&mut notes)?;
    let stored = (!notes.is_empty()).then(|| Value::Array(notes.clone()).to_string());
    tx.execute(
        "UPDATE lines SET notes = ?2 WHERE id = ?1",
        rusqlite::params![line_id, stored],
    )
    .map_err(|e| (500, e.to_string()))?;
    tx.commit().map_err(|e| (500, e.to_string()))?;
    Ok((out, notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_note_keeps_lines_and_drops_blank() {
        assert_eq!(
            clean_note(Some("  todo:\n- a\u{7}\n\tb  ")),
            Some("todo:\n- a\n\tb".into())
        );
        assert_eq!(clean_note(Some(" \n ")), None);
        assert_eq!(clean_note(None), None);
        assert_eq!(
            clean_note(Some(&"x".repeat(NOTE_MAX + 5))).unwrap().len(),
            NOTE_MAX
        );
    }

    #[test]
    fn parse_skips_garbage() {
        assert!(parse(None).is_empty());
        assert!(parse(Some("not json")).is_empty());
        let v = parse(Some(r#"[{"id":"a","text":"t"},{"id":1},"x"]"#));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["id"], "a");
    }

    #[test]
    fn add_edit_remove_round_trip() {
        let mut v = vec![];
        let n = add(&mut v, "first", 1_000).unwrap();
        let id = n["id"].as_str().unwrap().to_string();
        assert_eq!(n["createdAt"], "1970-01-01T00:00:01Z");
        let e = edit(&mut v, &id, "second", 61_000).unwrap();
        assert_eq!(
            (
                e["text"].as_str(),
                e["updatedAt"].as_str(),
                e["createdAt"].as_str()
            ),
            (
                Some("second"),
                Some("1970-01-01T00:01:01Z"),
                Some("1970-01-01T00:00:01Z")
            )
        );
        let same = edit(&mut v, &id, "second", 999_000).unwrap();
        assert_eq!(same["updatedAt"], "1970-01-01T00:01:01Z");
        assert_eq!(edit(&mut v, "nope", "x", 0).unwrap_err().0, 404);
        assert_eq!(remove(&mut v, "nope").unwrap_err().0, 404);
        remove(&mut v, &id).unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn notes_stay_out_of_what_agents_and_the_team_see() {
        // Given a session line carrying a note
        // When it is turned into the board line synced to the team and the session list agents read over MCP
        // Then only the station-local part shown to the user carries the note
        let l = crate::Line {
            id: "l1".into(),
            agent: "claude",
            state: crate::wire::STATE_IDLE,
            notes: vec![json!({"id": "n", "text": "private", "createdAt": "", "updatedAt": ""})],
            ..Default::default()
        };
        let v = l.to_json();
        assert_eq!(v["local"]["notes"][0]["text"], "private");
        assert!(!v["line"].to_string().contains("private"));
        let ix = crate::workspaces::Index {
            workspaces: vec![],
            tasks: vec![],
            projects: vec![],
        };
        assert!(!crate::organize::trim_session(&l, &ix)
            .to_string()
            .contains("private"));
    }

    #[test]
    fn add_caps_the_count() {
        let mut v = vec![];
        for _ in 0..NOTES_MAX {
            add(&mut v, "n", 0).unwrap();
        }
        assert_eq!(add(&mut v, "n", 0).unwrap_err().0, 409);
    }
}
