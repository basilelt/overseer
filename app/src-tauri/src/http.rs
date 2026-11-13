use std::future::Future;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

const MAX_HEAD: usize = 64 * 1024;
const BODY_BUF: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl Head {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn chunked(&self) -> bool {
        self.header("transfer-encoding")
            .is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
    }

    fn content_length(&self) -> Option<usize> {
        self.header("content-length")
            .and_then(|v| v.trim().parse().ok())
    }
}

#[derive(Debug)]
pub struct Response {
    pub head: Head,
    pub body: Vec<u8>,
}

pub fn encode_request(
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut out = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        out.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            b.len()
        ));
    } else if matches!(method, "POST" | "PUT" | "PATCH") {
        out.push_str("Content-Length: 0\r\n");
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    if let Some(b) = body {
        bytes.extend_from_slice(b);
    }
    bytes
}

pub fn encode_head(method: &str, path: &str, headers: &[(&str, &str)]) -> Vec<u8> {
    let mut out = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}

pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = if b[i] == b'%' {
            b.get(i + 1..i + 3)
                .filter(|h| h.iter().all(u8::is_ascii_hexdigit))
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        } else {
            None
        };
        match hex {
            Some(v) => {
                out.push(v);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn stalled() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "upload stalled")
}

async fn within<T>(
    idle: Duration,
    f: impl Future<Output = std::io::Result<T>>,
) -> std::io::Result<T> {
    tokio::time::timeout(idle, f).await.map_err(|_| stalled())?
}

async fn send_body<W, B, F>(
    w: &mut W,
    head: &[u8],
    len: u64,
    body: &mut B,
    idle: Duration,
    on_progress: &mut F,
) -> std::io::Result<()>
where
    W: AsyncWrite + Unpin,
    B: AsyncRead + Unpin,
    F: FnMut(u64),
{
    within(idle, w.write_all(head)).await?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut sent = 0u64;
    while sent < len {
        let want = usize::try_from(len - sent).map_or(buf.len(), |rest| rest.min(buf.len()));
        let n = within(idle, body.read(&mut buf[..want])).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "the file shrank while uploading",
            ));
        }
        within(idle, w.write_all(&buf[..n])).await?;
        sent += n as u64;
        on_progress(sent);
    }
    within(idle, w.flush()).await
}

pub async fn upload<S, B, F>(
    stream: S,
    path: &str,
    len: u64,
    mut body: B,
    idle: Duration,
    mut on_progress: F,
) -> std::io::Result<Response>
where
    S: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + Unpin,
    F: FnMut(u64),
{
    let mut r = BufReader::new(stream);
    let length = len.to_string();
    let head = encode_head(
        "POST",
        path,
        &[
            ("Content-Type", "application/octet-stream"),
            ("Content-Length", &length),
        ],
    );
    let sent = send_body(r.get_mut(), &head, len, &mut body, idle, &mut on_progress).await;
    let wait = if sent.is_ok() {
        idle
    } else {
        idle.min(Duration::from_secs(2))
    };
    let head = match tokio::time::timeout(wait, read_head(&mut r)).await {
        Ok(Ok(head)) => head,
        Ok(Err(e)) => return Err(sent.err().unwrap_or(e)),
        Err(_) => return Err(sent.err().unwrap_or_else(stalled)),
    };
    let mut out = Vec::new();
    within(
        idle,
        stream_body(&mut r, &head, |c| out.extend_from_slice(c)),
    )
    .await?;
    Ok(Response { head, body: out })
}

pub async fn read_head<R: AsyncRead + Unpin>(r: &mut BufReader<R>) -> std::io::Result<Head> {
    let mut line = String::new();
    let mut total = 0;
    total += r.read_line(&mut line).await?;
    let status = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid(format!("bad status line: {:?}", line.trim_end())))?;
    let mut headers = Vec::new();
    loop {
        line.clear();
        let n = r.read_line(&mut line).await?;
        total += n;
        if n == 0 {
            return Err(invalid("connection closed in headers"));
        }
        if total > MAX_HEAD {
            return Err(invalid("response head too large"));
        }
        let l = line.trim_end_matches(['\r', '\n']);
        if l.is_empty() {
            break;
        }
        if let Some((k, v)) = l.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok(Head { status, headers })
}

pub async fn stream_body<R, F>(
    r: &mut BufReader<R>,
    head: &Head,
    mut on_chunk: F,
) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    F: FnMut(&[u8]),
{
    if head.chunked() {
        let mut line = String::new();
        loop {
            line.clear();
            if r.read_line(&mut line).await? == 0 {
                return Err(invalid("connection closed in chunk size"));
            }
            let size_str = line.trim().split(';').next().unwrap_or("");
            let size = usize::from_str_radix(size_str, 16)
                .map_err(|_| invalid(format!("bad chunk size {size_str:?}")))?;
            if size == 0 {
                loop {
                    line.clear();
                    if r.read_line(&mut line).await? == 0 || line.trim().is_empty() {
                        return Ok(());
                    }
                }
            }
            let mut buf = vec![0; size];
            r.read_exact(&mut buf).await?;
            on_chunk(&buf);
            let mut crlf = [0; 2];
            r.read_exact(&mut crlf).await?;
        }
    } else if let Some(len) = head.content_length() {
        let mut buf = vec![0; len];
        r.read_exact(&mut buf).await?;
        on_chunk(&buf);
        Ok(())
    } else {
        let mut buf = vec![0; 8192];
        loop {
            let n = r.read(&mut buf).await?;
            if n == 0 {
                return Ok(());
            }
            on_chunk(&buf[..n]);
        }
    }
}

pub async fn request<S>(
    stream: S,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> std::io::Result<Response>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut r = BufReader::new(stream);
    r.get_mut()
        .write_all(&encode_request(method, path, &[], body))
        .await?;
    r.get_mut().flush().await?;
    let head = read_head(&mut r).await?;
    let mut out = Vec::new();
    if method != "HEAD" && head.status != 204 && head.status != 304 {
        stream_body(&mut r, &head, |c| out.extend_from_slice(c)).await?;
    }
    Ok(Response { head, body: out })
}

pub fn json_body(body: &[u8]) -> serde_json::Value {
    if body.iter().all(u8::is_ascii_whitespace) {
        return serde_json::Value::Null;
    }
    serde_json::from_slice(body)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(body).into_owned()))
}

pub fn error_field(body: &serde_json::Value) -> Option<String> {
    body.get("error").and_then(|e| e.as_str()).map(String::from)
}

fn invalid(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    async fn serve(
        reply: &'static [u8],
    ) -> (tokio::task::JoinHandle<Vec<u8>>, tokio::io::DuplexStream) {
        let (client, mut server) = duplex(64 * 1024);
        let h = tokio::spawn(async move {
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = server.read(&mut buf).await.unwrap();
                req.extend_from_slice(&buf[..n]);
                let s = String::from_utf8_lossy(&req);
                if let Some(i) = s.find("\r\n\r\n") {
                    let len: usize = s
                        .lines()
                        .find_map(|l| l.strip_prefix("Content-Length: "))
                        .map(|v| v.parse().unwrap())
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break;
                    }
                }
            }
            server.write_all(reply).await.unwrap();
            server.shutdown().await.unwrap();
            req
        });
        (h, client)
    }

    #[tokio::test]
    async fn content_length_response_and_request_framing() {
        let (h, c) = serve(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}trailing").await;
        let res = request(c, "POST", "/v1/tasks", Some(br#"{"a":1}"#))
            .await
            .unwrap();
        assert_eq!(res.head.status, 200);
        assert_eq!(json_body(&res.body), serde_json::json!({"ok": true}));
        let req = String::from_utf8(h.await.unwrap()).unwrap();
        assert!(req.starts_with("POST /v1/tasks HTTP/1.1\r\nHost: localhost\r\n"));
        assert!(req.contains("Content-Length: 7\r\n"));
        assert!(req.ends_with("\r\n\r\n{\"a\":1}"));
    }

    #[tokio::test]
    async fn chunked_response() {
        let (_h, c) = serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n[1,2\r\n3;ext=1\r\n,3]\r\n0\r\nX-Trailer: y\r\n\r\n").await;
        let res = request(c, "GET", "/v1/lines", None).await.unwrap();
        assert_eq!(json_body(&res.body), serde_json::json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn a_declared_size_far_beyond_the_body_is_an_error_not_an_allocation() {
        for reply in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 4611686018427387904\r\n\r\nshort"[..],
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4000000000000000\r\nshort"[..],
        ] {
            let (_h, c) = serve(reply).await;
            let err = request(c, "GET", "/v1/lines", None).await.unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
        }
    }

    #[tokio::test]
    async fn bodies_larger_than_the_read_buffer_arrive_whole() {
        let body = "x".repeat(3 * BODY_BUF + 7);
        let plain = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let chunked = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n2\r\nyz\r\n0\r\n\r\n",
            body.len()
        );
        for (reply, want) in [(plain, body.clone()), (chunked, format!("{body}yz"))] {
            let (_h, c) = serve(reply.leak().as_bytes()).await;
            let res = request(c, "GET", "/v1/file", None).await.unwrap();
            assert_eq!(String::from_utf8(res.body).unwrap(), want);
        }
    }

    #[tokio::test]
    async fn error_status_and_text_body() {
        let (_h, c) = serve(b"HTTP/1.1 409 Conflict\r\nContent-Length: 7\r\n\r\nrefused").await;
        let res = request(c, "POST", "/v1/lines/x/hold", None).await.unwrap();
        assert_eq!(res.head.status, 409);
        assert_eq!(json_body(&res.body), serde_json::json!("refused"));
    }

    #[tokio::test]
    async fn read_until_eof_and_empty_body() {
        let (_h, c) = serve(b"HTTP/1.1 200 OK\r\n\r\nhello").await;
        assert_eq!(request(c, "GET", "/", None).await.unwrap().body, b"hello");
        let (_h, c) = serve(b"HTTP/1.1 204 No Content\r\n\r\n").await;
        let res = request(c, "DELETE", "/v1/tasks/1", None).await.unwrap();
        assert_eq!(json_body(&res.body), serde_json::Value::Null);
    }

    #[tokio::test]
    async fn upload_streams_a_raw_body_with_explicit_length() {
        let (h, c) = serve(b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 51\r\n\r\n{\"path\":\"/tmp/u/a b.png\",\"name\":\"a b.png\",\"size\":3}").await;
        let data: Vec<u8> = (0..150_000u32).map(|i| (i % 251) as u8).collect();
        let mut seen = Vec::new();
        let res = upload(
            c,
            "/v1/uploads?name=a%20b.png",
            data.len() as u64,
            &data[..],
            Duration::from_secs(5),
            |n| seen.push(n),
        )
        .await
        .unwrap();
        assert_eq!(res.head.status, 201);
        assert_eq!(json_body(&res.body)["path"], "/tmp/u/a b.png");
        assert_eq!(seen, vec![65_536, 131_072, 150_000]);
        let req = h.await.unwrap();
        let split = req.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let head = String::from_utf8(req[..split].to_vec()).unwrap();
        assert!(head.starts_with(
            "POST /v1/uploads?name=a%20b.png HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n"
        ));
        assert!(head.contains("Content-Type: application/octet-stream\r\n"));
        assert!(head.ends_with("Content-Length: 150000"));
        assert!(!head.contains("application/json"));
        assert_eq!(&req[split + 4..], &data[..]);
    }

    #[tokio::test]
    async fn upload_fails_when_the_body_is_short() {
        let (_h, c) = serve(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n").await;
        let err = upload(
            c,
            "/v1/uploads?name=x",
            10,
            &b"abc"[..],
            Duration::from_millis(200),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn percent_round_trip() {
        assert_eq!(
            percent_encode("Capture d’écran 1.png"),
            "Capture%20d%E2%80%99%C3%A9cran%201.png"
        );
        assert_eq!(percent_encode("a/b?c&d=e"), "a%2Fb%3Fc%26d%3De");
        assert_eq!(
            percent_decode("Capture%20d%E2%80%99%C3%A9cran%201.png"),
            "Capture d’écran 1.png"
        );
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%4"), "%zz%4");
    }

    #[tokio::test]
    async fn malformed_status_is_an_error() {
        let (_h, c) = serve(b"garbage\r\n\r\n").await;
        assert!(request(c, "GET", "/", None).await.is_err());
    }

    #[test]
    fn error_field_reads_a_string_error_only() {
        let field = |v: serde_json::Value| error_field(&v);
        assert_eq!(
            field(serde_json::json!({"error": "no"})).as_deref(),
            Some("no")
        );
        assert_eq!(field(serde_json::json!({"error": 3})), None);
        assert_eq!(field(serde_json::json!({})), None);
        assert_eq!(field(serde_json::json!("plain")), None);
    }
}
