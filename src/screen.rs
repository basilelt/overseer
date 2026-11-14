use crate::deliver::Channel;
use crate::hold::sgr_dim;
use crate::http::err;
use crate::Ctx;
use serde_json::{json, Value};
use std::time::Duration;

pub const TAIL: usize = 256 << 10;
const MAX_KEYS: usize = 32;
const KEY_GAP: Duration = Duration::from_millis(40);
const MAX_TEXT: usize = 64 << 10;

#[derive(Clone, Copy, PartialEq, Debug)]
struct Cell {
    c: char,
    dim: bool,
}

const BLANK: Cell = Cell { c: ' ', dim: false };

pub struct Grid {
    rows: Vec<Vec<Cell>>,
}

impl Grid {
    fn lines(&self) -> Vec<&[Cell]> {
        let mut out: Vec<&[Cell]> = self
            .rows
            .iter()
            .map(|r| {
                let end = r.iter().rposition(|c| c.c != ' ').map_or(0, |i| i + 1);
                &r[..end]
            })
            .collect();
        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        out
    }

    pub fn text(&self) -> String {
        self.lines()
            .iter()
            .map(|l| l.iter().map(|c| c.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn ansi(&self) -> String {
        self.lines()
            .iter()
            .map(|l| {
                let mut s = String::new();
                let mut dim = false;
                for c in l.iter() {
                    if c.dim != dim {
                        s.push_str(if c.dim { "\x1b[2m" } else { "\x1b[0m" });
                        dim = c.dim;
                    }
                    s.push(c.c);
                }
                if dim {
                    s.push_str("\x1b[0m");
                }
                s
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

enum St {
    Ground,
    Esc,
    EscSkip,
    Csi(String),
    Str,
    StrEsc,
}

struct Vt {
    cols: usize,
    rows: usize,
    screen: Vec<Vec<Cell>>,
    other: Vec<Vec<Cell>>,
    alt: bool,
    x: usize,
    y: usize,
    dim: bool,
    top: usize,
    bottom: usize,
    saved: (usize, usize, bool),
    wrap: bool,
}

fn blank_screen(cols: usize, rows: usize) -> Vec<Vec<Cell>> {
    vec![vec![BLANK; cols]; rows]
}

impl Vt {
    fn new(cols: usize, rows: usize) -> Vt {
        Vt {
            cols,
            rows,
            screen: blank_screen(cols, rows),
            other: blank_screen(cols, rows),
            alt: false,
            x: 0,
            y: 0,
            dim: false,
            top: 0,
            bottom: rows - 1,
            saved: (0, 0, false),
            wrap: false,
        }
    }

    fn scroll_up(&mut self, n: usize) {
        for _ in 0..n.min(self.bottom + 1 - self.top) {
            self.screen.remove(self.top);
            self.screen.insert(self.bottom, vec![BLANK; self.cols]);
        }
    }

    fn scroll_down(&mut self, n: usize) {
        for _ in 0..n.min(self.bottom + 1 - self.top) {
            self.screen.remove(self.bottom);
            self.screen.insert(self.top, vec![BLANK; self.cols]);
        }
    }

    fn lf(&mut self) {
        if self.y == self.bottom {
            self.scroll_up(1);
        } else if self.y + 1 < self.rows {
            self.y += 1;
        }
    }

    fn put(&mut self, c: char) {
        if self.wrap {
            self.wrap = false;
            self.x = 0;
            self.lf();
        }
        self.screen[self.y][self.x] = Cell { c, dim: self.dim };
        if self.x + 1 < self.cols {
            self.x += 1;
        } else {
            self.wrap = true;
        }
    }

    fn goto(&mut self, x: usize, y: usize) {
        self.x = x.min(self.cols - 1);
        self.y = y.min(self.rows - 1);
        self.wrap = false;
    }

    fn erase(&mut self, y: usize, from: usize, to: usize) {
        let to = to.min(self.cols);
        for c in self.screen[y].iter_mut().take(to).skip(from) {
            *c = BLANK;
        }
    }

    fn set_alt(&mut self, on: bool, save: bool) {
        if on == self.alt {
            return;
        }
        if on && save {
            self.saved = (self.x, self.y, self.dim);
        }
        std::mem::swap(&mut self.screen, &mut self.other);
        self.alt = on;
        if on {
            self.screen = blank_screen(self.cols, self.rows);
        } else if save {
            let (x, y, d) = self.saved;
            self.goto(x, y);
            self.dim = d;
        }
    }

    fn csi(&mut self, seq: &str, fin: char) {
        let private = seq.starts_with(['?', '>', '<', '=']);
        let body = seq.trim_start_matches(['?', '>', '<', '=']);
        let body: String = body.chars().filter(|c| !(' '..='/').contains(c)).collect();
        let ps: Vec<usize> = body.split(';').map(|p| p.parse().unwrap_or(0)).collect();
        let n = |i: usize| ps.get(i).copied().filter(|v| *v > 0).unwrap_or(1);
        if private {
            if matches!(fin, 'h' | 'l') && seq.starts_with('?') {
                for p in &ps {
                    match p {
                        1049 => self.set_alt(fin == 'h', true),
                        47 | 1047 => self.set_alt(fin == 'h', false),
                        _ => {}
                    }
                }
            }
            return;
        }
        match fin {
            'A' => self.goto(self.x, self.y.saturating_sub(n(0))),
            'B' | 'e' => self.goto(self.x, self.y + n(0)),
            'C' | 'a' => self.goto(self.x + n(0), self.y),
            'D' => self.goto(self.x.saturating_sub(n(0)), self.y),
            'E' => self.goto(0, self.y + n(0)),
            'F' => self.goto(0, self.y.saturating_sub(n(0))),
            'G' | '`' => self.goto(n(0) - 1, self.y),
            'd' => self.goto(self.x, n(0) - 1),
            'H' | 'f' => self.goto(n(1) - 1, n(0) - 1),
            'K' => match ps[0] {
                0 => self.erase(self.y, self.x, self.cols),
                1 => self.erase(self.y, 0, self.x + 1),
                _ => self.erase(self.y, 0, self.cols),
            },
            'J' => match ps[0] {
                0 => {
                    self.erase(self.y, self.x, self.cols);
                    for y in self.y + 1..self.rows {
                        self.erase(y, 0, self.cols);
                    }
                }
                1 => {
                    for y in 0..self.y {
                        self.erase(y, 0, self.cols);
                    }
                    self.erase(self.y, 0, self.x + 1);
                }
                _ => self.screen = blank_screen(self.cols, self.rows),
            },
            'X' => self.erase(self.y, self.x, self.x + n(0)),
            '@' => {
                let row = &mut self.screen[self.y];
                for _ in 0..n(0).min(self.cols - self.x) {
                    row.pop();
                    row.insert(self.x, BLANK);
                }
            }
            'P' => {
                let row = &mut self.screen[self.y];
                for _ in 0..n(0).min(self.cols - self.x) {
                    row.remove(self.x);
                    row.push(BLANK);
                }
            }
            'L' | 'M' if (self.top..=self.bottom).contains(&self.y) => {
                let top = self.top;
                self.top = self.y;
                if fin == 'L' {
                    self.scroll_down(n(0));
                } else {
                    self.scroll_up(n(0));
                }
                self.top = top;
            }
            'S' => self.scroll_up(n(0)),
            'T' => self.scroll_down(n(0)),
            'r' => {
                let t = n(0) - 1;
                let b = ps
                    .get(1)
                    .copied()
                    .filter(|v| *v > 0)
                    .unwrap_or(self.rows)
                    .min(self.rows)
                    - 1;
                if t < b {
                    (self.top, self.bottom) = (t, b);
                    self.goto(0, 0);
                }
            }
            's' => self.saved = (self.x, self.y, self.dim),
            'u' => {
                let (x, y, d) = self.saved;
                self.goto(x, y);
                self.dim = d;
            }
            'm' => sgr_dim(&body, &mut self.dim),
            _ => {}
        }
    }

    fn feed(&mut self, text: &str) {
        let mut st = St::Ground;
        for c in text.chars() {
            st = match st {
                St::Ground => match c {
                    '\x1b' => St::Esc,
                    '\r' => {
                        self.goto(0, self.y);
                        St::Ground
                    }
                    '\n' | '\x0b' | '\x0c' => {
                        self.wrap = false;
                        self.lf();
                        St::Ground
                    }
                    '\x08' => {
                        self.goto(self.x.saturating_sub(1), self.y);
                        St::Ground
                    }
                    '\t' => {
                        self.goto((self.x / 8 + 1) * 8, self.y);
                        St::Ground
                    }
                    c if (c as u32) < 0x20 || c == '\x7f' => St::Ground,
                    c => {
                        self.put(c);
                        St::Ground
                    }
                },
                St::Esc => match c {
                    '[' => St::Csi(String::new()),
                    ']' | 'P' | '_' | '^' | 'X' => St::Str,
                    '7' => {
                        self.saved = (self.x, self.y, self.dim);
                        St::Ground
                    }
                    '8' => {
                        let (x, y, d) = self.saved;
                        self.goto(x, y);
                        self.dim = d;
                        St::Ground
                    }
                    'M' => {
                        if self.y == self.top {
                            self.scroll_down(1);
                        } else {
                            self.goto(self.x, self.y.saturating_sub(1));
                        }
                        St::Ground
                    }
                    'D' => {
                        self.lf();
                        St::Ground
                    }
                    'E' => {
                        self.goto(0, self.y);
                        self.lf();
                        St::Ground
                    }
                    'c' => {
                        *self = Vt::new(self.cols, self.rows);
                        St::Ground
                    }
                    ' '..='/' => St::EscSkip,
                    _ => St::Ground,
                },
                St::EscSkip => St::Ground,
                St::Csi(mut s) => {
                    if ('\x40'..='\x7e').contains(&c) {
                        self.csi(&s, c);
                        St::Ground
                    } else if s.len() > 64 {
                        St::Ground
                    } else {
                        s.push(c);
                        St::Csi(s)
                    }
                }
                St::Str => match c {
                    '\x07' => St::Ground,
                    '\x1b' => St::StrEsc,
                    _ => St::Str,
                },
                St::StrEsc => match c {
                    '\\' => St::Ground,
                    _ => St::Str,
                },
            };
        }
    }
}

pub fn render(bytes: &[u8], cols: u16, rows: u16) -> Grid {
    let cols = cols.clamp(1, 1000) as usize;
    let rows = rows.clamp(1, 1000) as usize;
    let tail = &bytes[bytes.len().saturating_sub(TAIL)..];
    let mut vt = Vt::new(cols, rows);
    vt.feed(&String::from_utf8_lossy(tail));
    Grid { rows: vt.screen }
}

pub fn plain(captured: &str) -> Grid {
    let lines: Vec<&str> = captured.lines().collect();
    let cols = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .clamp(1, 1000);
    render(
        captured.replace('\n', "\r\n").as_bytes(),
        cols as u16,
        (lines.len() + 1).min(1000) as u16,
    )
}

pub fn key_bytes(key: &str) -> Option<Vec<u8>> {
    let named: &[u8] = match key {
        "esc" => b"\x1b",
        "enter" => b"\r",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "tab" => b"\t",
        "shift-tab" => b"\x1b[Z",
        "ctrl-c" => b"\x03",
        "space" => b" ",
        "backspace" => b"\x7f",
        _ => {
            let mut it = key.chars();
            return match (it.next(), it.next()) {
                (Some(c), None) if !c.is_control() => Some(c.to_string().into_bytes()),
                _ => None,
            };
        }
    };
    Some(named.to_vec())
}

pub fn parse_text(v: &Value) -> Result<Option<String>, String> {
    match &v["text"] {
        Value::Null => Ok(None),
        Value::String(s) if s.is_empty() || s.len() > MAX_TEXT => {
            Err(format!("text must be 1 to {MAX_TEXT} bytes"))
        }
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err("text must be a string".into()),
    }
}

pub fn parse_keys(v: &Value) -> Result<Vec<Vec<u8>>, String> {
    if v["keys"].is_null() && !v["text"].is_null() {
        return Ok(vec![]);
    }
    let keys = v["keys"].as_array().ok_or("keys must be an array")?;
    if keys.is_empty() && !v["text"].is_null() {
        return Ok(vec![]);
    }
    if keys.is_empty() || keys.len() > MAX_KEYS {
        return Err(format!("give 1 to {MAX_KEYS} keys"));
    }
    keys.iter()
        .map(|k| {
            k.as_str()
                .and_then(key_bytes)
                .ok_or_else(|| format!("unknown key {k}"))
        })
        .collect()
}

fn channel_of(ctx: &Ctx, id: &str) -> Result<(String, Channel), (u16, Value)> {
    let lines = crate::daemon::cached_lines(ctx, Some(id)).map_err(|e| err(500, e))?;
    let l = crate::actions::find_line(&lines, id).map_err(|e| err(404, e))?;
    match crate::deliver::channel(l) {
        Some(ch) => Ok((l.id.clone(), ch)),
        None => Err(err(409, format!("line {} has no live terminal", l.id))),
    }
}

pub fn screen_route(ctx: &Ctx, id: &str) -> (u16, Value) {
    let (line, ch) = match channel_of(ctx, id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    match ch {
        Channel::Term(t) => {
            let i = t.info();
            json_screen(&line, &t.grid(), i.cols, i.rows, "term")
        }
        Channel::Pane(p) => match ctx.tmux.as_ref().and_then(|t| t.capture(&p)) {
            Some(s) => {
                let g = plain(&s);
                let cols = g.rows.first().map_or(0, |r| r.len()) as u16;
                json_screen(&line, &g, cols, g.rows.len() as u16, "tmux")
            }
            None => err(409, format!("cannot capture tmux pane {p}")),
        },
    }
}

fn json_screen(line: &str, g: &Grid, cols: u16, rows: u16, source: &str) -> (u16, Value) {
    (
        200,
        json!({"lineId": line, "text": g.text(), "cols": cols, "rows": rows, "source": source}),
    )
}

pub fn keys_route(ctx: &Ctx, id: &str, body: &Value) -> (u16, Value) {
    let (keys, text) = match parse_keys(body).and_then(|k| Ok((k, parse_text(body)?))) {
        Ok(v) => v,
        Err(e) => return err(400, e),
    };
    let (line, ch) = match channel_of(ctx, id) {
        Ok(v) => v,
        Err(e) => return e,
    };
    for (n, k) in keys.iter().enumerate() {
        if n > 0 {
            std::thread::sleep(KEY_GAP);
        }
        let sent = match &ch {
            Channel::Term(t) => t.input(k).map_err(|e| e.to_string()),
            Channel::Pane(p) => {
                let Some(tmux) = ctx.tmux.as_ref() else {
                    return err(409, "tmux is not available");
                };
                let hex: Vec<String> = k.iter().map(|b| format!("{b:02x}")).collect();
                let mut args = vec!["send-keys", "-t", p.as_str(), "-H"];
                args.extend(hex.iter().map(String::as_str));
                tmux.ok(&args)
            }
        };
        if let Err(e) = sent {
            return err(409, e);
        }
    }
    if let Some(text) = &text {
        if !keys.is_empty() {
            std::thread::sleep(KEY_GAP);
        }
        let buffer = format!("sb-keys-{line}");
        if let Err(e) = crate::deliver::insert(&ch, text, ctx.tmux.as_ref(), &buffer) {
            return err(409, e);
        }
        std::thread::sleep(crate::deliver::PASTE_SETTLE);
    }
    (
        200,
        json!({"sent": keys.len() + usize::from(text.is_some())}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &str, cols: u16, rows: u16) -> String {
        render(b.as_bytes(), cols, rows).text()
    }

    #[test]
    fn prints_wraps_and_scrolls() {
        assert_eq!(text("hello\r\nworld", 10, 3), "hello\nworld");
        assert_eq!(text("abcdefgh", 5, 3), "abcde\nfgh");
        assert_eq!(text("1\r\n2\r\n3\r\n4", 5, 3), "2\n3\n4");
        assert_eq!(text("abcde\r\nx", 5, 3), "abcde\nx");
        assert_eq!(text("a\tb", 20, 1), "a       b");
    }

    #[test]
    fn cursor_moves_and_erases() {
        assert_eq!(text("\x1b[2;3Hx\x1b[1;1Hy", 5, 3), "y\n  x");
        assert_eq!(text("hello\x1b[3D\x1b[K", 10, 1), "he");
        assert_eq!(text("hello\x1b[1G\x1b[2Pz", 10, 1), "zlo");
        assert_eq!(text("hello\r\nworld\x1b[2J", 10, 2), "");
        assert_eq!(text("hello\x1b[2G\x1b[1@", 10, 1), "h ello");
        assert_eq!(text("abc\x1b[1G\x1b[2X", 10, 1), "  c");
        assert_eq!(text("a\r\nb\r\nc\x1b[2;1H\x1b[J", 5, 3), "a");
        assert_eq!(text("a\r\nb\x1b[A\x1b[Cz", 5, 2), "a z\nb");
        assert_eq!(text("\x1b[3;1Hz\x1b[1;1H\x1b[1L", 5, 3), "");
        assert_eq!(text("x\x1b7\x1b[3;3Hy\x1b8z", 5, 3), "xz\n\n  y");
    }

    #[test]
    fn alt_screen_is_separate() {
        let b = "main\x1b[?1049h\x1b[Hfull screen app";
        assert_eq!(text(b, 20, 2), "full screen app");
        assert_eq!(text(&format!("{b}\x1b[?1049l"), 20, 2), "main");
    }

    #[test]
    fn skips_osc_and_tracks_dim() {
        let b = "\x1b]0;title\x07a\x1b[2mb\x1b[0mc\x1bP1$r\x1b\\d";
        let g = render(b.as_bytes(), 10, 1);
        assert_eq!(g.text(), "abcd");
        assert_eq!(g.ansi(), "a\x1b[2mb\x1b[0mcd");
    }

    #[test]
    fn scroll_region() {
        let b = "top\x1b[2;3r\x1b[2;1Ha\r\nb\r\nc";
        assert_eq!(text(b, 5, 3), "top\nb\nc");
    }

    #[test]
    fn claude_prompt_with_dim_placeholder_is_empty() {
        let rule = "─".repeat(30);
        let screen = format!(
            "\x1b[?2004h● Done.\r\n\r\n{rule}\r\n❯ \x1b[2mTry \"fix lint\"\x1b[0m\r\n{rule}\r\n  ? for shortcuts"
        );
        let g = render(screen.as_bytes(), 40, 10);
        assert!(crate::hold::input_empty(Some(&g.ansi())).ok);
        let typed = screen.replace("\x1b[2mTry \"fix lint\"\x1b[0m", "fix it");
        let g = render(typed.as_bytes(), 40, 10);
        assert!(!crate::hold::input_empty(Some(&g.ansi())).ok);
    }

    #[test]
    fn tmux_capture_is_stripped() {
        let g = plain("\x1b[1mbold\x1b[0m\nline two\n");
        assert_eq!(g.text(), "bold\nline two");
    }

    #[test]
    fn named_keys() {
        assert_eq!(key_bytes("esc").unwrap(), b"\x1b");
        assert_eq!(key_bytes("enter").unwrap(), b"\r");
        assert_eq!(key_bytes("up").unwrap(), b"\x1b[A");
        assert_eq!(key_bytes("shift-tab").unwrap(), b"\x1b[Z");
        assert_eq!(key_bytes("ctrl-c").unwrap(), b"\x03");
        assert_eq!(key_bytes("1").unwrap(), b"1");
        assert_eq!(key_bytes("é").unwrap(), "é".as_bytes());
        assert_eq!(key_bytes("ab"), None);
        assert_eq!(key_bytes(""), None);
        assert_eq!(key_bytes("\x1b"), None);
        assert_eq!(
            parse_keys(&json!({"keys": ["down", "enter"]})).unwrap(),
            vec![b"\x1b[B".to_vec(), b"\r".to_vec()]
        );
        assert!(parse_keys(&json!({"keys": ["ab"]})).is_err());
        assert!(parse_keys(&json!({"keys": []})).is_err());
        assert!(parse_keys(&json!({"keys": vec!["1"; 33]})).is_err());
        assert!(parse_keys(&json!({"keys": "esc"})).is_err());
        assert_eq!(
            parse_keys(&json!({"text": "hi"})).unwrap(),
            Vec::<Vec<u8>>::new()
        );
        assert_eq!(
            parse_keys(&json!({"keys": [], "text": "hi"})).unwrap(),
            Vec::<Vec<u8>>::new()
        );
        assert_eq!(parse_text(&json!({"keys": ["up"]})).unwrap(), None);
        assert_eq!(
            parse_text(&json!({"text": "a\nb"})).unwrap().as_deref(),
            Some("a\nb")
        );
        assert!(parse_text(&json!({"text": ""})).is_err());
        assert!(parse_text(&json!({"text": 3})).is_err());
        assert!(parse_text(&json!({"text": "x".repeat(MAX_TEXT + 1)})).is_err());
    }
}
