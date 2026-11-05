use super::diff::fnv;

pub const CONTEXT: usize = 3;
const SEARCH: usize = 400;

#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub line: usize,
    pub end: usize,
    pub code: Vec<String>,
    pub before: Vec<u64>,
    pub after: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Same,
    Moved(usize, usize),
    Changed(usize, usize),
    Outdated,
}

pub fn h(s: &str) -> u64 {
    fnv(s.trim().as_bytes())
}

pub fn capture(lines: &[String], line: usize, end: usize) -> Option<Anchor> {
    if line == 0 || end < line || end > lines.len() {
        return None;
    }
    let (s, e) = (line - 1, end);
    Some(Anchor {
        line,
        end,
        code: lines[s..e].to_vec(),
        before: lines[s.saturating_sub(CONTEXT)..s]
            .iter()
            .map(|l| h(l))
            .collect(),
        after: lines[e..(e + CONTEXT).min(lines.len())]
            .iter()
            .map(|l| h(l))
            .collect(),
    })
}

fn matches(lines: &[String], at: usize, want: &[u64]) -> bool {
    at + want.len() <= lines.len()
        && want
            .iter()
            .enumerate()
            .all(|(k, w)| h(&lines[at + k]) == *w)
}

pub fn locate(lines: &[String], a: &Anchor) -> Found {
    let code: Vec<u64> = a.code.iter().map(|l| h(l)).collect();
    let n = code.len();
    if n == 0 {
        return Found::Outdated;
    }
    let at = a.line - 1;
    if matches(lines, at, &code) {
        return Found::Same;
    }
    let dist = |i: usize| i.abs_diff(at);
    if let Some(i) = (0..lines.len().saturating_sub(n - 1))
        .filter(|&i| matches(lines, i, &code))
        .min_by_key(|&i| dist(i))
    {
        return Found::Moved(i + 1, i + n);
    }
    if a.before.is_empty() && a.after.is_empty() {
        return Found::Outdated;
    }
    let starts: Vec<usize> = if a.before.is_empty() {
        vec![0]
    } else {
        (0..lines.len().saturating_sub(a.before.len() - 1))
            .filter(|&i| matches(lines, i, &a.before))
            .map(|i| i + a.before.len())
            .collect()
    };
    let mut best: Option<(usize, usize)> = None;
    for s in starts {
        let end = if a.after.is_empty() {
            Some(lines.len())
        } else {
            (s..=(s + n + SEARCH).min(lines.len())).find(|&j| matches(lines, j, &a.after))
        };
        if let Some(j) = end {
            if best.is_none_or(|(b, _)| dist(s) < dist(b)) {
                best = Some((s, j));
            }
        }
    }
    match best {
        Some((s, j)) if j > s => Found::Changed(s + 1, j),
        _ => Found::Outdated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ls(s: &str) -> Vec<String> {
        s.split('\n').map(str::to_string).collect()
    }

    #[test]
    fn remaps_moves_edits_and_deletions() {
        let file = ls("a\nb\nc\nfn x() {\n    one();\n}\nd\ne\nf");
        let a = capture(&file, 5, 5).unwrap();
        assert_eq!(a.code, ["    one();"]);
        assert_eq!(locate(&file, &a), Found::Same);

        let moved = ls("new\nnew2\na\nb\nc\nfn x() {\n    one();\n}\nd\ne\nf");
        assert_eq!(locate(&moved, &a), Found::Moved(7, 7));

        let reindented = ls("a\nb\nc\nfn x() {\n        one();\n}\nd\ne\nf");
        assert_eq!(locate(&reindented, &a), Found::Same);

        let edited = ls("z\na\nb\nc\nfn x() {\n    two();\n    three();\n}\nd\ne\nf");
        assert_eq!(locate(&edited, &a), Found::Changed(6, 7));

        let gone = ls("a\nb\nc\nfn x() {\n}\nd\ne\nf");
        assert_eq!(locate(&gone, &a), Found::Outdated);

        let rewritten = ls("totally\ndifferent");
        assert_eq!(locate(&rewritten, &a), Found::Outdated);
    }

    #[test]
    fn edges_of_file() {
        let file = ls("first\nsecond\nthird");
        let top = capture(&file, 1, 1).unwrap();
        assert!(top.before.is_empty());
        assert_eq!(
            locate(&ls("FIRST\nsecond\nthird"), &top),
            Found::Changed(1, 1)
        );
        let last = capture(&file, 3, 3).unwrap();
        assert!(last.after.is_empty());
        assert_eq!(
            locate(&ls("first\nsecond\nTHIRD\nmore"), &last),
            Found::Changed(3, 4)
        );
        assert!(capture(&file, 0, 1).is_none());
        assert!(capture(&file, 2, 4).is_none());
        let range = capture(&file, 1, 3).unwrap();
        assert_eq!(locate(&file, &range), Found::Same);
    }
}
