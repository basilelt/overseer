use russh::keys::PublicKey;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub enum Verdict {
    Trusted,
    Unknown,
    Changed,
}

pub fn pattern(host: &str, port: u16) -> String {
    let host = host.to_ascii_lowercase();
    if port == 22 {
        host
    } else {
        format!("[{host}]:{port}")
    }
}

pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[cfg(test)]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn read(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    pub fn check(&self, host: &str, port: u16, key: &PublicKey) -> Verdict {
        check(&self.read(), host, port, key)
    }

    pub fn trust(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), String> {
        let kept = remove(&self.read(), host, port, Some(key.algorithm().as_str()));
        let line = format!("{} {}", pattern(host, port), key_text(key)?);
        self.write(&format!("{kept}{line}\n"))
    }

    pub fn forget(&self, host: &str, port: u16) -> Result<(), String> {
        self.write(&remove(&self.read(), host, port, None))
    }

    fn write(&self, text: &str) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &self.path))
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

fn key_text(key: &PublicKey) -> Result<String, String> {
    let mut k = key.clone();
    k.set_comment("");
    k.to_openssh()
        .map(|s| s.trim().to_string())
        .map_err(|e| e.to_string())
}

fn entries(text: &str) -> impl Iterator<Item = (&str, &str, &str)> {
    text.lines().filter_map(|l| {
        let l = l.trim();
        if l.is_empty() || l.starts_with('#') {
            return None;
        }
        let mut it = l.split_whitespace();
        Some((it.next()?, it.next()?, it.next()?))
    })
}

pub fn check(text: &str, host: &str, port: u16, key: &PublicKey) -> Verdict {
    let want = pattern(host, port);
    let Ok(mine) = key_text(key) else {
        return Verdict::Unknown;
    };
    let alg = key.algorithm();
    let mut same_alg_other_key = false;
    for (hosts, a, b64) in entries(text) {
        if !hosts.split(',').any(|h| h == want) {
            continue;
        }
        if format!("{a} {b64}") == mine {
            return Verdict::Trusted;
        }
        if a == alg.as_str() {
            same_alg_other_key = true;
        }
    }
    if same_alg_other_key {
        Verdict::Changed
    } else {
        Verdict::Unknown
    }
}

fn remove(text: &str, host: &str, port: u16, alg: Option<&str>) -> String {
    let want = pattern(host, port);
    let mut out = String::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let hit = matches!((it.next(), it.next()), (Some(h), Some(a)) if h.split(',').any(|x| x == want) && alg.is_none_or(|w| w == a));
        if !hit && !line.trim().is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct::keys;

    fn key() -> PublicKey {
        keys::generate("h").unwrap().public_key().clone()
    }

    #[test]
    fn patterns_follow_openssh() {
        assert_eq!(pattern("Box.lan", 22), "box.lan");
        assert_eq!(pattern("10.0.0.2", 2222), "[10.0.0.2]:2222");
    }

    #[test]
    fn unknown_then_trusted_then_changed() {
        let dir = std::env::temp_dir().join(format!(
            "sb-known-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let kh = KnownHosts::new(dir.join("known_hosts"));
        let (k1, k2) = (key(), key());
        assert_eq!(kh.check("box", 22, &k1), Verdict::Unknown);
        kh.trust("box", 22, &k1).unwrap();
        assert_eq!(kh.check("box", 22, &k1), Verdict::Trusted);
        assert_eq!(kh.check("BOX", 22, &k1), Verdict::Trusted);
        assert_eq!(kh.check("box", 2222, &k1), Verdict::Unknown);
        assert_eq!(kh.check("box", 22, &k2), Verdict::Changed);
        kh.trust("other", 2222, &k2).unwrap();
        let text = std::fs::read_to_string(kh.path()).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(
            text.lines()
                .nth(1)
                .unwrap()
                .starts_with("[other]:2222 ssh-ed25519 AAAA"),
            "{text}"
        );
        kh.trust("box", 22, &k2).unwrap();
        assert_eq!(kh.check("box", 22, &k2), Verdict::Trusted);
        assert_eq!(kh.check("box", 22, &k1), Verdict::Changed);
        kh.forget("box", 22).unwrap();
        assert_eq!(kh.check("box", 22, &k2), Verdict::Unknown);
        assert_eq!(kh.check("other", 2222, &k2), Verdict::Trusted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_openssh_lines_with_comments_and_host_lists() {
        let k = key();
        let line = key_text(&k).unwrap();
        let text = format!(
            "# comment\n\nalias,box.lan {line} user@laptop\nbox.lan ecdsa-sha2-nistp256 AAAAE2Vj\n"
        );
        assert_eq!(check(&text, "box.lan", 22, &k), Verdict::Trusted);
        assert_eq!(check(&text, "alias", 22, &k), Verdict::Trusted);
    }

    #[test]
    fn a_key_of_another_algorithm_than_the_trusted_one_is_a_change() {
        let k = key();
        let line = key_text(&k).unwrap();
        let other = "box.lan ecdsa-sha2-nistp256 AAAAE2Vj\n";
        assert_eq!(check(other, "box.lan", 22, &k), Verdict::Changed);
        assert_eq!(check(other, "box.lan", 2222, &k), Verdict::Unknown);
        assert_eq!(check(other, "elsewhere", 22, &k), Verdict::Unknown);
        assert_eq!(
            check(&format!("{other}box.lan {line}\n"), "box.lan", 22, &k),
            Verdict::Trusted
        );
    }
}
