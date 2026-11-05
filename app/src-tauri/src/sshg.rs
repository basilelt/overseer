use crate::sshconfig::{parse_destination, Destination};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolved {
    pub alias: String,
    pub host_name: String,
    pub port: u16,
    pub user: String,
    pub identity_files: Vec<String>,
    pub identity_agent: Option<String>,
    pub proxy_command: Option<String>,
    pub proxy_jump: Option<String>,
    pub known_hosts: Vec<String>,
    pub strict_host_key_checking: String,
    pub server_alive_interval: u32,
    pub server_alive_count_max: u32,
    pub connect_timeout: Option<u32>,
    pub address_family: String,
}

fn none(v: Option<&String>) -> Option<String> {
    v.filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
        .cloned()
}

pub fn parse(alias: &str, out: &str) -> Resolved {
    let mut one: HashMap<String, String> = HashMap::new();
    let mut many: HashMap<String, Vec<String>> = HashMap::new();
    for line in out.lines() {
        let Some((k, v)) = line.trim().split_once(' ') else {
            continue;
        };
        let k = k.to_ascii_lowercase();
        let v = v.trim().to_string();
        many.entry(k.clone()).or_default().push(v.clone());
        one.entry(k).or_insert(v);
    }
    let num = |k: &str| one.get(k).and_then(|v| v.parse().ok());
    Resolved {
        alias: alias.to_string(),
        host_name: one
            .get("hostname")
            .cloned()
            .unwrap_or_else(|| alias.to_string()),
        port: num("port").unwrap_or(22),
        user: one.get("user").cloned().unwrap_or_default(),
        identity_files: many.remove("identityfile").unwrap_or_default(),
        identity_agent: none(one.get("identityagent")),
        proxy_command: none(one.get("proxycommand")),
        proxy_jump: none(one.get("proxyjump")),
        known_hosts: one
            .get("userknownhostsfile")
            .map(|v| v.split_whitespace().map(String::from).collect())
            .unwrap_or_default(),
        strict_host_key_checking: one
            .get("stricthostkeychecking")
            .cloned()
            .unwrap_or_else(|| "ask".into()),
        server_alive_interval: one
            .get("serveraliveinterval")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        server_alive_count_max: one
            .get("serveralivecountmax")
            .and_then(|v| v.parse().ok())
            .unwrap_or(3),
        connect_timeout: one.get("connecttimeout").and_then(|v| v.parse().ok()),
        address_family: one
            .get("addressfamily")
            .cloned()
            .unwrap_or_else(|| "any".into()),
    }
}

pub fn substitute(template: &str, r: &Resolved, local_user: &str) -> String {
    let mut out = String::new();
    let mut it = template.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('h') => out.push_str(&r.host_name),
            Some('p') => out.push_str(&r.port.to_string()),
            Some('r') => out.push_str(&r.user),
            Some('n') => out.push_str(&r.alias),
            Some('u') => out.push_str(local_user),
            Some('%') => out.push('%'),
            Some(o) => {
                out.push('%');
                out.push(o);
            }
            None => out.push('%'),
        }
    }
    out
}

pub fn jump_chain(spec: &str) -> Vec<Destination> {
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none"))
        .map(parse_destination)
        .collect()
}

pub fn hop_args(d: &Destination) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(u) = &d.user {
        a.extend(["-l".to_string(), u.clone()]);
    }
    if let Some(p) = d.port {
        a.extend(["-p".to_string(), p.to_string()]);
    }
    a.push(d.host.clone());
    a
}

pub fn endpoint(r: &Resolved) -> String {
    let user = if r.user.is_empty() {
        String::new()
    } else {
        format!(" as {}", r.user)
    };
    format!("{}:{}{user}", r.host_name, r.port)
}

pub fn describe(r: &Resolved, hops: &[Resolved], local_user: &str) -> String {
    let mut s = format!("{} → {}", r.alias, endpoint(r));
    if let Some(pc) = &r.proxy_command {
        s.push_str(&format!(
            " via ProxyCommand `{}`",
            substitute(pc, r, local_user)
        ));
    } else if let Some(pj) = &r.proxy_jump {
        let shown: Vec<String> = jump_chain(pj)
            .iter()
            .enumerate()
            .map(|(i, d)| match hops.get(i) {
                Some(h) if h.host_name != d.host || h.port != d.port.unwrap_or(22) => {
                    format!("{} ({})", d.host, endpoint(h))
                }
                _ => d.host.clone(),
            })
            .collect();
        s.push_str(&format!(" via ProxyJump {}", shown.join(" → ")));
    }
    if let Some(a) = &r.identity_agent {
        s.push_str(&format!(", agent {a}"));
    }
    s
}

#[derive(Debug, Clone, Default)]
pub struct SshCmd {
    pub program: PathBuf,
    pub config: Option<PathBuf>,
}

impl SshCmd {
    pub fn from_env() -> Self {
        let program = std::env::var_os("OVERSEER_SSH")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(default_program);
        let config = std::env::var_os("OVERSEER_SSH_CONFIG")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self { program, config }
    }

    pub fn command(&self) -> tokio::process::Command {
        let mut c = tokio::process::Command::new(&self.program);
        if let Some(f) = &self.config {
            c.arg("-F").arg(f);
        }
        #[cfg(windows)]
        c.creation_flags(0x0800_0000);
        c
    }

    pub async fn resolve(&self, dest_args: &[String], alias: &str) -> Result<Resolved, String> {
        let mut c = self.command();
        c.arg("-G")
            .args(dest_args)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(10), c.output())
            .await
            .map_err(|_| "ssh -G timed out".to_string())?
            .map_err(|e| {
                format!(
                    "cannot run {}: {e}{}",
                    self.program.display(),
                    missing_hint()
                )
            })?;
        if !out.status.success() {
            return Err(format!(
                "ssh -G {alias}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(parse(alias, &String::from_utf8_lossy(&out.stdout)))
    }

    pub async fn resolve_with_hops(
        &self,
        dest_args: &[String],
        alias: &str,
    ) -> Result<(Resolved, Vec<Resolved>), String> {
        let r = self.resolve(dest_args, alias).await?;
        let mut hops = Vec::new();
        if r.proxy_command.is_none() {
            for d in jump_chain(r.proxy_jump.as_deref().unwrap_or("")) {
                if let Ok(h) = self.resolve(&hop_args(&d), &d.host).await {
                    hops.push(h);
                }
            }
        }
        Ok((r, hops))
    }
}

fn default_program() -> PathBuf {
    if cfg!(windows) {
        let sys = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let p = sys.join(r"System32\OpenSSH\ssh.exe");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("ssh")
}

fn missing_hint() -> &'static str {
    if cfg!(windows) {
        " (install the OpenSSH client: Settings → Apps → Optional features → OpenSSH Client)"
    } else {
        " (is the OpenSSH client installed and on PATH?)"
    }
}

pub fn dest_args(dest: &str) -> (Vec<String>, String) {
    let d = parse_destination(dest);
    (hop_args(&d), d.host)
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "user alice\nhostname workstation.example.net\nport 2222\nidentityfile ~/.ssh/id_ed25519\nidentityfile ~/.ssh/id_rsa\n\
identityagent \"~/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock\"\nproxycommand netbird ssh proxy %h %p\nproxyjump none\n\
userknownhostsfile ~/.ssh/known_hosts ~/.ssh/known_hosts2\nstricthostkeychecking accept-new\nserveraliveinterval 30\nserveralivecountmax 4\nconnecttimeout none\n";

    #[test]
    fn parses_ssh_g_output() {
        let r = parse("alice-workstation", G);
        assert_eq!(r.host_name, "workstation.example.net");
        assert_eq!((r.port, r.user.as_str()), (2222, "alice"));
        assert_eq!(r.identity_files, vec!["~/.ssh/id_ed25519", "~/.ssh/id_rsa"]);
        assert!(r.identity_agent.as_deref().unwrap().contains("1password"));
        assert_eq!(r.proxy_command.as_deref(), Some("netbird ssh proxy %h %p"));
        assert_eq!(r.proxy_jump, None);
        assert_eq!(
            r.known_hosts,
            vec!["~/.ssh/known_hosts", "~/.ssh/known_hosts2"]
        );
        assert_eq!(r.strict_host_key_checking, "accept-new");
        assert_eq!((r.server_alive_interval, r.server_alive_count_max), (30, 4));
        assert_eq!(r.connect_timeout, None);
        let d = parse("x", "");
        assert_eq!(
            (
                d.host_name.as_str(),
                d.port,
                d.server_alive_interval,
                d.strict_host_key_checking.as_str()
            ),
            ("x", 22, 0, "ask")
        );
    }

    #[test]
    fn substitutes_proxy_command_tokens() {
        let r = parse("ws", G);
        assert_eq!(
            substitute("nc %h %p", &r, "me"),
            "nc workstation.example.net 2222"
        );
        assert_eq!(
            substitute("ssh -W %h:%p %r@jump # %n %u 100%% %C %", &r, "me"),
            "ssh -W workstation.example.net:2222 alice@jump # ws me 100% %C %"
        );
    }

    #[test]
    fn builds_jump_chains() {
        let c = jump_chain("admin@bastion:2200, edge ,none");
        assert_eq!(c.len(), 2);
        assert_eq!(
            hop_args(&c[0]),
            vec!["-l", "admin", "-p", "2200", "bastion"]
        );
        assert_eq!(hop_args(&c[1]), vec!["edge"]);
        assert!(jump_chain("none").is_empty());
        assert_eq!(
            dest_args("me@host:2022"),
            (
                vec![
                    "-l".into(),
                    "me".into(),
                    "-p".into(),
                    "2022".into(),
                    "host".into()
                ],
                "host".into()
            )
        );
    }

    #[test]
    fn describes_the_resolution() {
        let r = parse("alice-workstation", G);
        assert_eq!(
            describe(&r, &[], "me"),
            "alice-workstation → workstation.example.net:2222 as alice via ProxyCommand `netbird ssh proxy workstation.example.net 2222`, agent \"~/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock\""
        );
        let gpu = parse(
            "gpu",
            "hostname 10.0.0.5\nport 22\nuser me\nproxyjump bastion,edge:2200\n",
        );
        let hops = vec![
            parse(
                "bastion",
                "hostname bastion.example.com\nport 22\nuser me\n",
            ),
            parse("edge", "hostname edge\nport 2200\n"),
        ];
        assert_eq!(
            describe(&gpu, &hops, "me"),
            "gpu → 10.0.0.5:22 as me via ProxyJump bastion (bastion.example.com:22 as me) → edge"
        );
    }

    #[tokio::test]
    async fn real_ssh_g_honors_match_blocks_the_old_parser_ignored() {
        let Ok(out) = std::process::Command::new("ssh").arg("-V").output() else {
            eprintln!("skipped: no ssh binary");
            return;
        };
        assert!(out.status.success() || !out.stderr.is_empty());
        let dir = std::env::temp_dir().join(format!("sb-sshg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("config");
        let text = "Match host alice-workstation\n  HostName workstation.example.net\n  ProxyCommand nc %h %p\n\nHost alice-workstation\n  HostName workstation\n  User alice\n";
        std::fs::write(&cfg, text).unwrap();
        let cmd = SshCmd {
            program: "ssh".into(),
            config: Some(cfg.clone()),
        };
        let (args, alias) = dest_args("alice-workstation");
        let r = cmd.resolve(&args, &alias).await.unwrap();
        assert_eq!(r.host_name, "workstation.example.net");
        assert_eq!(r.proxy_command.as_deref(), Some("nc %h %p"));
        let old =
            crate::sshconfig::SshConfig::parse(text, &mut |_| vec![]).resolve("alice-workstation");
        assert_eq!(old.host_name, "workstation");
        let _ = std::fs::remove_dir_all(dir);
    }
}
