const MODES: [(u16, bool); 15] = [
    (1049, false),
    (1047, false),
    (47, false),
    (1, false),
    (7, true),
    (25, true),
    (1000, false),
    (1002, false),
    (1003, false),
    (1004, false),
    (1005, false),
    (1006, false),
    (1007, true),
    (1015, false),
    (2004, false),
];

const MAX_PARAMS: usize = 16;
pub const MAX_SEQ: usize = 4096;

#[derive(Default, Clone, Copy, PartialEq, Debug)]
enum St {
    #[default]
    Ground,
    Esc,
    EscInt,
    Csi,
    Str,
    StrEsc,
}

#[derive(Default, Clone)]
pub struct Modes {
    changed: u16,
    st: St,
    private: bool,
    bad: bool,
    param: u16,
    params: [u16; MAX_PARAMS],
    n: usize,
}

impl Modes {
    pub fn idle(&self) -> bool {
        self.st == St::Ground
    }

    pub fn feed(&mut self, mut b: &[u8]) {
        while !b.is_empty() {
            if self.st == St::Ground {
                match b.iter().position(|&c| c == 0x1b) {
                    Some(i) => b = &b[i..],
                    None => return,
                }
            }
            self.byte(b[0]);
            b = &b[1..];
        }
    }

    pub fn byte(&mut self, c: u8) {
        if self.st == St::StrEsc && c != b'\\' {
            self.st = St::Esc;
        }
        self.st = match (self.st, c) {
            (St::Str, 0x07) => St::Ground,
            (St::Str, 0x1b) => St::StrEsc,
            (St::Str, _) => St::Str,
            (St::StrEsc, _) => St::Ground,
            (_, 0x1b) => St::Esc,
            (St::Ground, _) => St::Ground,
            (St::Esc, b'[') => {
                self.private = false;
                self.bad = false;
                self.param = 0;
                self.n = 0;
                St::Csi
            }
            (St::Esc, b']' | b'P' | b'_' | b'^' | b'X') => St::Str,
            (St::Esc, b'c') => {
                self.changed = 0;
                St::Ground
            }
            (St::Esc | St::EscInt, 0x20..=0x2f) => St::EscInt,
            (St::Esc | St::EscInt, _) => St::Ground,
            (St::Csi, _) => self.csi(c),
        };
    }

    fn csi(&mut self, c: u8) -> St {
        match c {
            b'0'..=b'9' => {
                self.param = self
                    .param
                    .saturating_mul(10)
                    .saturating_add(u16::from(c - b'0'));
                St::Csi
            }
            b';' => {
                self.push();
                St::Csi
            }
            b'?' if self.n == 0 && self.param == 0 && !self.private => {
                self.private = true;
                St::Csi
            }
            0x20..=0x3f => {
                self.bad = true;
                St::Csi
            }
            0x40..=0x7e => {
                self.push();
                if self.private && !self.bad && matches!(c, b'h' | b'l') {
                    for i in 0..self.n.min(MAX_PARAMS) {
                        self.set(self.params[i], c == b'h');
                    }
                }
                St::Ground
            }
            _ => St::Csi,
        }
    }

    fn push(&mut self) {
        if self.n < MAX_PARAMS {
            self.params[self.n] = self.param;
        }
        self.n += 1;
        self.param = 0;
    }

    fn set(&mut self, mode: u16, on: bool) {
        if let Some(i) = MODES.iter().position(|&(m, _)| m == mode) {
            let bit = 1 << i;
            if on != MODES[i].1 {
                self.changed |= bit;
            } else {
                self.changed &= !bit;
            }
        }
    }

    pub fn preamble(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, &(mode, default)) in MODES.iter().enumerate() {
            if self.changed & (1 << i) != 0 {
                out.extend_from_slice(
                    format!("\x1b[?{mode}{}", if default { 'l' } else { 'h' }).as_bytes(),
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(chunks: &[&[u8]]) -> String {
        let mut m = Modes::default();
        for c in chunks {
            m.feed(c);
        }
        String::from_utf8(m.preamble()).unwrap()
    }

    #[test]
    fn restores_the_modes_left_on() {
        let s = modes(&[b"hi\x1b[?1049h\x1b[?1000;1006h\x1b[?2004h\x1b[?1007l\x1b[?25l text"]);
        assert_eq!(
            s,
            "\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1006h\x1b[?1007l\x1b[?2004h"
        );
    }

    #[test]
    fn forgets_the_modes_turned_back() {
        let s = modes(&[
            b"\x1b[?1049h\x1b[?1000h\x1b[?25l",
            b"\x1b[?1049l\x1b[?1000l\x1b[?25h",
        ]);
        assert_eq!(s, "");
    }

    #[test]
    fn a_full_reset_clears_every_mode() {
        assert_eq!(modes(&[b"\x1b[?1049h\x1b[?1002h\x1bc"]), "");
    }

    #[test]
    fn reads_a_sequence_split_across_chunks() {
        assert_eq!(modes(&[b"\x1b[?10", b"49h"]), "\x1b[?1049h");
        assert_eq!(modes(&[b"\x1b", b"[?1006h"]), "\x1b[?1006h");
    }

    #[test]
    fn ignores_other_sequences_and_strings() {
        let s = modes(&[b"\x1b[1049h\x1b[>1049h\x1b[?99h\x1b]0;[?1049h title\x07\x1b(B\x1b[?1h"]);
        assert_eq!(s, "\x1b[?1h");
    }

    #[test]
    fn an_escape_inside_a_string_ends_it() {
        assert_eq!(modes(&[b"\x1b]0;title\x1b[?2004h"]), "\x1b[?2004h");
    }

    #[test]
    fn knows_when_it_is_inside_a_sequence() {
        let mut m = Modes::default();
        m.feed(b"text \x1b[?10");
        assert!(!m.idle());
        m.feed(b"49h");
        assert!(m.idle());
        m.feed(b"\x1b]8;;http://x");
        assert!(!m.idle());
        m.feed(b"\x1b\\");
        assert!(m.idle());
    }
}
