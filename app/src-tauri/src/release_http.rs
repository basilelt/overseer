use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use serde::Serialize;

pub const REPO: &str = "basilelt/overseer";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ApkError {
    Network { message: String },
    Checksum,
    Failed { message: String },
}

pub(crate) fn failed(e: impl ToString) -> ApkError {
    ApkError::Failed {
        message: e.to_string(),
    }
}

pub(crate) fn network(e: reqwest::Error) -> ApkError {
    ApkError::Network {
        message: e.without_url().to_string(),
    }
}

pub(crate) fn asset_url(base: &str, version: &str, name: &str) -> String {
    format!("{base}/{REPO}/releases/download/v{version}/{name}")
}

pub(crate) fn status_error(status: StatusCode) -> Option<ApkError> {
    (!status.is_success()).then(|| failed(format!("GitHub answered {status}")))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn client() -> Result<Client, ApkError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok());
    Client::builder()
        .user_agent("overseer-android-updater")
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .tls_certs_only(roots)
        .build()
        .map_err(failed)
}

pub(crate) async fn get(client: &Client, url: &str) -> Result<Response, ApkError> {
    let res = client.get(url).send().await.map_err(network)?;
    status_error(res.status()).map_or(Ok(res), Err)
}

#[cfg(test)]
pub(crate) mod testhttp {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub type Reply<'a> = (u16, Vec<(&'static str, &'static str)>, &'a str);

    pub async fn serve_once(
        status: u16,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let (base, task) = serve_script(vec![(status, vec![], body)]).await;
        let task = tokio::spawn(async move { task.await.unwrap().remove(0) });
        (base, task)
    }

    pub async fn serve_script(
        replies: Vec<Reply<'_>>,
    ) -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let replies: Vec<(u16, Vec<_>, String)> = replies
            .into_iter()
            .map(|(s, h, b)| (s, h, b.to_string()))
            .collect();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            for (status, headers, body) in replies {
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap();
                let extra: String = headers
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\r\n"))
                    .collect();
                let reply = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                sock.write_all(reply.as_bytes()).await.unwrap();
                seen.push(String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase());
            }
            seen
        });
        (base, task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_serialize_with_a_lowercase_kind() {
        let json = |e: ApkError| serde_json::to_string(&e).unwrap();
        assert_eq!(
            json(ApkError::Network {
                message: "x".into()
            }),
            r#"{"kind":"network","message":"x"}"#
        );
        assert_eq!(json(ApkError::Checksum), r#"{"kind":"checksum"}"#);
        assert_eq!(json(failed("y")), r#"{"kind":"failed","message":"y"}"#);
    }

    #[tokio::test]
    async fn get_sends_no_authorization_and_returns_the_body() {
        let (base, req) = testhttp::serve_once(200, "bytes").await;
        let res = get(&client().unwrap(), &format!("{base}/x")).await.unwrap();
        assert_eq!(res.text().await.unwrap(), "bytes");
        let req = req.await.unwrap();
        assert!(req.starts_with("get /x "), "{req}");
        assert!(!req.contains("authorization"), "{req}");
    }

    #[tokio::test]
    async fn get_follows_the_github_redirects() {
        let (base, reqs) = testhttp::serve_script(vec![
            (302, vec![("location", "/second")], ""),
            (302, vec![("location", "/third")], ""),
            (200, vec![], "payload"),
        ])
        .await;
        let res = get(&client().unwrap(), &format!("{base}/first"))
            .await
            .unwrap();
        assert_eq!(res.text().await.unwrap(), "payload");
        let reqs = reqs.await.unwrap();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[1].starts_with("get /second ") && reqs[2].starts_with("get /third "));
        assert!(reqs.iter().all(|r| !r.contains("authorization")));
    }

    #[tokio::test]
    async fn get_maps_a_non_success_status_to_failed() {
        for status in [403u16, 404, 429, 500] {
            let (base, _req) = testhttp::serve_once(status, "{}").await;
            let err = get(&client().unwrap(), &format!("{base}/x"))
                .await
                .unwrap_err();
            let ApkError::Failed { message } = err else {
                panic!("{status}")
            };
            assert!(message.starts_with("GitHub answered "), "{message}");
            assert!(message.contains(&status.to_string()), "{message}");
        }
    }

    #[tokio::test]
    async fn an_unreachable_host_is_a_network_error() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let err = get(&client().unwrap(), &format!("{base}/x"))
            .await
            .unwrap_err();
        assert!(matches!(err, ApkError::Network { .. }), "{err:?}");
    }
}
