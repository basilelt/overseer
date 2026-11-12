use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const FILE: &str = "daemon.log";
pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
pub const KEEP: usize = 5;
const WINDOW: u64 = 1 << 20;
const MASK: &str = "***";
const KEYS: [&str; 7] = [
    "authorization",
    "password",
    "passwd",
    "secret",
    "api_key",
    "apikey",
    "token",
];
const PREFIXES: [&str; 6] = ["github_pat_", "ghp_", "gho_", "ghu_", "ghs_", "ghr_"];

pub fn stamp(ms: i64) -> String {
    let s = crate::util::format_rfc3339(ms);
    format!("{}.{:03}Z", &s[..s.len() - 1], ms.rem_euclid(1000))
}

pub fn line(level: &str, msg: &str) {
    eprintln!(
        "{} {} {msg}",
        stamp(crate::util::now_ms()),
        level.to_ascii_uppercase()
    );
}

const SUPERVISE_MAX_BACKOFF: Duration = Duration::from_secs(60);
const SUPERVISE_HEALTHY: Duration = Duration::from_secs(300);

pub fn supervise<F>(name: &'static str, f: F) -> bool
where
    F: Fn() + Send + 'static,
{
    supervise_with(name, Duration::from_secs(1), f)
}

pub fn supervise_with<F>(name: &'static str, first: Duration, f: F) -> bool
where
    F: Fn() + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || {
            let mut backoff = first;
            loop {
                let began = Instant::now();
                let Err(p) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&f)) else {
                    return;
                };
                if began.elapsed() >= SUPERVISE_HEALTHY {
                    backoff = first;
                }
                line(
                    "error",
                    &format!(
                        "overseer daemon: {name} thread panicked ({}); restarting in {} ms",
                        crate::util::panic_text(p.as_ref()),
                        backoff.as_millis()
                    ),
                );
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(SUPERVISE_MAX_BACKOFF);
            }
        })
        .is_ok()
}

fn rotated(dir: &Path, i: usize) -> PathBuf {
    dir.join(format!("{FILE}.{i}"))
}

pub fn rotate(dir: &Path, max: u64, keep: usize) -> io::Result<bool> {
    let log = dir.join(FILE);
    match fs::metadata(&log) {
        Ok(m) if m.len() > max => {}
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    }
    if keep >= 2 {
        let _ = fs::remove_file(rotated(dir, keep - 1));
        for i in (1..keep - 1).rev() {
            let from = rotated(dir, i);
            if from.exists() {
                fs::rename(&from, rotated(dir, i + 1))?;
            }
        }
        fs::copy(&log, rotated(dir, 1))?;
    }
    OpenOptions::new().write(true).open(&log)?.set_len(0)?;
    Ok(true)
}

pub fn rotate_forever(dir: PathBuf) {
    loop {
        let _ = rotate(&dir, MAX_BYTES, KEEP);
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn last_lines(p: &Path) -> Vec<String> {
    let Ok(mut f) = File::open(p) else {
        return vec![];
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(WINDOW);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return vec![];
    }
    let mut buf = Vec::new();
    if f.take(WINDOW).read_to_end(&mut buf).is_err() {
        return vec![];
    }
    let mut lines: Vec<String> = String::from_utf8_lossy(&buf)
        .lines()
        .map(String::from)
        .collect();
    if from > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    lines
}

pub fn tail(dir: &Path, n: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in [dir.join(FILE), rotated(dir, 1)] {
        if out.len() >= n {
            break;
        }
        let mut lines = last_lines(&p);
        let start = lines.len().saturating_sub(n - out.len());
        let mut lines = lines.split_off(start);
        lines.append(&mut out);
        out = lines;
    }
    out.iter().map(|l| redact(l)).collect()
}

pub fn api(dir: &Path, query: Option<&str>) -> Value {
    let n = query
        .and_then(|t| t.parse::<usize>().ok())
        .unwrap_or(300)
        .clamp(1, 5000);
    let path = dir.join(FILE);
    let path = std::path::absolute(&path).unwrap_or(path);
    json!({"path": path.to_string_lossy(), "lines": tail(dir, n)})
}

pub fn redact(line: &str) -> String {
    let s = userinfo(line);
    let s = bearer(&s);
    let s = keyed(&s);
    prefixed(&s)
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn value_end(b: &[u8], mut j: usize) -> usize {
    while j < b.len() && !b[j].is_ascii_whitespace() && !b"\"',;&})]".contains(&b[j]) {
        j += 1;
    }
    j
}

fn skip_spaces(b: &[u8], mut j: usize) -> usize {
    while j < b.len() && b[j] == b' ' {
        j += 1;
    }
    j
}

fn userinfo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        let (head, after) = rest.split_at(i + 3);
        out.push_str(head);
        let end = after
            .find(|c: char| c.is_whitespace() || "/?#\"'<>".contains(c))
            .unwrap_or(after.len());
        let auth = &after[..end];
        let at = auth.rfind('@').unwrap_or(0);
        match auth[..at].find(':') {
            Some(c) => {
                out.push_str(&auth[..=c]);
                out.push_str(MASK);
                out.push_str(&auth[at..]);
            }
            None => out.push_str(auth),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn bearer(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let (mut last, mut i) = (0, 0);
    while i + 6 < b.len() {
        let hit = b[i..i + 6].eq_ignore_ascii_case(b"bearer")
            && b[i + 6] == b' '
            && (i == 0 || !is_word(b[i - 1]));
        if !hit {
            i += 1;
            continue;
        }
        let j = skip_spaces(b, i + 6);
        let end = value_end(b, j);
        if end > j {
            out.push_str(&s[last..j]);
            out.push_str(MASK);
            last = end;
        }
        i = end.max(i + 6);
    }
    out.push_str(&s[last..]);
    out
}

fn scheme_word(b: &[u8], j: usize) -> usize {
    for w in ["bearer ", "basic "] {
        let e = j + w.len();
        if e <= b.len() && b[j..e].eq_ignore_ascii_case(w.as_bytes()) {
            return skip_spaces(b, e);
        }
    }
    j
}

fn key_at(b: &[u8], i: usize) -> Option<usize> {
    let rest = &b[i..];
    KEYS.iter()
        .map(|k| k.as_bytes())
        .find(|k| {
            rest.get(..k.len())
                .is_some_and(|w| w.eq_ignore_ascii_case(k))
        })
        .map(|k| k.len())
}

fn keyed(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        let Some(k) = key_at(b, i) else {
            i += 1;
            continue;
        };
        let mut j = i + k;
        if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
            j += 1;
        }
        j = skip_spaces(b, j);
        if j >= b.len() || (b[j] != b'=' && b[j] != b':') {
            i += k;
            continue;
        }
        j = skip_spaces(b, j + 1);
        let (start, end) = match b.get(j) {
            Some(&q @ (b'"' | b'\'')) => {
                let mut e = j + 1;
                while e < b.len() && b[e] != q {
                    e += if b[e] == b'\\' { 2 } else { 1 };
                }
                (j + 1, e.min(b.len()))
            }
            _ => {
                let v = scheme_word(b, j);
                (v, value_end(b, v))
            }
        };
        if end > start {
            out.push_str(&s[last..start]);
            out.push_str(MASK);
            last = end;
        }
        i = end.max(i + k);
    }
    out.push_str(&s[last..]);
    out
}

fn run(b: &[u8], from: usize, ok: fn(u8) -> bool) -> usize {
    from + b[from..].iter().take_while(|c| ok(**c)).count()
}

fn secret_end(b: &[u8], i: usize) -> Option<usize> {
    for p in PREFIXES {
        if b[i..].starts_with(p.as_bytes()) {
            let e = run(b, i + p.len(), is_word);
            return (e > i + p.len()).then_some(e);
        }
    }
    if b[i..].starts_with(b"sk-") {
        let e = run(b, i + 3, |c| is_word(c) || c == b'-');
        return (e - (i + 3) >= 16).then_some(e);
    }
    if b[i..].starts_with(b"AKIA") || b[i..].starts_with(b"ASIA") {
        let e = i + 20;
        let key = b.get(i + 4..e)?;
        let upper = key
            .iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        return (upper && !b.get(e).is_some_and(|c| is_word(*c))).then_some(e);
    }
    None
}

fn prefixed(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let (mut last, mut i) = (0, 0);
    while i < b.len() {
        if i == 0 || !is_word(b[i - 1]) {
            if let Some(e) = secret_end(b, i) {
                out.push_str(&s[last..i]);
                out.push_str(MASK);
                last = e;
                i = e;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&s[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervised_threads_restart_after_a_panic_and_stop_when_done() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let runs = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        let r = runs.clone();
        assert!(supervise_with(
            "sb-test-sup",
            Duration::from_millis(5),
            move || {
                let n = r.fetch_add(1, Ordering::SeqCst) + 1;
                if n < 3 {
                    panic!("boom {n}");
                }
                let _ = tx.send(n);
            }
        ));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(3));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-dlog-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn read(d: &Path, name: &str) -> String {
        fs::read_to_string(d.join(name)).unwrap_or_default()
    }

    #[test]
    fn stamps() {
        assert_eq!(stamp(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(stamp(1_709_210_096_789), "2024-02-29T12:34:56.789Z");
    }

    #[test]
    fn rotation_shifts_keeps_and_truncates() {
        let d = dir("rotate");
        fs::write(d.join(FILE), "small\n").unwrap();
        assert!(!rotate(&d, 100, 5).unwrap());
        assert_eq!(read(&d, FILE), "small\n");
        for g in 1..=6 {
            fs::write(d.join(FILE), format!("gen{g}\n").repeat(21)).unwrap();
            assert!(rotate(&d, 100, 5).unwrap());
            assert_eq!(fs::metadata(d.join(FILE)).unwrap().len(), 0);
        }
        assert_eq!(read(&d, "daemon.log.1"), "gen6\n".repeat(21));
        assert_eq!(read(&d, "daemon.log.2"), "gen5\n".repeat(21));
        assert_eq!(read(&d, "daemon.log.4"), "gen3\n".repeat(21));
        assert!(!d.join("daemon.log.5").exists());
        let n = fs::read_dir(&d).unwrap().count();
        assert_eq!(n, 5);
        let missing = dir("missing");
        assert!(!rotate(&missing, 100, 5).unwrap());
        let _ = fs::remove_dir_all(&missing);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rotation_keeps_appending_through_the_same_fd() {
        use std::io::Write;
        let d = dir("append");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(d.join(FILE))
            .unwrap();
        f.write_all(&[b'x'; 200]).unwrap();
        assert!(rotate(&d, 100, 5).unwrap());
        f.write_all(b"after\n").unwrap();
        assert_eq!(read(&d, FILE), "after\n");
        let _ = fs::remove_dir_all(&d);
    }

    const CASES: &str = r#"Authorization: Bearer abc.def-123 => Authorization: Bearer ***
sent bearer xyz to host => sent bearer *** to host
token=abc123&x=1 => token=***&x=1
TOKEN: abc123 => TOKEN: ***
{"token":"abc","n":1} => {"token":"***","n":1}
{"password": "p\"w d"} => {"password": "***"}
passwd=hunter2 next => passwd=*** next
Secret = s3cr3t => Secret = ***
api_key=k1, apikey: k2 => api_key=***, apikey: ***
GITHUB_TOKEN=ghp_abc => GITHUB_TOKEN=***
authorization=Basic dXNlcjpw => authorization=Basic ***
push with ghp_A1b2C3_d4 ok => push with *** ok
gho_x ghu_y ghs_z ghr_w => *** *** *** ***
key AKIAIOSFODNN7EXAMPLE end => key *** end
tmp ASIAIOSFODNN7EXAMPLE end => tmp *** end
AKIA short and xAKIAIOSFODNN7EXAMPLE stay => AKIA short and xAKIAIOSFODNN7EXAMPLE stay
pat github_pat_11AB_cd => pat ***
key sk-abcdefghijklmnop-123 end => key *** end
sk-short stays => sk-short stays
clone https://user:pa55@github.com/a/b.git => clone https://user:***@github.com/a/b.git
http://host:8080/x and ssh://git@h/r => http://host:8080/x and ssh://git@h/r"#;

    #[test]
    fn redacts_secrets() {
        for case in CASES.lines() {
            let (input, want) = case.split_once(" => ").unwrap();
            assert_eq!(redact(input), want, "{input}");
        }
        let plain = "overseer daemon: listening on /run/x.sock (tokens: 3, é ünïcode)";
        assert_eq!(redact(plain), plain);
    }

    #[test]
    fn tails_across_rotated_files() {
        let d = dir("tail");
        assert!(tail(&d, 10).is_empty());
        fs::write(d.join("daemon.log.1"), "a1\na2\na3 token=x\n").unwrap();
        fs::write(d.join("daemon.log.2"), "old\n").unwrap();
        fs::write(d.join(FILE), "b1\nb2\n").unwrap();
        assert_eq!(tail(&d, 1), ["b2"]);
        assert_eq!(tail(&d, 2), ["b1", "b2"]);
        assert_eq!(tail(&d, 4), ["a2", "a3 token=***", "b1", "b2"]);
        assert_eq!(tail(&d, 100).len(), 5);
        let v = api(&d, Some("3"));
        assert_eq!(v["lines"], json!(["a3 token=***", "b1", "b2"]));
        assert!(v["path"].as_str().unwrap().ends_with(FILE));
        assert_eq!(api(&d, Some("0"))["lines"], json!(["b2"]));
        assert_eq!(api(&d, Some("nope"))["lines"].as_array().unwrap().len(), 5);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn tail_reads_only_the_end_of_big_files() {
        let d = dir("big");
        let mut s = "x".repeat(WINDOW as usize + 10);
        s.push_str("\nlast\n");
        fs::write(d.join(FILE), s).unwrap();
        assert_eq!(tail(&d, 5000), ["last"]);
        let _ = fs::remove_dir_all(&d);
    }
}
