use crate::procs::now_ms;
use crate::term::{self, Term};
use crate::tmux::Tmux;
use crate::Line;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const SILENCE_MS: i64 = 1500;

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
        Channel::Term(t) => term::paste(t, text).map_err(|e| e.to_string()),
        Channel::Pane(p) => {
            let tmux = tmux.ok_or("tmux is not available")?;
            insert(ch, text, Some(tmux), buffer)?;
            std::thread::sleep(Duration::from_millis(300));
            tmux.ok(&["send-keys", "-t", p, "Enter"])
        }
    }
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

fn trust_answer(screen: &[u8]) -> Option<&'static [u8]> {
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
    let at = plain.rfind("Itrustthisfolder")?;
    let cursor = plain[..at].rfind('❯')?;
    match plain[cursor..]
        .trim_start_matches(['❯', '.', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9'])
        .starts_with("Yes")
    {
        true => Some(b"\r"),
        false => Some(b"\x1b[B\r"),
    }
}

pub fn when_started(t: &Term, text: &str, own_dir: bool) {
    if !settle(t, Duration::from_secs(30), Duration::from_millis(100)) {
        return;
    }
    if let Some(keys) = trust_answer(&t.screen()) {
        let mark = t.written();
        let (limit, poll) = match own_dir {
            true => (30, 100),
            false => (86_400, 1000),
        };
        if own_dir {
            let _ = t.input(&keys[..keys.len() - 1]);
            std::thread::sleep(Duration::from_millis(200));
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
    let _ = term::paste(t, text);
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
        assert_eq!(trust_answer(no_first), Some(&b"\x1b[B\r"[..]));
        let yes_first = "❯ 1. Yes, I trust this folder\r\n  2. No, exit".as_bytes();
        assert_eq!(trust_answer(yes_first), Some(&b"\r"[..]));
        assert_eq!(trust_answer("❯ hello, I trust you".as_bytes()), None);
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
