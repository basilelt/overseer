use crate::deliver::{self as shared, Channel, NotIdle, Rules};
use crate::term::{self, Term};
use crate::Line;
use std::path::Path;
use std::sync::Arc;

const QUOTE_LINES: usize = 8;
const QUOTE_WIDTH: usize = 160;

pub struct Item {
    pub id: String,
    pub path: String,
    pub side: String,
    pub line: usize,
    pub end: usize,
    pub code: Vec<String>,
    pub comments: Vec<String>,
}

pub fn split_suggestion(body: &str) -> (String, Option<Vec<String>>) {
    let mut text = vec![];
    let mut sugg: Option<Vec<String>> = None;
    let mut cur: Option<Vec<String>> = None;
    for l in body.lines() {
        match &mut cur {
            None if l.trim_start().starts_with("```suggestion") => cur = Some(vec![]),
            None => text.push(l),
            Some(_) if l.trim_start().starts_with("```") => sugg = cur.take(),
            Some(c) => c.push(l.to_string()),
        }
    }
    if cur.is_some() {
        sugg = cur;
    }
    (text.join("\n").trim().to_string(), sugg)
}

fn clip(s: &str) -> String {
    let t = s.trim_end();
    match t.char_indices().nth(QUOTE_WIDTH) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t.to_string(),
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

pub fn format(kind: &str, items: &[Item], body: Option<&str>) -> String {
    let files = {
        let mut p: Vec<&str> = items.iter().map(|i| i.path.as_str()).collect();
        p.sort();
        p.dedup();
        p.len()
    };
    let count = items.iter().map(|i| i.comments.len()).sum::<usize>();
    let scope = format!(
        "{} ({})",
        plural(files, "file", "files"),
        plural(count, "comment", "comments")
    );
    let mut out = match (kind, items.is_empty()) {
        ("request_changes", false) => format!("Code review: changes requested on {scope}."),
        ("request_changes", true) => "Code review: changes requested.".to_string(),
        ("approve", false) => format!("Code review: approved, with {scope}."),
        ("approve", true) => "Code review: approved.".to_string(),
        (_, false) => format!("Code review: {scope}."),
        (_, true) => "Code review.".to_string(),
    };
    if !items.is_empty() {
        out.push_str(" Address each one. When done with one, call the overseer MCP tool resolve_review_comment with its id and a one-line note, if you have it.");
    }
    for (n, it) in items.iter().enumerate() {
        let at = if it.end > it.line {
            format!("{}:{}-{}", it.path, it.line, it.end)
        } else {
            format!("{}:{}", it.path, it.line)
        };
        let side = if it.side == "old" {
            ", a removed line of the base"
        } else {
            ""
        };
        out.push_str(&format!("\n\n{}. {at} (id {}{side})", n + 1, it.id));
        for l in it.code.iter().take(QUOTE_LINES) {
            out.push_str(&format!("\n   > {}", clip(l)));
        }
        if it.code.len() > QUOTE_LINES {
            out.push_str(&format!(
                "\n   > … {} more lines",
                it.code.len() - QUOTE_LINES
            ));
        }
        for c in &it.comments {
            let (text, sugg) = split_suggestion(c);
            for l in text.lines() {
                out.push_str(&format!("\n   {l}"));
            }
            if let Some(s) = sugg {
                out.push_str("\n   Suggested change:");
                out.push_str(&format!("\n   --- a/{}\n   +++ b/{}", it.path, it.path));
                out.push_str(&format!(
                    "\n   @@ -{},{} +{},{} @@",
                    it.line,
                    it.code.len(),
                    it.line,
                    s.len()
                ));
                for l in &it.code {
                    out.push_str(&format!("\n   -{l}"));
                }
                for l in &s {
                    out.push_str(&format!("\n   +{l}"));
                }
            }
        }
    }
    if let Some(b) = body.map(str::trim).filter(|b| !b.is_empty()) {
        out.push_str(&format!("\n\nOverall: {b}"));
    }
    out
}

fn same_dir(a: &str, b: &str) -> bool {
    a == b
        || Path::new(a)
            .canonicalize()
            .ok()
            .zip(Path::new(b).canonicalize().ok())
            .is_some_and(|(x, y)| x == y)
}

pub fn target<'a>(
    wt: &str,
    prefer: &[&str],
    task_term: Option<&str>,
    lines: &'a [Line],
) -> (Option<&'a Line>, Option<Channel>) {
    let mut cands: Vec<&Line> = lines
        .iter()
        .filter(|l| l.alive() && same_dir(&l.worktree_path, wt))
        .collect();
    cands.sort_by_key(|l| {
        (
            prefer.iter().position(|p| *p == l.id).unwrap_or(usize::MAX),
            -l.last_activity_ms,
        )
    });
    if let Some((l, ch)) = cands
        .iter()
        .find_map(|l| shared::channel(l).map(|c| (*l, c)))
    {
        return (Some(l), Some(ch));
    }
    let linked = |t: &Arc<Term>| {
        t.info()
            .line_id
            .and_then(|id| lines.iter().find(|l| l.id == id))
    };
    if let Some(t) = task_term.and_then(term::get).filter(|t| !t.exited()) {
        return (linked(&t), Some(Channel::Term(t)));
    }
    let t = term::all().into_iter().filter(|t| !t.exited()).find(|t| {
        let i = t.info();
        i.agent
            .as_deref()
            .is_some_and(|a| !crate::agents::is_shell(a))
            && same_dir(&i.cwd, wt)
    });
    if let Some(t) = t {
        return (linked(&t), Some(Channel::Term(t)));
    }
    (cands.first().copied(), None)
}

pub const RULES: Rules = Rules {
    settle_panes: true,
    require_output: true,
};

pub fn ready(
    line: Option<&Line>,
    ch: &Channel,
    tmux: Option<&crate::tmux::Tmux>,
) -> Result<(), String> {
    match shared::not_idle(line, ch, tmux, RULES) {
        None => Ok(()),
        Some(NotIdle::NeedsInput) => Err("the agent is waiting for input".into()),
        Some(NotIdle::Busy) => Err("the agent is busy".into()),
        Some(NotIdle::Settling) => Err("waiting for the agent's prompt".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_concise_instruction() {
        let items = vec![
            Item {
                id: "rt-1".into(),
                path: "src/a.rs".into(),
                side: "new".into(),
                line: 11,
                end: 12,
                code: vec!["    b();".into(), "    c();".into()],
                comments: vec!["Use b2.\n```suggestion\n    b2();\n```".into()],
            },
            Item {
                id: "rt-2".into(),
                path: "src/b.rs".into(),
                side: "old".into(),
                line: 4,
                end: 4,
                code: vec!["x".repeat(200)],
                comments: vec!["Why remove this?".into(), "Still unsure.".into()],
            },
        ];
        let t = format("request_changes", &items, Some(" keep the API "));
        assert!(t.starts_with("Code review: changes requested on 2 files (3 comments)."));
        assert!(t.contains("\n\n1. src/a.rs:11-12 (id rt-1)\n   >     b();\n   >     c();\n   Use b2.\n   Suggested change:\n   --- a/src/a.rs\n   +++ b/src/a.rs\n   @@ -11,2 +11,1 @@\n   -    b();\n   -    c();\n   +    b2();"), "{t}");
        assert!(t.contains("2. src/b.rs:4 (id rt-2, a removed line of the base)"));
        assert!(t.contains(&format!("> {}…", "x".repeat(160))));
        assert!(t.contains("   Why remove this?\n   Still unsure."));
        assert!(t.ends_with("\n\nOverall: keep the API"));
        assert!(!t.contains("```"));
        assert_eq!(format("approve", &[], None), "Code review: approved.");
        assert_eq!(
            format("comment", &items[..1], None).lines().next().unwrap(),
            "Code review: 1 file (1 comment). Address each one. When done with one, call the overseer MCP tool resolve_review_comment with its id and a one-line note, if you have it."
        );
    }

    #[test]
    fn splits_suggestions() {
        let (t, s) = split_suggestion("a\n```suggestion\nx\n\ny\n```\nb");
        assert_eq!(t, "a\nb");
        assert_eq!(s.unwrap(), ["x", "", "y"]);
        let (_, s) = split_suggestion("```suggestion\n```");
        assert_eq!(s.unwrap(), Vec::<String>::new());
        assert!(split_suggestion("plain").1.is_none());
    }
}
