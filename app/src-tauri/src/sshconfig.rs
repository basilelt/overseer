use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq)]
struct Block {
    patterns: Vec<String>,
    options: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SshConfig {
    blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostConfig {
    pub alias: String,
    pub host_name: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_files: Vec<String>,
    pub proxy_jump: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshHost {
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

fn split_kv(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let idx = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = line[..idx].to_ascii_lowercase();
    let value = line[idx..]
        .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
        .trim();
    Some((key, value.trim_matches('"').to_string()))
}

fn glob(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], t) || (!t.is_empty() && go(p, &t[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &t[1..]),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => go(&p[1..], &t[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

fn matches(patterns: &[String], host: &str) -> bool {
    let mut hit = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if glob(neg, host) {
                return false;
            }
        } else if glob(p, host) {
            hit = true;
        }
    }
    hit
}

impl SshConfig {
    pub fn parse(text: &str, include: &mut dyn FnMut(&str) -> Vec<String>) -> Self {
        let mut cfg = SshConfig {
            blocks: vec![Block {
                patterns: vec!["*".into()],
                options: vec![],
            }],
        };
        cfg.parse_into(text, include, 0);
        cfg
    }

    fn parse_into(
        &mut self,
        text: &str,
        include: &mut dyn FnMut(&str) -> Vec<String>,
        depth: usize,
    ) {
        for line in text.lines() {
            let Some((key, value)) = split_kv(line) else {
                continue;
            };
            match key.as_str() {
                "host" => self.blocks.push(Block {
                    patterns: value.split_whitespace().map(String::from).collect(),
                    options: vec![],
                }),
                "match" => self.blocks.push(Block {
                    patterns: vec![],
                    options: vec![],
                }),
                "include" if depth < 8 => {
                    for pat in value.split_whitespace() {
                        for sub in include(pat) {
                            self.parse_into(&sub, include, depth + 1);
                        }
                    }
                }
                _ => self.blocks.last_mut().unwrap().options.push((key, value)),
            }
        }
    }

    pub fn load(path: &Path) -> Self {
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Self::parse(&text, &mut |pat| read_include(&dir, pat))
    }

    pub fn resolve(&self, alias: &str) -> HostConfig {
        let get = |k: &str| -> Option<String> {
            self.blocks
                .iter()
                .filter(|b| matches(&b.patterns, alias))
                .flat_map(|b| b.options.iter())
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        let host_name = get("hostname")
            .map(|h| h.replace("%h", alias))
            .unwrap_or_else(|| alias.to_string());
        let user = get("user");
        let port = get("port").and_then(|p| p.parse().ok()).unwrap_or(22);
        let proxy_jump = get("proxyjump")
            .filter(|v| !v.eq_ignore_ascii_case("none"))
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let identity_files = self
            .blocks
            .iter()
            .filter(|b| matches(&b.patterns, alias))
            .flat_map(|b| b.options.iter())
            .filter(|(k, _)| k == "identityfile")
            .map(|(_, v)| v.clone())
            .collect();
        HostConfig {
            alias: alias.to_string(),
            host_name,
            user,
            port,
            identity_files,
            proxy_jump,
        }
    }

    pub fn hosts(&self) -> Vec<SshHost> {
        let mut out: Vec<SshHost> = Vec::new();
        for b in &self.blocks {
            for p in &b.patterns {
                if p.contains(['*', '?', '!']) || out.iter().any(|h| &h.alias == p) {
                    continue;
                }
                let r = self.resolve(p);
                out.push(SshHost {
                    alias: p.clone(),
                    host_name: (r.host_name != *p).then_some(r.host_name),
                    user: r.user,
                    port: (r.port != 22).then_some(r.port),
                });
            }
        }
        out
    }
}

fn read_include(dir: &Path, pattern: &str) -> Vec<String> {
    let expanded = expand_tilde(pattern);
    let full = if Path::new(&expanded).is_absolute() {
        PathBuf::from(expanded)
    } else {
        dir.join(expanded)
    };
    let (parent, name) = match (full.parent(), full.file_name().and_then(|n| n.to_str())) {
        (Some(p), Some(n)) => (p.to_path_buf(), n.to_string()),
        _ => return vec![],
    };
    let mut files: Vec<PathBuf> = if name.contains(['*', '?']) {
        std::fs::read_dir(&parent)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| glob(&name, n))
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![full]
    };
    files.sort();
    files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect()
}

pub fn expand_tilde(p: &str) -> String {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => p.to_string(),
    }
}

pub fn user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh").join("config"))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Destination {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

pub fn parse_destination(s: &str) -> Destination {
    let s = s.trim().trim_start_matches("ssh://");
    let (user, rest) = match s.rsplit_once('@') {
        Some((u, r)) => (Some(u.to_string()), r),
        None => (None, s),
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() && !h.is_empty() => {
            (h.to_string(), p.parse().ok())
        }
        _ => (rest.to_string(), None),
    };
    Destination { user, host, port }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: &str = r#"
# global
Host devbox
    HostName devbox.internal
    User alice
    IdentityFile ~/.ssh/id_dev

Host gpu gpu2
  HostName=gpu.lab.example.com
  Port 2222
  ProxyJump bastion,edge

Host *.corp !secret.corp
  User corp
  Port 2200

Host *
  User fallback
  IdentityFile ~/.ssh/id_ed25519
  ProxyJump none

Include extra/*.conf
"#;

    fn cfg() -> SshConfig {
        SshConfig::parse(CFG, &mut |pat| {
            assert_eq!(pat, "extra/*.conf");
            vec!["Host included\n  HostName inc.example.com\n  User late\n".into()]
        })
    }

    #[test]
    fn first_match_wins_and_identity_files_accumulate() {
        let h = cfg().resolve("devbox");
        assert_eq!(h.host_name, "devbox.internal");
        assert_eq!(h.user.as_deref(), Some("alice"));
        assert_eq!(h.port, 22);
        assert_eq!(h.identity_files, vec!["~/.ssh/id_dev", "~/.ssh/id_ed25519"]);
        assert!(h.proxy_jump.is_empty());
    }

    #[test]
    fn patterns_negation_and_equals_syntax() {
        let c = cfg();
        let g = c.resolve("gpu2");
        assert_eq!(
            (g.host_name.as_str(), g.port),
            ("gpu.lab.example.com", 2222)
        );
        assert_eq!(g.proxy_jump, vec!["bastion", "edge"]);
        assert_eq!(c.resolve("a.corp").user.as_deref(), Some("corp"));
        assert_eq!(c.resolve("secret.corp").user.as_deref(), Some("fallback"));
        assert_eq!(c.resolve("secret.corp").port, 22);
        assert_eq!(c.resolve("other").host_name, "other");
    }

    #[test]
    fn include_is_parsed_after_earlier_blocks() {
        let c = cfg();
        let h = c.resolve("included");
        assert_eq!(h.host_name, "inc.example.com");
        assert_eq!(h.user.as_deref(), Some("fallback"));
    }

    #[test]
    fn lists_concrete_hosts_only() {
        let aliases: Vec<_> = cfg().hosts().into_iter().map(|h| h.alias).collect();
        assert_eq!(aliases, vec!["devbox", "gpu", "gpu2", "included"]);
        let gpu = &cfg().hosts()[1];
        assert_eq!(gpu.port, Some(2222));
        assert_eq!(gpu.user.as_deref(), Some("fallback"));
    }

    #[test]
    fn destinations() {
        assert_eq!(
            parse_destination("me@host:2200"),
            Destination {
                user: Some("me".into()),
                host: "host".into(),
                port: Some(2200)
            }
        );
        assert_eq!(
            parse_destination("devbox"),
            Destination {
                user: None,
                host: "devbox".into(),
                port: None
            }
        );
        assert_eq!(
            parse_destination("ssh://a@b"),
            Destination {
                user: Some("a".into()),
                host: "b".into(),
                port: None
            }
        );
    }

    #[test]
    fn glob_matching() {
        assert!(glob("*.corp", "x.CORP"));
        assert!(glob("h?st", "host"));
        assert!(!glob("h?st", "hoost"));
    }
}
