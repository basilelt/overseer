use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::io::AsyncWriteExt;

use crate::release_http::{
    asset_url, client, failed, get, hex, network, status_error, ApkError, REPO,
};

const GITHUB: &str = "https://github.com";
const DOWNLOADS: &str = "updates";
const PROGRESS_STEP: u64 = 256 * 1024;

pub(crate) fn manifest_url(base: &str) -> String {
    format!("{base}/{REPO}/releases/latest/download/latest.json")
}

pub(crate) fn release_url(base: &str, version: &str) -> String {
    format!("{base}/{REPO}/releases/tag/v{version}")
}

pub(crate) fn apk_name(version: &str) -> String {
    format!("Overseer_{version}_android.apk")
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    version: String,
    name: String,
    url: String,
    sha256: String,
    notes: String,
    date: Option<String>,
}

pub(crate) fn checksum_outcome(
    status: StatusCode,
    body: &str,
    file: &str,
) -> Result<Option<String>, ApkError> {
    if status == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if let Some(e) = status_error(status) {
        return Err(e);
    }
    parse_sha256(body, file)
        .map(Some)
        .ok_or_else(|| failed("The checksum file of the release is not valid"))
}

fn version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches('v')).ok()
}

pub fn is_newer(tag: &str, current: &str) -> bool {
    matches!((version(tag), version(current)), (Some(latest), Some(installed)) if latest > installed)
}

pub fn parse_sha256(text: &str, file: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    let hex = words.next()?;
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match words.next() {
        Some(name) if name.trim_start_matches('*') != file => None,
        _ => Some(hex.to_ascii_lowercase()),
    }
}

pub fn is_download(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "apk" || e == "part")
}

fn clean(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        if is_download(&path) {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone)]
struct Pending {
    found: Found,
    ready: Option<PathBuf>,
}

#[derive(Default)]
pub struct ApkState {
    pending: Mutex<Option<Pending>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApkStatus {
    current: String,
    supported: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum CheckResult {
    None { current: String },
    Available(UpdateInfo),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    version: String,
    notes: String,
    date: Option<String>,
    release_url: String,
    size: u64,
    ready: bool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum DownloadEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Verifying,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallOutcome {
    Started,
    Permission,
    Signature,
}

async fn find_update(
    client: &Client,
    base: &str,
    current: &str,
) -> Result<Option<Found>, ApkError> {
    let manifest: Manifest = get(client, &manifest_url(base))
        .await?
        .json()
        .await
        .map_err(network)?;
    let version = manifest.version.trim().trim_start_matches('v').to_string();
    if !is_newer(&version, current) {
        return Ok(None);
    }
    let name = apk_name(&version);
    let res = client
        .get(asset_url(base, &version, &format!("{name}.sha256")))
        .send()
        .await
        .map_err(network)?;
    let status = res.status();
    let body = if status.is_success() {
        res.text().await.map_err(network)?
    } else {
        String::new()
    };
    Ok(checksum_outcome(status, &body, &name)?.map(|sha256| Found {
        url: asset_url(base, &version, &name),
        name,
        sha256,
        notes: manifest.notes.unwrap_or_default(),
        date: manifest.pub_date,
        version,
    }))
}

fn downloads_dir(app: &AppHandle) -> Result<PathBuf, ApkError> {
    app.path()
        .app_cache_dir()
        .map(|d| d.join(DOWNLOADS))
        .map_err(failed)
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn apk_update_status(app: AppHandle) -> Result<ApkStatus, ApkError> {
    Ok(ApkStatus {
        current: current_version(&app),
        supported: cfg!(target_os = "android"),
    })
}

#[tauri::command]
pub async fn apk_update_check(
    app: AppHandle,
    state: State<'_, ApkState>,
) -> Result<CheckResult, ApkError> {
    crate::diag::info("updater", "checking for an APK update");
    let current = current_version(&app);
    let Some(found) = find_update(&client()?, GITHUB, &current).await? else {
        *state.pending.lock().unwrap() = None;
        if let Ok(dir) = downloads_dir(&app) {
            clean(&dir);
        }
        return Ok(CheckResult::None { current });
    };
    let mut pending = state.pending.lock().unwrap();
    let ready = pending
        .as_ref()
        .filter(|p| p.found.version == found.version && p.found.sha256 == found.sha256)
        .and_then(|p| p.ready.clone())
        .filter(|p| p.is_file());
    let info = UpdateInfo {
        current,
        version: found.version.clone(),
        notes: found.notes.clone(),
        date: found.date.clone(),
        release_url: release_url(GITHUB, &found.version),
        size: 0,
        ready: ready.is_some(),
    };
    *pending = Some(Pending { found, ready });
    Ok(CheckResult::Available(info))
}

#[tauri::command]
pub async fn apk_update_download(
    app: AppHandle,
    state: State<'_, ApkState>,
    on_progress: Channel<DownloadEvent>,
) -> Result<(), ApkError> {
    crate::diag::info("updater", "downloading the APK update");
    let pending = state
        .pending
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| failed("No update to download, check again"))?;
    let dir = downloads_dir(&app)?;
    clean(&dir);
    tokio::fs::create_dir_all(&dir).await.map_err(failed)?;
    let client = client()?;
    let found = pending.found;
    let mut res = get(&client, &found.url).await?;
    let total = res.content_length();
    let part = dir.join(format!("{}.part", found.name));
    let mut file = tokio::fs::File::create(&part).await.map_err(failed)?;
    let mut hasher = Sha256::new();
    let (mut downloaded, mut reported) = (0u64, 0u64);
    let _ = on_progress.send(DownloadEvent::Progress { downloaded, total });
    let streamed: Result<(), ApkError> = async {
        while let Some(chunk) = res.chunk().await.map_err(network)? {
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(failed)?;
            downloaded += chunk.len() as u64;
            if downloaded - reported >= PROGRESS_STEP || Some(downloaded) == total {
                reported = downloaded;
                let _ = on_progress.send(DownloadEvent::Progress { downloaded, total });
            }
        }
        file.flush().await.map_err(failed)
    }
    .await;
    drop(file);
    if let Err(e) = streamed {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e);
    }
    let _ = on_progress.send(DownloadEvent::Verifying);
    if hex(&hasher.finalize()) != found.sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(ApkError::Checksum);
    }
    let apk = dir.join(&found.name);
    tokio::fs::rename(&part, &apk).await.map_err(failed)?;
    if let Some(p) = state
        .pending
        .lock()
        .unwrap()
        .as_mut()
        .filter(|p| p.found.version == found.version)
    {
        p.ready = Some(apk);
    }
    Ok(())
}

#[tauri::command]
pub async fn apk_update_install(
    app: AppHandle,
    state: State<'_, ApkState>,
) -> Result<InstallOutcome, ApkError> {
    crate::diag::info("updater", "opening the Android installer");
    let path = state
        .pending
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|p| p.ready.clone())
        .filter(|p| p.is_file())
        .ok_or_else(|| failed("Download the update first"))?;
    installer::install(&app, &path)
}

#[tauri::command]
pub async fn apk_update_allow_install(app: AppHandle) -> Result<(), ApkError> {
    installer::open_settings(&app)
}

#[cfg(target_os = "android")]
pub use installer::plugin as installer_plugin;

#[cfg(target_os = "android")]
mod installer {
    use super::{failed, ApkError, InstallOutcome};
    use serde::{Deserialize, Serialize};
    use std::path::Path;
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{AppHandle, Manager, Wry};

    struct Installer(PluginHandle<Wry>);

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("apk-installer")
            .setup(|app, api| {
                let handle = api
                    .register_android_plugin("io.github.basilelt.overseer", "ApkInstallerPlugin")?;
                app.manage(Installer(handle));
                Ok(())
            })
            .build()
    }

    #[derive(Serialize)]
    struct Args {
        path: String,
    }

    #[derive(Deserialize)]
    struct Outcome {
        status: String,
    }

    pub fn install(app: &AppHandle, path: &Path) -> Result<InstallOutcome, ApkError> {
        let handle = &app.state::<Installer>().0;
        match handle.run_mobile_plugin::<Outcome>(
            "installApk",
            Args {
                path: path.to_string_lossy().into_owned(),
            },
        ) {
            Ok(o) if o.status == "permission" => Ok(InstallOutcome::Permission),
            Ok(_) => Ok(InstallOutcome::Started),
            Err(tauri::plugin::mobile::PluginInvokeError::InvokeRejected(e))
                if e.code.as_deref() == Some("signature") =>
            {
                Ok(InstallOutcome::Signature)
            }
            Err(tauri::plugin::mobile::PluginInvokeError::InvokeRejected(e)) => Err(failed(
                e.message
                    .unwrap_or_else(|| "The installer could not start".into()),
            )),
            Err(e) => Err(failed(e)),
        }
    }

    pub fn open_settings(app: &AppHandle) -> Result<(), ApkError> {
        let handle = &app.state::<Installer>().0;
        handle
            .run_mobile_plugin::<()>("openInstallSettings", ())
            .map_err(failed)
    }
}

#[cfg(not(target_os = "android"))]
mod installer {
    use super::{failed, ApkError, InstallOutcome};
    use std::path::Path;
    use tauri::AppHandle;

    const UNSUPPORTED: &str = "In-app updates are only for the Android APK";

    pub fn install(_: &AppHandle, _: &Path) -> Result<InstallOutcome, ApkError> {
        Err(failed(UNSUPPORTED))
    }

    pub fn open_settings(_: &AppHandle) -> Result<(), ApkError> {
        Err(failed(UNSUPPORTED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release_http::testhttp;

    #[test]
    fn compares_release_tags_as_semver() {
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("0.9.1", "0.9.0"));
        assert!(is_newer("v1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("v0.9.0", "0.9.0"));
        assert!(!is_newer("v0.8.9", "0.9.0"));
        assert!(!is_newer("nightly", "0.9.0"));
        assert!(!is_newer("v0.9.1", "garbage"));
    }

    #[test]
    fn parses_sha256sum_output() {
        let sum = "A".repeat(64);
        let lower = "a".repeat(64);
        assert_eq!(
            parse_sha256(
                &format!("{sum}  Overseer_0.9.0_android.apk\n"),
                "Overseer_0.9.0_android.apk"
            ),
            Some(lower.clone())
        );
        assert_eq!(
            parse_sha256(
                &format!("{sum} *Overseer_0.9.0_android.apk"),
                "Overseer_0.9.0_android.apk"
            ),
            Some(lower.clone())
        );
        assert_eq!(parse_sha256(&format!("{sum}\n"), "x.apk"), Some(lower));
        assert_eq!(
            parse_sha256(&format!("{sum}  other.apk"), "Overseer_0.9.0_android.apk"),
            None
        );
        assert_eq!(parse_sha256("abc  x.apk", "x.apk"), None);
        assert_eq!(
            parse_sha256(&format!("{}  x.apk", "g".repeat(64)), "x.apk"),
            None
        );
        assert_eq!(parse_sha256("", "x.apk"), None);
    }

    #[test]
    fn hashes_match_the_sha256sum_format() {
        let digest = Sha256::digest(b"abc");
        assert_eq!(
            hex(&digest),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            parse_sha256(&format!("{}  f.apk", hex(&digest)), "f.apk"),
            Some(hex(&digest))
        );
    }

    const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn builds_the_public_download_urls() {
        assert_eq!(
            manifest_url(GITHUB),
            "https://github.com/basilelt/overseer/releases/latest/download/latest.json"
        );
        assert_eq!(apk_name("0.27.0"), "Overseer_0.27.0_android.apk");
        assert_eq!(
            asset_url(GITHUB, "0.27.0", &apk_name("0.27.0")),
            "https://github.com/basilelt/overseer/releases/download/v0.27.0/Overseer_0.27.0_android.apk"
        );
        assert_eq!(
            asset_url(GITHUB, "0.27.0", "Overseer_0.27.0_android.apk.sha256"),
            "https://github.com/basilelt/overseer/releases/download/v0.27.0/Overseer_0.27.0_android.apk.sha256"
        );
        assert_eq!(
            release_url(GITHUB, "0.27.0"),
            "https://github.com/basilelt/overseer/releases/tag/v0.27.0"
        );
    }

    #[test]
    fn a_missing_checksum_means_no_update() {
        assert_eq!(
            checksum_outcome(StatusCode::NOT_FOUND, "", "a.apk"),
            Ok(None)
        );
    }

    #[test]
    fn a_published_checksum_is_kept_for_the_download() {
        assert_eq!(
            checksum_outcome(StatusCode::OK, &format!("{SHA}  a.apk\n"), "a.apk"),
            Ok(Some(SHA.to_string()))
        );
    }

    #[test]
    fn other_statuses_and_garbage_checksums_fail() {
        for s in [
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::BAD_GATEWAY,
        ] {
            assert_eq!(
                checksum_outcome(s, "", "a.apk"),
                Err(failed(format!("GitHub answered {s}")))
            );
        }
        assert!(matches!(
            checksum_outcome(StatusCode::OK, "nonsense", "a.apk"),
            Err(ApkError::Failed { .. })
        ));
    }

    fn manifest(version: &str) -> String {
        format!(
            r#"{{"version":"{version}","notes":"- fixed","pub_date":"2026-11-01T10:00:00Z","platforms":{{}}}}"#
        )
    }

    #[tokio::test]
    async fn finds_a_newer_release_with_its_checksum() {
        let sums = format!("{SHA}  Overseer_0.27.0_android.apk\n");
        let (base, reqs) = testhttp::serve_script(vec![
            (200, vec![], &manifest("0.27.0")),
            (200, vec![], &sums),
        ])
        .await;
        let found = find_update(&client().unwrap(), &base, "0.26.6")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.version, "0.27.0");
        assert_eq!(found.name, "Overseer_0.27.0_android.apk");
        assert_eq!(
            found.url,
            format!(
                "{base}/basilelt/overseer/releases/download/v0.27.0/Overseer_0.27.0_android.apk"
            )
        );
        assert_eq!(found.sha256, SHA);
        assert_eq!(found.notes, "- fixed");
        assert_eq!(found.date.as_deref(), Some("2026-11-01T10:00:00Z"));
        let reqs = reqs.await.unwrap();
        assert!(
            reqs[0].starts_with("get /basilelt/overseer/releases/latest/download/latest.json "),
            "{}",
            reqs[0]
        );
        assert!(
            reqs[1].starts_with(
                "get /basilelt/overseer/releases/download/v0.27.0/overseer_0.27.0_android.apk.sha256 "
            ),
            "{}",
            reqs[1]
        );
        assert!(reqs.iter().all(|r| !r.contains("authorization")));
    }

    #[tokio::test]
    async fn a_release_without_an_android_asset_is_no_update() {
        let (base, _reqs) = testhttp::serve_script(vec![
            (200, vec![], &manifest("0.27.0")),
            (404, vec![], "Not Found"),
        ])
        .await;
        assert_eq!(
            find_update(&client().unwrap(), &base, "0.26.6").await,
            Ok(None)
        );
    }

    #[tokio::test]
    async fn an_up_to_date_app_never_asks_for_the_checksum() {
        let (base, _reqs) = testhttp::serve_script(vec![(200, vec![], &manifest("0.26.6"))]).await;
        assert_eq!(
            find_update(&client().unwrap(), &base, "0.26.6").await,
            Ok(None)
        );
    }

    #[tokio::test]
    async fn a_failing_manifest_request_is_reported() {
        let (base, _reqs) = testhttp::serve_script(vec![(500, vec![], "")]).await;
        assert!(matches!(
            find_update(&client().unwrap(), &base, "0.26.6").await,
            Err(ApkError::Failed { .. })
        ));
    }

    #[test]
    fn cleans_only_downloaded_apks() {
        let dir = std::env::temp_dir().join(format!("sb-apk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for f in [
            "Overseer_0.8.0_android.apk",
            "Overseer_0.9.0_android.apk.part",
            "keep.txt",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        clean(&dir);
        let left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, vec!["keep.txt".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
