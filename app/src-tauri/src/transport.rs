use crate::http;
use crate::ssh::Tunnel;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}
pub type BoxIo = Box<dyn Io>;

#[allow(dead_code)]
pub mod wire {
    include!("../../../contract/wire.rs");
}

pub const PROTOCOL: u64 = wire::PROTOCOL as u64;

pub enum DaemonConn {
    Local(PathBuf),
    Loopback { addr: String, token: String },
    Tunnel(Arc<Tunnel>),
}

pub fn state_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("OVERSEER_STATE_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(d);
    }
    let home = dirs::home_dir().unwrap_or_default();
    if cfg!(windows) {
        return dirs::data_local_dir()
            .unwrap_or(home)
            .join(wire::STATE_DIR_NAME);
    }
    if cfg!(target_os = "macos") {
        return home.join("Library/Application Support/overseer");
    }
    std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/state"))
        .join(wire::STATE_DIR_NAME)
}

pub fn local_socket() -> PathBuf {
    std::env::var_os("OVERSEER_SOCKET")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| state_dir().join(wire::SOCKET_FILE))
}

pub fn local() -> Result<DaemonConn, String> {
    if !cfg!(windows) {
        return Ok(DaemonConn::Local(local_socket()));
    }
    let file = state_dir().join(wire::ENDPOINT_FILE);
    let raw = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    loopback_from(&raw).ok_or_else(|| format!("{}: no addr or token", file.display()))
}

fn loopback_from(raw: &[u8]) -> Option<DaemonConn> {
    let v = http::json_body(raw);
    let field = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
    Some(DaemonConn::Loopback {
        addr: field("addr")?,
        token: field("token")?,
    })
}

impl DaemonConn {
    pub async fn open(&self) -> std::io::Result<BoxIo> {
        match self {
            #[cfg(unix)]
            DaemonConn::Local(path) => Ok(Box::new(tokio::net::UnixStream::connect(path).await?)),
            #[cfg(not(unix))]
            DaemonConn::Local(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "the local daemon is not supported on Windows yet; connect to a host instead",
            )),
            DaemonConn::Loopback { addr, .. } => Ok(Box::new(
                tokio::net::TcpStream::connect(addr.as_str()).await?,
            )),
            DaemonConn::Tunnel(t) => Ok(Box::new(t.mux.open("daemon").await?)),
        }
    }

    pub fn path(&self, path: &str) -> String {
        match self {
            DaemonConn::Loopback { token, .. } => format!(
                "{path}{}token={token}",
                if path.contains('?') { '&' } else { '?' }
            ),
            _ => path.to_string(),
        }
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<http::Response, String> {
        self.request_within(method, path, body, Duration::from_secs(30))
            .await
    }

    pub async fn request_within(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        limit: Duration,
    ) -> Result<http::Response, String> {
        let fut = async {
            let io = self.open().await?;
            http::request(io, method, &self.path(path), body).await
        };
        self.timed(method, path, limit, fut).await
    }

    pub async fn request_on(
        &self,
        io: BoxIo,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
    ) -> Result<http::Response, String> {
        let full = self.path(path);
        let fut = http::request(io, method, &full, body);
        self.timed(method, path, Duration::from_secs(30), fut).await
    }

    async fn timed(
        &self,
        method: &str,
        path: &str,
        limit: Duration,
        fut: impl std::future::Future<Output = std::io::Result<http::Response>>,
    ) -> Result<http::Response, String> {
        let started = std::time::Instant::now();
        let res = match tokio::time::timeout(limit, fut).await {
            Ok(r) => r.map_err(|e| e.to_string()),
            Err(_) => Err(format!("{method} {path}: timed out")),
        };
        let took = started.elapsed();
        let route = crate::diag::strip_query(path);
        match &res {
            Err(e) => crate::diag::warn(
                "daemon",
                format!(
                    "{method} {route} failed after {:.1} s: {e}",
                    took.as_secs_f64()
                ),
            ),
            Ok(r) if r.head.status >= 500 => crate::diag::warn(
                "daemon",
                format!(
                    "{method} {route}: HTTP {} after {:.1} s",
                    r.head.status,
                    took.as_secs_f64()
                ),
            ),
            Ok(_) if took >= crate::diag::SLOW_DAEMON => crate::diag::warn(
                "perf",
                format!("daemon {method} {route} took {:.1} s", took.as_secs_f64()),
            ),
            Ok(_) => {}
        }
        res
    }

    pub async fn upload<B, F>(
        &self,
        path: &str,
        len: u64,
        body: B,
        on_progress: F,
    ) -> Result<http::Response, String>
    where
        B: AsyncRead + Unpin,
        F: FnMut(u64),
    {
        let idle = Duration::from_secs(30);
        let io = tokio::time::timeout(idle, self.open())
            .await
            .map_err(|_| format!("POST {path}: timed out"))?
            .map_err(|e| e.to_string())?;
        http::upload(io, &self.path(path), len, body, idle, on_progress)
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn version(&self) -> Result<String, String> {
        let res = self.request("GET", "/v1/version", None).await?;
        let from = |body: &[u8]| {
            http::json_body(body)
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from)
        };
        if res.head.status == 200 {
            if let Some(p) = http::json_body(&res.body)
                .get("protocol")
                .and_then(|v| v.as_u64())
                .filter(|p| *p != PROTOCOL)
            {
                return Err(format!("the daemon speaks protocol {p}, this app speaks {PROTOCOL}; update one of them"));
            }
            if let Some(v) = from(&res.body) {
                return Ok(v);
            }
        }
        let res = self.request("GET", "/v1/status", None).await?;
        if res.head.status != 200 {
            return Err(format!("daemon answered HTTP {}", res.head.status));
        }
        Ok(from(&res.body).unwrap_or_else(|| "unknown".into()))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn loopback_adds_the_token_to_the_query() {
        let c = loopback_from(br#"{"addr":"127.0.0.1:4100","token":"ab","pid":3}"#).unwrap();
        assert_eq!(c.path("/v1/lines"), "/v1/lines?token=ab");
        assert_eq!(
            c.path("/v1/lines?scope=local"),
            "/v1/lines?scope=local&token=ab"
        );
        assert!(loopback_from(b"{}").is_none());
        assert_eq!(DaemonConn::Local(PathBuf::new()).path("/v1/x"), "/v1/x");
    }

    #[tokio::test]
    async fn refuses_another_protocol() {
        let path = std::env::temp_dir().join(format!("sb-app-proto-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf).await.unwrap();
            let body = r#"{"version":"0.3.0","protocol":2}"#;
            s.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        });
        let err = DaemonConn::Local(path.clone()).version().await.unwrap_err();
        assert!(err.contains("protocol 2"), "{err}");
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn local_version_falls_back_to_status() {
        let path = std::env::temp_dir().join(format!("sb-app-test-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            for reply in [
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 35\r\n\r\n{\"stationId\":\"s\",\"version\":\"0.1.0\"}",
            ] {
                let (mut s, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 1024];
                let n = s.read(&mut buf).await.unwrap();
                assert!(buf[..n].starts_with(b"GET /v1/"));
                s.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        let conn = DaemonConn::Local(path.clone());
        assert_eq!(conn.version().await.unwrap(), "0.1.0");
        server.await.unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(DaemonConn::Local(path).version().await.is_err());
    }
}
