use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::apk_update::{api, asset, client, hex, read_token, ApkError, Asset, REPO};
use crate::install::{Platform, SERVER_VERSION};
use crate::ssh::{Fetch, FetchFuture};

pub const SUMS: &str = "SHA256SUMS";
pub const RATE_LIMITED: &str = "GitHub's limit for downloads without a token is reached: add a GitHub token (Settings > Updates, GitHub access), try again later, or connect to this host once from the Overseer desktop app";

pub fn asset_name(version: &str, platform: Platform) -> String {
    format!("overseer-{version}-{}", platform.target_triple())
}

pub fn parse_sums(text: &str, name: &str) -> Option<String> {
    text.lines()
        .find_map(|line| {
            let mut words = line.split_whitespace();
            let hash = words.next()?;
            (words.next()?.trim_start_matches('*') == name).then_some(hash)
        })
        .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
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
        ApkError::RateLimited => RATE_LIMITED.into(),
        ApkError::Rejected | ApkError::Forbidden => "GitHub refused the saved GitHub token: check it in Settings > Updates (GitHub access)".into(),
        ApkError::NoAccess => format!("no release v{version} of {REPO} is visible with the saved GitHub token: check it in Settings > Updates (GitHub access)"),
        ApkError::Network { message } => format!("cannot download the server from GitHub: {message}"),
        ApkError::Checksum => format!("the downloaded server does not match the {SUMS} of release v{version}"),
        ApkError::Failed { message } => message,
    }
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    assets: Vec<Asset>,
}

async fn from_github(data: &Path, version: &str, platform: Platform) -> Result<Vec<u8>, ApkError> {
    let data = data.to_path_buf();
    let token = tokio::task::spawn_blocking(move || read_token(&data))
        .await
        .map_err(|e| ApkError::Failed {
            message: e.to_string(),
        })??;
    let token = token.as_deref();
    let client = client()?;
    let release: Release = api(
        &client,
        token,
        &format!("/repos/{REPO}/releases/tags/v{version}"),
    )
    .await?
    .json()
    .await
    .map_err(|e| ApkError::Network {
        message: e.without_url().to_string(),
    })?;
    let name = asset_name(version, platform);
    let missing = |what: &str| ApkError::Failed {
        message: format!("release v{version} has no {what}"),
    };
    let bin = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| missing(&name))?;
    let sums = release
        .assets
        .iter()
        .find(|a| a.name == SUMS)
        .ok_or_else(|| missing(SUMS))?;
    let network = |e: reqwest::Error| ApkError::Network {
        message: e.without_url().to_string(),
    };
    let text = asset(&client, token, &sums.url)
        .await?
        .text()
        .await
        .map_err(network)?;
    let want =
        parse_sums(&text, &name).ok_or_else(|| missing(&format!("{name} line in {SUMS}")))?;
    let bytes = asset(&client, token, &bin.url)
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

pub async fn download(
    data: &Path,
    cache: &Path,
    version: &str,
    platform: Platform,
) -> Result<Vec<u8>, String> {
    let file = cached(cache, version, platform);
    if let Ok(bytes) = tokio::fs::read(&file).await {
        if !bytes.is_empty() {
            return Ok(bytes);
        }
    }
    let bytes = from_github(data, version, platform)
        .await
        .map_err(|e| describe(e, version))?;
    let _ = store(&file, &bytes).await;
    Ok(bytes)
}

pub fn source(data: PathBuf, cache: PathBuf) -> Arc<Fetch> {
    Arc::new(move |platform| -> FetchFuture {
        let (data, cache) = (data.clone(), cache.clone());
        Box::pin(async move { download(&data, &cache, SERVER_VERSION, platform).await })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parses_release_sha256sums() {
        let a = "a".repeat(64);
        let b = "B".repeat(64);
        let sums = format!(
            "{a}  Overseer_0.9.0_android.apk\n{b}  overseer-0.9.0-x86_64-unknown-linux-musl\n{a} *overseer-0.9.0-aarch64-apple-darwin\n{a}  overseer-0.9.0-x86_64-unknown-linux-musl.sig\n"
        );
        assert_eq!(
            parse_sums(&sums, "overseer-0.9.0-x86_64-unknown-linux-musl"),
            Some("b".repeat(64))
        );
        assert_eq!(
            parse_sums(&sums, "overseer-0.9.0-aarch64-apple-darwin"),
            Some(a.clone())
        );
        assert_eq!(
            parse_sums(&sums, "overseer-0.9.0-aarch64-unknown-linux-musl"),
            None
        );
        assert_eq!(
            parse_sums(
                "xyz  overseer-0.9.0-x86_64-apple-darwin\n",
                "overseer-0.9.0-x86_64-apple-darwin"
            ),
            None
        );
        assert_eq!(parse_sums("", "x"), None);
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
            download(&d.join("data"), &d, "0.9.0", Platform::LinuxAarch64)
                .await
                .unwrap(),
            b"new"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn rate_limit_without_a_token_suggests_adding_one() {
        let err = describe(ApkError::RateLimited, "0.9.0");
        assert_eq!(err, RATE_LIMITED);
        assert!(err.contains("GitHub token") && err.contains("desktop"));
    }

    #[test]
    fn saved_token_errors_point_to_settings() {
        for e in [ApkError::Rejected, ApkError::Forbidden, ApkError::NoAccess] {
            assert!(describe(e, "0.9.0").contains("Settings > Updates"));
        }
    }
}
