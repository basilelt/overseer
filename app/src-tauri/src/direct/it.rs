use super::client::{self, Ask, Asker, Passwords, Session};
use super::hosts::{Auth as HostAuth, Endpoint, SavedHost};
use super::keys;
use super::known::{KnownHosts, Verdict};
use crate::install::{Platform, SERVER_VERSION};
use crate::mux::BANNER;
use crate::ssh::{Fetch, FetchFuture};
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, ChannelOpenHandle, Msg, VerifiedPublicKey};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct Fake {
    allowed: PublicKey,
    password: Option<String>,
    installed: bool,
    commands: Arc<Mutex<Vec<String>>>,
    input: Arc<Mutex<Vec<u8>>>,
    uploads: Arc<Mutex<Vec<Vec<u8>>>>,
    exec: Option<ChannelId>,
}

enum Request {
    Run,
    Install(Vec<u8>),
    Bad,
}

fn request(buf: &[u8]) -> Option<Request> {
    if buf.starts_with(b"RUN\n") {
        return Some(Request::Run);
    }
    let nl = buf.iter().position(|b| *b == b'\n')?;
    let head = String::from_utf8_lossy(&buf[..nl]).into_owned();
    let mut words = head.split_whitespace();
    let (Some("INSTALL"), Some(size), Some(hash)) = (
        words.next(),
        words.next().and_then(|n| n.parse::<usize>().ok()),
        words.next(),
    ) else {
        return Some(Request::Bad);
    };
    let body = buf.get(nl + 1..nl + 1 + size)?;
    Some(if crate::install::sha256_hex(body) == hash {
        Request::Install(body.to_vec())
    } else {
        Request::Bad
    })
}

impl server::Handler for Fake {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        if key.public_key.key_data() == self.allowed.key_data() {
            return Ok(Auth::Accept);
        }
        let next: &[MethodKind] = if self.password.is_some() {
            &[MethodKind::Password]
        } else {
            &[]
        };
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(next)),
            partial_success: false,
        })
    }

    async fn auth_password(&mut self, _: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if self.password.as_deref() == Some(password) {
            Auth::Accept
        } else {
            Auth::Reject {
                proceed_with_methods: Some(MethodSet::from(&[MethodKind::Password][..])),
                partial_success: false,
            }
        })
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host: &str,
        port: u32,
        _: &str,
        _: u32,
        reply: ChannelOpenHandle,
        _: &mut server::Session,
    ) -> Result<(), Self::Error> {
        let target = format!("{host}:{port}");
        reply.accept().await;
        tokio::spawn(async move {
            if let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await {
                let mut ch = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut ch, &mut tcp).await;
            }
        });
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        self.commands
            .lock()
            .unwrap()
            .push(String::from_utf8_lossy(data).into_owned());
        self.input.lock().unwrap().clear();
        self.exec = Some(channel);
        session.channel_success(channel)?;
        let v = if self.installed { "0.0.1" } else { "-" };
        session.data(
            channel,
            format!("Welcome\nSB:HELLO Linux x86_64 {v}\n").into_bytes(),
        )?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> Result<(), Self::Error> {
        if self.exec != Some(channel) {
            return Ok(());
        }
        let req = {
            let mut input = self.input.lock().unwrap();
            input.extend_from_slice(data);
            let req = request(&input);
            if req.is_some() {
                input.clear();
            }
            req
        };
        match req {
            Some(Request::Run) => session.data(channel, format!("{BANNER}\n").into_bytes())?,
            Some(Request::Install(body)) => {
                self.uploads.lock().unwrap().push(body);
                session.data(channel, format!("{BANNER}\n").into_bytes())?;
            }
            Some(Request::Bad) => session.data(
                channel,
                b"SB:ERROR checksum mismatch after upload\n".to_vec(),
            )?,
            None => {}
        }
        Ok(())
    }
}

struct Server {
    port: u16,
    host_key: PublicKey,
    commands: Arc<Mutex<Vec<String>>>,
    uploads: Arc<Mutex<Vec<Vec<u8>>>>,
}

async fn serve(allowed: PublicKey, password: Option<&str>, installed: bool) -> Server {
    let host_key = keys::generate("host").unwrap();
    let public = host_key.public_key().clone();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::from_millis(1),
        auth_rejection_time_initial: Some(Duration::ZERO),
        methods: MethodSet::from(&[MethodKind::PublicKey, MethodKind::Password][..]),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let uploads = Arc::new(Mutex::new(Vec::new()));
    let fake = Fake {
        allowed,
        password: password.map(String::from),
        installed,
        commands: commands.clone(),
        input: Arc::default(),
        uploads: uploads.clone(),
        exec: None,
    };
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let _ = server::run_stream(config.clone(), socket, fake.clone()).await;
        }
    });
    Server {
        port,
        host_key: public,
        commands,
        uploads,
    }
}

#[derive(Default)]
struct Memory(Mutex<HashMap<String, String>>);

impl Passwords for Memory {
    fn known(&self, who: &str) -> Option<String> {
        self.0.lock().unwrap().get(who).cloned()
    }

    fn accepted(&self, who: &str, password: &str) {
        self.0.lock().unwrap().insert(who.into(), password.into());
    }

    fn rejected(&self, who: &str) {
        self.0.lock().unwrap().remove(who);
    }
}

type Asked = Arc<Mutex<Vec<String>>>;

fn asker(answers: &[&str]) -> (Asker, Asked) {
    let asked: Asked = Arc::default();
    let queue = Arc::new(Mutex::new(
        answers.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
    ));
    let seen = asked.clone();
    let ask: Asker = Arc::new(move |a| {
        seen.lock().unwrap().push(match &a {
            Ask::HostKey { host, fingerprint } => format!("hostkey {host} {fingerprint}"),
            Ask::Password { who } => format!("password {who}"),
        });
        let next = {
            let mut q = queue.lock().unwrap();
            (!q.is_empty()).then(|| q.remove(0))
        };
        Box::pin(async move { next })
    });
    (ask, asked)
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sb-direct-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn host(port: u16, auth: HostAuth) -> SavedHost {
    SavedHost {
        id: "h1".into(),
        label: "test".into(),
        host: "127.0.0.1".into(),
        port,
        user: "alice".into(),
        jump: None,
        auth,
        remember_password: false,
    }
}

fn session(key: &PrivateKey, known: &KnownHosts, ask: Asker) -> Session {
    Session {
        key: Arc::new(key.clone()),
        known: Arc::new(KnownHosts::new(known.path())),
        ask,
        passwords: Arc::new(Memory::default()),
        server: None,
    }
}

async fn connect(h: &SavedHost, s: &Session) -> Result<crate::ssh::Tunnel, String> {
    let steps = Mutex::new(Vec::new());
    let r = tokio::time::timeout(
        Duration::from_secs(20),
        client::connect(h, s, false, &|p: &'static str, _: Option<String>| {
            steps.lock().unwrap().push(p)
        }),
    )
    .await
    .expect("connect timed out");
    if r.is_ok() {
        assert_eq!(*steps.lock().unwrap(), vec!["checking", "starting"]);
    }
    r
}

#[tokio::test]
async fn trusts_the_host_once_then_runs_the_installed_server_over_the_mux() {
    let device = keys::generate("overseer-test").unwrap();
    let srv = serve(device.public_key().clone(), None, true).await;
    let d = dir("trust");
    let known = KnownHosts::new(d.join("known_hosts"));
    let (ask, asked) = asker(&["yes"]);
    let s = session(&device, &known, ask);
    let h = host(srv.port, HostAuth::Key);

    let t = connect(&h, &s).await.unwrap();
    assert!(!t.mux.is_dead());
    let fp = keys::fingerprint(&srv.host_key);
    assert_eq!(
        *asked.lock().unwrap(),
        vec![format!("hostkey [127.0.0.1]:{} {fp}", srv.port)]
    );
    assert_eq!(
        known.check("127.0.0.1", srv.port, &srv.host_key),
        Verdict::Trusted
    );
    let cmds = srv.commands.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].contains("proxy"), "{}", cmds[0]);
    assert!(srv.uploads.lock().unwrap().is_empty());
    drop(t);

    let t = connect(&h, &s).await.unwrap();
    assert_eq!(asked.lock().unwrap().len(), 1);
    drop(t);
    let _ = std::fs::remove_dir_all(d);
}

#[tokio::test]
async fn refuses_an_untrusted_or_changed_host_key() {
    let device = keys::generate("overseer-test").unwrap();
    let srv = serve(device.public_key().clone(), None, true).await;
    let d = dir("changed");
    let known = KnownHosts::new(d.join("known_hosts"));
    let h = host(srv.port, HostAuth::Key);

    let (ask, _) = asker(&["no"]);
    let err = connect(&h, &session(&device, &known, ask))
        .await
        .err()
        .unwrap();
    assert!(err.contains("not trusted (cancelled)"), "{err}");
    assert!(crate::ssh::permanent(&err));
    assert_eq!(
        known.check("127.0.0.1", srv.port, &srv.host_key),
        Verdict::Unknown
    );

    known
        .trust(
            "127.0.0.1",
            srv.port,
            keys::generate("other").unwrap().public_key(),
        )
        .unwrap();
    let (ask, asked) = asker(&["yes"]);
    let err = connect(&h, &session(&device, &known, ask))
        .await
        .err()
        .unwrap();
    assert!(
        err.contains("REMOTE HOST IDENTIFICATION HAS CHANGED"),
        "{err}"
    );
    assert!(crate::ssh::permanent(&err));
    assert!(asked.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(d);
}

#[tokio::test]
async fn explains_a_missing_server_and_a_refused_key() {
    let device = keys::generate("overseer-test").unwrap();
    let d = dir("missing");
    let known = KnownHosts::new(d.join("known_hosts"));

    let srv = serve(device.public_key().clone(), None, false).await;
    let (ask, _) = asker(&["yes"]);
    let err = connect(
        &host(srv.port, HostAuth::Key),
        &session(&device, &known, ask),
    )
    .await
    .err()
    .unwrap();
    assert!(
        err.contains(crate::ssh::NOT_INSTALLED) && err.contains("desktop"),
        "{err}"
    );

    let srv = serve(
        keys::generate("someone else").unwrap().public_key().clone(),
        None,
        true,
    )
    .await;
    let (ask, _) = asker(&["yes"]);
    let err = connect(
        &host(srv.port, HostAuth::Key),
        &session(&device, &known, ask),
    )
    .await
    .err()
    .unwrap();
    assert!(err.contains("did not accept this device's key"), "{err}");
    assert!(crate::ssh::permanent(&err));
    let _ = std::fs::remove_dir_all(d);
}

#[tokio::test]
async fn falls_back_to_a_password_and_remembers_it_for_reconnects() {
    let device = keys::generate("overseer-test").unwrap();
    let srv = serve(
        keys::generate("someone else").unwrap().public_key().clone(),
        Some("s3cret"),
        true,
    )
    .await;
    let d = dir("password");
    let known = KnownHosts::new(d.join("known_hosts"));
    let (ask, asked) = asker(&["yes", "wrong", "s3cret"]);
    let s = session(&device, &known, ask);
    let h = host(srv.port, HostAuth::Key);

    let t = connect(&h, &s).await.unwrap();
    let who = format!("alice@127.0.0.1:{}", srv.port);
    assert_eq!(
        asked.lock().unwrap()[1..],
        [format!("password {who}"), format!("password {who}")]
    );
    assert_eq!(s.passwords.known(&who).as_deref(), Some("s3cret"));
    drop(t);

    let t = connect(&host(srv.port, HostAuth::Password), &s)
        .await
        .unwrap();
    assert_eq!(asked.lock().unwrap().len(), 3);
    drop(t);
    let _ = std::fs::remove_dir_all(d);
}

#[tokio::test]
async fn reaches_a_host_through_one_proxy_jump() {
    let device = keys::generate("overseer-test").unwrap();
    let bastion = serve(device.public_key().clone(), None, true).await;
    let target = serve(device.public_key().clone(), None, true).await;
    let d = dir("jump");
    let known = KnownHosts::new(d.join("known_hosts"));
    let (ask, asked) = asker(&["yes", "yes"]);
    let mut h = host(target.port, HostAuth::Key);
    h.jump = Some(Endpoint {
        host: "127.0.0.1".into(),
        port: bastion.port,
        user: "alice".into(),
    });

    let t = connect(&h, &session(&device, &known, ask)).await.unwrap();
    assert!(!t.mux.is_dead());
    let asked = asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2);
    assert!(
        asked[0].contains(&format!(":{}", bastion.port))
            && asked[1].contains(&format!(":{}", target.port)),
        "{asked:?}"
    );
    assert!(bastion.commands.lock().unwrap().is_empty());
    assert_eq!(target.commands.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(d);
}

async fn connect_steps(
    h: &SavedHost,
    s: &Session,
) -> (Result<crate::ssh::Tunnel, String>, Vec<&'static str>) {
    let steps = Mutex::new(Vec::new());
    let r = tokio::time::timeout(
        Duration::from_secs(20),
        client::connect(h, s, false, &|p: &'static str, _: Option<String>| {
            steps.lock().unwrap().push(p)
        }),
    )
    .await
    .expect("connect timed out");
    (r, steps.into_inner().unwrap())
}

fn source(
    result: Result<&'static [u8], &'static str>,
    calls: Arc<Mutex<Vec<Platform>>>,
) -> Arc<Fetch> {
    Arc::new(move |p: Platform| -> FetchFuture {
        calls.lock().unwrap().push(p);
        Box::pin(async move { result.map(<[u8]>::to_vec).map_err(String::from) })
    })
}

#[tokio::test]
async fn installs_a_missing_server_from_the_release_download() {
    let device = keys::generate("overseer-test").unwrap();
    let srv = serve(device.public_key().clone(), None, false).await;
    let d = dir("fetch");
    let known = KnownHosts::new(d.join("known_hosts"));
    let (ask, _) = asker(&["yes"]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let stub: &'static [u8] = b"#!/bin/sh\necho stub overseer\n";
    let mut s = session(&device, &known, ask);
    s.server = Some(source(Ok(stub), calls.clone()));
    let h = host(srv.port, HostAuth::Key);

    let (t, steps) = connect_steps(&h, &s).await;
    let t = t.unwrap();
    assert_eq!(
        steps,
        vec!["checking", "downloading", "installing", "starting"]
    );
    assert_eq!(*calls.lock().unwrap(), vec![Platform::LinuxX86_64]);
    assert_eq!(*srv.uploads.lock().unwrap(), vec![stub.to_vec()]);
    assert_eq!(t.installed.as_deref(), Some(SERVER_VERSION));
    assert!(!t.mux.is_dead());
    drop(t);

    s.server = Some(source(
        Err("cannot download the server from GitHub: connection refused"),
        calls.clone(),
    ));
    let (t, _) = connect_steps(&h, &s).await;
    let err = t.err().unwrap();
    assert!(
        err.contains(crate::ssh::NOT_INSTALLED) && err.contains("cannot download the server"),
        "{err}"
    );
    assert!(crate::ssh::permanent(&err));
    assert_eq!(srv.uploads.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(d);
}

#[tokio::test]
async fn keeps_an_older_server_when_the_download_fails() {
    let device = keys::generate("overseer-test").unwrap();
    let srv = serve(device.public_key().clone(), None, true).await;
    let d = dir("fetch-old");
    let known = KnownHosts::new(d.join("known_hosts"));
    let (ask, _) = asker(&["yes"]);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut s = session(&device, &known, ask);
    s.server = Some(source(Err("offline"), calls.clone()));

    let (t, steps) = connect_steps(&host(srv.port, HostAuth::Key), &s).await;
    let t = t.unwrap();
    assert_eq!(steps, vec!["checking", "downloading", "starting"]);
    assert_eq!(t.installed.as_deref(), Some("0.0.1"));
    assert!(srv.uploads.lock().unwrap().is_empty());
    assert_eq!(calls.lock().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(d);
}
