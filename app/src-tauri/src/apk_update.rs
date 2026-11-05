use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{ACCEPT, LOCATION};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::io::AsyncWriteExt;

use crate::direct::secrets::Secrets;
use crate::direct::Store;

pub const REPO: &str = "basilelt/overseer";
const TOKEN: &str = "github-token";
const APK_SUFFIX: &str = "_android.apk";
const DOWNLOADS: &str = "updates";
const PROGRESS_STEP: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ApkError {
    Rejected,
    Forbidden,
    RateLimited,
    NoAccess,
    Network { message: String },
    Checksum,
    Failed { message: String },
}

fn failed(e: impl ToString) -> ApkError {
    ApkError::Failed {
        message: e.to_string(),
    }
}

fn network(e: reqwest::Error) -> ApkError {
    ApkError::Network {
        message: e.without_url().to_string(),
    }
}

pub fn access_error(status: StatusCode, with_token: bool) -> Option<ApkError> {
    match status {
        s if s.is_success() => None,
        StatusCode::UNAUTHORIZED if with_token => Some(ApkError::Rejected),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS if with_token => {
            Some(ApkError::Forbidden)
        }
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Some(ApkError::RateLimited),
        StatusCode::NOT_FOUND if with_token => Some(ApkError::NoAccess),
        s => Some(failed(format!("GitHub answered {s}"))),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    html_url: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\']) && !name.starts_with('.')
}

pub fn pick_assets(assets: &[Asset]) -> Option<(&Asset, &Asset)> {
    let apk = assets
        .iter()
        .find(|a| a.name.ends_with(APK_SUFFIX) && plain_name(&a.name))?;
    let sum = format!("{}.sha256", apk.name);
    let sha = assets.iter().find(|a| a.name == sum)?;
    Some((apk, sha))
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

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
    version: String,
    apk: Asset,
    sha: Asset,
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
    has_token: bool,
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

pub(crate) fn client() -> Result<Client, ApkError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|c| reqwest::Certificate::from_der(c.as_ref()).ok());
    Client::builder()
        .user_agent("overseer-android-updater")
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .tls_certs_only(roots)
        .build()
        .map_err(failed)
}

fn with_bearer(req: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    match token {
        Some(t) => req.bearer_auth(t),
        None => req,
    }
}

const GITHUB_API: &str = "https://api.github.com";

pub(crate) async fn api(
    client: &Client,
    token: Option<&str>,
    path: &str,
) -> Result<Response, ApkError> {
    api_at(client, GITHUB_API, token, path).await
}

async fn api_at(
    client: &Client,
    base: &str,
    token: Option<&str>,
    path: &str,
) -> Result<Response, ApkError> {
    let req = client
        .get(format!("{base}{path}"))
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(Duration::from_secs(20));
    let res = with_bearer(req, token).send().await.map_err(network)?;
    access_error(res.status(), token.is_some()).map_or(Ok(res), Err)
}

pub(crate) async fn asset(
    client: &Client,
    token: Option<&str>,
    url: &str,
) -> Result<Response, ApkError> {
    let req = client
        .get(url)
        .header(ACCEPT, "application/octet-stream")
        .header("X-GitHub-Api-Version", "2022-11-28");
    let res = with_bearer(req, token).send().await.map_err(network)?;
    if !res.status().is_redirection() {
        return access_error(res.status(), token.is_some()).map_or(Ok(res), Err);
    }
    let location = res
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| failed("GitHub redirected the download without a location"))?;
    let target = res.url().join(location).map_err(failed)?;
    let res = client.get(target).send().await.map_err(network)?;
    match res.status() {
        s if s.is_success() => Ok(res),
        s => Err(failed(format!("The download answered {s}"))),
    }
}

fn secrets(app: &AppHandle) -> Result<Secrets, ApkError> {
    app.path()
        .app_data_dir()
        .map(|d| Store::new(d).secrets())
        .map_err(failed)
}

pub(crate) fn read_token(data: &Path) -> Result<Option<String>, ApkError> {
    let token = Store::new(data).secrets().get(TOKEN).map_err(failed)?;
    Ok(token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty()))
}

async fn stored_token(app: &AppHandle) -> Result<Option<String>, ApkError> {
    let dir = app.path().app_data_dir().map_err(failed)?;
    tokio::task::spawn_blocking(move || read_token(&dir))
        .await
        .map_err(failed)?
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
        has_token: stored_token(&app).await?.is_some(),
        supported: cfg!(target_os = "android"),
    })
}

#[tauri::command]
pub async fn apk_update_token_set(app: AppHandle, token: String) -> Result<(), ApkError> {
    let token = token.trim().to_string();
    if token.is_empty() || token.chars().any(|c| c.is_whitespace() || !c.is_ascii()) {
        return Err(ApkError::Rejected);
    }
    api(&client()?, Some(&token), &format!("/repos/{REPO}")).await?;
    let s = secrets(&app)?;
    tokio::task::spawn_blocking(move || s.set(TOKEN, &token))
        .await
        .map_err(failed)?
        .map_err(failed)
}

#[tauri::command]
pub async fn apk_update_token_clear(
    app: AppHandle,
    state: State<'_, ApkState>,
) -> Result<(), ApkError> {
    let s = secrets(&app)?;
    tokio::task::spawn_blocking(move || s.delete(TOKEN))
        .await
        .map_err(failed)?;
    *state.pending.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
pub async fn apk_update_check(
    app: AppHandle,
    state: State<'_, ApkState>,
) -> Result<CheckResult, ApkError> {
    crate::diag::info("updater", "checking for an APK update");
    let current = current_version(&app);
    let token = stored_token(&app).await?;
    let release: Release = api(
        &client()?,
        token.as_deref(),
        &format!("/repos/{REPO}/releases/latest"),
    )
    .await?
    .json()
    .await
    .map_err(network)?;
    let found = if is_newer(&release.tag_name, &current) {
        pick_assets(&release.assets)
    } else {
        None
    };
    let Some((apk, sha)) = found else {
        *state.pending.lock().unwrap() = None;
        if let Ok(dir) = downloads_dir(&app) {
            clean(&dir);
        }
        return Ok(CheckResult::None { current });
    };
    let version = release.tag_name.trim_start_matches('v').to_string();
    let mut pending = state.pending.lock().unwrap();
    let ready = pending
        .as_ref()
        .filter(|p| p.version == version && p.apk.name == apk.name)
        .and_then(|p| p.ready.clone())
        .filter(|p| p.is_file());
    let info = UpdateInfo {
        current,
        version: version.clone(),
        notes: release.body.unwrap_or_default(),
        date: release.published_at,
        release_url: release.html_url,
        size: apk.size,
        ready: ready.is_some(),
    };
    *pending = Some(Pending {
        version,
        apk: apk.clone(),
        sha: sha.clone(),
        ready,
    });
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
    let token = stored_token(&app).await?;
    let token = token.as_deref();
    let dir = downloads_dir(&app)?;
    clean(&dir);
    tokio::fs::create_dir_all(&dir).await.map_err(failed)?;
    let client = client()?;
    let sums = asset(&client, token, &pending.sha.url)
        .await?
        .text()
        .await
        .map_err(network)?;
    let expected = parse_sha256(&sums, &pending.apk.name)
        .ok_or_else(|| failed("The checksum file of the release is not valid"))?;
    let mut res = asset(&client, token, &pending.apk.url).await?;
    let total = res
        .content_length()
        .or((pending.apk.size > 0).then_some(pending.apk.size));
    let part = dir.join(format!("{}.part", pending.apk.name));
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
    if hex(&hasher.finalize()) != expected {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(ApkError::Checksum);
    }
    let apk = dir.join(&pending.apk.name);
    tokio::fs::rename(&part, &apk).await.map_err(failed)?;
    if let Some(p) = state
        .pending
        .lock()
        .unwrap()
        .as_mut()
        .filter(|p| p.version == pending.version)
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
pub(crate) mod testhttp {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    pub async fn serve_once(
        status: u16,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap();
            let reply = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
            sock.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).to_ascii_lowercase()
        });
        (base, task)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            url: format!("https://api.github.com/assets/{name}"),
            size: 1,
        }
    }

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
    fn picks_the_android_apk_and_its_checksum() {
        let assets = vec![
            asset("Overseer_0.9.0_aarch64.dmg"),
            asset("SHA256SUMS"),
            asset("Overseer_0.9.0_android.apk.sha256"),
            asset("Overseer_0.9.0_android.apk"),
            asset("latest.json"),
        ];
        let (apk, sha) = pick_assets(&assets).unwrap();
        assert_eq!(apk.name, "Overseer_0.9.0_android.apk");
        assert_eq!(sha.name, "Overseer_0.9.0_android.apk.sha256");
    }

    #[test]
    fn needs_both_the_apk_and_a_checksum() {
        assert!(pick_assets(&[asset("Overseer_0.9.0_android.apk")]).is_none());
        assert!(pick_assets(&[asset("Overseer_0.9.0_android.apk.sha256")]).is_none());
        assert!(pick_assets(&[
            asset("../evil_android.apk"),
            asset("../evil_android.apk.sha256")
        ])
        .is_none());
        assert!(pick_assets(&[]).is_none());
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

    #[test]
    fn maps_github_statuses_to_token_errors() {
        assert_eq!(access_error(StatusCode::OK, true), None);
        assert_eq!(access_error(StatusCode::OK, false), None);
        assert_eq!(
            access_error(StatusCode::UNAUTHORIZED, true),
            Some(ApkError::Rejected)
        );
        assert_eq!(
            access_error(StatusCode::FORBIDDEN, true),
            Some(ApkError::Forbidden)
        );
        assert_eq!(
            access_error(StatusCode::TOO_MANY_REQUESTS, true),
            Some(ApkError::Forbidden)
        );
        assert_eq!(
            access_error(StatusCode::NOT_FOUND, true),
            Some(ApkError::NoAccess)
        );
        assert!(matches!(
            access_error(StatusCode::BAD_GATEWAY, true),
            Some(ApkError::Failed { .. })
        ));
    }

    #[test]
    fn without_a_token_a_rate_limit_asks_for_one_and_other_errors_stay_generic() {
        assert_eq!(
            access_error(StatusCode::FORBIDDEN, false),
            Some(ApkError::RateLimited)
        );
        assert_eq!(
            access_error(StatusCode::TOO_MANY_REQUESTS, false),
            Some(ApkError::RateLimited)
        );
        assert!(matches!(
            access_error(StatusCode::UNAUTHORIZED, false),
            Some(ApkError::Failed { .. })
        ));
        assert!(matches!(
            access_error(StatusCode::NOT_FOUND, false),
            Some(ApkError::Failed { .. })
        ));
    }

    #[tokio::test]
    async fn api_sends_no_authorization_header_without_a_token() {
        let (base, req) = testhttp::serve_once(200, "{}").await;
        api_at(&client().unwrap(), &base, None, "/repos/x/releases/latest")
            .await
            .unwrap();
        let req = req.await.unwrap();
        assert!(req.starts_with("get /repos/x/releases/latest "), "{req}");
        assert!(!req.contains("authorization"), "{req}");
        assert!(req.contains("application/vnd.github+json"));
    }

    #[tokio::test]
    async fn api_sends_the_bearer_token_when_present() {
        let (base, req) = testhttp::serve_once(200, "{}").await;
        api_at(&client().unwrap(), &base, Some("ghp_abc"), "/x")
            .await
            .unwrap();
        assert!(req.await.unwrap().contains("authorization: bearer ghp_abc"));
    }

    #[tokio::test]
    async fn api_rate_limit_without_a_token_is_rate_limited() {
        let (base, _req) = testhttp::serve_once(403, "{}").await;
        let err = api_at(&client().unwrap(), &base, None, "/x")
            .await
            .unwrap_err();
        assert_eq!(err, ApkError::RateLimited);
    }

    #[tokio::test]
    async fn api_unauthorized_with_a_token_is_rejected() {
        let (base, _req) = testhttp::serve_once(401, "{}").await;
        let err = api_at(&client().unwrap(), &base, Some("bad"), "/x")
            .await
            .unwrap_err();
        assert_eq!(err, ApkError::Rejected);
    }

    #[tokio::test]
    async fn asset_download_sends_no_authorization_header_without_a_token() {
        let (base, req) = testhttp::serve_once(200, "bytes").await;
        let res = super::asset(
            &client().unwrap(),
            None,
            &format!("{base}/repos/x/releases/assets/1"),
        )
        .await
        .unwrap();
        assert_eq!(res.text().await.unwrap(), "bytes");
        let req = req.await.unwrap();
        assert!(req.contains("application/octet-stream"));
        assert!(!req.contains("authorization"), "{req}");
    }

    #[tokio::test]
    async fn asset_download_sends_the_bearer_token_when_present() {
        let (base, req) = testhttp::serve_once(200, "bytes").await;
        super::asset(&client().unwrap(), Some("ghp_abc"), &format!("{base}/a"))
            .await
            .unwrap();
        assert!(req.await.unwrap().contains("authorization: bearer ghp_abc"));
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
