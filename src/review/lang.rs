#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Rust,
    Ts,
    Svelte,
    Python,
    Go,
    Dart,
    Doc,
    Other,
}

pub struct Spec {
    pub comments: &'static [&'static str],
    pub imports: &'static [&'static str],
    pub modifiers: &'static [&'static str],
    pub defs: &'static [(&'static str, &'static str)],
}

const C_LIKE: &[&str] = &["//", "/*", "*/", "* "];

const RUST: Spec = Spec {
    comments: C_LIKE,
    imports: &["use ", "extern crate ", "mod "],
    modifiers: &[
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const unsafe ",
        "unsafe ",
        "extern \"C\" ",
        "default ",
    ],
    defs: &[
        ("fn ", "fn"),
        ("struct ", "type"),
        ("enum ", "type"),
        ("union ", "type"),
        ("trait ", "type"),
        ("type ", "type"),
        ("impl", "impl"),
        ("mod ", "mod"),
        ("const ", "const"),
        ("static ", "const"),
        ("macro_rules! ", "fn"),
    ],
};

const TS: Spec = Spec {
    comments: C_LIKE,
    imports: &["import ", "import{", "export * from", "export {"],
    modifiers: &[
        "export ",
        "default ",
        "declare ",
        "abstract ",
        "async ",
        "public ",
        "private ",
        "protected ",
        "static ",
        "readonly ",
        "override ",
    ],
    defs: &[
        ("function* ", "fn"),
        ("function ", "fn"),
        ("class ", "class"),
        ("interface ", "type"),
        ("type ", "type"),
        ("enum ", "type"),
        ("const ", "const"),
        ("let ", "const"),
    ],
};

const SVELTE: Spec = Spec {
    comments: &["//", "/*", "*/", "* ", "<!--", "-->"],
    ..TS
};

const PYTHON: Spec = Spec {
    comments: &["#", "\"\"\"", "'''"],
    imports: &["import ", "from "],
    modifiers: &["async "],
    defs: &[("def ", "fn"), ("class ", "class")],
};

const GO: Spec = Spec {
    comments: C_LIKE,
    imports: &["import ", "\""],
    modifiers: &[],
    defs: &[
        ("func ", "fn"),
        ("type ", "type"),
        ("const ", "const"),
        ("var ", "const"),
    ],
};

const DART: Spec = Spec {
    comments: C_LIKE,
    imports: &["import ", "export ", "part ", "library "],
    modifiers: &[
        "@override ",
        "external ",
        "abstract ",
        "sealed ",
        "base ",
        "final ",
        "interface ",
        "static ",
        "factory ",
        "const ",
    ],
    defs: &[
        ("class ", "class"),
        ("mixin class ", "class"),
        ("mixin ", "class"),
        ("enum ", "type"),
        ("extension type ", "type"),
        ("extension ", "type"),
        ("typedef ", "type"),
    ],
};

const NONE: Spec = Spec {
    comments: &[],
    imports: &[],
    modifiers: &[],
    defs: &[],
};

pub fn of(path: &str) -> Lang {
    let ext = path.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "rs" => Lang::Rust,
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => Lang::Ts,
        "svelte" => Lang::Svelte,
        "py" => Lang::Python,
        "go" => Lang::Go,
        "dart" => Lang::Dart,
        "md" | "markdown" | "txt" | "rst" | "adoc" => Lang::Doc,
        _ => Lang::Other,
    }
}

impl Lang {
    pub fn spec(self) -> &'static Spec {
        match self {
            Lang::Rust => &RUST,
            Lang::Ts => &TS,
            Lang::Svelte => &SVELTE,
            Lang::Python => &PYTHON,
            Lang::Go => &GO,
            Lang::Dart => &DART,
            Lang::Doc | Lang::Other => &NONE,
        }
    }

    pub fn is_comment(self, line: &str) -> bool {
        let t = line.trim_start();
        let spec = self.spec();
        !t.is_empty()
            && (spec.comments.iter().any(|p| t.starts_with(p))
                || (t.trim_end() == "*" && spec.comments.contains(&"* ")))
    }

    pub fn is_import(self, line: &str) -> bool {
        let t = line.trim_start();
        let t = t.strip_prefix("pub ").unwrap_or(t);
        let spec = self.spec();
        if self == Lang::Go && t.starts_with('"') {
            return t.trim_end().ends_with('"');
        }
        if self == Lang::Rust && t.starts_with("mod ") {
            return t.trim_end().ends_with(';');
        }
        spec.imports.iter().any(|p| t.starts_with(p))
            || (matches!(self, Lang::Ts | Lang::Svelte)
                && t.starts_with("export ")
                && t.contains(" from "))
    }

    pub fn def(self, line: &str) -> Option<(&'static str, String)> {
        let spec = self.spec();
        let mut t = line.trim_start();
        if t.is_empty() || self.is_comment(t) {
            return None;
        }
        'strip: loop {
            for m in spec.modifiers {
                if let Some(rest) = t.strip_prefix(m) {
                    t = rest.trim_start();
                    continue 'strip;
                }
            }
            break;
        }
        for (kw, kind) in spec.defs {
            let Some(rest) = t.strip_prefix(kw) else {
                continue;
            };
            let name = match (self, *kind) {
                (Lang::Rust, "impl") => impl_name(rest)?,
                (Lang::Rust, "const")
                    if rest.starts_with("fn ") || rest.starts_with("unsafe fn ") =>
                {
                    return Some(("fn", ident(rest.split_once("fn ")?.1)?));
                }
                (Lang::Dart, "type") if rest.trim_start().starts_with("on ") => return None,
                (Lang::Go, "fn") => ident(rest.strip_prefix('(').map_or(rest, |r| {
                    r.split_once(')').map_or(r, |(_, after)| after.trim_start())
                }))?,
                (Lang::Ts | Lang::Svelte, "const") => {
                    let rest = rest.trim_start();
                    let n = ident(rest)?;
                    let after = rest[n.len()..].trim_start();
                    let body = after.split_once('=')?.1;
                    if !(body.contains("=>") || body.trim_start().starts_with("function")) {
                        return None;
                    }
                    n
                }
                _ => ident(rest)?,
            };
            return Some((kind, name));
        }
        if self == Lang::Dart {
            return dart_member(t);
        }
        if matches!(self, Lang::Ts | Lang::Svelte) {
            let n = ident(t)?;
            let after = &t[n.len()..];
            let control = [
                "if", "for", "while", "switch", "catch", "return", "function", "await", "new",
                "typeof", "super", "this",
            ];
            if after.starts_with('(')
                && t.trim_end().ends_with('{')
                && !control.contains(&n.as_str())
            {
                return Some(("fn", n));
            }
        }
        None
    }
}

fn strip_generics(s: &str) -> String {
    let mut depth = 0;
    s.chars()
        .filter(|c| {
            let keep = depth == 0 && *c != '<';
            match c {
                '<' => depth += 1,
                '>' if depth > 0 => depth -= 1,
                _ => {}
            }
            keep
        })
        .collect()
}

const DART_KEYWORDS: &[&str] = &[
    "return", "await", "throw", "yield", "new", "else", "case", "if", "for", "while", "switch",
    "catch", "var", "assert", "do", "try", "super", "this", "late", "when",
];

fn whole_ident(w: &str) -> bool {
    ident(w).is_some_and(|n| n.len() == w.len()) && !DART_KEYWORDS.contains(&w)
}

fn dart_body(rest: &str) -> bool {
    let r = rest.trim_start();
    ["{", "=>", ";", ":", "async", "sync*"]
        .iter()
        .any(|k| r.starts_with(k))
}

fn dart_member(t: &str) -> Option<(&'static str, String)> {
    let t = strip_generics(t.trim_end());
    let cut = [t.find('('), t.find("=>"), t.find('{')]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(t.len());
    let (head, tail) = t.split_at(cut);
    let words: Vec<&str> = head.split_whitespace().collect();
    let (name, types) = words.split_last()?;
    let typed = |w: &&str| whole_ident(w.trim_end_matches('?')) || w.split('.').all(whole_ident);
    let named =
        whole_ident(name) || (name.split('.').count() == 2 && name.split('.').all(whole_ident));
    if !named || !types.iter().all(typed) {
        return None;
    }
    if !tail.starts_with('(') {
        let getter = types.len() >= 2 && types.last() == Some(&"get");
        return (getter && !tail.is_empty()).then(|| ("fn", name.to_string()));
    }
    let mut depth = 0;
    let close = tail.char_indices().find_map(|(i, c)| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(i)
    });
    let upper = name.starts_with(|c: char| c.is_ascii_uppercase());
    let ok = match (close, types.is_empty()) {
        (Some(i), true) if upper => {
            let r = tail[i + 1..].trim_start();
            dart_body(r) && !(r.starts_with(';') && name.contains('.'))
        }
        (Some(i), true) => {
            let r = tail[i + 1..].trim_start();
            r.starts_with('{') || r.starts_with("=>") || r.starts_with("async")
        }
        (Some(i), false) => dart_body(&tail[i + 1..]),
        (None, true) => upper && tail == "({",
        (None, false) => tail.ends_with('(') || tail.ends_with("({") || tail.ends_with(','),
    };
    ok.then(|| ("fn", name.to_string()))
}

pub fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

pub fn ident(s: &str) -> Option<String> {
    let n: String = s
        .trim_start()
        .chars()
        .take_while(|c| is_ident(*c))
        .collect();
    (!n.is_empty() && !n.starts_with(|c: char| c.is_ascii_digit())).then_some(n)
}

fn impl_name(rest: &str) -> Option<String> {
    let mut s = rest.trim_start();
    if s.starts_with('<') {
        let mut depth = 0;
        let end = s.char_indices().find(|(_, c)| {
            match c {
                '<' => depth += 1,
                '>' => depth -= 1,
                _ => {}
            }
            depth == 0
        })?;
        s = s[end.0 + 1..].trim_start();
    } else if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let head = s.split(['{', '\n']).next().unwrap_or(s);
    let head = head.split(" where ").next().unwrap_or(head);
    let target = head.rsplit_once(" for ").map_or(head, |(_, t)| t);
    let target = target.trim().trim_start_matches(['&', '*']);
    let base = target.split('<').next().unwrap_or(target);
    let last = base.rsplit("::").next().unwrap_or(base);
    ident(last)
}

pub fn generated(path: &str, head: &[&str]) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    const LOCKS: &[&str] = &[
        "Cargo.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lockb",
        "go.sum",
        "poetry.lock",
        "uv.lock",
        "Gemfile.lock",
        "composer.lock",
        "flake.lock",
        "pubspec.lock",
    ];
    LOCKS.contains(&name)
        || name.ends_with(".snap")
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".pb.go")
        || name.ends_with(".generated.ts")
        || [".g.dart", ".freezed.dart", ".mocks.dart", ".gr.dart"]
            .iter()
            .any(|e| name.ends_with(e))
        || name.contains("_generated.")
        || path.split('/').any(|d| d == "__snapshots__")
        || head
            .iter()
            .take(5)
            .any(|l| l.contains("@generated") || l.contains("GENERATED CODE - DO NOT MODIFY"))
}

pub fn test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/')
        .rev()
        .skip(1)
        .any(|d| matches!(d, "tests" | "test" | "__tests__" | "spec" | "e2e"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.ends_with("_test.dart")
        || (name.starts_with("test_") && name.ends_with(".py"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_definitions_per_language() {
        let r = Lang::Rust;
        assert_eq!(
            r.def("pub(crate) async fn go(x: u8) {"),
            Some(("fn", "go".into()))
        );
        assert_eq!(
            r.def("impl<T: Clone> Display for Wrap<T> {"),
            Some(("impl", "Wrap".into()))
        );
        assert_eq!(r.def("impl Store {"), Some(("impl", "Store".into())));
        assert_eq!(r.def("pub struct Hunk {"), Some(("type", "Hunk".into())));
        assert_eq!(r.def("let fn_name = 1;"), None);
        assert_eq!(r.def("// fn nope() {"), None);
        let t = Lang::Ts;
        assert_eq!(
            t.def("export async function load(a) {"),
            Some(("fn", "load".into()))
        );
        assert_eq!(
            t.def("export const sum = (a: number) => a;"),
            Some(("const", "sum".into()))
        );
        assert_eq!(t.def("const N = 3;"), None);
        assert_eq!(
            t.def("  private async fetchDiff() {"),
            Some(("fn", "fetchDiff".into()))
        );
        assert_eq!(t.def("  if (x) {"), None);
        assert_eq!(
            t.def("export class ReviewStore {"),
            Some(("class", "ReviewStore".into()))
        );
        let g = Lang::Go;
        assert_eq!(
            g.def("func (s *Server) Run(ctx context.Context) error {"),
            Some(("fn", "Run".into()))
        );
        assert_eq!(
            g.def("type Config struct {"),
            Some(("type", "Config".into()))
        );
        assert_eq!(
            Lang::Python.def("    async def handle(self):"),
            Some(("fn", "handle".into()))
        );
    }

    #[test]
    fn finds_dart_types_members_and_constructors() {
        let d = Lang::Dart;
        let cases = [
            (
                "abstract class Repo<T> extends Base {",
                Some(("class", "Repo")),
            ),
            ("sealed class State {", Some(("class", "State"))),
            ("mixin class Both {", Some(("class", "Both"))),
            ("mixin Loggable on Widget {", Some(("class", "Loggable"))),
            ("enum Kind { a, b }", Some(("type", "Kind"))),
            ("extension StringX on String {", Some(("type", "StringX"))),
            ("extension on String {", None),
            ("extension type Id(int v) {", Some(("type", "Id"))),
            ("typedef Cb = void Function(int);", Some(("type", "Cb"))),
            ("void main() {", Some(("fn", "main"))),
            (
                "Future<Map<String, int>> load(String id) async {",
                Some(("fn", "load")),
            ),
            ("  @override", None),
            (
                "  @override Widget build(BuildContext c) {",
                Some(("fn", "build")),
            ),
            ("  Widget build(", Some(("fn", "build"))),
            (
                "  static T first<T>(List<T> xs) => xs[0];",
                Some(("fn", "first")),
            ),
            ("  int get count => _items.length;", Some(("fn", "count"))),
            ("  String? get label {", Some(("fn", "label"))),
            ("  set count(int v) {", Some(("fn", "count"))),
            ("  const Box({", Some(("fn", "Box"))),
            ("  Box(this.x);", Some(("fn", "Box"))),
            ("  Box.named(int y) : x = y;", Some(("fn", "Box.named"))),
            ("  factory Box.from(Map m) {", Some(("fn", "Box.from"))),
            ("  void dispose();", Some(("fn", "dispose"))),
            ("  final x = compute(a);", None),
            ("    setState(() {", None),
            ("    expect(a, b);", None),
            ("    return Text('a');", None),
            ("    child: Container(", None),
            ("    const EdgeInsets.only(", None),
            ("    Logger.info('x');", None),
            ("    super.dispose();", None),
            ("    if (ready) {", None),
            ("    } else if (x) {", None),
            ("    test('adds', () {", None),
            ("  final String name;", None),
        ];
        for (line, want) in cases {
            assert_eq!(d.def(line), want.map(|(k, n)| (k, n.to_string())), "{line}");
        }
    }

    #[test]
    fn classifies_dart_imports_tests_and_generated_files() {
        assert!(Lang::Dart.is_import("import 'package:app/src/a.dart';"));
        assert!(Lang::Dart.is_import("export 'src/b.dart' show B;"));
        assert!(Lang::Dart.is_import("part 'a.g.dart';"));
        assert!(Lang::Dart.is_comment("  /// doc"));
        assert_eq!(of("lib/main.dart"), Lang::Dart);
        assert!(test_path("test/a_test.dart") && test_path("pkg/b_test.dart"));
        assert!(generated("lib/a.g.dart", &[]));
        assert!(generated("test/a_test.mocks.dart", &[]));
        assert!(generated("lib/a.freezed.dart", &[]));
        assert!(generated("pubspec.lock", &[]));
        assert!(generated(
            "lib/x.dart",
            &["// GENERATED CODE - DO NOT MODIFY BY HAND"]
        ));
        assert!(!generated("lib/x.dart", &["class X {}"]));
    }

    #[test]
    fn classifies_imports_comments_and_paths() {
        assert!(Lang::Rust.is_import("use std::path::Path;"));
        assert!(Lang::Rust.is_import("mod lang;"));
        assert!(!Lang::Rust.is_import("mod tests {"));
        assert!(Lang::Ts.is_import("export { a } from './a';"));
        assert!(Lang::Go.is_import("\t\"fmt\""));
        assert!(Lang::Python.is_import("from a.b import c"));
        assert!(Lang::Rust.is_comment("    /// doc"));
        assert!(Lang::Python.is_comment("  # note"));
        assert!(!Lang::Other.is_comment("# heading"));
        assert!(generated("app/pnpm-lock.yaml", &[]));
        assert!(generated("src/x.rs", &["// @generated by tool"]));
        assert!(!generated("src/x.rs", &["fn main() {}"]));
        assert!(
            test_path("src/tests/a.rs") && test_path("app/x.test.ts") && test_path("pkg/a_test.go")
        );
        assert!(!test_path("src/testing.rs"));
        assert_eq!(of("a/b.svelte"), Lang::Svelte);
    }

    #[test]
    fn multibyte_whitespace_and_identifiers_never_split_a_char() {
        assert_eq!(
            Lang::Ts.def("const \u{3000}x = () => 1"),
            Some(("const", "x".into()))
        );
        assert_eq!(
            Lang::Ts.def("const  aé = () => 1"),
            Some(("const", "aé".into()))
        );
        let tricky = [
            "\u{3000}", "\u{a0}", "é", "日本", "<", ">", "(", ")", "'", "\"", "=", "=>", "::",
            " for ",
        ];
        for lg in [
            Lang::Rust,
            Lang::Ts,
            Lang::Svelte,
            Lang::Python,
            Lang::Go,
            Lang::Dart,
        ] {
            for (kw, _) in lg.spec().defs {
                for a in tricky {
                    for b in tricky {
                        let line = format!("{kw}{a} {b}x{a}{b} = () => {{");
                        let _ = lg.def(&line);
                        let _ = lg.is_import(&line);
                        let _ = lg.def(&format!("{a}{kw}{b}"));
                    }
                }
            }
        }
    }
}
