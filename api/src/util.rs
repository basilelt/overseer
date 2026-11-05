use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use std::io::Read;

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn rfc3339(ms: i64) -> String {
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

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .expect("/dev/urandom is required");
    b
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn token(prefix: &str) -> String {
    format!("{prefix}_{}", hex(&random::<32>()))
}

pub fn sha256(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

pub fn hmac_hex(key: &str, msg: &str) -> String {
    let mut m = Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("hmac accepts any key");
    m.update(msg.as_bytes());
    hex(&m.finalize().into_bytes())
}

pub fn sign(secret: &str, t: i64, body: &str) -> String {
    format!("t={t},v1={}", hmac_hex(secret, &format!("{t}.{body}")))
}

pub fn ulid(ms: i64) -> String {
    const A: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut v: u128 = (ms as u128 & 0xFFFF_FFFF_FFFF) << 80;
    for (i, b) in random::<10>().iter().enumerate() {
        v |= (*b as u128) << (8 * (9 - i));
    }
    (0..26)
        .map(|i| A[((v >> (125 - 5 * i)) & 31) as usize] as char)
        .collect()
}

pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
}

pub fn ticket_ok(s: &str) -> bool {
    s.split_once('-').is_some_and(|(p, n)| {
        p.len() >= 2
            && p.as_bytes()[0].is_ascii_uppercase()
            && p.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            && !n.is_empty()
            && n.bytes().all(|b| b.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_761_652_800_000), "2025-10-28T12:00:00Z");
        assert_eq!(ulid(0).len(), 26);
        assert!(
            ticket_ok("AS-12") && ticket_ok("PROJ2-7") && !ticket_ok("AS-") && !ticket_ok("as-1")
        );
        assert!(!ticket_ok("A-1") && !ticket_ok("1A-1") && !ticket_ok("AS-1-2"));
        assert!(valid_id("vm-node2-dev1") && !valid_id("a/b") && !valid_id(""));
        assert_eq!(
            sha256("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hmac_hex("key", "The quick brown fox jumps over the lazy dog"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
        assert_eq!(
            sign("k", 7, "b"),
            format!("t=7,v1={}", hmac_hex("k", "7.b"))
        );
    }
}
