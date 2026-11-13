use crate::http::err;
use serde_json::{json, Value};
use std::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub dark: bool,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

static THEME: RwLock<Option<Theme>> = RwLock::new(None);

const DA1: &[u8] = b"\x1b[?62;22c";
const MAX_SEQ: usize = 32;

pub fn current() -> Option<Theme> {
    *THEME.read().unwrap_or_else(|e| e.into_inner())
}

fn store(t: Theme) -> bool {
    let mut g = THEME.write().unwrap_or_else(|e| e.into_inner());
    let changed = g.is_none_or(|old| old.dark != t.dark);
    *g = Some(t);
    changed
}

pub fn colorfgbg() -> Option<&'static str> {
    current().map(|t| if t.dark { "15;0" } else { "0;15" })
}

pub fn report(dark: bool) -> &'static [u8] {
    if dark {
        b"\x1b[?997;1n"
    } else {
        b"\x1b[?997;2n"
    }
}

fn hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([p(0)?, p(2)?, p(4)?])
}

pub fn saved() -> Value {
    let css = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    current().map_or(
        Value::Null,
        |t| json!({"dark": t.dark, "fg": css(t.fg), "bg": css(t.bg)}),
    )
}

pub fn restore(v: &Value) {
    if let Ok(t) = parse(v) {
        store(t);
    }
}

pub fn parse(v: &Value) -> Result<Theme, String> {
    let dark = v["dark"].as_bool().ok_or("dark must be a boolean")?;
    let color = |k: &str| {
        v[k].as_str()
            .and_then(hex)
            .ok_or_else(|| format!("{k} must be #rrggbb"))
    };
    Ok(Theme {
        dark,
        fg: color("fg")?,
        bg: color("bg")?,
    })
}

fn osc_color(code: u8, c: [u8; 3], st: &[u8]) -> Vec<u8> {
    let mut v = format!(
        "\x1b]{code};rgb:{0:02x}{0:02x}/{1:02x}{1:02x}/{2:02x}{2:02x}",
        c[0], c[1], c[2]
    )
    .into_bytes();
    v.extend_from_slice(st);
    v
}

enum Seq {
    Color(u8, usize, usize),
    Da1(usize),
    Mode(bool, usize),
    ThemeQuery(usize),
    Other,
    Partial,
}

fn osc_query(s: &[u8]) -> Seq {
    for (code, pat) in [(10u8, b"\x1b]10;?"), (11, b"\x1b]11;?")] {
        let pat: &[u8] = pat;
        if s.len() < pat.len() {
            if pat.starts_with(s) {
                return Seq::Partial;
            }
            continue;
        }
        if !s.starts_with(pat) {
            continue;
        }
        return match s.get(pat.len()) {
            None => Seq::Partial,
            Some(0x07) => Seq::Color(code, pat.len() + 1, 1),
            Some(0x1b) => match s.get(pat.len() + 1) {
                None => Seq::Partial,
                Some(b'\\') => Seq::Color(code, pat.len() + 2, 2),
                Some(_) => Seq::Other,
            },
            Some(_) => Seq::Other,
        };
    }
    Seq::Other
}

fn csi(s: &[u8]) -> Seq {
    let private = s.get(2) == Some(&b'?');
    let start = if private { 3 } else { 2 };
    let mut i = start;
    while let Some(&b) = s.get(i) {
        if b.is_ascii_digit() || b == b';' {
            i += 1;
            if i > MAX_SEQ {
                return Seq::Other;
            }
            continue;
        }
        let params = &s[start..i];
        let mut nums = params.split(|&c| c == b';');
        return match (private, b) {
            (false, b'c') if params.is_empty() || params == b"0" => Seq::Da1(i + 1),
            (true, b'h' | b'l') if nums.any(|n| n == b"2031") => Seq::Mode(b == b'h', i + 1),
            (true, b'n') if params == b"996" => Seq::ThemeQuery(i + 1),
            _ => Seq::Other,
        };
    }
    Seq::Partial
}

fn classify(s: &[u8]) -> Seq {
    match s.get(1) {
        None => Seq::Partial,
        Some(b']') => osc_query(s),
        Some(b'[') => csi(s),
        Some(_) => Seq::Other,
    }
}

#[derive(Default)]
pub struct Scanner {
    carry: Vec<u8>,
    pub notify: bool,
}

impl Scanner {
    pub fn feed(&mut self, chunk: &[u8], theme: Option<Theme>) -> Vec<u8> {
        let mut out = Vec::new();
        if self.carry.is_empty() && !chunk.contains(&0x1b) {
            return out;
        }
        let joined;
        let data: &[u8] = if self.carry.is_empty() {
            chunk
        } else {
            let mut j = std::mem::take(&mut self.carry);
            j.extend_from_slice(chunk);
            joined = j;
            &joined
        };
        let mut i = 0;
        while let Some(off) = data[i..].iter().position(|&b| b == 0x1b) {
            let at = i + off;
            let s = &data[at..];
            i = at + 1;
            match classify(s) {
                Seq::Partial => {
                    if s.len() <= MAX_SEQ {
                        self.carry = s.to_vec();
                    }
                    break;
                }
                Seq::Other => {}
                Seq::Mode(on, n) => {
                    self.notify = on;
                    i = at + n;
                }
                Seq::Color(code, n, st) => {
                    if let Some(t) = theme {
                        let c = if code == 10 { t.fg } else { t.bg };
                        out.extend(osc_color(code, c, &s[n - st..n]));
                    }
                    i = at + n;
                }
                Seq::Da1(n) => {
                    out.extend_from_slice(DA1);
                    i = at + n;
                }
                Seq::ThemeQuery(n) => {
                    if let Some(t) = theme {
                        out.extend_from_slice(report(t.dark));
                    }
                    i = at + n;
                }
            }
        }
        out
    }
}

pub fn routes(method: &str, segs: &[&str], body: &Value) -> Option<(u16, Value)> {
    if segs != ["v1", "theme"] {
        return None;
    }
    Some(match method {
        "GET" => (
            200,
            current().map_or(Value::Null, |t| json!({"dark": t.dark})),
        ),
        "POST" | "PUT" => match parse(body) {
            Ok(t) => {
                if store(t) {
                    crate::term::theme_changed(t.dark);
                }
                (200, json!({"dark": t.dark}))
            }
            Err(e) => err(400, e),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT: Theme = Theme {
        dark: false,
        fg: [0x18, 0x18, 0x1b],
        bg: [0xff, 0xff, 0xff],
    };

    #[test]
    fn answers_background_query_then_da1_in_order() {
        let mut s = Scanner::default();
        let out = s.feed(
            b"\x1b[?2031h\x1b]11;?\x07\x1b[c\x1b[>0q\x1b[?u",
            Some(LIGHT),
        );
        assert_eq!(out, b"\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?62;22c".to_vec());
        assert!(s.notify);
    }

    #[test]
    fn foreground_query_keeps_the_st_terminator() {
        let mut s = Scanner::default();
        let out = s.feed(b"x\x1b]10;?\x1b\\y", Some(LIGHT));
        assert_eq!(out, b"\x1b]10;rgb:1818/1818/1b1b\x1b\\".to_vec());
    }

    #[test]
    fn queries_split_across_chunks_are_answered_once() {
        let mut s = Scanner::default();
        let q = b"ab\x1b]11;?\x07\x1b[0c";
        let mut out = Vec::new();
        for c in q.chunks(3) {
            out.extend(s.feed(c, Some(LIGHT)));
        }
        assert_eq!(out, b"\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?62;22c".to_vec());
        assert!(s.carry.is_empty());
    }

    #[test]
    fn only_da1_is_answered_without_a_theme() {
        let mut s = Scanner::default();
        let out = s.feed(b"\x1b[?1004;2031h\x1b]11;?\x07\x1b[c", None);
        assert_eq!(out, DA1.to_vec());
        assert!(s.notify);
        s.feed(b"\x1b[?2031l", None);
        assert!(!s.notify);
    }

    #[test]
    fn ignores_ordinary_output() {
        let mut s = Scanner::default();
        let out = s.feed(
            b"\x1b[38;5;123mhi\x1b[0m\x1b]0;title\x07\x1b[?25h\x1b[2c",
            Some(LIGHT),
        );
        assert!(out.is_empty());
        assert!(s.carry.is_empty());
        assert!(!s.notify);
    }

    #[test]
    fn answers_theme_query_with_dark_or_light() {
        let mut s = Scanner::default();
        assert_eq!(s.feed(b"\x1b[?996n", Some(LIGHT)), b"\x1b[?997;2n".to_vec());
        let dark = Theme {
            dark: true,
            ..LIGHT
        };
        assert_eq!(s.feed(b"\x1b[?996n", Some(dark)), b"\x1b[?997;1n".to_vec());
    }

    #[test]
    fn theme_survives_a_save_and_restore() {
        store(LIGHT);
        let v = saved();
        assert_eq!(v, json!({"dark": false, "fg": "#18181b", "bg": "#ffffff"}));
        restore(&Value::Null);
        assert_eq!(current(), Some(LIGHT));
        restore(&v);
        assert_eq!(current(), Some(LIGHT));
    }

    #[test]
    fn parses_theme_body() {
        let t = parse(&json!({"dark": false, "fg": "#18181b", "bg": "#ffffff"})).unwrap();
        assert_eq!(t, LIGHT);
        assert!(parse(&json!({"dark": true, "fg": "red", "bg": "#000000"})).is_err());
        assert!(parse(&json!({"fg": "#000000", "bg": "#000000"})).is_err());
    }
}
