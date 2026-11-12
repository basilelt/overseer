use std::time::{Duration, Instant};

pub const SHELL_FALLBACK: &str = r#"exec "${SHELL:-/bin/sh}""#;

pub fn safe_session_id(id: &str) -> bool {
    !(id.is_empty() || id.contains('/') || id.contains(".."))
}

pub fn ticket(s: &str) -> Option<String> {
    static KEYS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let keys = KEYS.get_or_init(|| {
        std::env::var("OVERSEER_TICKET_KEYS")
            .unwrap_or_default()
            .split(',')
            .map(|k| k.trim().to_ascii_uppercase())
            .filter(|k| !k.is_empty())
            .collect()
    });
    ticket_in(s, keys)
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn ticket_any(s: &str) -> Option<String> {
    ticket(&s.to_ascii_uppercase())
}

pub fn ticket_exact(k: &str) -> Result<String, String> {
    ticket_any(k)
        .filter(|t| t.eq_ignore_ascii_case(k))
        .ok_or_else(|| format!("'{k}' is not a ticket key (PROJECT-123)"))
}

fn ticket_in(s: &str, keys: &[String]) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_alphabetic() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 1;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let digits = b
                .get(j + 1..)
                .map_or(0, |r| r.iter().take_while(|c| c.is_ascii_digit()).count());
            if j - i >= 2 && j < b.len() && b[j] == b'-' && digits > 0 {
                let project = &s[i..j];
                let upper = project.bytes().all(|c| !c.is_ascii_lowercase());
                if upper || keys.iter().any(|k| k.eq_ignore_ascii_case(project)) {
                    let key = project.to_ascii_uppercase();
                    return Some(format!("{key}-{}", &s[j + 1..j + 1 + digits]));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

pub fn normalize_remote(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let (host, path) = if let Some(i) = url.find("://") {
        let scheme = &url[..i];
        let rest = &url[i + 3..];
        if scheme == "file" {
            return Some(format!("local:{}", strip_git(rest)));
        }
        let (auth, path) = rest.split_once('/').unwrap_or((rest, ""));
        (auth.to_string(), path.to_string())
    } else if url.starts_with('/') || url.starts_with('.') {
        return Some(format!("local:{}", strip_git(url)));
    } else if let Some((auth, path)) = url.split_once(':') {
        (auth.to_string(), path.to_string())
    } else {
        return Some(format!("local:{}", strip_git(url)));
    };
    let host = host.rsplit('@').next().unwrap_or("");
    let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
    let path = strip_git(path.trim_matches('/'));
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!("{host}/{path}"))
}

fn strip_git(p: &str) -> String {
    let p = p.trim_end_matches('/');
    p.strip_suffix(".git").unwrap_or(p).to_string()
}

pub fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@%+,-".contains(c));
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?);
    let mut ms = ((days * 24 + n(11..13)?) * 60 + n(14..16)?) * 60_000 + n(17..19)? * 1000;
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let digits: String = frac.chars().take_while(|c| c.is_ascii_digit()).collect();
        let padded = format!("{:0<3}", &digits[..digits.len().min(3)]);
        ms += padded.parse::<i64>().ok()?;
        rest = &frac[digits.len()..];
    }
    if rest == "Z" || rest == "z" {
        return Some(ms);
    }
    let sign = match rest.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let off = rest.get(1..3)?.parse::<i64>().ok()? * 60 + rest.get(4..6)?.parse::<i64>().ok()?;
    Some(ms - sign * off * 60_000)
}

pub fn format_rfc3339(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86400));
    let t = secs.rem_euclid(86400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        t / 3600,
        t / 60 % 60,
        t % 60
    )
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn ulid(ms: i64) -> String {
    const A: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut rnd = [0u8; 10];
    if crate::sys::random(&mut rnd).is_err() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            ^ (std::process::id() as u128) << 64;
        rnd.copy_from_slice(&nanos.to_le_bytes()[..10]);
    }
    let mut v: u128 = (ms as u128 & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in rnd.iter().enumerate() {
        v |= (*b as u128) << (8 * (9 - i));
    }
    (0..26)
        .map(|i| A[((v >> (125 - 5 * i)) & 31) as usize] as char)
        .collect()
}

const SUPERVISE_MAX_BACKOFF: Duration = Duration::from_secs(60);
const SUPERVISE_HEALTHY: Duration = Duration::from_secs(300);

pub fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

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
                crate::dlog::line(
                    "error",
                    &format!(
                        "overseer daemon: {name} thread panicked ({}); restarting in {} ms",
                        panic_text(p.as_ref()),
                        backoff.as_millis()
                    ),
                );
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(SUPERVISE_MAX_BACKOFF);
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_with_path_parts_are_unsafe() {
        assert!(safe_session_id("abc-123"));
        for bad in ["", "a/b", "..", "a..b", "../x"] {
            assert!(!safe_session_id(bad), "{bad}");
        }
    }

    #[test]
    fn the_shell_fallback_is_the_login_shell_or_sh() {
        assert_eq!(SHELL_FALLBACK, "exec \"${SHELL:-/bin/sh}\"");
    }

    #[test]
    fn percent_encode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(percent_encode("aZ09-._~"), "aZ09-._~");
        assert_eq!(percent_encode("a b/é&"), "a%20b%2F%C3%A9%26");
    }

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
        assert_eq!(panic_text(&"x"), "x");
        assert_eq!(panic_text(&String::from("y")), "y");
        assert_eq!(panic_text(&5u8), "unknown panic");
    }

    #[test]
    fn tickets() {
        assert_eq!(ticket("feature/AS-1234-login").as_deref(), Some("AS-1234"));
        assert_eq!(ticket("feat/PROJ-12-x").as_deref(), Some("PROJ-12"));
        assert_eq!(ticket("fix/OPS2_X-7").as_deref(), Some("OPS2_X-7"));
        assert_eq!(ticket("CLASS-12").as_deref(), Some("CLASS-12"));
        assert_eq!(ticket("fix/issue-57-open-files"), None);
        assert_eq!(ticket("build/as-10844-version"), None);
        assert_eq!(ticket("X-12"), None);
        assert_eq!(ticket("feat/aCLASS-12"), None);
        assert_eq!(ticket("as-x12"), None);
        assert_eq!(ticket("main"), None);
        assert_eq!(ticket("AS-"), None);
        assert_eq!(ticket("AS-x"), None);
        assert_eq!(ticket_any("proj-9").as_deref(), Some("PROJ-9"));
    }

    #[test]
    fn exact_ticket_keys_only() {
        assert_eq!(ticket_exact("proj-9").as_deref(), Ok("PROJ-9"));
        assert_eq!(
            ticket_exact("feat/AS-1"),
            Err("'feat/AS-1' is not a ticket key (PROJECT-123)".to_string())
        );
        assert!(ticket_exact("").is_err());
    }

    #[test]
    fn configured_keys_match_lowercase_branches() {
        let keys = vec!["AS".to_string()];
        assert_eq!(
            ticket_in("build/as-10844-version", &keys).as_deref(),
            Some("AS-10844")
        );
        assert_eq!(ticket_in("feedback/As-7", &keys).as_deref(), Some("AS-7"));
        assert_eq!(ticket_in("fix/issue-57", &keys), None);
        assert_eq!(ticket_in("feat/OPS-3", &keys).as_deref(), Some("OPS-3"));
    }

    #[test]
    fn remotes() {
        let n = |u| normalize_remote(u);
        assert_eq!(
            n("git@github.com:Acme/widget.git").as_deref(),
            Some("github.com/Acme/widget")
        );
        assert_eq!(
            n("ssh://git@host:2222/A/b.git").as_deref(),
            Some("host/A/b")
        );
        assert_eq!(
            n("https://user:tok@GitHub.com/A/b.git/").as_deref(),
            Some("github.com/A/b")
        );
        assert_eq!(n("https://host:443/A/b").as_deref(), Some("host/A/b"));
        assert_eq!(n("/srv/git/x.git").as_deref(), Some("local:/srv/git/x"));
        assert_eq!(n("file:///srv/x.git").as_deref(), Some("local:/srv/x"));
        assert_eq!(n(""), None);
    }

    #[test]
    fn quoting() {
        assert_eq!(shell_quote("/a/b-c"), "/a/b-c");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn times() {
        let ms = parse_rfc3339("2026-10-28T09:00:00.000Z").unwrap();
        assert_eq!(format_rfc3339(ms), "2026-10-28T09:00:00Z");
        assert_eq!(parse_rfc3339("2026-10-28T11:00:00+02:00"), Some(ms));
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00.5Z"), Some(500));
        assert_eq!(parse_rfc3339("nope"), None);
    }

    #[test]
    fn ulids() {
        let a = ulid(1_793_178_000_000);
        assert_eq!(a.len(), 26);
        assert_ne!(a, ulid(1_793_178_000_000));
        assert!(ulid(1) < ulid(2));
    }
}
