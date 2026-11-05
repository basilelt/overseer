use super::hosts::{self, Auth, Endpoint, SavedHost};
use super::known::{KnownHosts, Verdict};
use crate::install::SERVER_VERSION;
use crate::ssh::{self, BootFail, Fetch, Install, Link, Tunnel};
use russh::client::{self, AuthResult, Handle};
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh::MethodKind;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::BufReader;

pub const KEEPALIVE: Duration = Duration::from_secs(15);
const TCP_TIMEOUT: Duration = Duration::from_secs(20);

pub enum Ask {
    HostKey { host: String, fingerprint: String },
    Password { who: String },
}

pub type Asker =
    Arc<dyn Fn(Ask) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

pub trait Passwords: Send + Sync {
    fn known(&self, who: &str) -> Option<String>;
    fn accepted(&self, who: &str, password: &str);
    fn rejected(&self, who: &str);
}

pub struct Session {
    pub key: Arc<PrivateKey>,
    pub known: Arc<KnownHosts>,
    pub ask: Asker,
    pub passwords: Arc<dyn Passwords>,
    pub server: Option<Arc<Fetch>>,
}

struct Checker {
    host: String,
    port: u16,
    known: Arc<KnownHosts>,
    ask: Asker,
    rejection: Arc<Mutex<Option<String>>>,
}

pub fn host_key_changed(host: &str, port: u16) -> String {
    format!(
        "REMOTE HOST IDENTIFICATION HAS CHANGED for {}: the server offered another host key than the trusted one. Refusing to connect. If the server was reinstalled, remove its trusted key in the host's settings and connect again.",
        hosts::describe(&Endpoint { host: host.into(), port, user: String::new() }).trim_start_matches('@')
    )
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let pk = match key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(c) => PublicKey::new(c.public_key().clone(), ""),
        };
        let verdict = match self.known.check(&self.host, self.port, &pk) {
            Verdict::Trusted => Ok(()),
            Verdict::Changed => Err(host_key_changed(&self.host, self.port)),
            Verdict::Unknown => {
                let fingerprint = super::keys::fingerprint(&pk);
                let host = super::known::pattern(&self.host, self.port);
                match (self.ask)(Ask::HostKey { host, fingerprint })
                    .await
                    .as_deref()
                {
                    Some("yes") => self
                        .known
                        .trust(&self.host, self.port, &pk)
                        .map_err(|e| format!("cannot save the host key: {e}")),
                    _ => Err(format!("Host key of {} not trusted (cancelled)", self.host)),
                }
            }
        };
        match verdict {
            Ok(()) => Ok(true),
            Err(msg) => {
                *self.rejection.lock().unwrap() = Some(msg);
                Ok(false)
            }
        }
    }
}

fn config() -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(KEEPALIVE),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    })
}

async fn handshake<S>(
    e: &Endpoint,
    s: &Session,
    stream: Option<S>,
) -> Result<Handle<Checker>, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let rejection = Arc::new(Mutex::new(None));
    let checker = Checker {
        host: e.host.clone(),
        port: e.port,
        known: s.known.clone(),
        ask: s.ask.clone(),
        rejection: rejection.clone(),
    };
    let res = match stream {
        Some(st) => client::connect_stream(config(), st, checker).await,
        None => {
            let tcp = match tokio::time::timeout(
                TCP_TIMEOUT,
                tokio::net::TcpStream::connect((e.host.as_str(), e.port)),
            )
            .await
            {
                Ok(Ok(t)) => t,
                Ok(Err(err)) => {
                    return Err(format!(
                        "ssh: connect to host {} port {}: {err}",
                        e.host, e.port
                    ))
                }
                Err(_) => {
                    return Err(format!(
                        "ssh: connect to host {} port {}: Connection timed out",
                        e.host, e.port
                    ))
                }
            };
            let _ = tcp.set_nodelay(true);
            client::connect_stream(config(), tcp, checker).await
        }
    };
    res.map_err(|err| {
        rejection
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| format!("ssh: {}:{}: {err}", e.host, e.port))
    })
}

pub fn auth_failed(who: &str) -> String {
    format!(
        "Permission denied for {who}: the server did not accept this device's key. Add the device key to ~/.ssh/authorized_keys on the server (Settings > Device key), or switch the host to password authentication."
    )
}

async fn authenticate(
    handle: &mut Handle<Checker>,
    e: &Endpoint,
    prefer_password: bool,
    s: &Session,
) -> Result<(), String> {
    let who = hosts::describe(e);
    let fail = |err: russh::Error| format!("ssh: authentication of {who}: {err}");
    let mut password_allowed = prefer_password;
    if !prefer_password {
        match handle
            .authenticate_publickey(&e.user, PrivateKeyWithHashAlg::new(s.key.clone(), None))
            .await
            .map_err(fail)?
        {
            AuthResult::Success => return Ok(()),
            AuthResult::Failure {
                remaining_methods, ..
            } => password_allowed = remaining_methods.contains(&MethodKind::Password),
        }
    }
    if !password_allowed {
        return Err(auth_failed(&who));
    }
    let mut prompted = false;
    let mut password = s.passwords.known(&who);
    for _ in 0..3 {
        let pw = match password.take() {
            Some(p) => p,
            None => {
                prompted = true;
                match (s.ask)(Ask::Password { who: who.clone() }).await {
                    Some(p) => p,
                    None => return Err(format!("password for {who}: cancelled")),
                }
            }
        };
        if handle
            .authenticate_password(&e.user, &pw)
            .await
            .map_err(fail)?
            .success()
        {
            s.passwords.accepted(&who, &pw);
            return Ok(());
        }
        s.passwords.rejected(&who);
    }
    Err(if prompted {
        format!("Permission denied for {who}: wrong password (cancelled after 3 tries)")
    } else {
        auth_failed(&who)
    })
}

type Handles = (Handle<Checker>, Option<Handle<Checker>>);

struct SessionLink(Mutex<Option<Handles>>);

impl Link for SessionLink {
    fn exited(&self) -> bool {
        self.0
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(|(h, j)| h.is_closed() || j.as_ref().is_some_and(|j| j.is_closed()))
    }

    fn kill(&self) {
        self.0.lock().unwrap().take();
    }
}

pub async fn connect(
    host: &SavedHost,
    s: &Session,
    replace: bool,
    on_step: &(dyn Fn(&'static str, Option<String>) + Send + Sync),
) -> Result<Tunnel, String> {
    let main = host.endpoint();
    let password = host.auth == Auth::Password;
    let (mut handle, jump) = match &host.jump {
        None => (
            handshake::<tokio::net::TcpStream>(&main, s, None).await?,
            None,
        ),
        Some(j) => {
            let mut jh = handshake::<tokio::net::TcpStream>(j, s, None).await?;
            authenticate(&mut jh, j, password, s).await?;
            let ch = jh
                .channel_open_direct_tcpip(main.host.clone(), u32::from(main.port), "127.0.0.1", 0)
                .await
                .map_err(|e| {
                    format!(
                        "ssh: ProxyJump {} to {}:{}: {e}",
                        hosts::describe(j),
                        main.host,
                        main.port
                    )
                })?;
            (handshake(&main, s, Some(ch.into_stream())).await?, Some(jh))
        }
    };
    authenticate(&mut handle, &main, password, s).await?;
    let ch = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("ssh: open session: {e}"))?;
    ch.exec(true, ssh::installed_command(SERVER_VERSION, replace))
        .await
        .map_err(|e| format!("ssh: exec: {e}"))?;
    let (r, w) = tokio::io::split(ch.into_stream());
    let mode = match &s.server {
        Some(fetch) => Install::Fetch(fetch.as_ref()),
        None => Install::RunInstalled,
    };
    let (mux, installed) = match ssh::bootstrap(BufReader::new(r), w, mode, on_step).await {
        Ok(m) => m,
        Err(BootFail::Closed) => {
            return Err(format!(
                "{} closed the session before the overseer server answered",
                host.describe()
            ))
        }
        Err(BootFail::Remote(e) | BootFail::Missing(e) | BootFail::Io(e)) => return Err(e),
    };
    Ok(
        Tunnel::new(mux, Box::new(SessionLink(Mutex::new(Some((handle, jump))))))
            .with_installed(installed),
    )
}
