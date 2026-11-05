use crate::conn::{ConnManager, ConnState, Ctx, Target};
use crate::forward::{PortSettings, Ports};
use crate::http::json_body;
use crate::install::{Platform, SERVER_VERSION};
use crate::sshg::SshCmd;
use crate::transport::DaemonConn;
use crate::ws::{self, TermInput};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

fn daemon_binary() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OVERSEER_BIN") {
        return Some(PathBuf::from(p));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    ["target/debug/overseer", "target/release/overseer"]
        .iter()
        .map(|p| repo.join(p))
        .find(|p| p.is_file())
}

async fn call(conn: &DaemonConn, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let raw = body.map(|b| b.to_string().into_bytes());
    let res = conn
        .request(method, path, raw.as_deref())
        .await
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
    (res.head.status, json_body(&res.body))
}

async fn until<T>(what: &str, secs: u64, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = f().await {
            return v;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

struct Attached {
    input: mpsc::Sender<TermInput>,
    output: Arc<Mutex<String>>,
}

async fn attach(conn: &DaemonConn, id: &str) -> Attached {
    let io = conn.open().await.unwrap();
    let socket = ws::connect(io, &format!("/v1/terms/{id}/io"))
        .await
        .unwrap();
    let (input, rx) = mpsc::channel(8);
    let output = Arc::new(Mutex::new(String::new()));
    let out = output.clone();
    tokio::spawn(ws::pump(socket, rx, move |b| {
        out.lock().unwrap().push_str(&String::from_utf8_lossy(&b))
    }));
    Attached { input, output }
}

impl Attached {
    async fn wait(&self, needle: &str) {
        let out = self.output.clone();
        until(needle, 20, async || {
            out.lock().unwrap().contains(needle).then_some(())
        })
        .await;
    }
}

async fn http_status(port: u16) -> Option<String> {
    let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .ok()?;
    s.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .await
        .ok()?;
    let mut buf = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), s.read_to_end(&mut buf))
        .await
        .ok()?
        .ok()?;
    String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .map(String::from)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub struct Scenario<'a> {
    pub ctx: Ctx,
    pub host: &'a str,
    pub expect_install: bool,
    pub http_port: u16,
    pub while_busy: &'a (dyn Fn() + Sync),
    pub drop_link: &'a (dyn Fn(&DaemonConn) + Sync),
}

pub async fn run(s: Scenario<'_>) {
    let mgr = Arc::new(ConnManager::default());
    let ports = Ports::new(mgr.clone());
    ports.ensure_tracking();
    let states: Arc<Mutex<Vec<ConnState>>> = Arc::default();
    let st = states.clone();
    let target = Target {
        kind: "ssh".into(),
        host: Some(s.host.into()),
        label: s.host.into(),
    };
    mgr.connect(
        target,
        Arc::new(move |c| st.lock().unwrap().push(c)),
        s.ctx.clone(),
    )
    .await
    .unwrap_or_else(|e| panic!("connect {}: {e}", s.host));
    ports.configure(PortSettings::default()).await;
    {
        let seen = states.lock().unwrap();
        let phases: Vec<&str> = seen.iter().map(|c| c.phase).collect();
        assert_eq!(
            phases.contains(&"installing"),
            s.expect_install,
            "{phases:?}"
        );
        assert_eq!(phases.last(), Some(&"connected"));
        let resolved = seen.last().unwrap().resolved.clone().unwrap_or_default();
        assert!(
            resolved.starts_with(&format!("{} → ", s.host)),
            "{resolved}"
        );
    }
    let conn = mgr.current().unwrap();
    let (status, v) = call(&conn, "GET", "/v1/version", None).await;
    assert_eq!((status, v["version"].as_str()), (200, Some(SERVER_VERSION)));

    let mut ids = Vec::new();
    let mut attached = Vec::new();
    for i in 0..3 {
        let cmd = json!([
            "sh",
            "-c",
            format!("echo sb-term-{i}; while read l; do echo got-{i}:$l; done")
        ]);
        let (status, term) = call(
            &conn,
            "POST",
            "/v1/terms",
            Some(json!({"cwd": "/tmp", "cmd": cmd})),
        )
        .await;
        assert_eq!(status, 201, "{term}");
        let id = term["id"].as_str().unwrap().to_string();
        let a = attach(&conn, &id).await;
        a.wait(&format!("sb-term-{i}")).await;
        ids.push(id);
        attached.push(a);
    }
    for (i, a) in attached.iter().enumerate() {
        a.input
            .send(TermInput::Data(format!("ping{i}\r")))
            .await
            .unwrap();
        a.wait(&format!("got-{i}:ping{i}")).await;
    }
    let (status, terms) = call(&conn, "GET", "/v1/terms", None).await;
    assert_eq!(status, 200);
    assert!(terms.as_array().unwrap().len() >= 3);
    (s.while_busy)();

    let port = s.http_port;
    let cmd = json!([
        "python3",
        "-m",
        "http.server",
        port.to_string(),
        "--bind",
        "127.0.0.1"
    ]);
    let (status, web) = call(
        &conn,
        "POST",
        "/v1/terms",
        Some(json!({"cwd": "/tmp", "cmd": cmd})),
    )
    .await;
    assert_eq!(status, 201, "{web}");
    let web_id = web["id"].as_str().unwrap().to_string();
    let fwd = until("port.opened → auto-forward", 30, async || {
        ports
            .snapshot()
            .forwards
            .into_iter()
            .find(|f| f.remote_port == port)
    })
    .await;
    assert_eq!(
        (fwd.source, fwd.process.as_deref(), fwd.term_id.as_deref()),
        ("auto", Some("python3"), Some(web_id.as_str()))
    );
    let local = fwd.local_port;
    let line = until("HTTP through the forward", 20, async || {
        http_status(local).await.filter(|l| l.contains("200"))
    })
    .await;
    assert!(line.starts_with("HTTP/1.0 200"), "{line}");

    let before = states.lock().unwrap().len();
    drop(attached);
    (s.drop_link)(&conn);
    drop(conn);
    until("reconnect", 90, async || {
        let seen = states.lock().unwrap();
        let after: Vec<&str> = seen[before..].iter().map(|c| c.phase).collect();
        (after.contains(&"reconnecting") && after.last() == Some(&"connected")).then_some(())
    })
    .await;
    let conn = mgr.current().unwrap();
    let again = attach(&conn, &ids[1]).await;
    again.wait("sb-term-1").await;
    again.wait("got-1:ping1").await;
    again
        .input
        .send(TermInput::Data("after\r".into()))
        .await
        .unwrap();
    again.wait("got-1:after").await;
    let line = until("HTTP after reconnect", 30, async || {
        http_status(local).await.filter(|l| l.contains("200"))
    })
    .await;
    assert!(line.contains("200"));

    for id in ids.iter().chain([&web_id]) {
        let _ = call(
            &conn,
            "POST",
            &format!("/v1/terms/{id}/kill"),
            Some(json!({"confirm": true})),
        )
        .await;
    }
    ports.reset();
    mgr.disconnect();
}

fn write_exec(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn server_dir(root: &Path, bin: &Path) -> PathBuf {
    let platform = Platform::local().expect("supported platform");
    let dir = root.join("bins");
    std::fs::create_dir_all(dir.join(platform.name())).unwrap();
    std::fs::copy(bin, dir.join(platform.name()).join("overseer")).unwrap();
    dir
}

#[tokio::test]
async fn one_tunnel_through_a_local_fake_ssh() {
    let Some(bin) = daemon_binary() else {
        eprintln!("skipped: build the daemon (cargo build at the repo root) or set OVERSEER_BIN");
        return;
    };
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("skipped: no python3");
        return;
    }
    let root = std::env::temp_dir().join(format!("sbrt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).unwrap();
    let fake = root.join("ssh");
    write_exec(
        &fake,
        &format!(
            "#!/bin/sh\nexport HOME={r}/home OVERSEER_STATE_DIR={r}/state OVERSEER_SOCKET={r}/d.sock XDG_STATE_HOME={r}/home/.local/state SHELL=/bin/sh TMUX_TMPDIR={r}/tmux\n\
for a; do last=$a; done\ncase \" $* \" in *\" -G \"*) printf 'hostname fake.internal\\nport 22\\nuser tester\\n'; exit 0;; esac\nexec sh -c \"$last\"\n",
            r = root.display()
        ),
    );
    let ctx = Ctx {
        bin_dir: Some(server_dir(&root, &bin)),
        ssh: SshCmd {
            program: fake,
            config: None,
        },
        ..Default::default()
    };
    let kill = |c: &DaemonConn| {
        if let DaemonConn::Tunnel(t) = c {
            t.kill_link();
        }
    };
    run(Scenario {
        ctx,
        host: "fakehost",
        expect_install: true,
        http_port: free_port(),
        while_busy: &|| {},
        drop_link: &kill,
    })
    .await;
    if let Ok(pid) = std::fs::read_to_string(root.join("state/daemon.pid")) {
        let _ = Command::new("kill").arg(pid.trim()).status();
    }
    let _ = std::fs::remove_dir_all(&root);
}

mod docker {
    use super::*;

    const ASKPASS: &str = "#!/usr/bin/env python3\nimport os, socket, sys\n\
s = socket.socket(socket.AF_UNIX)\ns.connect(os.environ['OVERSEER_ASKPASS'])\n\
s.sendall((os.environ.get('OVERSEER_ASKPASS_TOKEN', '') + '\\n' + os.environ.get('SSH_ASKPASS_PROMPT', '') + '\\n' + (sys.argv[1] if len(sys.argv) > 1 else '')).encode())\n\
s.shutdown(socket.SHUT_WR)\nr = b''.join(iter(lambda: s.recv(4096), b'')).decode()\n\
sys.exit(1) if not r.startswith('OK\\n') else print(r[3:])\n";

    const LABEL: &str = "overseer-remote-it";
    const IMAGE: &str = "overseer-remote-it:sshd";

    struct Fixture {
        net: String,
        names: Vec<String>,
        dir: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for n in &self.names {
                let _ = Command::new("docker").args(["rm", "-f", n]).output();
            }
            let _ = Command::new("docker")
                .args(["network", "rm", &self.net])
                .output();
            if std::env::var_os("OVERSEER_DOCKER_KEEP_IMAGE").is_none() {
                let _ = Command::new("docker").args(["rmi", "-f", IMAGE]).output();
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn docker(args: &[&str]) -> String {
        let out = Command::new("docker").args(args).output().expect("docker");
        assert!(
            out.status.success(),
            "docker {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    const DOCKERFILE: &str = "FROM debian:trixie-slim\n\
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server netcat-openbsd python3 iproute2 procps && rm -rf /var/lib/apt/lists/*\n\
RUN useradd -m -s /bin/bash tester && mkdir -p /run/sshd /home/tester/.ssh && chmod 700 /home/tester/.ssh\n\
RUN printf 'PasswordAuthentication no\\nAllowTcpForwarding yes\\nPermitTunnel no\\nX11Forwarding no\\n' > /etc/ssh/sshd_config.d/test.conf\n\
COPY key.pub /home/tester/.ssh/authorized_keys\n\
RUN chown -R tester:tester /home/tester/.ssh && chmod 600 /home/tester/.ssh/authorized_keys\n\
CMD [\"/usr/sbin/sshd\", \"-D\", \"-e\"]\n";

    fn host_port(name: &str) -> u16 {
        let out = docker(&["port", name, "22/tcp"]);
        out.lines()
            .find_map(|l| l.rsplit(':').next()?.parse().ok())
            .expect("published port")
    }

    fn established(name: &str) -> usize {
        let out = docker(&[
            "exec",
            name,
            "ss",
            "-Htn",
            "state",
            "established",
            "( sport = :22 )",
        ]);
        out.lines().filter(|l| !l.trim().is_empty()).count()
    }

    #[tokio::test]
    async fn real_sshd_in_docker() {
        if std::env::var("OVERSEER_DOCKER_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped: set OVERSEER_DOCKER_TESTS=1");
            return;
        }
        let bin = std::env::var_os("OVERSEER_REMOTE_BIN")
            .map(PathBuf::from)
            .expect("OVERSEER_REMOTE_BIN: a static linux-x86_64 overseer (make dist)");
        let tag = format!(
            "{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos() % 100_000
        );
        let dir = std::env::temp_dir().join(format!("sbdk-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut fx = Fixture {
            net: format!("sbit-net-{tag}"),
            names: vec![],
            dir: dir.clone(),
        };
        let key = dir.join("key");
        assert!(Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "sbit", "-f"])
            .arg(&key)
            .status()
            .unwrap()
            .success());
        let build = dir.join("build");
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("Dockerfile"), DOCKERFILE).unwrap();
        std::fs::copy(dir.join("key.pub"), build.join("key.pub")).unwrap();
        docker(&[
            "build",
            "-q",
            "--label",
            LABEL,
            "-t",
            IMAGE,
            build.to_str().unwrap(),
        ]);
        docker(&["network", "create", "--label", LABEL, &fx.net]);
        let target = format!("sbit-target-{tag}");
        let bastion = format!("sbit-bastion-{tag}");
        for (name, publish) in [(&target, true), (&bastion, true)] {
            let mut args = vec![
                "run",
                "-d",
                "--label",
                LABEL,
                "--name",
                name.as_str(),
                "--network",
                fx.net.as_str(),
                "--network-alias",
                name.as_str(),
            ];
            if publish {
                args.extend(["-p", "127.0.0.1::22"]);
            }
            args.push(IMAGE);
            docker(&args);
            fx.names.push(name.clone());
        }
        let (tport, bport) = (host_port(&target), host_port(&bastion));
        let known = dir.join("known_hosts");
        let cfg = dir.join("config");
        let k = key.display();
        let kh = known.display();
        let text = format!(
            "Host test-ask\n  HostName 127.0.0.1\n  Port {tport}\n  UserKnownHostsFile {kh}.ask\n  StrictHostKeyChecking ask\n\n\
Host test-*\n  User tester\n  IdentityFile {k}\n  IdentitiesOnly yes\n  IdentityAgent none\n  UserKnownHostsFile {kh}\n  StrictHostKeyChecking accept-new\n  LogLevel ERROR\n\n\
Match host test-match\n  HostName 127.0.0.1\n  Port {tport}\n\n\
Host test-match\n  HostName nowhere.invalid\n\n\
Host test-direct\n  HostName 127.0.0.1\n  Port {tport}\n\n\
Host test-bastion\n  HostName 127.0.0.1\n  Port {bport}\n\n\
Host test-jump\n  HostName {target}\n  Port 22\n  ProxyJump test-bastion\n\n\
Host test-cmd\n  HostName 127.0.0.1\n  Port {tport}\n  ProxyCommand nc %h %p\n"
        );
        std::fs::write(&cfg, text).unwrap();
        let bins = dir.join("bins/linux-x86_64");
        std::fs::create_dir_all(&bins).unwrap();
        std::fs::copy(&bin, bins.join("overseer")).unwrap();
        let ctx = Ctx {
            bin_dir: Some(dir.join("bins")),
            ssh: SshCmd {
                program: "ssh".into(),
                config: Some(cfg.clone()),
            },
            ..Default::default()
        };
        until("sshd up", 30, async || {
            let ok = Command::new("ssh")
                .arg("-F")
                .arg(&cfg)
                .args(["-o", "BatchMode=yes", "test-direct", "true"])
                .output()
                .ok()?
                .status
                .success();
            ok.then_some(())
        })
        .await;

        let t2 = target.clone();
        let busy = move || {
            let n = established(&t2);
            assert_eq!(
                n, 1,
                "one TCP connection to sshd while 3 terminals, SSE, ports and API calls are live"
            );
        };
        let t3 = target.clone();
        let drop_link = move |_: &DaemonConn| {
            let out = Command::new("docker")
                .args([
                    "exec",
                    &t3,
                    "pkill",
                    "-f",
                    "sshd-session: tester|sshd: tester",
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "pkill: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(Scenario {
            ctx: ctx.clone(),
            host: "test-match",
            expect_install: true,
            http_port: 8765,
            while_busy: &busy,
            drop_link: &drop_link,
        })
        .await;

        for host in ["test-jump", "test-cmd"] {
            let mgr = Arc::new(ConnManager::default());
            let states: Arc<Mutex<Vec<ConnState>>> = Arc::default();
            let st = states.clone();
            let t = Target {
                kind: "ssh".into(),
                host: Some(host.into()),
                label: host.into(),
            };
            mgr.connect(
                t,
                Arc::new(move |c| st.lock().unwrap().push(c)),
                ctx.clone(),
            )
            .await
            .unwrap_or_else(|e| panic!("{host}: {e}"));
            let resolved = states
                .lock()
                .unwrap()
                .last()
                .unwrap()
                .resolved
                .clone()
                .unwrap();
            let via = if host == "test-jump" {
                "via ProxyJump test-bastion"
            } else {
                "via ProxyCommand `nc 127.0.0.1"
            };
            assert!(resolved.contains(via), "{resolved}");
            let conn = mgr.current().unwrap();
            assert_eq!(call(&conn, "GET", "/v1/version", None).await.0, 200);
            mgr.disconnect();
        }

        let askpass = dir.join("askpass");
        write_exec(&askpass, ASKPASS);
        let ask_ctx = Ctx {
            askpass: Some(askpass),
            ..ctx.clone()
        };
        let ask_known = PathBuf::from(format!("{}.ask", known.display()));
        assert!(!ask_known.exists());
        for (answer, ok) in [(None, false), (Some("yes"), true)] {
            let mgr = Arc::new(ConnManager::default());
            let prompts: Arc<Mutex<Vec<crate::askpass::Prompt>>> = Arc::default();
            let seen = prompts.clone();
            let t = Target {
                kind: "ssh".into(),
                host: Some("test-ask".into()),
                label: "ask".into(),
            };
            let emit = Arc::new(move |c: ConnState| {
                if let Some(p) = c.prompt {
                    seen.lock().unwrap().push(p.clone());
                    crate::askpass::answer(p.id, answer.map(String::from));
                }
            });
            let res = mgr.connect(t, emit, ask_ctx.clone()).await;
            let p = prompts
                .lock()
                .unwrap()
                .first()
                .cloned()
                .expect("a host key prompt");
            assert_eq!(p.kind, "hostkey");
            assert!(
                p.fingerprint
                    .as_deref()
                    .is_some_and(|f| f.starts_with("SHA256:")),
                "{p:?}"
            );
            match ok {
                false => {
                    let err = res.unwrap_err();
                    assert!(err.contains("Host key verification failed"), "{err}");
                    assert!(
                        !ask_known.exists()
                            || std::fs::read_to_string(&ask_known)
                                .unwrap()
                                .trim()
                                .is_empty()
                    );
                }
                true => {
                    res.unwrap();
                    assert!(std::fs::read_to_string(&ask_known)
                        .unwrap()
                        .contains("ssh-ed25519"));
                }
            }
            mgr.disconnect();
        }

        let mgr = Arc::new(ConnManager::default());
        let t = Target {
            kind: "ssh".into(),
            host: Some("test-unreachable".into()),
            label: "x".into(),
        };
        std::fs::write(
            &cfg,
            format!(
                "{}\nHost test-unreachable\n  HostName 127.0.0.1\n  Port 1\n",
                std::fs::read_to_string(&cfg).unwrap()
            ),
        )
        .unwrap();
        let err = mgr
            .connect(t, Arc::new(|_| {}), ctx.clone())
            .await
            .unwrap_err();
        assert!(
            err.contains("Connection refused")
                && err.contains("test-unreachable → 127.0.0.1:1 as tester"),
            "{err}"
        );
    }
}
