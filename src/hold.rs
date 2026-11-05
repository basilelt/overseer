use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn check(name: &'static str, ok: bool, detail: impl Into<String>) -> Check {
    Check {
        name,
        ok,
        detail: if ok { String::new() } else { detail.into() },
    }
}

#[derive(Default, Clone)]
pub struct HoldInput {
    pub agent: String,
    pub alive: bool,
    pub busy: Option<bool>,
    pub last_event: Option<String>,
    pub pane: Option<String>,
    pub term: Option<String>,
    pub screen: Option<String>,
    pub children: Vec<Vec<String>>,
    pub mcp_hints: Vec<String>,
    pub subagents: i64,
    pub tasks: i64,
    pub pinned: bool,
    pub needs_input: bool,
    pub duplicate: bool,
}

pub fn alive(alive: bool, duplicate: bool) -> Check {
    if duplicate {
        return check("alive", false, "two live processes share this session id");
    }
    check("alive", alive, "no live agent process for this line")
}

pub fn in_tmux(pane: Option<&str>, term: Option<&str>) -> Check {
    check(
        "inTmux",
        pane.is_some() || term.is_some(),
        "agent is neither in a tmux pane nor in an Overseer terminal",
    )
}

pub fn at_prompt(busy: Option<bool>, last_event: Option<&str>) -> Check {
    match busy {
        None => check(
            "atPrompt",
            false,
            "turn state unknown (no hook event or status)",
        ),
        Some(true) => check("atPrompt", false, "agent is mid-turn"),
        Some(false) => check(
            "atPrompt",
            last_event != Some("UserPromptSubmit"),
            "agent is mid-turn",
        ),
    }
}

pub fn not_pinned(pinned: bool) -> Check {
    check("notPinned", !pinned, "line is pinned")
}

pub fn not_needs_input(needs_input: bool) -> Check {
    check("notNeedsInput", !needs_input, "agent is waiting for you")
}

pub fn is_tool_child(argv: &[String]) -> bool {
    argv.iter().any(|a| a.contains("shell-snapshots"))
}

pub fn is_mcp_child(argv: &[String], hints: &[String]) -> bool {
    let joined = argv.join(" ");
    let lower = joined.to_ascii_lowercase();
    lower.contains("mcp")
        || hints
            .iter()
            .any(|h| !h.is_empty() && joined.contains(h.as_str()))
}

pub fn no_tool_running(
    children: &[Vec<String>],
    hints: &[String],
    subagents: i64,
    tasks: i64,
) -> Check {
    if subagents > 0 {
        return check("noToolRunning", false, "a subagent is running");
    }
    if tasks > 0 {
        return check("noToolRunning", false, "a background task is running");
    }
    for c in children {
        if is_tool_child(c) {
            return check("noToolRunning", false, "a tool subprocess is running");
        }
        if !is_mcp_child(c, hints) {
            let name = c
                .first()
                .map(|a| a.rsplit('/').next().unwrap_or(a))
                .unwrap_or("?");
            return check(
                "noToolRunning",
                false,
                format!("unknown child process '{name}' is running"),
            );
        }
    }
    check("noToolRunning", true, "")
}

#[derive(Clone, Copy, PartialEq)]
struct Cell {
    c: char,
    dim: bool,
}

pub(crate) fn sgr_dim(params: &str, dim: &mut bool) {
    let ps: Vec<u32> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
    let mut i = 0;
    while i < ps.len() {
        match ps[i] {
            0 | 22 | 39 => *dim = false,
            2 | 90 => *dim = true,
            30..=37 | 91..=97 => *dim = false,
            38 if ps.get(i + 1) == Some(&5) => {
                let n = ps.get(i + 2).copied().unwrap_or(0);
                *dim = n == 8 || (232..=250).contains(&n);
                i += 2;
            }
            38 if ps.get(i + 1) == Some(&2) => {
                let (r, g, b) = (
                    ps.get(i + 2).copied().unwrap_or(0),
                    ps.get(i + 3).copied().unwrap_or(0),
                    ps.get(i + 4).copied().unwrap_or(0),
                );
                *dim = r == g && g == b && r < 170;
                i += 4;
            }
            _ => {}
        }
        i += 1;
    }
}

fn cells(screen: &str) -> Vec<Vec<Cell>> {
    let mut lines = vec![];
    let mut cur = vec![];
    let mut dim = false;
    let mut it = screen.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                Some('[') => {
                    let mut params = String::new();
                    for d in it.by_ref() {
                        if ('\x40'..='\x7e').contains(&d) {
                            if d == 'm' {
                                sgr_dim(&params, &mut dim);
                            }
                            break;
                        }
                        params.push(d);
                    }
                }
                Some(']') => {
                    while let Some(d) = it.next() {
                        if d == '\x07' || (d == '\x1b' && it.peek() == Some(&'\\')) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' => lines.push(std::mem::take(&mut cur)),
            '\r' => {}
            c => cur.push(Cell { c, dim }),
        }
    }
    lines.push(cur);
    lines
}

const MARKERS: &[char] = &['>', '❯', '›'];
const BOX: &[char] = &['│', '┃', '|', '╭', '╮', '╰', '╯', '▌'];

fn trim_box(l: &[Cell]) -> &[Cell] {
    let mut s = 0;
    let mut e = l.len();
    while s < e && (l[s].c.is_whitespace() || BOX.contains(&l[s].c)) {
        s += 1;
    }
    while e > s && (l[e - 1].c.is_whitespace() || BOX.contains(&l[e - 1].c)) {
        e -= 1;
    }
    &l[s..e]
}

fn is_rule(l: &[Cell]) -> bool {
    let t = trim_box(l);
    t.len() >= 10 && t.iter().all(|c| matches!(c.c, '─' | '━' | '-' | '╌' | '═'))
}

pub fn input_empty(screen: Option<&str>) -> Check {
    let Some(screen) = screen else {
        return check("inputEmpty", false, "terminal content unavailable");
    };
    let lines = cells(screen);
    let last = lines.iter().rposition(|l| !trim_box(l).is_empty());
    let Some(last) = last else {
        return check("inputEmpty", false, "prompt not found on screen");
    };
    let from = last.saturating_sub(15);
    let prompt = (from..=last).rev().find(|&i| {
        let t = trim_box(&lines[i]);
        !t.is_empty()
            && !t[0].dim
            && MARKERS.contains(&t[0].c)
            && t.get(1)
                .is_none_or(|c| c.c.is_whitespace() || c.c == '\u{a0}')
    });
    let Some(p) = prompt else {
        return check("inputEmpty", false, "prompt not found on screen");
    };
    let mut rest: Vec<Cell> = trim_box(&lines[p])[1..].to_vec();
    for l in &lines[p + 1..=last] {
        if is_rule(l) {
            break;
        }
        rest.extend_from_slice(trim_box(l));
    }
    let typed = rest
        .iter()
        .any(|c| !c.c.is_whitespace() && c.c != '\u{a0}' && !c.dim);
    check("inputEmpty", !typed, "input box is not empty")
}

pub fn evaluate(i: &HoldInput) -> Vec<Check> {
    vec![
        alive(i.alive, i.duplicate),
        in_tmux(i.pane.as_deref(), i.term.as_deref()),
        at_prompt(i.busy, i.last_event.as_deref()),
        input_empty(i.screen.as_deref()),
        no_tool_running(&i.children, &i.mcp_hints, i.subagents, i.tasks),
        not_pinned(i.pinned),
        not_needs_input(i.needs_input),
    ]
}

pub fn to_json(checks: &[Check]) -> Value {
    let mut m = serde_json::Map::new();
    for c in checks {
        let mut v = json!({ "ok": c.ok });
        if !c.ok {
            v["detail"] = json!(c.detail);
        }
        m.insert(c.name.to_string(), v);
    }
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    const RULE: &str = "────────────────────────────────────";

    #[test]
    fn empty_prompt_passes() {
        let screen = format!(
            "answer text\n> quoted line in output\n{RULE}\n❯ \n{RULE}\n  ? for shortcuts\n\n"
        );
        assert!(input_empty(Some(&screen)).ok);
        assert!(input_empty(Some(&format!("{RULE}\n│ > \u{a0}  │\n{RULE}"))).ok);
        let boxed = "╭──────────────────────╮\n│ >                    │\n╰──────────────────────╯\n  status line";
        assert!(input_empty(Some(boxed)).ok);
    }

    #[test]
    fn claude_2_1_layout_with_status_lines() {
        let screen = format!(
            "● done\n\n{RULE}\n❯ \n{RULE}\n  [model] | [ctx] | xxx 1.2 | 5h 10%\n  ⏵⏵ accept edits on (shift+tab to cycle) · ← for agents\n{:>60}\n",
            "/ide"
        );
        assert!(input_empty(Some(&screen)).ok);
        let typed = screen.replace("❯ \n", "❯ fix the build\n");
        assert!(!input_empty(Some(&typed)).ok);
    }

    #[test]
    fn typed_text_refuses() {
        let screen = format!("{RULE}\n❯ half typed\n{RULE}\n  ? for shortcuts");
        let c = input_empty(Some(&screen));
        assert!(!c.ok);
        assert_eq!(c.detail, "input box is not empty");
        let multi = format!("{RULE}\n> \n  second line typed\n{RULE}");
        assert!(!input_empty(Some(&multi)).ok);
    }

    #[test]
    fn dim_placeholder_counts_as_empty() {
        let screen = format!("{RULE}\n❯ \x1b[2mTry \"fix lint errors\"\x1b[0m\n{RULE}");
        assert!(input_empty(Some(&screen)).ok);
        let gray = format!("{RULE}\n❯ \x1b[38;5;244msuggestion\x1b[39m\n{RULE}");
        assert!(input_empty(Some(&gray)).ok);
        let typed = format!("{RULE}\n❯ \x1b[2mhint\x1b[0m typed\n{RULE}");
        assert!(!input_empty(Some(&typed)).ok);
    }

    #[test]
    fn unknown_screen_refuses() {
        assert!(!input_empty(None).ok);
        assert!(!input_empty(Some("")).ok);
        assert!(!input_empty(Some("Do you want to proceed?\n 1. Yes\n 2. No")).ok);
        assert!(!input_empty(Some("$ ")).ok);
    }

    #[test]
    fn codex_prompt() {
        assert!(!input_empty(Some("› \n\n  ⏎ send")).ok);
        assert!(input_empty(Some("output\n› ")).ok);
        assert!(!input_empty(Some("output\n› typed")).ok);
    }

    #[test]
    fn mid_turn_refuses() {
        assert!(at_prompt(Some(false), Some("Stop")).ok);
        assert!(!at_prompt(Some(true), Some("Stop")).ok);
        assert!(!at_prompt(Some(false), Some("UserPromptSubmit")).ok);
        assert!(!at_prompt(None, None).ok);
        assert_eq!(at_prompt(Some(true), None).detail, "agent is mid-turn");
    }

    #[test]
    fn running_tool_refuses() {
        let tool = s(&[
            "/bin/zsh",
            "-c",
            "source ~/.claude/shell-snapshots/snapshot-zsh-1.sh && npm test",
        ]);
        let mcp = s(&["node", "/x/server-filesystem-mcp/index.js"]);
        let custom = s(&["/opt/tools/linear-server", "--stdio"]);
        assert!(no_tool_running(&[], &[], 0, 0).ok);
        assert!(no_tool_running(std::slice::from_ref(&mcp), &[], 0, 0).ok);
        assert!(!no_tool_running(&[mcp.clone(), tool], &[], 0, 0).ok);
        assert!(!no_tool_running(std::slice::from_ref(&custom), &[], 0, 0).ok);
        assert!(no_tool_running(&[custom], &s(&["linear-server"]), 0, 0).ok);
        assert_eq!(
            no_tool_running(&[], &[], 1, 0).detail,
            "a subagent is running"
        );
        assert_eq!(
            no_tool_running(&[], &[], 0, 2).detail,
            "a background task is running"
        );
    }

    #[test]
    fn pinned_and_needs_input_refuse() {
        assert!(not_pinned(false).ok);
        assert_eq!(not_pinned(true).detail, "line is pinned");
        assert!(not_needs_input(false).ok);
        assert!(!not_needs_input(true).ok);
        assert!(!alive(true, true).ok);
        assert!(!alive(false, false).ok);
        assert!(!in_tmux(None, None).ok);
        assert!(in_tmux(None, Some("t1")).ok);
    }

    #[test]
    fn evaluate_reports_every_predicate() {
        let i = HoldInput {
            alive: true,
            busy: Some(false),
            pane: Some("%1".into()),
            screen: Some(format!("{RULE}\n❯ \n{RULE}")),
            ..Default::default()
        };
        let r = evaluate(&i);
        assert_eq!(r.len(), 7);
        assert!(r.iter().all(|c| c.ok), "{r:?}");
        let j = to_json(&r);
        assert_eq!(j["inputEmpty"]["ok"], true);
    }

    #[test]
    fn daemon_term_lines_pass_the_terminal_check() {
        let rule = "─".repeat(30);
        let raw = format!(
            "● Done.\r\n{rule}\r\n❯ \x1b[2mTry \"fix lint\"\x1b[0m\r\n{rule}\r\n  ? for shortcuts"
        );
        let i = HoldInput {
            alive: true,
            busy: Some(false),
            term: Some("t1".into()),
            screen: Some(crate::screen::render(raw.as_bytes(), 40, 8).ansi()),
            ..Default::default()
        };
        let r = evaluate(&i);
        assert!(r.iter().all(|c| c.ok), "{r:?}");
        for bad in [
            HoldInput {
                busy: Some(true),
                ..i.clone()
            },
            HoldInput {
                pinned: true,
                ..i.clone()
            },
            HoldInput {
                needs_input: true,
                ..i.clone()
            },
            HoldInput {
                term: None,
                ..i.clone()
            },
        ] {
            assert!(!evaluate(&bad).iter().all(|c| c.ok));
        }
    }
}
