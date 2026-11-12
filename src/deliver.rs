use crate::agents;
use crate::procs::now_ms;
use crate::term::{self, Term};
use crate::tmux::Tmux;
use crate::Line;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const SILENCE_MS: i64 = 1500;
pub const PASTE_SETTLE: Duration = Duration::from_millis(300);
const ANSWER_GAP: Duration = Duration::from_millis(200);
const CONFIRM_POLL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartupPrompt {
    AnswerForUser,
    WaitForUser,
}

#[derive(Clone)]
pub enum Channel {
    Term(Arc<Term>),
    Pane(String),
}

impl Channel {
    pub fn label(&self) -> String {
        match self {
            Channel::Term(t) => format!("term:{}", t.info().id),
            Channel::Pane(p) => format!("tmux:{p}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotIdle {
    NeedsInput,
    Busy,
    Settling,
}

#[derive(Clone, Copy, Default)]
pub struct Rules {
    pub settle_panes: bool,
    pub require_output: bool,
}

fn live(t: Arc<Term>) -> Option<Arc<Term>> {
    (!t.exited()).then_some(t)
}

pub fn line_term(l: &Line) -> Option<Arc<Term>> {
    l.term_id
        .as_deref()
        .and_then(term::get)
        .and_then(live)
        .or_else(|| {
            term::all()
                .into_iter()
                .find(|t| !t.exited() && t.info().line_id.as_deref() == Some(l.id.as_str()))
        })
}

pub fn channel(l: &Line) -> Option<Channel> {
    match (line_term(l), &l.pane) {
        (Some(t), _) => Some(Channel::Term(t)),
        (None, Some(p)) => Some(Channel::Pane(p.clone())),
        (None, None) => None,
    }
}

static SCREENS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

pub fn not_idle(
    l: Option<&Line>,
    ch: &Channel,
    tmux: Option<&Tmux>,
    rules: Rules,
) -> Option<NotIdle> {
    if let Some(l) = l {
        if l.needs_input {
            return Some(NotIdle::NeedsInput);
        }
        if l.busy_known && l.busy {
            return Some(NotIdle::Busy);
        }
    }
    let known = l.is_some_and(|l| l.busy_known);
    match ch {
        Channel::Term(t) => {
            let last = t.last_output_ms();
            if last == 0 && rules.require_output && !known {
                return Some(NotIdle::Settling);
            }
            (last > 0 && now_ms() - last < SILENCE_MS).then_some(NotIdle::Busy)
        }
        Channel::Pane(p) if rules.settle_panes && !known => {
            let h = tmux
                .and_then(|t| t.capture(p))
                .map(|s| crate::review::diff::fnv(s.as_bytes()));
            let mut g = SCREENS.lock().unwrap_or_else(|e| e.into_inner());
            let m = g.get_or_insert_with(HashMap::new);
            if m.len() >= 1024 {
                m.clear();
            }
            let prev = h.and_then(|h| m.insert(p.clone(), h));
            (h.is_none() || prev != h).then_some(NotIdle::Settling)
        }
        Channel::Pane(_) => None,
    }
}

pub fn send(ch: &Channel, text: &str, tmux: Option<&Tmux>, buffer: &str) -> Result<(), String> {
    match ch {
        Channel::Term(t) => paste(t, text).map_err(|e| e.to_string()),
        Channel::Pane(p) => {
            let tmux = tmux.ok_or("tmux is not available")?;
            insert(ch, text, Some(tmux), buffer)?;
            std::thread::sleep(PASTE_SETTLE);
            tmux.ok(&["send-keys", "-t", p, "Enter"])
        }
    }
}

pub fn paste(t: &Term, text: &str) -> std::io::Result<()> {
    term::insert(t, text)?;
    std::thread::sleep(PASTE_SETTLE);
    t.input(b"\r")
}

pub fn insert(ch: &Channel, text: &str, tmux: Option<&Tmux>, buffer: &str) -> Result<(), String> {
    match ch {
        Channel::Term(t) => term::insert(t, text).map_err(|e| e.to_string()),
        Channel::Pane(p) => {
            let tmux = tmux.ok_or("tmux is not available")?;
            tmux.load_buffer(buffer, text)?;
            tmux.ok(&["paste-buffer", "-p", "-d", "-b", buffer, "-t", p])
        }
    }
}

fn settle(t: &Term, limit: Duration, poll: Duration) -> bool {
    let begin = Instant::now();
    let (mut last, mut since) = (t.written(), Instant::now());
    while begin.elapsed() < limit {
        std::thread::sleep(poll);
        if t.exited() {
            return false;
        }
        let n = t.written();
        if n != last {
            (last, since) = (n, Instant::now());
        } else if n > 0 && since.elapsed() >= Duration::from_millis(SILENCE_MS as u64) {
            break;
        }
    }
    true
}

fn trust_answer(screen: &[u8], prompt: &agents::StartupPrompt) -> Option<&'static [u8]> {
    let s = String::from_utf8_lossy(screen);
    let mut plain = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match (c, it.peek()) {
            ('\x1b', Some('[')) => {
                it.next();
                while it.next().is_some_and(|c| !('@'..='~').contains(&c)) {}
            }
            ('\x1b', Some(']')) => while it.next().is_some_and(|c| c != '\x07' && c != '\x1b') {},
            (c, _) if c.is_whitespace() || c.is_control() => {}
            (c, _) => plain.push(c),
        }
    }
    let at = plain.rfind(prompt.marker)?;
    let cursor = plain[..at].rfind(prompt.cursor)?;
    match plain[cursor..]
        .trim_start_matches([
            prompt.cursor,
            '.',
            '0',
            '1',
            '2',
            '3',
            '4',
            '5',
            '6',
            '7',
            '8',
            '9',
        ])
        .starts_with(prompt.accept_label)
    {
        true => Some(prompt.accept),
        false => Some(prompt.select_then_accept),
    }
}

fn startup_answer(screen: &[u8]) -> Option<&'static [u8]> {
    agents::AGENTS
        .iter()
        .filter_map(|a| a.startup_prompt.as_ref())
        .find_map(|p| trust_answer(screen, p))
}

pub fn when_started(t: &Term, text: &str, startup: StartupPrompt) {
    if !settle(t, Duration::from_secs(30), Duration::from_millis(100)) {
        return;
    }
    if let Some(keys) = startup_answer(&t.screen()) {
        let mark = t.written();
        let (limit, poll) = match startup {
            StartupPrompt::AnswerForUser => (30, 100),
            StartupPrompt::WaitForUser => (86_400, 1000),
        };
        if startup == StartupPrompt::AnswerForUser {
            let _ = t.input(&keys[..keys.len() - 1]);
            std::thread::sleep(ANSWER_GAP);
            let _ = t.input(b"\r");
        }
        let begin = Instant::now();
        while t.written() == mark {
            if t.exited() || begin.elapsed() >= Duration::from_secs(limit) {
                return;
            }
            std::thread::sleep(Duration::from_millis(poll));
        }
        if !settle(t, Duration::from_secs(30), Duration::from_millis(100)) {
            return;
        }
    }
    submit(t, text, READY_WAIT, CONFIRM_POLL, CONFIRM_AFTER);
}

const READY_WAIT: Duration = Duration::from_secs(5);
const CONFIRM_AFTER: Duration = Duration::from_secs(1);
const RESENDS: usize = 3;

fn empty_prompt(t: &Term, limit: Duration, poll: Duration) -> bool {
    let begin = Instant::now();
    loop {
        if t.exited() {
            return false;
        }
        if crate::hold::input_text(&t.grid().ansi()).is_some_and(|s| s.is_empty()) {
            return true;
        }
        if begin.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(poll);
    }
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn holds(typed: &str, text: &str) -> bool {
    let head: String = squash(text).chars().take(24).collect();
    let typed = squash(typed);
    !head.is_empty() && (typed.starts_with(&head) || typed.starts_with("[Pasted"))
}

// An agent can show the pasted text yet miss the Enter that follows it. Press Enter again
// only while its input box still holds this text, so a dialog or menu that replaced the box
// is never answered by mistake.
fn submit(t: &Term, text: &str, ready_wait: Duration, poll: Duration, confirm: Duration) {
    let ready = empty_prompt(t, ready_wait, poll);
    if t.exited() || paste(t, text).is_err() {
        return;
    }
    let watch = match ready {
        true => confirm * RESENDS as u32,
        false => ready_wait * 3,
    };
    let begin = Instant::now();
    let mut resent = 0;
    while resent < RESENDS && begin.elapsed() < watch {
        std::thread::sleep(confirm);
        if t.exited() {
            return;
        }
        match crate::hold::input_text(&t.grid().ansi()) {
            Some(typed) if holds(&typed, text) => {
                let _ = t.input(b"\r");
                resent += 1;
            }
            Some(_) => return,
            None => {}
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Job {
    pub id: String,
    pub target: String,
    pub text: String,
    pub at_ms: i64,
}

pub struct Queue(Mutex<Vec<Job>>);

impl Queue {
    pub const fn new() -> Queue {
        Queue(Mutex::new(vec![]))
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Job>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn push(&self, j: Job) {
        let mut g = self.lock();
        if !g.iter().any(|x| x.id == j.id && x.target == j.target) {
            g.push(j);
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.lock().iter().any(|j| j.id == id)
    }

    pub fn remove(&self, id: &str) {
        self.lock().retain(|j| j.id != id);
    }

    pub fn snapshot(&self) -> Vec<Job> {
        self.lock().clone()
    }

    pub fn drain(&self, mut keep: impl FnMut(&Job) -> bool) {
        let pending = std::mem::take(&mut *self.lock());
        let mut kept: Vec<Job> = pending.into_iter().filter(|j| keep(j)).collect();
        let mut g = self.lock();
        kept.append(&mut g);
        *g = kept;
    }
}

impl Default for Queue {
    fn default() -> Self {
        Queue::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_the_claude_trust_dialog_with_yes() {
        let no_first = b"\x1b[2GQuick\x1b[8Gsafety check\r\n\x1b[2G\x1b[38;2;177;185;249m\xe2\x9d\xaf\x1b[4GNo,\x1b[8Gexit\x1b[39m\r\r\n\x1b[4GYes,\x1b[9GI\x1b[11Gtrust\x1b[17Gthis\x1b[22Gfolder\r\n\x1b]11;?\x07";
        assert_eq!(startup_answer(no_first), Some(&b"\x1b[B\r"[..]));
        let yes_first = "❯ 1. Yes, I trust this folder\r\n  2. No, exit".as_bytes();
        assert_eq!(startup_answer(yes_first), Some(&b"\r"[..]));
        assert_eq!(startup_answer("❯ hello, I trust you".as_bytes()), None);
    }

    fn unsent(screen: &str, text: &str) -> bool {
        let shown = crate::screen::render(screen.as_bytes(), 60, 12).ansi();
        crate::hold::input_text(&shown).is_some_and(|typed| holds(&typed, text))
    }

    #[test]
    fn enter_is_pressed_again_only_while_the_box_holds_the_text() {
        let rule = "─".repeat(30);
        let text = "Fix the login\n\nThe button does nothing when clicked twice.";
        let typed = format!(
            "{rule}\r\n❯ Fix the login The button does nothing when\r\nclicked twice.\r\n{rule}"
        );
        assert!(unsent(&typed, text));
        let pasted = format!("{rule}\r\n❯ [Pasted text #1 +40 lines]\r\n{rule}");
        assert!(unsent(&pasted, text));
        let codex = "› [Pasted Content 4120 chars]\r\n\r\n  ⏎ send";
        assert!(unsent(codex, text));
        let empty = format!("● Working\r\n{rule}\r\n❯ \x1b[2mTry \"fix lint\"\x1b[0m\r\n{rule}");
        assert!(!unsent(&empty, text));
        let other = format!("{rule}\r\n❯ something the user typed\r\n{rule}");
        assert!(!unsent(&other, text));
        let menu = "Do you want to proceed?\r\n❯ 1. Yes\r\n  2. No, exit";
        assert!(!unsent(menu, text));
        assert!(!unsent("Loading…", text));
        assert!(!unsent(&typed, ""));
    }

    #[cfg(unix)]
    fn agent(script: &str) -> Arc<Term> {
        term::spawn(term::Spec {
            cwd: std::env::temp_dir().to_string_lossy().into_owned(),
            cmd: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        })
        .unwrap()
    }

    #[cfg(unix)]
    fn wait_screen(t: &Term, word: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let s = t.grid().text();
            if s.contains(word) || Instant::now() >= deadline {
                return s;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(unix)]
    const RULE: &str = "r=────────────────────────────────; ";

    #[cfg(unix)]
    #[test]
    fn a_dropped_enter_is_pressed_again() {
        let t = agent(&format!(
            r#"{RULE}sleep 0.3; printf '%s\n❯ ' "$r"; read a; printf '%s\n❯ %s\n%s\n' "$r" "$a" "$r"; read b; echo "got: $a"; sleep 30"#
        ));
        let ms = Duration::from_millis;
        submit(&t, "Fix it\n\nnow", ms(3000), ms(20), ms(200));
        let s = wait_screen(&t, "got:");
        t.kill().unwrap();
        term::remove(&t.info().id);
        assert!(s.contains("got: Fix it  now"), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn a_dropped_enter_is_pressed_again_when_the_prompt_shows_late() {
        let t = agent(&format!(
            r#"{RULE}read a; printf '%s\n❯ %s\n%s\n' "$r" "$a" "$r"; read b; echo "got: $a"; sleep 30"#
        ));
        let ms = Duration::from_millis;
        submit(&t, "Fix it", ms(300), ms(20), ms(200));
        let s = wait_screen(&t, "got:");
        t.kill().unwrap();
        term::remove(&t.info().id);
        assert!(s.contains("got: Fix it"), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn a_submitted_prompt_gets_no_extra_enter() {
        let t = agent(&format!(
            r#"{RULE}printf '%s\n❯ ' "$r"; read a; echo "got: $a"; printf '%s\n❯ \n%s\n' "$r" "$r"; read b; echo "extra: $b"; sleep 30"#
        ));
        let ms = Duration::from_millis;
        submit(&t, "Fix it", ms(3000), ms(20), ms(500));
        std::thread::sleep(ms(300));
        let s = t.grid().text();
        t.kill().unwrap();
        term::remove(&t.info().id);
        assert!(s.contains("got: Fix it") && !s.contains("extra:"), "{s}");
    }

    #[cfg(unix)]
    #[test]
    fn a_menu_after_the_paste_is_never_answered() {
        let t = agent(&format!(
            r#"{RULE}printf '%s\n❯ ' "$r"; read a; printf 'Proceed?\n❯ 1. Yes\n  2. No\n'; read b; echo "answered: $b"; sleep 30"#
        ));
        let ms = Duration::from_millis;
        submit(&t, "Fix it", ms(3000), ms(20), ms(500));
        std::thread::sleep(ms(300));
        let s = t.grid().text();
        t.kill().unwrap();
        term::remove(&t.info().id);
        assert!(s.contains("Proceed?") && !s.contains("answered:"), "{s}");
    }

    #[test]
    fn needs_input_and_busy_lines_are_never_idle() {
        let ch = Channel::Pane("%1".into());
        let mut l = Line {
            needs_input: true,
            busy_known: true,
            ..Default::default()
        };
        assert_eq!(
            not_idle(Some(&l), &ch, None, Rules::default()),
            Some(NotIdle::NeedsInput)
        );
        l.needs_input = false;
        l.busy = true;
        assert_eq!(
            not_idle(Some(&l), &ch, None, Rules::default()),
            Some(NotIdle::Busy)
        );
        l.busy = false;
        assert_eq!(
            not_idle(
                Some(&l),
                &ch,
                None,
                Rules {
                    settle_panes: true,
                    ..Default::default()
                }
            ),
            None
        );
        assert_eq!(not_idle(None, &ch, None, Rules::default()), None);
        assert_eq!(
            not_idle(
                None,
                &ch,
                None,
                Rules {
                    settle_panes: true,
                    ..Default::default()
                }
            ),
            Some(NotIdle::Settling)
        );
    }

    #[test]
    fn queue_keeps_order_dedups_and_drains() {
        let q = Queue::new();
        for (id, t) in [("a", "x"), ("b", "y"), ("a", "x")] {
            q.push(Job {
                id: id.into(),
                target: t.into(),
                text: String::new(),
                at_ms: 0,
            });
        }
        assert!(q.contains("a") && q.contains("b"));
        let mut seen = vec![];
        q.drain(|j| {
            seen.push(j.id.clone());
            j.id == "b"
        });
        assert_eq!(seen, ["a", "b"]);
        assert!(!q.contains("a") && q.contains("b"));
        q.remove("b");
        assert!(!q.contains("b"));
    }
}
