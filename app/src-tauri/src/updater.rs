use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reqwest::header::ACCEPT;
use reqwest::Client;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::release_http::{
    asset_url, client, failed, get, manifest_url, network, release_url, stream_to,
    sums_valid_match, ApkError, CheckResult, GITHUB, REPO, SUMS,
};

#[cfg(any(target_os = "macos", test))]
const APP_BUNDLE: &str = "Applications/Overseer.app";

#[derive(Default)]
pub struct UpdaterState {
    pending: Mutex<Option<Update>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    Bundle,
    Msix,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    kind: InstallKind,
    releases_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    current: String,
    version: String,
    notes: String,
    date: Option<String>,
    kind: InstallKind,
    release_url: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum UpdateEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Installing,
}

pub fn install_kind(exe: &Path) -> InstallKind {
    if exe
        .to_string_lossy()
        .to_ascii_lowercase()
        .contains("\\windowsapps\\")
    {
        InstallKind::Msix
    } else {
        InstallKind::Bundle
    }
}

fn current_kind() -> InstallKind {
    install_kind(&std::env::current_exe().unwrap_or_default())
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

pub fn msix_name(version: &str, arch: &str) -> String {
    let arch = if arch == "aarch64" { "arm64" } else { "x64" };
    format!("Overseer_{version}_{arch}.msix")
}

#[tauri::command]
pub async fn update_status(app: AppHandle) -> UpdateStatus {
    UpdateStatus {
        current: current_version(&app),
        kind: current_kind(),
        releases_url: format!("{GITHUB}/{REPO}/releases"),
    }
}

#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, UpdaterState>,
) -> Result<CheckResult<UpdateInfo>, String> {
    let res = check_update(app, state).await;
    match &res {
        Ok(CheckResult::None { current }) => {
            crate::diag::info("updater", format!("up to date ({current})"))
        }
        Ok(CheckResult::Available(i)) => crate::diag::info(
            "updater",
            format!("update available: {} -> {}", i.current, i.version),
        ),
        Err(e) => crate::diag::warn("updater", format!("check failed: {e}")),
    }
    res
}

async fn check_update(
    app: AppHandle,
    state: State<'_, UpdaterState>,
) -> Result<CheckResult<UpdateInfo>, String> {
    let current = current_version(&app);
    let endpoint = reqwest::Url::parse(&manifest_url(GITHUB)).map_err(|e| e.to_string())?;
    let update = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .and_then(|b| b.header(ACCEPT, "application/octet-stream"))
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let Some(update) = update else {
        *state.pending.lock().unwrap() = None;
        return Ok(CheckResult::None { current });
    };
    let info = UpdateInfo {
        current,
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
        date: update
            .raw_json
            .get("pub_date")
            .and_then(|d| d.as_str())
            .map(str::to_string),
        kind: current_kind(),
        release_url: release_url(GITHUB, &update.version),
    };
    *state.pending.lock().unwrap() = Some(update);
    Ok(CheckResult::Available(info))
}

#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    state: State<'_, UpdaterState>,
    on_progress: Channel<UpdateEvent>,
) -> Result<(), String> {
    crate::diag::info("updater", "downloading and installing the update");
    let res = install_update(app, state, on_progress).await;
    match &res {
        Ok(()) => crate::diag::info("updater", "update installed"),
        Err(e) => crate::diag::warn("updater", format!("install failed: {e}")),
    }
    res
}

async fn install_update(
    app: AppHandle,
    state: State<'_, UpdaterState>,
    on_progress: Channel<UpdateEvent>,
) -> Result<(), String> {
    let update = state
        .pending
        .lock()
        .unwrap()
        .take()
        .ok_or("No update to install, check again")?;
    let result = match current_kind() {
        InstallKind::Msix => install_msix(&app, &update.version, &on_progress).await,
        InstallKind::Bundle => install_bundle(&update, &on_progress).await,
    };
    if result.is_err() {
        *state.pending.lock().unwrap() = Some(update);
    }
    result
}

async fn install_bundle(update: &Update, on_progress: &Channel<UpdateEvent>) -> Result<(), String> {
    let mut downloaded = 0u64;
    let installing = on_progress.clone();
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = on_progress.send(UpdateEvent::Progress { downloaded, total });
            },
            move || {
                if cfg!(windows) {
                    crate::diag::diag_session_end("installing update".into());
                }
                let _ = installing.send(UpdateEvent::Installing);
            },
        )
        .await
        .map_err(|e| e.to_string())
}

fn http_error(e: ApkError) -> String {
    match e {
        ApkError::Network { message } | ApkError::Failed { message } => message,
        ApkError::Checksum => "The download does not match the release checksum".into(),
    }
}

async fn install_msix(
    app: &AppHandle,
    version: &str,
    on_progress: &Channel<UpdateEvent>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("updates");
    let name = msix_name(version, std::env::consts::ARCH);
    let path = download_verified(
        &client().map_err(http_error)?,
        GITHUB,
        version,
        &name,
        &dir,
        |downloaded, total| {
            let _ = on_progress.send(UpdateEvent::Progress { downloaded, total });
        },
    )
    .await
    .map_err(http_error)?;
    let _ = on_progress.send(UpdateEvent::Installing);
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

pub async fn download_verified(
    client: &Client,
    base: &str,
    version: &str,
    name: &str,
    dir: &Path,
    progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, ApkError> {
    let sums = get(client, &asset_url(base, version, SUMS))
        .await?
        .text()
        .await
        .map_err(network)?;
    let want = sums_valid_match(&sums, name)
        .ok_or_else(|| failed(format!("{name} is not in the release checksums")))?;
    tokio::fs::create_dir_all(dir).await.map_err(failed)?;
    let part = dir.join(format!("{name}.part"));
    let streamed = stream_to(client, &asset_url(base, version, name), &part, progress).await;
    match streamed {
        Ok(got) if got == want => {
            let out = dir.join(name);
            tokio::fs::rename(&part, &out).await.map_err(failed)?;
            Ok(out)
        }
        Ok(_) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(ApkError::Checksum)
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(e)
        }
    }
}

#[cfg(any(target_os = "macos", test))]
pub fn bundle_path(home: Option<&Path>, is_dir: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    std::iter::once(Path::new("/").join(APP_BUNDLE))
        .chain(home.map(|h| h.join(APP_BUNDLE)))
        .find(|p| is_dir(p))
}

#[cfg(any(target_os = "macos", test))]
const RELAUNCH_WAIT: &str = r#"pid=$1; shift; i=0; while kill -0 "$pid" 2>/dev/null && [ $i -lt 300 ]; do sleep 0.1; i=$((i+1)); done; exec /usr/bin/open "$@""#;

#[cfg(any(target_os = "macos", test))]
pub fn app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .last()
        .map(Path::to_path_buf)
}

#[cfg(any(target_os = "macos", test))]
pub fn is_translocated(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == "AppTranslocation")
}

#[cfg(any(target_os = "macos", test))]
pub fn relaunch_target(
    exe: Option<&Path>,
    installed: Option<PathBuf>,
    is_dir: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    exe.and_then(app_bundle)
        .filter(|b| {
            !is_translocated(b) && !b.components().any(|c| c.as_os_str() == "Caskroom") && is_dir(b)
        })
        .or(installed)
}

#[cfg(any(target_os = "macos", test))]
pub fn relaunch_args(pid: u32, bundle: Option<&Path>, identifier: &str) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> = vec![
        "-c".into(),
        RELAUNCH_WAIT.into(),
        "sh".into(),
        pid.to_string().into(),
    ];
    match bundle {
        Some(b) => args.push(b.as_os_str().to_owned()),
        None => args.extend(["-b".into(), identifier.into()]),
    }
    args
}

#[cfg(target_os = "macos")]
fn relaunch_bundle(app: &AppHandle, reason: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().ok();
    if exe.as_deref().is_some_and(is_translocated) {
        crate::diag::warn(
            "updater",
            "running from an App Translocation copy: move Overseer.app to /Applications",
        );
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let bundle = relaunch_target(
        exe.as_deref(),
        bundle_path(home.as_deref(), Path::is_dir),
        Path::is_dir,
    );
    if let Some(b) = &bundle {
        let _ = std::process::Command::new("/usr/bin/xattr")
            .args(["-dr", "com.apple.quarantine"])
            .arg(b)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    let target = bundle
        .as_deref()
        .map_or_else(|| "by bundle id".to_string(), |b| b.display().to_string());
    crate::diag::info("updater", format!("relaunching {target}"));
    std::process::Command::new("/bin/sh")
        .args(relaunch_args(
            std::process::id(),
            bundle.as_deref(),
            &app.config().identifier,
        ))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    crate::diag::diag_session_end(reason.into());
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn update_relaunch(app: AppHandle, reason: Option<String>) -> Result<(), String> {
    let reason = reason.unwrap_or_else(|| "relaunch after update".into());
    #[cfg(target_os = "macos")]
    return relaunch_bundle(&app, &reason);
    #[cfg(not(target_os = "macos"))]
    {
        crate::diag::diag_session_end(reason);
        app.request_restart();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release_http::testhttp;

    #[test]
    fn app_bundle_location() {
        let home = Path::new("/Users/me");
        assert_eq!(
            bundle_path(Some(home), |_| true),
            Some(PathBuf::from("/Applications/Overseer.app"))
        );
        let user = |p: &Path| p == Path::new("/Users/me/Applications/Overseer.app");
        assert_eq!(
            bundle_path(Some(home), user),
            Some(PathBuf::from("/Users/me/Applications/Overseer.app"))
        );
        assert_eq!(bundle_path(None, user), None);
    }

    #[test]
    fn relaunch_targets_the_running_bundle() {
        let exe = Path::new("/Users/me/Apps/Overseer.app/Contents/MacOS/overseer-app");
        assert_eq!(
            app_bundle(exe),
            Some(PathBuf::from("/Users/me/Apps/Overseer.app"))
        );
        let helper = Path::new(
            "/Applications/Overseer.app/Contents/Frameworks/Helper.app/Contents/MacOS/helper",
        );
        assert_eq!(
            app_bundle(helper),
            Some(PathBuf::from("/Applications/Overseer.app"))
        );
        assert_eq!(app_bundle(Path::new("/usr/local/bin/overseer-app")), None);

        let installed = Some(PathBuf::from("/Applications/Overseer.app"));
        let any = |_: &Path| true;
        assert_eq!(
            relaunch_target(Some(exe), installed.clone(), any),
            Some(PathBuf::from("/Users/me/Apps/Overseer.app"))
        );
        assert_eq!(
            relaunch_target(Some(exe), installed.clone(), |_| false),
            installed
        );
        assert_eq!(relaunch_target(None, None, any), None);

        let moved = Path::new("/private/var/folders/x1/abc/T/AppTranslocation/0F2A-11/d/Overseer.app/Contents/MacOS/overseer-app");
        assert!(is_translocated(moved));
        assert!(!is_translocated(exe));
        assert_eq!(
            relaunch_target(Some(moved), installed.clone(), any),
            installed
        );
        assert_eq!(relaunch_target(Some(moved), None, any), None);

        let cask = Path::new(
            "/opt/homebrew/Caskroom/overseer/0.6.1/Overseer.app/Contents/MacOS/overseer-app",
        );
        assert_eq!(
            relaunch_target(Some(cask), installed.clone(), any),
            installed
        );
    }

    #[test]
    fn relaunch_waits_for_exit_then_opens_without_new_instance() {
        let bundle = Path::new("/Users/me/My Apps/Bob's Overseer.app");
        let args = relaunch_args(4242, Some(bundle), "io.github.basilelt.overseer");
        assert_eq!(args.len(), 5);
        assert_eq!(args[0], "-c");
        assert_eq!(args[2], "sh");
        assert_eq!(args[3], "4242");
        assert_eq!(args[4], bundle.as_os_str());
        let script = args[1].to_str().unwrap();
        assert!(script.contains("kill -0 \"$pid\""));
        assert!(script.contains("-lt 300"));
        assert!(script.ends_with("exec /usr/bin/open \"$@\""));
        assert!(!script.contains(" -n"));

        let by_id = relaunch_args(7, None, "io.github.basilelt.overseer");
        assert_eq!(by_id[4..], ["-b", "io.github.basilelt.overseer"]);
    }

    #[test]
    fn desktop_endpoint_is_the_public_latest_download() {
        assert_eq!(
            manifest_url(GITHUB),
            "https://github.com/basilelt/overseer/releases/latest/download/latest.json"
        );
    }

    #[test]
    fn only_a_windowsapps_install_is_msix() {
        for exe in [
            "/Applications/Overseer.app/Contents/MacOS/overseer-app",
            "/opt/homebrew/Caskroom/overseer/0.2.0/Overseer.app/Contents/MacOS/overseer-app",
            "/usr/bin/overseer-app",
            "/tmp/.mount_Overseer/usr/bin/overseer-app",
            "C:\\Users\\me\\AppData\\Local\\Overseer\\overseer-app.exe",
        ] {
            assert_eq!(install_kind(Path::new(exe)), InstallKind::Bundle, "{exe}");
        }
        assert_eq!(
            install_kind(Path::new(
                "C:\\Program Files\\WindowsApps\\io.github.basilelt.overseer_1.2.3.0_arm64__x\\Overseer.exe"
            )),
            InstallKind::Msix
        );
    }

    #[test]
    fn msix_asset_follows_the_release_names() {
        assert_eq!(msix_name("0.35.0", "x86_64"), "Overseer_0.35.0_x64.msix");
        assert_eq!(msix_name("0.35.0", "aarch64"), "Overseer_0.35.0_arm64.msix");
    }

    const SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-upd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[tokio::test]
    async fn download_verified_keeps_a_matching_file() {
        let sums = format!("{SHA}  pkg.msix\n");
        let (base, reqs) =
            testhttp::serve_script(vec![(200, vec![], &sums), (200, vec![], "abc")]).await;
        let dir = tmp("ok");
        let mut seen = Vec::new();
        let out = download_verified(
            &client().unwrap(),
            &base,
            "0.35.0",
            "pkg.msix",
            &dir,
            |d, _| seen.push(d),
        )
        .await
        .unwrap();
        assert_eq!(out, dir.join("pkg.msix"));
        assert_eq!(std::fs::read(&out).unwrap(), b"abc");
        assert_eq!(seen.last(), Some(&3));
        let reqs = reqs.await.unwrap();
        assert!(
            reqs[0].starts_with("get /basilelt/overseer/releases/download/v0.35.0/sha256sums "),
            "{}",
            reqs[0]
        );
        assert!(
            reqs[1].starts_with("get /basilelt/overseer/releases/download/v0.35.0/pkg.msix "),
            "{}",
            reqs[1]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn download_verified_rejects_a_tampered_file() {
        let sums = format!("{SHA}  pkg.msix\n");
        let (base, _r) =
            testhttp::serve_script(vec![(200, vec![], &sums), (200, vec![], "evil")]).await;
        let dir = tmp("bad");
        let err = download_verified(
            &client().unwrap(),
            &base,
            "0.35.0",
            "pkg.msix",
            &dir,
            |_, _| {},
        )
        .await;
        assert_eq!(err, Err(ApkError::Checksum));
        assert!(!dir.join("pkg.msix").exists() && !dir.join("pkg.msix.part").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn download_verified_fails_when_the_asset_is_not_listed() {
        let (base, _r) = testhttp::serve_script(vec![(200, vec![], "")]).await;
        let err = download_verified(
            &client().unwrap(),
            &base,
            "0.35.0",
            "pkg.msix",
            &tmp("missing"),
            |_, _| {},
        )
        .await;
        assert!(matches!(err, Err(ApkError::Failed { message }) if message.contains("pkg.msix")));
    }

    #[tokio::test]
    async fn download_verified_reports_a_missing_release() {
        let (base, _r) = testhttp::serve_script(vec![(404, vec![], "")]).await;
        let err = download_verified(
            &client().unwrap(),
            &base,
            "0.35.0",
            "pkg.msix",
            &tmp("404"),
            |_, _| {},
        )
        .await;
        assert!(matches!(err, Err(ApkError::Failed { message }) if message.contains("404")));
    }

    #[test]
    fn http_errors_read_as_text() {
        assert_eq!(
            http_error(ApkError::Checksum),
            "The download does not match the release checksum"
        );
        assert_eq!(http_error(failed("x")), "x");
    }
}
