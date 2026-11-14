use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};

use crate::install::{Platform, SERVER_VERSION};
use crate::release_http::{
    asset_url, client, get, hex, network, signed, sums_first_match, ApkError, GITHUB, RELEASE_KEY,
    SUMS, SUMS_SIG,
};
use crate::ssh::{Fetch, FetchFuture};

pub fn asset_name(version: &str, platform: Platform) -> String {
    format!("overseer-{version}-{}", platform.target_triple())
}

pub fn cached(cache: &Path, version: &str, platform: Platform) -> PathBuf {
    cache
        .join("server")
        .join(platform.target_triple())
        .join(asset_name(version, platform))
}

fn keep_only(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn describe(e: ApkError, version: &str) -> String {
    match e {
        ApkError::Network { message } => {
            format!("cannot download the server from GitHub: {message}")
        }
        ApkError::Checksum => {
            format!("the downloaded server does not match the {SUMS} of release v{version}")
        }
        ApkError::Failed { message } => message,
    }
}

pub(crate) fn server_urls(base: &str, version: &str, platform: Platform) -> (String, String) {
    (
        asset_url(base, version, SUMS),
        asset_url(base, version, &asset_name(version, platform)),
    )
}

async fn from_github(version: &str, platform: Platform) -> Result<Vec<u8>, ApkError> {
    from_base(GITHUB, RELEASE_KEY, version, platform).await
}

fn unsigned(version: &str) -> ApkError {
    ApkError::Failed {
        message: format!(
            "the {SUMS} of release v{version} has no valid signature ({SUMS_SIG}), so the server was not installed"
        ),
    }
}

async fn from_base(
    base: &str,
    key: &str,
    version: &str,
    platform: Platform,
) -> Result<Vec<u8>, ApkError> {
    let client = client()?;
    let name = asset_name(version, platform);
    let (sums_url, bin_url) = server_urls(base, version, platform);
    let text = get(&client, &sums_url)
        .await?
        .text()
        .await
        .map_err(network)?;
    let signature = match get(&client, &asset_url(base, version, SUMS_SIG)).await {
        Ok(res) => res.text().await.map_err(network)?,
        Err(ApkError::Failed { .. }) => return Err(unsigned(version)),
        Err(e) => return Err(e),
    };
    if !signed(key, text.as_bytes(), &signature) {
        return Err(unsigned(version));
    }
    let want = sums_first_match(&text, &name).ok_or_else(|| ApkError::Failed {
        message: format!("release v{version} has no {name} line in {SUMS}"),
    })?;
    let bytes = get(&client, &bin_url)
        .await?
        .bytes()
        .await
        .map_err(network)?;
    if hex(&Sha256::digest(&bytes)) != want {
        return Err(ApkError::Checksum);
    }
    Ok(bytes.to_vec())
}

async fn store(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = file
        .parent()
        .ok_or_else(|| std::io::Error::other("no cache folder"))?;
    tokio::fs::create_dir_all(dir).await?;
    let part = dir.join(".download.part");
    tokio::fs::write(&part, bytes).await?;
    tokio::fs::rename(&part, file).await?;
    keep_only(dir, file);
    Ok(())
}

pub async fn download(cache: &Path, version: &str, platform: Platform) -> Result<Vec<u8>, String> {
    let file = cached(cache, version, platform);
    if let Ok(bytes) = tokio::fs::read(&file).await {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }
    let bytes = from_github(version, platform)
        .await
        .map_err(|e| describe(e, version))?;
    let _ = store(&file, &bytes).await;
    Ok(bytes)
}

pub fn source(cache: PathBuf) -> Arc<Fetch> {
    Arc::new(move |platform| -> FetchFuture {
        let cache = cache.clone();
        Box::pin(async move { download(&cache, SERVER_VERSION, platform).await })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release_http::testhttp::{TEST_KEY, TEST_SUMS, TEST_SUMS_SIG};

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-fetch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn maps_uname_to_release_assets() {
        let name = |uname: &str| asset_name("0.9.0", Platform::from_uname(uname).unwrap());
        assert_eq!(
            name("Linux x86_64"),
            "overseer-0.9.0-x86_64-unknown-linux-musl"
        );
        assert_eq!(
            name("Linux amd64"),
            "overseer-0.9.0-x86_64-unknown-linux-musl"
        );
        assert_eq!(
            name("Linux aarch64"),
            "overseer-0.9.0-aarch64-unknown-linux-musl"
        );
        assert_eq!(
            name("Linux arm64"),
            "overseer-0.9.0-aarch64-unknown-linux-musl"
        );
        assert_eq!(name("Darwin x86_64"), "overseer-0.9.0-x86_64-apple-darwin");
        assert_eq!(name("Darwin arm64"), "overseer-0.9.0-aarch64-apple-darwin");
    }

    #[tokio::test]
    async fn uses_the_cached_binary_and_keeps_one_version() {
        let d = dir("cache");
        let file = cached(&d, "0.9.0", Platform::LinuxAarch64);
        assert!(file.ends_with(
            "server/aarch64-unknown-linux-musl/overseer-0.9.0-aarch64-unknown-linux-musl"
        ));
        let old = cached(&d, "0.8.0", Platform::LinuxAarch64);
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, b"old").unwrap();
        store(&file, b"new").await.unwrap();
        assert!(!old.exists());
        assert_eq!(
            download(&d, "0.9.0", Platform::LinuxAarch64).await.unwrap(),
            b"new"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn builds_the_public_server_urls() {
        let (sums, bin) = server_urls("https://github.com", "0.9.0", Platform::LinuxX86_64);
        assert_eq!(
            sums,
            "https://github.com/basilelt/overseer/releases/download/v0.9.0/SHA256SUMS"
        );
        assert_eq!(
            bin,
            "https://github.com/basilelt/overseer/releases/download/v0.9.0/overseer-0.9.0-x86_64-unknown-linux-musl"
        );
    }

    #[test]
    fn errors_are_described_without_any_token_hint() {
        assert!(describe(ApkError::Checksum, "0.9.0").contains("v0.9.0"));
        assert!(describe(
            ApkError::Network {
                message: "dns".into()
            },
            "0.9.0"
        )
        .contains("dns"));
        assert_eq!(
            describe(
                ApkError::Failed {
                    message: "GitHub answered 404 Not Found".into()
                },
                "0.9.0"
            ),
            "GitHub answered 404 Not Found"
        );
    }

    const BIN: &str = "#!/bin/sh\necho hi\n";

    #[tokio::test]
    async fn downloads_and_verifies_the_server_without_authorization() {
        let name = asset_name("0.9.0", Platform::LinuxX86_64);
        assert_eq!(
            TEST_SUMS,
            format!("{}  {name}\n", hex(&Sha256::digest(BIN.as_bytes())))
        );
        let (base, reqs) = crate::release_http::testhttp::serve_script(vec![
            (200, vec![], TEST_SUMS),
            (200, vec![], TEST_SUMS_SIG),
            (302, vec![("location", "/cdn")], ""),
            (200, vec![], BIN),
        ])
        .await;
        let got = from_base(&base, TEST_KEY, "0.9.0", Platform::LinuxX86_64)
            .await
            .unwrap();
        assert_eq!(got, BIN.as_bytes());
        let reqs = reqs.await.unwrap();
        assert!(reqs[0].starts_with("get /basilelt/overseer/releases/download/v0.9.0/sha256sums "));
        assert!(
            reqs[1].starts_with("get /basilelt/overseer/releases/download/v0.9.0/sha256sums.sig ")
        );
        assert!(reqs[2].contains(&format!("/v0.9.0/{name} ")));
        assert!(reqs[3].starts_with("get /cdn "));
        assert!(reqs.iter().all(|r| !r.contains("authorization")));
    }

    #[tokio::test]
    async fn a_corrupt_server_download_fails_the_checksum() {
        let (base, _reqs) = crate::release_http::testhttp::serve_script(vec![
            (200, vec![], TEST_SUMS),
            (200, vec![], TEST_SUMS_SIG),
            (200, vec![], "tampered"),
        ])
        .await;
        assert_eq!(
            from_base(&base, TEST_KEY, "0.9.0", Platform::LinuxX86_64).await,
            Err(ApkError::Checksum)
        );
    }

    #[tokio::test]
    async fn checksums_without_a_valid_signature_are_refused_before_the_download() {
        let rewritten: &'static str = Box::leak(
            format!(
                "{}  {}\n",
                hex(&Sha256::digest(b"other")),
                asset_name("0.9.0", Platform::LinuxX86_64)
            )
            .into_boxed_str(),
        );
        let cases = [
            (TEST_KEY, TEST_SUMS, 404, "Not Found"),
            (TEST_KEY, rewritten, 200, TEST_SUMS_SIG),
            (RELEASE_KEY, TEST_SUMS, 200, TEST_SUMS_SIG),
        ];
        for (key, sums, status, signature) in cases {
            let (base, reqs) = crate::release_http::testhttp::serve_script(vec![
                (200, vec![], sums),
                (status, vec![], signature),
            ])
            .await;
            let err = from_base(&base, key, "0.9.0", Platform::LinuxX86_64)
                .await
                .unwrap_err();
            assert_eq!(err, unsigned("0.9.0"));
            assert!(describe(err, "0.9.0").contains("no valid signature (SHA256SUMS.sig)"));
            assert_eq!(reqs.await.unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn a_missing_release_reports_the_github_status() {
        let (base, _reqs) =
            crate::release_http::testhttp::serve_script(vec![(404, vec![], "Not Found")]).await;
        let err = from_base(&base, TEST_KEY, "9.9.9", Platform::LinuxX86_64)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ApkError::Failed { ref message } if message.contains("404")),
            "{err:?}"
        );
    }
}
