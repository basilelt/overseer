use super::lang::Lang;

#[derive(Clone, Debug, PartialEq)]
pub struct Sym {
    pub kind: &'static str,
    pub name: String,
    pub line: u32,
    pub end: u32,
    pub parent: Option<usize>,
}

impl Sym {
    pub fn contains(&self, n: u32) -> bool {
        self.line <= n && n <= self.end
    }
}

pub fn innermost(syms: &[Sym], n: u32) -> Option<usize> {
    syms.iter()
        .enumerate()
        .filter(|(_, s)| s.contains(n))
        .min_by_key(|(_, s)| s.end.saturating_sub(s.line))
        .map(|(i, _)| i)
}

pub fn qualified(syms: &[Sym], i: usize) -> String {
    let mut parts = vec![syms[i].name.as_str()];
    let mut p = syms[i].parent;
    while let Some(j) = p {
        parts.push(&syms[j].name);
        p = syms[j].parent;
    }
    parts.reverse();
    parts.join("::")
}

pub fn scan(lang: Lang, path: &str, lines: &[&str]) -> Vec<Sym> {
    match lang {
        Lang::Svelte => {
            let file = path.rsplit('/').next().unwrap_or(path);
            vec![Sym {
                kind: "component",
                name: file.trim_end_matches(".svelte").to_string(),
                line: 1,
                end: lines.len().max(1) as u32,
                parent: None,
            }]
        }
        Lang::Python => indented(lang, lines),
        Lang::Rust | Lang::Ts | Lang::Go | Lang::Dart => braces(lang, lines),
        Lang::Doc | Lang::Other => vec![],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Str {
    None,
    Block,
    Quote(u8),
    Triple(u8),
}

fn code_of(lang: Lang, line: &str, state: &mut Str) -> Vec<u8> {
    let mut out = vec![];
    let cs = line.as_bytes();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        let next = cs.get(i + 1).copied();
        match *state {
            Str::Block => {
                if c == b'*' && next == Some(b'/') {
                    *state = Str::None;
                    i += 1;
                }
            }
            Str::Quote(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    *state = Str::None;
                }
            }
            Str::Triple(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q && next == Some(q) && cs.get(i + 2) == Some(&q) {
                    *state = Str::None;
                    i += 2;
                }
            }
            Str::None => match c {
                b'"' | b'\''
                    if lang == Lang::Dart && next == Some(c) && cs.get(i + 2) == Some(&c) =>
                {
                    *state = Str::Triple(c);
                    i += 2;
                }
                b'/' if next == Some(b'/') => break,
                b'/' if next == Some(b'*') => {
                    *state = Str::Block;
                    i += 1;
                }
                b'#' if lang == Lang::Python => break,
                b'"' | b'`' => *state = Str::Quote(c),
                b'\'' if lang == Lang::Rust => {
                    if next == Some(b'\\') {
                        i += cs
                            .get(i + 2..)
                            .and_then(|r| r.iter().position(|&x| x == b'\''))
                            .map_or(cs.len(), |p| p + 2);
                    } else if cs.get(i + 2) == Some(&b'\'') {
                        i += 2;
                    }
                }
                b'\'' => *state = Str::Quote(c),
                b'{' | b'}' | b';' => out.push(c),
                _ => {}
            },
        }
        i += 1;
    }
    if matches!(*state, Str::Quote(b'\''))
        || (matches!(*state, Str::Quote(b'"')) && lang != Lang::Rust)
    {
        *state = Str::None;
    }
    out
}

fn braces(lang: Lang, lines: &[&str]) -> Vec<Sym> {
    let mut syms: Vec<Sym> = vec![];
    let mut open: Vec<(usize, i32)> = vec![];
    let mut pending: Option<(usize, i32)> = None;
    let mut depth = 0i32;
    let mut state = Str::None;
    for (i, raw) in lines.iter().enumerate() {
        let n = i as u32 + 1;
        let at_start = state;
        let code = code_of(lang, raw, &mut state);
        if let Some((p, _)) = pending.filter(|(p, _)| n - syms[*p].line > 8) {
            syms[p].end = syms[p].line;
            pending = None;
        }
        if at_start == Str::None {
            if let Some((kind, name)) = lang.def(raw) {
                if let Some((p, _)) = pending.take() {
                    syms[p].end = n - 1;
                }
                syms.push(Sym {
                    kind,
                    name,
                    line: n,
                    end: n,
                    parent: open.last().map(|o| o.0),
                });
                pending = Some((syms.len() - 1, depth));
            }
        }
        for c in code {
            match c {
                b'{' => {
                    if let Some((p, d)) = pending.take() {
                        open.push((p, d));
                    }
                    depth += 1;
                }
                b'}' => {
                    depth -= 1;
                    while open.last().is_some_and(|o| o.1 >= depth) {
                        let (p, _) = open.pop().unwrap_or_default();
                        syms[p].end = n;
                    }
                }
                b';' if pending.is_some_and(|(_, d)| d == depth) => {
                    if let Some((p, _)) = pending.take() {
                        syms[p].end = n;
                    }
                }
                _ => {}
            }
        }
    }
    let last = lines.len() as u32;
    for (p, _) in open {
        syms[p].end = last;
    }
    syms
}

fn indented(lang: Lang, lines: &[&str]) -> Vec<Sym> {
    let mut syms: Vec<Sym> = vec![];
    let mut open: Vec<(usize, usize)> = vec![];
    let mut last_code = 0u32;
    for (i, raw) in lines.iter().enumerate() {
        let n = i as u32 + 1;
        if raw.trim().is_empty() || lang.is_comment(raw) {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        while open.last().is_some_and(|o| o.1 >= indent) {
            let (p, _) = open.pop().unwrap_or_default();
            syms[p].end = last_code.max(syms[p].line);
        }
        if let Some((kind, name)) = lang.def(raw) {
            syms.push(Sym {
                kind,
                name,
                line: n,
                end: n,
                parent: open.last().map(|o| o.0),
            });
            open.push((syms.len() - 1, indent));
        }
        last_code = n;
    }
    for (p, _) in open {
        syms[p].end = last_code.max(syms[p].line);
    }
    syms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(lang: Lang, path: &str, src: &str) -> Vec<(String, &'static str, u32, u32)> {
        let lines: Vec<&str> = src.lines().collect();
        let syms = scan(lang, path, &lines);
        (0..syms.len())
            .map(|i| (qualified(&syms, i), syms[i].kind, syms[i].line, syms[i].end))
            .collect()
    }

    #[test]
    fn rust_spans_skip_strings_comments_and_lifetimes() {
        let src = "use std::fmt;\n\npub struct Unit;\n\nimpl<'a> Store<'a> {\n    pub fn get(&self) -> &'a str {\n        let s = \"}\";\n        let c = '{';\n        // }\n        /* { */\n        s\n    }\n\n    fn put(&mut self) {}\n}\n\nfn free(\n    a: u8,\n) -> u8 {\n    a\n}\n";
        assert_eq!(
            spans(Lang::Rust, "x.rs", src),
            [
                ("Unit".into(), "type", 3, 3),
                ("Store".into(), "impl", 5, 15),
                ("Store::get".into(), "fn", 6, 12),
                ("Store::put".into(), "fn", 14, 14),
                ("free".into(), "fn", 17, 21),
            ]
        );
    }

    #[test]
    fn ts_spans_cover_classes_methods_and_arrows() {
        let src = "import { a } from './a';\n\nexport class Box {\n  private items = `{`;\n  add(x: number) {\n    return x;\n  }\n}\n\nexport const twice = (n: number) => {\n  return n * 2;\n};\nconst one = (n: number) => n;\nexport function last() {\n  if (a) {\n    return '}';\n  }\n}\n";
        assert_eq!(
            spans(Lang::Ts, "x.ts", src),
            [
                ("Box".into(), "class", 3, 8),
                ("Box::add".into(), "fn", 5, 7),
                ("twice".into(), "const", 10, 12),
                ("one".into(), "const", 13, 13),
                ("last".into(), "fn", 14, 18),
            ]
        );
    }

    #[test]
    fn dart_spans_cover_classes_members_and_skip_strings() {
        let src = "import 'package:app/a.dart';\n\nclass Counter extends ChangeNotifier {\n  Counter({\n    required this.start,\n  });\n\n  final int start;\n  int get value => start;\n\n  @override\n  Widget build(BuildContext context) {\n    final s = '}';\n    final t = '''\n    { unbalanced\n    ''';\n    setState(() {\n      x++;\n    });\n    return Text('$s');\n  }\n}\n\nvoid main() => runApp(App());\n\nextension Words on String {\n  int get words => split(' ').length;\n}\n";
        assert_eq!(
            spans(Lang::Dart, "lib/x.dart", src),
            [
                ("Counter".into(), "class", 3, 22),
                ("Counter::Counter".into(), "fn", 4, 6),
                ("Counter::value".into(), "fn", 9, 9),
                ("Counter::build".into(), "fn", 12, 21),
                ("main".into(), "fn", 24, 24),
                ("Words".into(), "type", 26, 28),
                ("Words::words".into(), "fn", 27, 27),
            ]
        );
    }

    #[test]
    fn go_python_and_svelte_spans() {
        let go = "package x\n\ntype Server struct {\n\tport int\n}\n\nfunc (s *Server) Run() error {\n\treturn nil\n}\n";
        assert_eq!(
            spans(Lang::Go, "x.go", go),
            [("Server".into(), "type", 3, 5), ("Run".into(), "fn", 7, 9)]
        );
        let py = "import os\n\nclass A:\n    def f(self):\n        return 1\n\n    # note\n    def g(self):\n        pass\n\ndef top():\n    return 2\n";
        assert_eq!(
            spans(Lang::Python, "x.py", py),
            [
                ("A".into(), "class", 3, 9),
                ("A::f".into(), "fn", 4, 5),
                ("A::g".into(), "fn", 8, 9),
                ("top".into(), "fn", 11, 12),
            ]
        );
        assert_eq!(
            spans(
                Lang::Svelte,
                "app/Panel.svelte",
                "<script>\n</script>\n<div></div>\n"
            ),
            [("Panel".into(), "component", 1, 3)]
        );
        let syms = scan(Lang::Rust, "x.rs", &["fn a() {", "    fn b() {}", "}"]);
        assert_eq!(innermost(&syms, 2), Some(1));
        assert_eq!(innermost(&syms, 3), Some(0));
        assert_eq!(innermost(&syms, 4), None);
    }
}
