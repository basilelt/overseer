use std::path::Path;
use std::time::Duration;

use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub const REPO: &str = "basilelt/overseer";
pub const GITHUB: &str = "https://github.com";
pub const SUMS: &str = "SHA256SUMS";
pub const PROGRESS_STEP: u64 = 256 * 1024;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum CheckResult<T> {
    None { current: String },
    Available(T),
}

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

pub(crate) fn manifest_url(base: &str) -> String {
    format!("{base}/{REPO}/releases/latest/download/latest.json")
}

pub(crate) fn release_url(base: &str, version: &str) -> String {
    format!("{base}/{REPO}/releases/tag/v{version}")
}

pub(crate) fn status_error(status: StatusCode) -> Option<ApkError> {
    (!status.is_success()).then(|| failed(format!("GitHub answered {status}")))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_sha256_hex(word: &str) -> bool {
    word.len() == 64 && word.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn sums_first_match(text: &str, name: &str) -> Option<String> {
    text.lines()
        .find_map(|line| {
            let mut words = line.split_whitespace();
            let hash = words.next()?;
            (words.next()?.trim_start_matches('*') == name).then_some(hash)
        })
        .filter(|h| is_sha256_hex(h))
        .map(str::to_ascii_lowercase)
}

pub fn sums_valid_match(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let hash = words.next()?;
        let name = words.next()?.trim_start_matches('*');
        (name == file && is_sha256_hex(hash)).then(|| hash.to_ascii_lowercase())
    })
}

pub fn single_checksum(text: &str, file: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let hash = words.next()?;
    if !is_sha256_hex(hash) {
        return None;
    }
    match words.next() {
        Some(name) if name.trim_start_matches('*') != file => None,
        _ => Some(hash.to_ascii_lowercase()),
    }
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

pub(crate) async fn stream_to(
    client: &Client,
    url: &str,
    part: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<String, ApkError> {
    let mut res = get(client, url).await?;
    let total = res.content_length();
    let mut file = tokio::fs::File::create(part).await.map_err(failed)?;
    let mut hasher = Sha256::new();
    let (mut downloaded, mut reported) = (0u64, 0u64);
    progress(0, total);
    while let Some(chunk) = res.chunk().await.map_err(network)? {
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(failed)?;
        downloaded += chunk.len() as u64;
        if downloaded - reported >= PROGRESS_STEP || Some(downloaded) == total {
            reported = downloaded;
            progress(downloaded, total);
        }
    }
    file.flush().await.map_err(failed)?;
    Ok(hex(&hasher.finalize()))
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

    const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sums_first_match_reads_the_first_line_of_the_file_name() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!(
            "{a}  Overseer_0.9.0_android.apk\n{b}  overseer-0.9.0-x86_64-unknown-linux-musl\n{a} *overseer-0.9.0-aarch64-apple-darwin\n{a}  overseer-0.9.0-x86_64-unknown-linux-musl.sig\n"
        );
        assert_eq!(
            sums_first_match(&sums, "overseer-0.9.0-x86_64-unknown-linux-musl"),
            Some("b".repeat(64))
        );
        assert_eq!(
            sums_first_match(&sums, "overseer-0.9.0-aarch64-apple-darwin"),
            Some(a.clone())
        );
        assert_eq!(
            sums_first_match(&sums, "overseer-0.9.0-aarch64-unknown-linux-musl"),
            None
        );
        assert_eq!(
            sums_first_match(
                "xyz  overseer-0.9.0-x86_64-apple-darwin\n",
                "overseer-0.9.0-x86_64-apple-darwin"
            ),
            None
        );
        assert_eq!(sums_first_match("", "x"), None);
    }

    #[test]
    fn sums_valid_match_reads_one_file_from_sha256sums() {
        let sums = format!(
            "{}  Overseer_0.35.0_arm64.msix\n{SHA}  Overseer_0.35.0_x64.msix\n{} *other\n",
            "0".repeat(64),
            "1".repeat(64)
        );
        assert_eq!(
            sums_valid_match(&sums, "Overseer_0.35.0_x64.msix").as_deref(),
            Some(SHA)
        );
        assert_eq!(
            sums_valid_match(&sums, "other").as_deref(),
            Some("1".repeat(64).as_str())
        );
        assert_eq!(sums_valid_match(&sums, "Overseer_0.35.0_x64.msi"), None);
        assert_eq!(sums_valid_match("nothex  f\n", "f"), None);
    }

    #[test]
    fn the_two_sums_readers_differ_on_a_malformed_line_before_a_good_one() {
        let sums = format!("bad  f\n{SHA}  f\n");
        assert_eq!(sums_first_match(&sums, "f"), None);
        assert_eq!(sums_valid_match(&sums, "f").as_deref(), Some(SHA));
    }

    #[test]
    fn single_checksum_parses_sha256sum_output() {
        let sum = "A".repeat(64);
        let lower = "a".repeat(64);
        assert_eq!(
            single_checksum(
                &format!("{sum}  Overseer_0.9.0_android.apk\n"),
                "Overseer_0.9.0_android.apk"
            ),
            Some(lower.clone())
        );
        assert_eq!(
            single_checksum(
                &format!("{sum} *Overseer_0.9.0_android.apk"),
                "Overseer_0.9.0_android.apk"
            ),
            Some(lower.clone())
        );
        assert_eq!(single_checksum(&format!("{sum}\n"), "x.apk"), Some(lower));
        assert_eq!(
            single_checksum(&format!("{sum}  other.apk"), "Overseer_0.9.0_android.apk"),
            None
        );
        assert_eq!(single_checksum("abc  x.apk", "x.apk"), None);
        assert_eq!(
            single_checksum(&format!("{}  x.apk", "g".repeat(64)), "x.apk"),
            None
        );
        assert_eq!(single_checksum("", "x.apk"), None);
    }

    #[test]
    fn hashes_match_the_sha256sum_format() {
        let digest = Sha256::digest(b"abc");
        assert_eq!(hex(&digest), SHA);
        assert_eq!(
            single_checksum(&format!("{}  f.apk", hex(&digest)), "f.apk"),
            Some(hex(&digest))
        );
    }

    #[test]
    fn release_urls_are_built_from_the_base() {
        assert_eq!(
            manifest_url(GITHUB),
            "https://github.com/basilelt/overseer/releases/latest/download/latest.json"
        );
        assert_eq!(
            release_url(GITHUB, "0.27.0"),
            "https://github.com/basilelt/overseer/releases/tag/v0.27.0"
        );
        assert_eq!(
            asset_url(GITHUB, "0.9.0", SUMS),
            "https://github.com/basilelt/overseer/releases/download/v0.9.0/SHA256SUMS"
        );
    }

    #[test]
    fn check_results_serialize_with_a_lowercase_status() {
        let none = CheckResult::<u8>::None {
            current: "1".into(),
        };
        assert_eq!(
            serde_json::to_string(&none).unwrap(),
            r#"{"status":"none","current":"1"}"#
        );
        let some = CheckResult::Available(serde_json::json!({"version": "2"}));
        assert_eq!(
            serde_json::to_string(&some).unwrap(),
            r#"{"status":"available","version":"2"}"#
        );
    }

    #[tokio::test]
    async fn stream_to_reports_progress_and_returns_the_digest() {
        let (base, _req) = testhttp::serve_once(200, "abc").await;
        let dir = std::env::temp_dir().join(format!("sb-rh-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("x.part");
        let mut seen = Vec::new();
        let got = stream_to(&client().unwrap(), &format!("{base}/x"), &part, |d, t| {
            seen.push((d, t))
        })
        .await
        .unwrap();
        assert_eq!(got, SHA);
        assert_eq!(std::fs::read(&part).unwrap(), b"abc");
        assert_eq!(seen, vec![(0, Some(3)), (3, Some(3))]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn stream_to_a_missing_asset_creates_no_file() {
        let (base, _req) = testhttp::serve_once(404, "").await;
        let dir = std::env::temp_dir().join(format!("sb-rh-stream404-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let part = dir.join("x.part");
        let err = stream_to(&client().unwrap(), &format!("{base}/x"), &part, |_, _| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ApkError::Failed { .. }), "{err:?}");
        assert!(!part.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
