use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::io::{Read, Write};
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::oneshot;

pub const ENV_ADDR: &str = "OVERSEER_ASKPASS";
pub const ENV_TOKEN: &str = "OVERSEER_ASKPASS_TOKEN";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prompt {
    pub id: u32,
    pub kind: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

pub fn classify(text: &str, hint: &str) -> (&'static str, Option<String>, Option<String>) {
    if text.contains("authenticity of host")
        || (text.contains("fingerprint") && text.contains("(yes/no"))
    {
        let fp = text
            .split_whitespace()
            .find(|w| w.starts_with("SHA256:") || w.starts_with("MD5:"))
            .map(|w| w.trim_end_matches(['.', ',']).to_string());
        let host = text.split('\'').nth(1).map(String::from);
        return ("hostkey", fp, host);
    }
    if hint.eq_ignore_ascii_case("confirm") || hint.eq_ignore_ascii_case("none") {
        return ("confirm", None, None);
    }
    ("secret", None, None)
}

pub type Handler =
    Arc<dyn Fn(Prompt) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

#[derive(Default)]
struct Registry {
    handlers: HashMap<String, Handler>,
    pending: HashMap<u32, oneshot::Sender<Option<String>>>,
}

static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
static NEXT: AtomicU32 = AtomicU32::new(1);
static ENDPOINT: tokio::sync::OnceCell<Result<String, String>> = tokio::sync::OnceCell::const_new();

fn reg() -> std::sync::MutexGuard<'static, Registry> {
    REG.get_or_init(Default::default).lock().unwrap()
}

pub struct Registration {
    pub token: String,
    pub endpoint: String,
}

impl Drop for Registration {
    fn drop(&mut self) {
        reg().handlers.remove(&self.token);
    }
}

pub async fn register(handler: Handler) -> Result<Registration, String> {
    let endpoint = ENDPOINT.get_or_init(serve).await.clone()?;
    let token = random_token();
    reg().handlers.insert(token.clone(), handler);
    Ok(Registration { token, endpoint })
}

pub fn ask(prompt_text: String, hint: &str) -> (u32, Prompt, oneshot::Receiver<Option<String>>) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (kind, fingerprint, host) = classify(&prompt_text, hint);
    let (tx, rx) = oneshot::channel();
    reg().pending.insert(id, tx);
    (
        id,
        Prompt {
            id,
            kind,
            text: prompt_text.trim().to_string(),
            fingerprint,
            host,
        },
        rx,
    )
}

pub fn ask_as(
    kind: &'static str,
    text: String,
    fingerprint: Option<String>,
    host: Option<String>,
) -> (u32, Prompt, oneshot::Receiver<Option<String>>) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    reg().pending.insert(id, tx);
    (
        id,
        Prompt {
            id,
            kind,
            text,
            fingerprint,
            host,
        },
        rx,
    )
}

pub fn forget(id: u32) {
    reg().pending.remove(&id);
}

pub fn answer(id: u32, answer: Option<String>) -> bool {
    match reg().pending.remove(&id) {
        Some(tx) => tx.send(answer).is_ok(),
        None => false,
    }
}

pub(crate) fn random_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::new();
    for i in 0..4u64 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(i ^ std::process::id() as u64);
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

async fn handle<S: AsyncRead + AsyncWrite + Unpin>(mut s: S) {
    let mut raw = Vec::new();
    if (&mut s)
        .take(64 * 1024)
        .read_to_end(&mut raw)
        .await
        .is_err()
    {
        return;
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut parts = text.splitn(3, '\n');
    let (token, hint, prompt) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    );
    let handler = reg().handlers.get(token).cloned();
    let reply = match handler {
        Some(h) => {
            let (id, p, rx) = ask(prompt.to_string(), hint);
            let fut = h(p);
            let ans = tokio::select! {
                a = rx => a.ok().flatten(),
                a = fut => a,
            };
            reg().pending.remove(&id);
            ans
        }
        None => None,
    };
    let out = match reply {
        Some(a) => format!("OK\n{a}"),
        None => "NO\n".into(),
    };
    let _ = s.write_all(out.as_bytes()).await;
    let _ = s.shutdown().await;
}

#[cfg(unix)]
async fn serve() -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "sb-askpass-{}-{}",
        std::process::id(),
        &random_token()[..8]
    ));
    std::fs::create_dir_all(&dir).map_err(|e| format!("askpass: {e}"))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("askpass: {e}"))?;
    let path = dir.join("s");
    let l = tokio::net::UnixListener::bind(&path)
        .map_err(|e| format!("askpass socket {}: {e}", path.display()))?;
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            tokio::spawn(handle(s));
        }
    });
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(not(unix))]
async fn serve() -> Result<String, String> {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("askpass: {e}"))?;
    let addr = l.local_addr().map_err(|e| e.to_string())?;
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            tokio::spawn(handle(s));
        }
    });
    Ok(format!("tcp:{addr}"))
}

fn exchange<S: Read + Write>(
    mut s: S,
    token: &str,
    hint: &str,
    prompt: &str,
    shutdown: impl FnOnce(&S),
) -> Option<String> {
    s.write_all(format!("{token}\n{hint}\n{prompt}").as_bytes())
        .ok()?;
    shutdown(&s);
    let mut reply = String::new();
    s.read_to_string(&mut reply).ok()?;
    reply.strip_prefix("OK\n").map(String::from)
}

pub fn client_main() -> Option<i32> {
    let addr = std::env::var(ENV_ADDR).ok().filter(|v| !v.is_empty())?;
    let token = std::env::var(ENV_TOKEN).unwrap_or_default();
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let hint = std::env::var("SSH_ASKPASS_PROMPT").unwrap_or_default();
    let answer = if let Some(tcp) = addr.strip_prefix("tcp:") {
        std::net::TcpStream::connect(tcp).ok().and_then(|s| {
            exchange(s, &token, &hint, &prompt, |s| {
                drop(s.shutdown(std::net::Shutdown::Write))
            })
        })
    } else {
        #[cfg(unix)]
        {
            std::os::unix::net::UnixStream::connect(&addr)
                .ok()
                .and_then(|s| {
                    exchange(s, &token, &hint, &prompt, |s| {
                        drop(s.shutdown(std::net::Shutdown::Write))
                    })
                })
        }
        #[cfg(not(unix))]
        None
    };
    match answer {
        Some(a) => {
            println!("{a}");
            Some(0)
        }
        None => Some(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTKEY: &str = "The authenticity of host '[127.0.0.1]:2222 ([127.0.0.1]:2222)' can't be established.\nED25519 key fingerprint is SHA256:AbCdEf0123+/xyz.\nThis key is not known by any other names.\nAre you sure you want to continue connecting (yes/no/[fingerprint])? ";

    #[test]
    fn classifies_prompts() {
        let (kind, fp, host) = classify(HOSTKEY, "");
        assert_eq!(kind, "hostkey");
        assert_eq!(fp.as_deref(), Some("SHA256:AbCdEf0123+/xyz"));
        assert_eq!(host.as_deref(), Some("[127.0.0.1]:2222 ([127.0.0.1]:2222)"));
        assert_eq!(
            classify("Enter passphrase for key '/home/me/.ssh/id_ed25519': ", "").0,
            "secret"
        );
        assert_eq!(classify("me@host's password: ", "").0, "secret");
        assert_eq!(classify("Allow use of key ...?", "confirm").0, "confirm");
    }

    #[tokio::test]
    async fn helper_round_trip_through_the_app_endpoint() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let reg = register(Arc::new(move |p: Prompt| {
            s2.lock().unwrap().push(p.clone());
            Box::pin(async move { (p.kind == "hostkey").then(|| "yes".to_string()) })
        }))
        .await
        .unwrap();
        let (endpoint, token) = (reg.endpoint.clone(), reg.token.clone());
        let ask = |prompt: &'static str, tok: String| {
            let endpoint = endpoint.clone();
            tokio::task::spawn_blocking(move || {
                #[cfg(unix)]
                let s = std::os::unix::net::UnixStream::connect(&endpoint).unwrap();
                #[cfg(not(unix))]
                let s = std::net::TcpStream::connect(endpoint.trim_start_matches("tcp:")).unwrap();
                exchange(s, &tok, "", prompt, |s| {
                    drop(s.shutdown(std::net::Shutdown::Write))
                })
            })
        };
        assert_eq!(
            ask(HOSTKEY, token.clone()).await.unwrap().as_deref(),
            Some("yes")
        );
        assert_eq!(
            ask("Enter passphrase: ", token.clone()).await.unwrap(),
            None
        );
        assert_eq!(ask(HOSTKEY, "wrong".into()).await.unwrap(), None);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen[0].fingerprint.as_deref(),
            Some("SHA256:AbCdEf0123+/xyz")
        );
        drop(reg);
    }

    #[tokio::test]
    async fn answer_resolves_a_pending_prompt() {
        let (id, p, rx) = ask("pw: ".into(), "");
        assert_eq!(p.kind, "secret");
        assert!(answer(id, Some("hunter2".into())));
        assert_eq!(rx.await.unwrap().as_deref(), Some("hunter2"));
        assert!(!answer(id, None));
    }
}
