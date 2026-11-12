use crate::askpass::{self, Handler, Registration};
use crate::install::{self, Platform, SERVER_VERSION};
use crate::mux::{Mux, BANNER};
use crate::relay::{self, Relay};
use crate::sshg::{self, Resolved, SshCmd};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Child;

pub const PING: Duration = Duration::from_secs(15);

pub trait Link: Send + Sync {
    fn exited(&self) -> bool;
    fn kill(&self);
}

struct ProcessLink(Mutex<Option<Child>>);

impl Link for ProcessLink {
    fn exited(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .as_mut()
            .map(|c| c.try_wait().ok().flatten().is_some())
            .unwrap_or(true)
    }

    fn kill(&self) {
        if let Some(c) = self.0.lock().unwrap().as_mut() {
            let _ = c.start_kill();
        }
    }
}

pub struct Tunnel {
    pub mux: Arc<Mux>,
    pub installed: Option<String>,
    link: Box<dyn Link>,
    _askpass: Option<Registration>,
    _relay: Option<Relay>,
}

impl Tunnel {
    pub fn new(mux: Arc<Mux>, link: Box<dyn Link>) -> Self {
        Self {
            mux,
            installed: None,
            link,
            _askpass: None,
            _relay: None,
        }
    }

    pub fn with_installed(mut self, installed: Option<String>) -> Self {
        self.installed = installed;
        self
    }
}

#[cfg(test)]
impl Tunnel {
    pub fn kill_link(&self) {
        self.link.kill();
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.mux.kill("disconnected");
        self.link.kill();
    }
}

pub struct Options {
    pub ssh: SshCmd,
    pub askpass_program: Option<PathBuf>,
    pub binaries: Box<dyn Fn(Platform) -> Vec<PathBuf> + Send + Sync>,
    pub replace: bool,
}

#[cfg(desktop)]
pub fn list_hosts() -> Vec<crate::sshconfig::SshHost> {
    let path = crate::sshg::config_override()
        .map(PathBuf::from)
        .or_else(crate::sshconfig::user_config_path);
    path.map(|p| crate::sshconfig::SshConfig::load(&p))
        .unwrap_or_default()
        .hosts()
}

fn local_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

fn detach_flags(replace: bool) -> &'static str {
    if replace {
        "--detach --replace"
    } else {
        "--detach"
    }
}

fn check_version(version: &str) {
    assert!(version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || ".-+".contains(c)));
}

fn receive_upload(dest: &str) -> String {
    format!(
        "mkdir -p \"$d\" || {{ echo \"SB:ERROR cannot create $d\"; exit 3; }}; t=\"$d/.overseer.$$\"; head -c \"$s\" > \"$t\"; \
         x=$( (sha256sum \"$t\" 2>/dev/null || shasum -a 256 \"$t\") | cut -d\" \" -f1); \
         [ \"$x\" = \"$h\" ] || {{ rm -f \"$t\"; echo \"SB:ERROR checksum mismatch after upload ($x)\"; exit 3; }}; \
         chmod 755 \"$t\" && mv -f \"$t\" \"{dest}\" || {{ echo \"SB:ERROR cannot install {dest}\"; exit 3; }}"
    )
}

pub fn bootstrap_script(version: &str, replace: bool) -> String {
    check_version(version);
    [
        &format!("v={version}"),
        "d=\"$HOME/.overseer/server/$v\"",
        "b=\"$d/overseer\"",
        "i=$(\"$b\" version 2>/dev/null | cut -d\" \" -f2)",
        "echo \"SB:HELLO $(uname -s) $(uname -m) ${i:--}\"",
        "read c s h || exit 1",
        &format!("if [ \"$c\" = INSTALL ]; then {}; fi", receive_upload("$b")),
        &format!("\"$b\" daemon {} >&2 || {{ echo \"SB:ERROR the daemon did not start, see its daemon.log\"; exit 4; }}", detach_flags(replace)),
        "exec \"$b\" proxy",
    ]
    .join("; ")
}

pub fn remote_command(version: &str, replace: bool) -> String {
    format!("exec sh -c '{}'", bootstrap_script(version, replace))
}

pub fn installed_script(version: &str, replace: bool) -> String {
    check_version(version);
    [
        &format!("r=\"$HOME/.overseer/server\"; b=\"$r/{version}/overseer\""),
        "[ -x \"$b\" ] || b=$(ls -t \"$r\"/*/overseer 2>/dev/null | head -n 1)",
        "i=-; [ -n \"$b\" ] && i=$(\"$b\" version 2>/dev/null | cut -d\" \" -f2)",
        "echo \"SB:HELLO $(uname -s) $(uname -m) ${i:--}\"",
        "read c s h || exit 1",
        &format!(
            "if [ \"$c\" = INSTALL ]; then d=\"$r/{version}\"; {}; b=\"$d/overseer\"; \
             elif [ \"$c\" = RUN ]; then :; else exit 1; fi",
            receive_upload("$d/overseer")
        ),
        "[ -n \"$b\" ] || exit 1",
        &format!("f=--detach; [ \"$b\" = \"$r/{version}/overseer\" ] && f=\"{}\"", detach_flags(replace)),
        "\"$b\" daemon $f >&2 || { echo \"SB:ERROR the daemon did not start, see its daemon.log\"; exit 4; }",
        "exec \"$b\" proxy",
    ]
    .join("; ")
}

pub fn installed_command(version: &str, replace: bool) -> String {
    format!("exec sh -c '{}'", installed_script(version, replace))
}

pub fn ssh_args(r: &Resolved, dest_args: &[String]) -> Vec<String> {
    let mut a: Vec<String> = [
        "-T",
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "-o",
        "ClearAllForwardings=yes",
        "-o",
        "ForwardX11=no",
        "-o",
        "RequestTTY=no",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if r.server_alive_interval == 0 {
        a.extend(
            [
                "-o",
                "ServerAliveInterval=15",
                "-o",
                "ServerAliveCountMax=3",
            ]
            .map(String::from),
        );
    }
    if r.connect_timeout.is_none() {
        a.extend(["-o", "ConnectTimeout=20"].map(String::from));
    }
    a.extend(dest_args.iter().cloned());
    a
}

#[derive(Debug, PartialEq)]
pub enum Hello {
    Platform(Platform, Option<String>),
    Error(String),
}

pub fn parse_hello(line: &str) -> Option<Result<Hello, String>> {
    let line = line.trim();
    if let Some(e) = line.strip_prefix("SB:ERROR ") {
        return Some(Ok(Hello::Error(e.to_string())));
    }
    let rest = line.strip_prefix("SB:HELLO ")?;
    let mut it = rest.split_whitespace();
    let (os, arch, ver) = (
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
        it.next().unwrap_or("-"),
    );
    Some(
        Platform::from_uname(&format!("{os} {arch}"))
            .map(|p| Hello::Platform(p, (ver != "-").then(|| ver.to_string()))),
    )
}

pub fn ssh_error(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("Warning: Permanently added")
                && !l.starts_with("** ")
                && !l.contains("post-quantum")
        })
        .collect();
    let tail = &lines[lines.len().saturating_sub(6)..];
    if tail.is_empty() {
        "ssh exited without an error message".into()
    } else {
        tail.join("\n")
    }
}

pub fn hint(err: &str, macos: bool) -> &'static str {
    if macos && err.contains("No route to host") {
        return "\nIf `ssh` works in Terminal, macOS may be blocking local network access for Overseer: System Settings → Privacy & Security → Local Network.";
    }
    ""
}

pub fn permanent(err: &str) -> bool {
    [
        "Host key verification failed",
        "REMOTE HOST IDENTIFICATION HAS CHANGED",
        "cancelled",
        "unsupported remote platform",
        "no overseer",
        NOT_INSTALLED,
        "did not accept this device's key",
        "wrong password",
    ]
    .iter()
    .any(|p| err.contains(p))
}

async fn stderr_now(buf: Arc<Mutex<String>>) -> String {
    tokio::time::sleep(Duration::from_millis(150)).await;
    let text = buf.lock().unwrap().clone();
    ssh_error(&text)
}

async fn read_line_until<R: AsyncRead + Unpin>(
    r: &mut BufReader<R>,
    want: impl Fn(&str) -> bool,
) -> std::io::Result<Option<String>> {
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        if want(line.trim_end()) {
            return Ok(Some(line.trim_end().to_string()));
        }
    }
}

pub async fn connect(
    dest: &str,
    opts: &Options,
    on_prompt: Handler,
    on_step: &(dyn Fn(&'static str, Option<String>, &str) + Send + Sync),
) -> Result<Tunnel, String> {
    let (dargs, alias) = sshg::dest_args(dest);
    let (resolved, hops) = opts.ssh.resolve_with_hops(&dargs, &alias).await?;
    let mut described = sshg::describe(&resolved, &hops, &local_user());
    on_step("connecting", None, &described);
    let context = |d: &str, e: String| {
        format!(
            "{e}\n({d}, from ssh -G){}",
            hint(&e, cfg!(target_os = "macos"))
        )
    };
    let relay = if relay::wanted(
        cfg!(target_os = "macos"),
        &resolved,
        std::env::var(relay::ENV_MODE).ok().as_deref(),
    ) {
        let timeout = Duration::from_secs(resolved.connect_timeout.unwrap_or(20).max(1).into());
        let stream = relay::connect_tcp(
            &resolved.host_name,
            resolved.port,
            &resolved.address_family,
            timeout,
        )
        .await
        .map_err(|e| context(&described, e.to_string()))?;
        let r = relay::open(stream)
            .await
            .map_err(|e| context(&described, format!("relay: {e}")))?;
        described.push_str(&format!(", in-app connection to {}", r.peer));
        on_step("connecting", None, &described);
        Some(r)
    } else {
        None
    };
    let with_context = |e: String| context(&described, e);

    let registration = match &opts.askpass_program {
        Some(_) => Some(askpass::register(on_prompt).await?),
        None => None,
    };
    let mut cmd = opts.ssh.command();
    if let Some(r) = &relay {
        let exe = std::env::current_exe().map_err(|e| format!("relay: {e}"))?;
        cmd.arg("-o")
            .arg(format!(
                "ProxyCommand={}",
                relay::proxy_command(&exe, r.addr)
            ))
            .args(["-o", "ProxyUseFdpass=no"])
            .env(relay::ENV_TOKEN, &r.token);
    }
    cmd.args(ssh_args(&resolved, &dargs))
        .arg(remote_command(SERVER_VERSION, opts.replace))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let (Some(prog), Some(reg)) = (&opts.askpass_program, &registration) {
        cmd.env("SSH_ASKPASS", prog)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env(askpass::ENV_ADDR, &reg.endpoint)
            .env(askpass::ENV_TOKEN, &reg.token);
        if std::env::var_os("DISPLAY").is_none() {
            cmd.env("DISPLAY", "overseer:0");
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", opts.ssh.program.display()))?;
    let stdin = child.stdin.take().ok_or("ssh stdin")?;
    let stdout = BufReader::new(child.stdout.take().ok_or("ssh stdout")?);
    let stderr_buf = Arc::new(Mutex::new(String::new()));
    if let Some(mut err) = child.stderr.take() {
        let buf = stderr_buf.clone();
        tokio::spawn(async move {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = err.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                let mut b = buf.lock().unwrap();
                b.push_str(&String::from_utf8_lossy(&chunk[..n]));
                if b.len() > 16 * 1024 {
                    let cut = b.len() - 8 * 1024;
                    let cut = (cut..b.len()).find(|i| b.is_char_boundary(*i)).unwrap_or(0);
                    b.drain(..cut);
                }
            }
        });
    }
    let step = |phase: &'static str, detail: Option<String>| on_step(phase, detail, &described);
    let (mux, installed) = match bootstrap(
        stdout,
        stdin,
        Install::Upload(opts.binaries.as_ref()),
        &step,
    )
    .await
    {
        Ok(m) => m,
        Err(BootFail::Closed) => {
            let _ = child.wait().await;
            return Err(with_context(stderr_now(stderr_buf.clone()).await));
        }
        Err(BootFail::Remote(e)) => {
            let _ = child.start_kill();
            return Err(with_context(e));
        }
        Err(BootFail::Missing(e)) => {
            let _ = child.start_kill();
            return Err(e);
        }
        Err(BootFail::Io(e)) => {
            let _ = child.start_kill();
            return Err(format!("{e}\n{}", stderr_now(stderr_buf.clone()).await));
        }
    };
    let link = ProcessLink(Mutex::new(Some(child)));
    Ok(Tunnel {
        mux,
        installed,
        link: Box::new(link),
        _askpass: registration,
        _relay: relay,
    })
}

#[derive(Debug, PartialEq)]
pub enum BootFail {
    Closed,
    Remote(String),
    Missing(String),
    Io(String),
}

pub type FetchFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>>;
pub type Fetch = dyn Fn(Platform) -> FetchFuture + Send + Sync;

pub enum Install<'a> {
    Upload(&'a (dyn Fn(Platform) -> Vec<PathBuf> + Send + Sync)),
    RunInstalled,
    Fetch(&'a Fetch),
}

async fn upload<W: AsyncWrite + Unpin>(
    stdin: &mut W,
    bytes: &[u8],
    platform: Platform,
    on_step: &(dyn Fn(&'static str, Option<String>) + Send + Sync),
) -> Result<(), BootFail> {
    on_step(
        "installing",
        Some(format!(
            "{} ({:.1} MB)",
            platform.name(),
            bytes.len() as f64 / 1_048_576.0
        )),
    );
    let head = format!("INSTALL {} {}\n", bytes.len(), install::sha256_hex(bytes));
    let sent = async {
        stdin.write_all(head.as_bytes()).await?;
        stdin.write_all(bytes).await?;
        stdin.flush().await
    };
    sent.await
        .map_err(|e| BootFail::Io(format!("upload failed: {e}")))
}

async fn run<W: AsyncWrite + Unpin>(stdin: &mut W) -> Result<(), BootFail> {
    let sent = async {
        stdin.write_all(b"RUN\n").await?;
        stdin.flush().await
    };
    sent.await.map_err(|e| BootFail::Io(format!("ssh: {e}")))
}

pub const NOT_INSTALLED: &str = "the overseer server is not installed on this host";

pub async fn bootstrap<R, W>(
    mut stdout: BufReader<R>,
    mut stdin: W,
    mode: Install<'_>,
    on_step: &(dyn Fn(&'static str, Option<String>) + Send + Sync),
) -> Result<(Arc<Mux>, Option<String>), BootFail>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let hello = match read_line_until(&mut stdout, |l| {
        l.starts_with("SB:HELLO ") || l.starts_with("SB:ERROR ")
    })
    .await
    {
        Ok(Some(l)) => parse_hello(&l).unwrap_or_else(|| Err(l.clone())),
        _ => return Err(BootFail::Closed),
    };
    let (platform, installed) = match hello {
        Ok(Hello::Platform(p, v)) => (p, v),
        Ok(Hello::Error(e)) | Err(e) => return Err(BootFail::Remote(e)),
    };
    on_step("checking", installed.clone());
    let mut installed = installed;
    match mode {
        Install::RunInstalled if installed.is_none() => {
            return Err(BootFail::Missing(format!(
                "{NOT_INSTALLED} ({}). Connect to it once from the Overseer desktop app, which installs the server, then try again.",
                platform.name()
            )));
        }
        Install::Upload(binaries)
            if install::needs_install(installed.as_deref(), SERVER_VERSION) =>
        {
            let Some(bin) = binaries(platform).into_iter().find(|p| p.is_file()) else {
                return Err(BootFail::Missing(format!(
                    "no overseer {SERVER_VERSION} binary for {} ({}); put it in the server binaries folder as {}/overseer",
                    platform.name(),
                    platform.target_triple(),
                    platform.name()
                )));
            };
            let bytes = tokio::fs::read(&bin)
                .await
                .map_err(|e| BootFail::Missing(format!("{}: {e}", bin.display())))?;
            upload(&mut stdin, &bytes, platform, on_step).await?;
            installed = Some(SERVER_VERSION.to_string());
        }
        Install::Fetch(fetch) if install::wants_fetch(installed.as_deref(), SERVER_VERSION) => {
            on_step("downloading", Some(platform.name().replacen('-', "/", 1)));
            match fetch(platform).await {
                Ok(bytes) => {
                    upload(&mut stdin, &bytes, platform, on_step).await?;
                    installed = Some(SERVER_VERSION.to_string());
                }
                Err(_) if installed.is_some() => run(&mut stdin).await?,
                Err(e) => {
                    return Err(BootFail::Missing(format!(
                        "{NOT_INSTALLED} ({}): {e}",
                        platform.name()
                    )))
                }
            }
        }
        _ => run(&mut stdin).await?,
    }
    on_step("starting", None);
    match read_line_until(&mut stdout, |l| {
        l.starts_with(BANNER) || l.starts_with("SB:ERROR ")
    })
    .await
    {
        Ok(Some(l)) if l.starts_with(BANNER) => {}
        Ok(Some(l)) => return Err(BootFail::Io(l.trim_start_matches("SB:ERROR ").to_string())),
        _ => return Err(BootFail::Closed),
    }
    Ok((Mux::start(stdout, stdin, PING), installed))
}

pub fn watch_exit(t: &Arc<Tunnel>) {
    let weak = Arc::downgrade(t);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let Some(t) = weak.upgrade() else { return };
            if t.mux.is_dead() {
                t.link.kill();
                return;
            }
            if t.link.exited() {
                t.mux.kill("the ssh connection ended");
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_is_one_line_without_single_quotes_or_bangs() {
        let s = bootstrap_script("0.2.0", false);
        assert!(
            !s.contains('\'') && !s.contains('!') && !s.contains('\n'),
            "{s}"
        );
        assert!(
            s.contains("daemon --detach >&2") && !s.contains("--replace"),
            "{s}"
        );
        let r = bootstrap_script("0.2.0", true);
        assert!(
            !r.contains('\'') && !r.contains('!') && r.contains("daemon --detach --replace >&2"),
            "{r}"
        );
        assert!(s.starts_with("v=0.2.0; d=\"$HOME/.overseer/server/$v\""));
        let order = [
            "SB:HELLO",
            "read c s h",
            "head -c",
            "sha256sum",
            "mv -f",
            "daemon --detach",
            "exec \"$b\" proxy",
        ];
        let pos: Vec<usize> = order
            .iter()
            .map(|p| s.find(p).unwrap_or_else(|| panic!("{p}")))
            .collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
        let c = remote_command("1.0.0", true);
        assert!(c.starts_with("exec sh -c 'v=1.0.0; d=") && c.ends_with("exec \"$b\" proxy'"));
    }

    #[test]
    #[should_panic(expected = "assertion failed: version")]
    fn bootstrap_rejects_odd_versions() {
        bootstrap_script("1.0; rm -rf ~", false);
    }

    #[test]
    fn bootstrap_runs_under_sh() {
        let home = std::env::temp_dir().join(format!("sb-boot-{}", std::process::id()));
        let bin = home.join(".overseer/server/9.9.9");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = "#!/bin/sh\ncase \"$1\" in version) echo \"overseer 9.9.9 (x)\";; daemon) echo started >&2;; proxy) echo \"SB:MUX 1\";; esac\n";
        std::fs::write(bin.join("overseer"), fake).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("overseer"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let mut c = std::process::Command::new("sh")
            .arg("-c")
            .arg(remote_command("9.9.9", false))
            .env("HOME", &home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        c.stdin.take().unwrap().write_all(b"RUN\n").unwrap();
        let out = c.wait_with_output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert_eq!(
            parse_hello(lines[0]).unwrap().unwrap(),
            Hello::Platform(
                Platform::from_uname(&format!("{} {}", uname("-s"), uname("-m"))).unwrap(),
                Some("9.9.9".into())
            )
        );
        assert_eq!(lines[1], BANNER);
        let _ = std::fs::remove_dir_all(home);
    }

    fn run_installed(home: &std::path::Path, version: &str, replace: bool) -> Vec<String> {
        run_installed_with(home, version, replace, b"RUN\n")
    }

    fn run_installed_with(
        home: &std::path::Path,
        version: &str,
        replace: bool,
        input: &[u8],
    ) -> Vec<String> {
        let mut c = std::process::Command::new("sh")
            .arg("-c")
            .arg(installed_command(version, replace))
            .env("HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        c.stdin.take().unwrap().write_all(input).unwrap();
        let out = c.wait_with_output().unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn installed_script_installs_an_uploaded_server() {
        let s = installed_script("0.2.0", true);
        assert!(
            !s.contains('\'') && !s.contains('!') && !s.contains('\n'),
            "{s}"
        );
        let order = [
            "SB:HELLO",
            "read c s h",
            "INSTALL",
            "head -c",
            "sha256sum",
            "mv -f",
            "daemon $f",
            "exec \"$b\" proxy",
        ];
        let pos: Vec<usize> = order
            .iter()
            .map(|p| s.find(p).unwrap_or_else(|| panic!("{p}")))
            .collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
        let home = std::env::temp_dir().join(format!("sb-installed-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let old = home.join(".overseer/server/9.9.8");
        std::fs::create_dir_all(&old).unwrap();
        let stub = |v: &str| {
            format!("#!/bin/sh\ncase \"$1\" in version) echo \"overseer {v} (x)\";; daemon) echo \"$0 $*\" > \"$HOME/flags\";; proxy) echo \"SB:MUX 1\";; esac\n")
        };
        std::fs::write(old.join("overseer"), stub("9.9.8")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(old.join("overseer"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let body = stub("9.9.9");
        let mut input = format!(
            "INSTALL {} {}\n",
            body.len(),
            install::sha256_hex(body.as_bytes())
        )
        .into_bytes();
        input.extend_from_slice(body.as_bytes());
        let lines = run_installed_with(&home, "9.9.9", true, &input);
        assert!(
            matches!(parse_hello(&lines[0]), Some(Ok(Hello::Platform(_, Some(v)))) if v == "9.9.8"),
            "{lines:?}"
        );
        assert_eq!(lines.get(1).map(String::as_str), Some(BANNER), "{lines:?}");
        let installed = home.join(".overseer/server/9.9.9/overseer");
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), body);
        let flags = std::fs::read_to_string(home.join("flags")).unwrap();
        assert_eq!(
            flags.trim(),
            format!("{} daemon --detach --replace", installed.display())
        );

        let mut bad = format!("INSTALL {} {}\n", body.len(), "0".repeat(64)).into_bytes();
        bad.extend_from_slice(body.as_bytes());
        let lines = run_installed_with(&home, "9.9.7", false, &bad);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("SB:ERROR checksum mismatch")),
            "{lines:?}"
        );
        assert!(!home.join(".overseer/server/9.9.7/overseer").exists());
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn installed_script_runs_any_installed_server() {
        let home = std::env::temp_dir().join(format!("sb-installed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let lines = run_installed(&home, "9.9.9", false);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].ends_with(" -"), "{lines:?}");
        let bin = home.join(".overseer/server/9.9.8");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = |v: &str| {
            format!("#!/bin/sh\ncase \"$1\" in version) echo \"overseer {v} (x)\";; daemon) echo \"$*\" > \"$HOME/flags\";; proxy) echo \"SB:MUX 1\";; esac\n")
        };
        let install = |dir: &std::path::Path, v: &str| {
            std::fs::write(dir.join("overseer"), fake(v)).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    dir.join("overseer"),
                    std::fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            }
        };
        install(&bin, "9.9.8");
        let flags = || {
            std::fs::read_to_string(home.join("flags"))
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let lines = run_installed(&home, "9.9.9", true);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(
            matches!(parse_hello(&lines[0]), Some(Ok(Hello::Platform(_, Some(v)))) if v == "9.9.8"),
            "{lines:?}"
        );
        assert_eq!(lines[1], BANNER);
        assert_eq!(flags(), "daemon --detach");
        let wanted = home.join(".overseer/server/9.9.9");
        std::fs::create_dir_all(&wanted).unwrap();
        install(&wanted, "9.9.9");
        let lines = run_installed(&home, "9.9.9", true);
        assert!(
            matches!(parse_hello(&lines[0]), Some(Ok(Hello::Platform(_, Some(v)))) if v == "9.9.9"),
            "{lines:?}"
        );
        assert_eq!(flags(), "daemon --detach --replace");
        run_installed(&home, "9.9.9", false);
        assert_eq!(flags(), "daemon --detach");
        let _ = std::fs::remove_dir_all(home);
    }

    #[tokio::test]
    async fn bootstrap_without_upload_refuses_a_host_without_server() {
        let (app, server) = tokio::io::duplex(4096);
        let (ar, aw) = tokio::io::split(app);
        let (sr, mut sw) = tokio::io::split(server);
        sw.write_all(b"motd\nSB:HELLO Linux x86_64 -\n")
            .await
            .unwrap();
        let err = bootstrap(BufReader::new(ar), aw, Install::RunInstalled, &|_, _| {})
            .await
            .err()
            .unwrap();
        drop(sr);
        match err {
            BootFail::Missing(m) => {
                assert!(m.contains(NOT_INSTALLED) && m.contains("desktop"), "{m}");
                assert!(permanent(&m));
            }
            e => panic!("{e:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_without_upload_runs_the_installed_server() {
        let (app, server) = tokio::io::duplex(4096);
        let (ar, aw) = tokio::io::split(app);
        let (sr, mut sw) = tokio::io::split(server);
        let remote = tokio::spawn(async move {
            sw.write_all(b"SB:HELLO Linux aarch64 0.0.1\n")
                .await
                .unwrap();
            let mut r = BufReader::new(sr);
            let mut line = String::new();
            r.read_line(&mut line).await.unwrap();
            sw.write_all(format!("{BANNER}\n").as_bytes())
                .await
                .unwrap();
            (line, r, sw)
        });
        let steps = Mutex::new(Vec::new());
        let (mux, installed) = bootstrap(BufReader::new(ar), aw, Install::RunInstalled, &|p, _| {
            steps.lock().unwrap().push(p)
        })
        .await
        .unwrap();
        assert_eq!(installed.as_deref(), Some("0.0.1"));
        let (line, _r, _w) = remote.await.unwrap();
        assert_eq!(line, "RUN\n");
        assert_eq!(*steps.lock().unwrap(), vec!["checking", "starting"]);
        assert!(!mux.is_dead());
    }

    async fn fetch_boot(
        hello: &'static str,
        fetch: &Fetch,
    ) -> (Result<Option<String>, BootFail>, String, Vec<&'static str>) {
        let (app, server) = tokio::io::duplex(1 << 16);
        let (ar, aw) = tokio::io::split(app);
        let (sr, mut sw) = tokio::io::split(server);
        let remote = tokio::spawn(async move {
            sw.write_all(hello.as_bytes()).await.unwrap();
            let mut r = BufReader::new(sr);
            let mut line = String::new();
            if r.read_line(&mut line).await.unwrap_or(0) == 0 {
                return (line, r, sw);
            }
            if let Some(n) = line
                .strip_prefix("INSTALL ")
                .and_then(|h| h.split_whitespace().next())
                .and_then(|n| n.parse::<usize>().ok())
            {
                let mut body = vec![0u8; n];
                r.read_exact(&mut body).await.unwrap();
                line.push_str(&String::from_utf8_lossy(&body));
            }
            sw.write_all(format!("{BANNER}\n").as_bytes())
                .await
                .unwrap();
            (line, r, sw)
        });
        let steps = Mutex::new(Vec::new());
        let res = bootstrap(BufReader::new(ar), aw, Install::Fetch(fetch), &|p, _| {
            steps.lock().unwrap().push(p)
        })
        .await
        .map(|(_, installed)| installed);
        let (line, _r, _w) = remote.await.unwrap();
        let steps = steps.lock().unwrap().clone();
        (res, line, steps)
    }

    #[tokio::test]
    async fn bootstrap_fetches_and_uploads_a_missing_server() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let fetch: Box<Fetch> = Box::new(move |p: Platform| -> FetchFuture {
            seen.lock().unwrap().push(p);
            Box::pin(async { Ok(b"stub".to_vec()) })
        });
        let (res, line, steps) = fetch_boot("SB:HELLO Linux x86_64 -\n", fetch.as_ref()).await;
        assert_eq!(res.unwrap().as_deref(), Some(SERVER_VERSION));
        assert_eq!(
            line,
            "INSTALL 4 725c546b990dd1b41f3d5791b37c3c0edcb1f08cf150bdae32a73dfd166e02d7\nstub"
        );
        assert_eq!(
            steps,
            vec!["checking", "downloading", "installing", "starting"]
        );
        assert_eq!(*calls.lock().unwrap(), vec![Platform::LinuxX86_64]);

        let (res, line, steps) = fetch_boot("SB:HELLO Linux aarch64 0.0.1\n", fetch.as_ref()).await;
        assert_eq!(res.unwrap().as_deref(), Some(SERVER_VERSION));
        assert!(line.starts_with("INSTALL 4 "), "{line}");
        assert_eq!(
            steps,
            vec!["checking", "downloading", "installing", "starting"]
        );

        let newer =
            Box::leak(format!("SB:HELLO Linux aarch64 {}\n", SERVER_VERSION).into_boxed_str());
        let (res, line, steps) = fetch_boot(newer, fetch.as_ref()).await;
        assert_eq!(res.unwrap().as_deref(), Some(SERVER_VERSION));
        assert_eq!(line, "RUN\n");
        assert_eq!(steps, vec!["checking", "starting"]);
        assert_eq!(calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn bootstrap_falls_back_or_explains_when_the_download_fails() {
        let fetch: Box<Fetch> = Box::new(|_: Platform| -> FetchFuture {
            Box::pin(async { Err("cannot download the server from GitHub".to_string()) })
        });
        let (res, line, _) = fetch_boot("SB:HELLO Linux x86_64 0.0.1\n", fetch.as_ref()).await;
        assert_eq!(res.unwrap().as_deref(), Some("0.0.1"));
        assert_eq!(line, "RUN\n");

        let (res, _, _) = fetch_boot("SB:HELLO Darwin arm64 -\n", fetch.as_ref()).await;
        match res {
            Err(BootFail::Missing(m)) => {
                assert!(
                    m.contains(NOT_INSTALLED)
                        && m.contains("darwin-aarch64")
                        && m.contains("cannot download the server"),
                    "{m}"
                );
                assert!(permanent(&m));
            }
            r => panic!("{r:?}"),
        }
    }

    fn uname(flag: &str) -> String {
        String::from_utf8(
            std::process::Command::new("uname")
                .arg(flag)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    #[test]
    fn hello_lines() {
        assert_eq!(
            parse_hello("SB:HELLO Linux x86_64 -").unwrap().unwrap(),
            Hello::Platform(Platform::LinuxX86_64, None)
        );
        assert_eq!(
            parse_hello("SB:HELLO Darwin arm64 0.2.0").unwrap().unwrap(),
            Hello::Platform(Platform::DarwinAarch64, Some("0.2.0".into()))
        );
        assert!(parse_hello("SB:HELLO FreeBSD amd64 -").unwrap().is_err());
        assert_eq!(
            parse_hello("SB:ERROR boom").unwrap().unwrap(),
            Hello::Error("boom".into())
        );
        assert!(parse_hello("motd line").is_none());
    }

    #[test]
    fn args_honor_the_users_keepalive() {
        let base = sshg::parse("h", "hostname h\nport 22\nserveraliveinterval 0\n");
        let a = ssh_args(&base, &["h".into()]);
        assert!(a.windows(2).any(|w| w == ["-o", "ServerAliveInterval=15"]));
        assert!(a.windows(2).any(|w| w == ["-o", "ControlMaster=no"]));
        assert!(!a.iter().any(|x| x.contains("StrictHostKeyChecking")));
        assert_eq!(a.last().unwrap(), "h");
        let mine = sshg::parse("h", "serveraliveinterval 60\nconnecttimeout 5\n");
        let a = ssh_args(&mine, &["h".into()]);
        assert!(!a
            .iter()
            .any(|x| x.starts_with("ServerAlive") || x.starts_with("ConnectTimeout")));
    }

    #[test]
    fn errors_keep_the_cause_and_drop_noise() {
        let e = ssh_error("Warning: Permanently added 'x' (ED25519) to the list of known hosts.\nssh: connect to host workstation port 22: No route to host\n");
        assert_eq!(
            e,
            "ssh: connect to host workstation port 22: No route to host"
        );
        assert!(permanent("Host key verification failed."));
        assert!(!permanent(
            "ssh: connect to host workstation port 22: No route to host"
        ));
        assert!(!permanent("Permission denied (publickey)."));
        assert!(
            hint("ssh: connect to host w port 22: No route to host", true)
                .contains("Local Network")
        );
        assert_eq!(
            hint("ssh: connect to host w port 22: No route to host", false),
            ""
        );
    }
}
