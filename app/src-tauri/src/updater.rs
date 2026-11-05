use std::ffi::OsStr;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::io::{AsyncBufReadExt, BufReader};

pub const REPO: &str = "basilelt/overseer";
const ENV_TOKENS: [&str; 3] = [
    "OVERSEER_GITHUB_TOKEN",
    "HOMEBREW_GITHUB_API_TOKEN",
    "GITHUB_TOKEN",
];
const GH_BINS: [&str; 3] = ["gh", "/opt/homebrew/bin/gh", "/usr/local/bin/gh"];
const GH_TIMEOUT: Duration = Duration::from_secs(3);
const GITHUB_API: &str = "https://api.github.com";
const TOKEN_FILE: &str = "github-token";
const BREW_BINS: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];
const BREW_TAP: &str = "basilelt/tap";
const CASK: &str = "overseer";
#[cfg(any(target_os = "macos", test))]
const APP_BUNDLE: &str = "Applications/Overseer.app";
const AUTH_MARKERS: [&str; 8] = [
    "error: 401",
    "error: 403",
    "error: 404",
    "404 not found",
    "bad credentials",
    "authentication failed",
    "could not read username",
    "repository not found",
];

#[derive(Default)]
pub struct UpdaterState {
    pending: Mutex<Option<Update>>,
    brewing: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallKind {
    Bundle,
    Homebrew,
    Package,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    current: String,
    kind: InstallKind,
    token_source: Option<&'static str>,
    releases_url: String,
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
    kind: InstallKind,
    release_url: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub enum UpdateEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Installing,
    Step { name: &'static str },
    Line { text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BrewError {
    Missing,
    Untrusted { tap: String },
    Auth,
    Stale,
    Failed { message: String },
}

impl BrewError {
    fn failed(message: impl Into<String>) -> Self {
        BrewError::Failed {
            message: message.into(),
        }
    }
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub async fn resolve_token<E, G, F>(
    stored: Option<String>,
    env: E,
    gh: G,
) -> Option<(String, &'static str)>
where
    E: Fn(&str) -> Option<String>,
    G: FnOnce() -> F,
    F: Future<Output = Option<String>>,
{
    if let Some(t) = clean(stored) {
        return Some((t, "settings"));
    }
    for k in ENV_TOKENS {
        if let Some(t) = clean(env(k)) {
            return Some((t, k));
        }
    }
    clean(gh().await).map(|t| (t, "gh"))
}

async fn gh_token() -> Option<String> {
    for bin in GH_BINS {
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(["auth", "token", "--hostname", "github.com"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000);
        let Ok(child) = cmd.spawn() else { continue };
        return match tokio::time::timeout(GH_TIMEOUT, child.wait_with_output()).await {
            Ok(Ok(out)) if out.status.success() => String::from_utf8(out.stdout).ok(),
            _ => None,
        };
    }
    None
}

pub fn install_kind(
    os: &str,
    exe: &Path,
    caskrooms: &[PathBuf],
    is_dir: impl Fn(&Path) -> bool,
    appimage: bool,
) -> InstallKind {
    match os {
        "macos" => {
            let in_caskroom = exe.components().any(|c| c.as_os_str() == "Caskroom");
            if in_caskroom || caskrooms.iter().any(|r| is_dir(&r.join("overseer"))) {
                InstallKind::Homebrew
            } else {
                InstallKind::Bundle
            }
        }
        "linux" if !appimage => InstallKind::Package,
        "windows" if is_msix(exe) => InstallKind::Package,
        _ => InstallKind::Bundle,
    }
}

fn is_msix(exe: &Path) -> bool {
    exe.to_string_lossy()
        .to_ascii_lowercase()
        .contains("\\windowsapps\\")
}

fn caskrooms() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::var_os("HOMEBREW_PREFIX")
        .map(|p| PathBuf::from(p).join("Caskroom"))
        .into_iter()
        .collect();
    roots.push("/opt/homebrew/Caskroom".into());
    roots.push("/usr/local/Caskroom".into());
    roots
}

fn current_kind(app: &AppHandle) -> InstallKind {
    #[cfg(target_os = "linux")]
    let appimage = app.env().appimage.is_some();
    #[cfg(not(target_os = "linux"))]
    let appimage = {
        let _ = app;
        false
    };
    let exe = std::env::current_exe().unwrap_or_default();
    install_kind(
        std::env::consts::OS,
        &exe,
        &caskrooms(),
        Path::is_dir,
        appimage,
    )
}

fn token_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(TOKEN_FILE))
}

fn stored_token(app: &AppHandle) -> Option<String> {
    token_path(app).and_then(|p| std::fs::read_to_string(p).ok())
}

async fn token(app: &AppHandle) -> Option<(String, &'static str)> {
    resolve_token(stored_token(app), |k| std::env::var(k).ok(), gh_token).await
}

fn current_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn release_url(version: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    body: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    url: String,
}

fn bearer(token: Option<&str>) -> Option<String> {
    token.map(|t| format!("Bearer {t}"))
}

fn release_error(status: StatusCode, with_token: bool) -> Option<String> {
    match status {
        s if s.is_success() => None,
        StatusCode::UNAUTHORIZED if with_token => Some("GitHub rejected the token, check it in Settings > Updates".into()),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS if with_token => Some("GitHub refused the request: the rate limit is reached or the token has no access, try again later".into()),
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS => Some("GitHub's limit for requests without a token is reached: add a GitHub token in Settings > Updates, or try again later".into()),
        StatusCode::NOT_FOUND if with_token => Some(format!("No release of {REPO} is visible with this token")),
        StatusCode::NOT_FOUND => Some(format!("No release of {REPO} was found")),
        s => Some(format!("GitHub answered {s}")),
    }
}

async fn latest_release(api: &str, token: Option<&str>) -> Result<(reqwest::Url, String), String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = reqwest::Client::builder()
        .user_agent("overseer-updater")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client
        .get(format!("{api}/repos/{REPO}/releases/latest"))
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(h) = bearer(token) {
        req = req.header(AUTHORIZATION, h);
    }
    let res = req
        .send()
        .await
        .map_err(|e| format!("GitHub is unreachable: {}", e.without_url()))?;
    if let Some(e) = release_error(res.status(), token.is_some()) {
        return Err(e);
    }
    let release: Release = res.json().await.map_err(|e| e.to_string())?;
    let asset = release
        .assets
        .into_iter()
        .find(|a| a.name == "latest.json")
        .ok_or("The latest release has no latest.json")?;
    let url = reqwest::Url::parse(&asset.url).map_err(|e| e.to_string())?;
    Ok((url, release.body.unwrap_or_default()))
}

pub fn release_notes(manifest: Option<&str>, release: &str) -> String {
    manifest
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| release.trim())
        .to_string()
}

#[tauri::command]
pub async fn update_status(app: AppHandle) -> UpdateStatus {
    UpdateStatus {
        current: current_version(&app),
        kind: current_kind(&app),
        token_source: token(&app).await.map(|(_, s)| s),
        releases_url: format!("https://github.com/{REPO}/releases"),
    }
}

#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, UpdaterState>,
) -> Result<CheckResult, String> {
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
) -> Result<CheckResult, String> {
    let current = current_version(&app);
    let token = token(&app).await.map(|(t, _)| t);
    let (endpoint, body) = latest_release(GITHUB_API, token.as_deref()).await?;
    let mut builder = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|e| e.to_string())?;
    if let Some(h) = bearer(token.as_deref()) {
        builder = builder
            .header(AUTHORIZATION, h)
            .map_err(|e| e.to_string())?;
    }
    let update = builder
        .header(ACCEPT, "application/octet-stream")
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
        notes: release_notes(update.body.as_deref(), &body),
        date: update
            .raw_json
            .get("pub_date")
            .and_then(|d| d.as_str())
            .map(str::to_string),
        kind: current_kind(&app),
        release_url: release_url(&update.version),
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
    if current_kind(&app) != InstallKind::Bundle {
        return Err("This installation is updated by its package manager".into());
    }
    let update = state
        .pending
        .lock()
        .unwrap()
        .take()
        .ok_or("No update to install, check again")?;
    let mut downloaded = 0u64;
    let installing = on_progress.clone();
    let result = update
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
        .await;
    if let Err(e) = result {
        *state.pending.lock().unwrap() = Some(update);
        return Err(e.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn update_token_set(app: AppHandle, token: String) -> Result<(), String> {
    let path = token_path(&app).ok_or("No config folder")?;
    let token = token.trim();
    if token.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    write_private(&path, token).map_err(|e| e.to_string())
}

fn write_private(path: &Path, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?.write_all(data.as_bytes())
}

pub fn brew_path(path_env: Option<&OsStr>, is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    BREW_BINS
        .iter()
        .map(PathBuf::from)
        .chain(
            path_env
                .into_iter()
                .flat_map(std::env::split_paths)
                .map(|d| d.join("brew")),
        )
        .find(|p| is_file(p))
}

fn valid_tap(tap: &str) -> bool {
    let part = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    };
    matches!(tap.split_once('/'), Some((user, repo)) if part(user) && part(repo))
}

fn untrusted_tap(log: &str) -> String {
    log.split("brew trust ")
        .skip(1)
        .filter_map(|rest| rest.split_whitespace().next())
        .map(|t| {
            t.trim_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_')
            })
            .to_lowercase()
        })
        .find(|t| valid_tap(t))
        .unwrap_or_else(|| BREW_TAP.to_string())
}

fn last_error(log: &str) -> String {
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let line = lines
        .iter()
        .rfind(|l| l.starts_with("Error:"))
        .or(lines.last())
        .copied()
        .unwrap_or("Homebrew failed");
    line.chars().take(400).collect()
}

pub fn classify(log: &str, ok: bool) -> Result<(), BrewError> {
    let lower = log.to_lowercase();
    if lower.contains("untrusted") || lower.contains("not trusted") || lower.contains("brew trust ")
    {
        return Err(BrewError::Untrusted {
            tap: untrusted_tap(log),
        });
    }
    if ok {
        return Ok(());
    }
    if AUTH_MARKERS.iter().any(|m| lower.contains(m)) {
        return Err(BrewError::Auth);
    }
    Err(BrewError::failed(last_error(log)))
}

fn semver(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches('v')).ok()
}

pub fn installed_version(list: &str) -> Option<&str> {
    let versions = list
        .lines()
        .find_map(|l| l.trim().strip_prefix(CASK).filter(|r| r.starts_with(' ')))?
        .split_whitespace();
    versions
        .clone()
        .filter_map(|v| semver(v).map(|s| (s, v)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| v)
        .or_else(|| versions.last())
}

pub fn brew_outcome(
    target: Option<&str>,
    list: &str,
    log: &str,
) -> Result<Option<String>, BrewError> {
    let installed = installed_version(list);
    let upgraded = log.to_lowercase().contains("was successfully upgraded");
    let current = match (target, installed) {
        (None, _) => true,
        (Some(t), Some(i)) => match (semver(t), semver(i)) {
            (Some(t), Some(i)) => i >= t,
            _ => i == t,
        },
        (Some(_), None) => false,
    };
    let older = matches!((target.and_then(semver), installed.and_then(semver)), (Some(t), Some(i)) if i < t);
    if current || (upgraded && !older) {
        Ok(installed.map(str::to_string))
    } else {
        Err(BrewError::Stale)
    }
}

#[cfg(any(target_os = "macos", test))]
pub fn bundle_path(home: Option<&Path>, is_dir: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    std::iter::once(Path::new("/").join(APP_BUNDLE))
        .chain(home.map(|h| h.join(APP_BUNDLE)))
        .find(|p| is_dir(p))
}

fn scrub(line: &str, token: &str) -> String {
    let line = line
        .split('\r')
        .map(str::trim_end)
        .rfind(|s| !s.is_empty())
        .unwrap_or("");
    if token.len() >= 8 {
        line.replace(token, "***")
    } else {
        line.to_string()
    }
}

fn brew_command(brew: &Path, args: &[&str], token: Option<&str>) -> tokio::process::Command {
    let mut dirs: Vec<PathBuf> = brew.parent().map(Path::to_path_buf).into_iter().collect();
    let inherited = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    let defaults = BREW_BINS
        .iter()
        .filter_map(|b| Path::new(b).parent().map(Path::to_path_buf));
    let system = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(PathBuf::from);
    for d in defaults.chain(inherited).chain(system) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    let mut cmd = tokio::process::Command::new(brew);
    cmd.args(args)
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_COLOR", "1")
        .env("HOMEBREW_NO_EMOJI", "1")
        .env("NO_COLOR", "1")
        .env("NONINTERACTIVE", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("HOMEBREW_NO_AUTO_UPDATE")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(t) = token {
        cmd.env("HOMEBREW_GITHUB_API_TOKEN", t);
    }
    if let Ok(path) = std::env::join_paths(dirs) {
        cmd.env("PATH", path);
    }
    if args.first() != Some(&"update") {
        cmd.env("HOMEBREW_NO_AUTO_UPDATE", "1");
    }
    cmd
}

fn push_line(log: &mut String, line: &str, token: &str, out: Option<&Channel<UpdateEvent>>) {
    let line = scrub(line, token);
    if line.is_empty() {
        return;
    }
    log.push_str(&line);
    log.push('\n');
    if let Some(ch) = out {
        let _ = ch.send(UpdateEvent::Line { text: line });
    }
}

async fn run_brew(
    brew: &Path,
    args: &[&str],
    token: Option<&str>,
    out: Option<&Channel<UpdateEvent>>,
) -> (bool, String) {
    let mut child = match brew_command(brew, args, token).spawn() {
        Ok(c) => c,
        Err(e) => return (false, format!("Error: {e}")),
    };
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return (false, "Error: brew has no output".into());
    };
    let (mut stdout, mut stderr) = (
        BufReader::new(stdout).lines(),
        BufReader::new(stderr).lines(),
    );
    let (mut o, mut e) = (true, true);
    let mut log = String::new();
    while o || e {
        tokio::select! {
            l = stdout.next_line(), if o => match l {
                Ok(Some(l)) => push_line(&mut log, &l, token.unwrap_or(""), out),
                _ => o = false,
            },
            l = stderr.next_line(), if e => match l {
                Ok(Some(l)) => push_line(&mut log, &l, token.unwrap_or(""), out),
                _ => e = false,
            },
        }
    }
    let ok = child.wait().await.map(|s| s.success()).unwrap_or(false);
    (ok, log)
}

struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn update_brew(
    app: AppHandle,
    state: State<'_, UpdaterState>,
    trust: Option<String>,
    on_progress: Channel<UpdateEvent>,
) -> Result<Option<String>, BrewError> {
    crate::diag::info("updater", "updating through Homebrew");
    let res = brew_update(app, state, trust, on_progress).await;
    match &res {
        Ok(v) => crate::diag::info(
            "updater",
            format!(
                "Homebrew update done ({})",
                v.as_deref().unwrap_or("unknown version")
            ),
        ),
        Err(e) => crate::diag::warn("updater", format!("Homebrew update failed: {e:?}")),
    }
    res
}

async fn brew_update(
    app: AppHandle,
    state: State<'_, UpdaterState>,
    trust: Option<String>,
    on_progress: Channel<UpdateEvent>,
) -> Result<Option<String>, BrewError> {
    if current_kind(&app) != InstallKind::Homebrew {
        return Err(BrewError::failed(
            "This installation is not managed by Homebrew",
        ));
    }
    let brew =
        brew_path(std::env::var_os("PATH").as_deref(), Path::is_file).ok_or(BrewError::Missing)?;
    let token = token(&app).await.map(|(t, _)| t);
    let token = token.as_deref();
    if state.brewing.swap(true, Ordering::SeqCst) {
        return Err(BrewError::failed("An update is already running"));
    }
    let _busy = Busy(&state.brewing);
    let target = state
        .pending
        .lock()
        .unwrap()
        .as_ref()
        .map(|u| u.version.clone());
    let out = Some(&on_progress);
    let step = |name: &'static str| {
        let _ = on_progress.send(UpdateEvent::Step { name });
    };

    if let Some(tap) = trust {
        if tap != BREW_TAP {
            return Err(BrewError::failed(format!(
                "Overseer only trusts {BREW_TAP}, run `brew trust {tap}` yourself"
            )));
        }
        step("trust");
        let (ok, log) = run_brew(&brew, &["trust", tap.as_str()], token, out).await;
        if !ok {
            return Err(BrewError::failed(last_error(&log)));
        }
    }

    step("update");
    let (ok, log) = run_brew(&brew, &["update"], token, out).await;
    if let Err(e @ BrewError::Untrusted { .. }) = classify(&log, ok) {
        return Err(e);
    }

    step("upgrade");
    let (ok, upgrade_log) = run_brew(&brew, &["upgrade", "--cask", CASK], token, out).await;
    classify(&upgrade_log, ok)?;

    if run_brew(&brew, &["list", "--formula", CASK], token, None)
        .await
        .0
    {
        step("formula");
        run_brew(&brew, &["upgrade", "--formula", CASK], token, out).await;
    }

    let (_, list) = run_brew(&brew, &["list", "--cask", "--versions", CASK], token, None).await;
    brew_outcome(target.as_deref(), &list, &upgrade_log)
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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

#[tauri::command]
pub async fn update_brew_relaunch(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return relaunch_bundle(&app, "relaunch after Homebrew update");
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Homebrew updates are macOS only".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            vars.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    async fn resolve(
        stored: Option<&str>,
        vars: &[(&str, &str)],
        gh: Option<&str>,
    ) -> Option<(String, &'static str)> {
        let gh = gh.map(str::to_string);
        resolve_token(stored.map(str::to_string), env(vars), || async move { gh }).await
    }

    #[tokio::test]
    async fn token_resolution_order() {
        let all = [
            ("OVERSEER_GITHUB_TOKEN", "s"),
            ("HOMEBREW_GITHUB_API_TOKEN", "h"),
            ("GITHUB_TOKEN", "g"),
        ];
        assert_eq!(
            resolve(Some("set"), &all, Some("cli")).await,
            Some(("set".into(), "settings"))
        );
        assert_eq!(
            resolve(None, &all, Some("cli")).await,
            Some(("s".into(), "OVERSEER_GITHUB_TOKEN"))
        );
        assert_eq!(
            resolve(None, &all[1..], Some("cli")).await,
            Some(("h".into(), "HOMEBREW_GITHUB_API_TOKEN"))
        );
        assert_eq!(
            resolve(None, &all[2..], Some("cli")).await,
            Some(("g".into(), "GITHUB_TOKEN"))
        );
        assert_eq!(
            resolve(None, &[], Some("cli\n")).await,
            Some(("cli".into(), "gh"))
        );
        assert_eq!(resolve(None, &[], None).await, None);
    }

    #[tokio::test]
    async fn blank_values_fall_through() {
        let vars = [("OVERSEER_GITHUB_TOKEN", "  "), ("GITHUB_TOKEN", "g")];
        assert_eq!(
            resolve(Some("\n"), &vars, None).await,
            Some(("g".into(), "GITHUB_TOKEN"))
        );
        assert_eq!(resolve(None, &[], Some("")).await, None);
    }

    #[tokio::test]
    async fn gh_runs_only_as_last_resort() {
        let called = std::cell::Cell::new(false);
        let r = resolve_token(None, env(&[("GITHUB_TOKEN", "g")]), || {
            called.set(true);
            async { Some("cli".to_string()) }
        })
        .await;
        assert_eq!(r.map(|(_, s)| s), Some("GITHUB_TOKEN"));
        assert!(!called.get());
    }

    #[test]
    fn homebrew_detection() {
        let roots = [
            PathBuf::from("/opt/homebrew/Caskroom"),
            PathBuf::from("/usr/local/Caskroom"),
        ];
        let none = |_: &Path| false;
        let intel = |p: &Path| p == Path::new("/usr/local/Caskroom/overseer");
        let app = Path::new("/Applications/Overseer.app/Contents/MacOS/overseer-app");
        let cask = Path::new(
            "/opt/homebrew/Caskroom/overseer/0.2.0/Overseer.app/Contents/MacOS/overseer-app",
        );
        assert_eq!(
            install_kind("macos", app, &roots, none, false),
            InstallKind::Bundle
        );
        assert_eq!(
            install_kind("macos", cask, &roots, none, false),
            InstallKind::Homebrew
        );
        assert_eq!(
            install_kind("macos", app, &roots, intel, false),
            InstallKind::Homebrew
        );
        assert_eq!(
            install_kind("macos", app, &[], intel, false),
            InstallKind::Bundle
        );
    }

    #[test]
    fn brew_path_prefers_known_prefixes_then_path() {
        let path = std::env::join_paths(["/nix/bin", "/custom/bin"]).unwrap();
        let only = |want: &'static str| move |p: &Path| p == Path::new(want);
        assert_eq!(
            brew_path(Some(&path), |_| true),
            Some(PathBuf::from("/opt/homebrew/bin/brew"))
        );
        assert_eq!(
            brew_path(Some(&path), only("/usr/local/bin/brew")),
            Some(PathBuf::from("/usr/local/bin/brew"))
        );
        assert_eq!(
            brew_path(Some(&path), only("/custom/bin/brew")),
            Some(PathBuf::from("/custom/bin/brew"))
        );
        assert_eq!(brew_path(None, only("/custom/bin/brew")), None);
        assert_eq!(brew_path(Some(&path), |_| false), None);
    }

    #[test]
    fn brew_success() {
        let log = "==> Upgrading 1 outdated package:\noverseer 0.6.1 -> 0.6.2\n==> Moving App 'Overseer.app' to '/Applications/Overseer.app'\n🍺  overseer was successfully upgraded!\n";
        assert_eq!(classify(log, true), Ok(()));
    }

    #[test]
    fn brew_untrusted_tap() {
        let log = "Error: Refusing to load untrusted tap basilelt/tap.\nRun `brew trust basilelt/tap` to trust it.\n";
        assert_eq!(
            classify(log, false),
            Err(BrewError::Untrusted {
                tap: "basilelt/tap".into()
            })
        );
        let other = "Warning: tap Other/Tap is not trusted. To trust it: brew trust Other/Tap\n";
        assert_eq!(
            classify(other, true),
            Err(BrewError::Untrusted {
                tap: "other/tap".into()
            })
        );
        assert_eq!(
            classify("Error: untrusted tap\n", false),
            Err(BrewError::Untrusted {
                tap: BREW_TAP.into()
            })
        );
        assert_eq!(untrusted_tap("brew trust `evil;rm -rf`"), BREW_TAP);
    }

    #[test]
    fn brew_missing_token_or_404() {
        let log = "==> Downloading https://api.github.com/repos/basilelt/overseer/releases/assets/1\ncurl: (22) The requested URL returned error: 404\nError: Download failed on Cask 'overseer' with message: Download failed\n";
        assert_eq!(classify(log, false), Err(BrewError::Auth));
        let git = "fatal: could not read Username for 'https://github.com': terminal prompts disabled\nError: Fetching /opt/homebrew/Library/Taps/basilelt/homebrew-tap failed!\n";
        assert_eq!(classify(git, false), Err(BrewError::Auth));
    }

    #[test]
    fn brew_other_failures_keep_the_error_line() {
        let log = "==> Upgrading overseer\nError: It seems there is already an App at '/Applications/Overseer.app'.\nsome trailing noise\n";
        assert_eq!(
            classify(log, false),
            Err(BrewError::failed(
                "Error: It seems there is already an App at '/Applications/Overseer.app'."
            ))
        );
        assert_eq!(
            classify("\n  last words  \n", false),
            Err(BrewError::failed("last words"))
        );
        assert_eq!(
            classify("", false),
            Err(BrewError::failed("Homebrew failed"))
        );
    }

    #[test]
    fn installed_cask_version() {
        assert_eq!(installed_version("overseer 0.6.2\n"), Some("0.6.2"));
        assert_eq!(installed_version("overseer 0.6.1 0.6.2\n"), Some("0.6.2"));
        assert_eq!(
            installed_version("overseer 0.11.1 0.10.0\n"),
            Some("0.11.1")
        );
        assert_eq!(
            installed_version("Warning: something\noverseer 0.12.0\n"),
            Some("0.12.0")
        );
        assert_eq!(installed_version("overseer latest\n"), Some("latest"));
        assert_eq!(installed_version("overseer-nightly 1.0\n"), None);
        assert_eq!(installed_version(""), None);
    }

    const UPGRADED: &str = "==> Upgrading overseer\noverseer 0.10.0 -> 0.11.1\n🍺  overseer was successfully upgraded!\n";

    #[test]
    fn brew_outcome_newer_than_offered() {
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer 0.11.1\n", UPGRADED),
            Ok(Some("0.11.1".into()))
        );
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer 0.12.0\n", ""),
            Ok(Some("0.12.0".into()))
        );
        assert_eq!(
            brew_outcome(Some("0.9.0"), "overseer 0.10.0\n", ""),
            Ok(Some("0.10.0".into()))
        );
    }

    #[test]
    fn brew_outcome_same_as_offered() {
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer 0.11.0\n", ""),
            Ok(Some("0.11.0".into()))
        );
        assert_eq!(
            brew_outcome(None, "overseer 0.11.0\n", ""),
            Ok(Some("0.11.0".into()))
        );
        assert_eq!(brew_outcome(None, "", ""), Ok(None));
    }

    #[test]
    fn brew_outcome_still_older() {
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer 0.10.0\n", ""),
            Err(BrewError::Stale)
        );
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer 0.10.5\n", UPGRADED),
            Err(BrewError::Stale)
        );
        assert_eq!(brew_outcome(Some("0.11.0"), "", ""), Err(BrewError::Stale));
    }

    #[test]
    fn brew_outcome_trusts_the_success_line() {
        assert_eq!(brew_outcome(Some("0.11.0"), "", UPGRADED), Ok(None));
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer latest\n", UPGRADED),
            Ok(Some("latest".into()))
        );
        assert_eq!(
            brew_outcome(Some("0.11.0"), "overseer latest\n", ""),
            Err(BrewError::Stale)
        );
    }

    #[test]
    fn notes_fall_back_to_the_release_body() {
        assert_eq!(release_notes(Some("- manifest"), "- body"), "- manifest");
        assert_eq!(release_notes(Some("  \n"), "- body\n"), "- body");
        assert_eq!(release_notes(None, "- body"), "- body");
        assert_eq!(release_notes(None, ""), "");
    }

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
    fn bearer_header_only_with_a_token() {
        assert_eq!(bearer(None), None);
        assert_eq!(bearer(Some("ghp_x")), Some("Bearer ghp_x".to_string()));
    }

    #[test]
    fn release_errors_depend_on_whether_a_token_was_sent() {
        assert_eq!(release_error(StatusCode::OK, false), None);
        assert!(release_error(StatusCode::UNAUTHORIZED, true)
            .unwrap()
            .contains("rejected the token"));
        assert!(release_error(StatusCode::UNAUTHORIZED, false)
            .unwrap()
            .contains("401"));
        let limited = release_error(StatusCode::FORBIDDEN, false).unwrap();
        assert!(limited.contains("without a token") && limited.contains("add a GitHub token"));
        assert_eq!(
            release_error(StatusCode::TOO_MANY_REQUESTS, false),
            Some(limited)
        );
        assert!(!release_error(StatusCode::FORBIDDEN, true)
            .unwrap()
            .contains("add a GitHub token"));
        assert!(release_error(StatusCode::NOT_FOUND, true)
            .unwrap()
            .contains("with this token"));
        assert!(!release_error(StatusCode::NOT_FOUND, false)
            .unwrap()
            .contains("token"));
    }

    const RELEASE_JSON: &str = r#"{"body":"notes","assets":[{"name":"x.dmg","url":"http://h/1"},{"name":"latest.json","url":"http://h/2"}]}"#;

    #[tokio::test]
    async fn latest_release_works_without_a_token_and_sends_no_authorization() {
        let (base, req) = crate::apk_update::testhttp::serve_once(200, RELEASE_JSON).await;
        let (url, body) = latest_release(&base, None).await.unwrap();
        assert_eq!(url.as_str(), "http://h/2");
        assert_eq!(body, "notes");
        let req = req.await.unwrap();
        assert!(
            req.starts_with("get /repos/basilelt/overseer/releases/latest "),
            "{req}"
        );
        assert!(!req.contains("authorization"), "{req}");
    }

    #[tokio::test]
    async fn latest_release_sends_the_token_when_present() {
        let (base, req) = crate::apk_update::testhttp::serve_once(200, RELEASE_JSON).await;
        latest_release(&base, Some("ghp_abc")).await.unwrap();
        assert!(req.await.unwrap().contains("authorization: bearer ghp_abc"));
    }

    #[tokio::test]
    async fn latest_release_rate_limit_without_a_token_suggests_adding_one() {
        let (base, _req) = crate::apk_update::testhttp::serve_once(403, "{}").await;
        let err = latest_release(&base, None).await.unwrap_err();
        assert!(err.contains("add a GitHub token"), "{err}");
    }

    #[tokio::test]
    async fn latest_release_unauthorized_with_a_token_is_reported() {
        let (base, _req) = crate::apk_update::testhttp::serve_once(401, "{}").await;
        let err = latest_release(&base, Some("bad")).await.unwrap_err();
        assert!(err.contains("rejected the token"), "{err}");
    }

    #[test]
    fn brew_gets_the_token_env_only_when_there_is_a_token() {
        let env = |token: Option<&str>| {
            let cmd = brew_command(Path::new("/opt/homebrew/bin/brew"), &["update"], token);
            cmd.as_std()
                .get_envs()
                .find(|(k, _)| *k == "HOMEBREW_GITHUB_API_TOKEN")
                .map(|(_, v)| v.map(|v| v.to_string_lossy().into_owned()))
        };
        assert_eq!(env(Some("ghp_x")), Some(Some("ghp_x".to_string())));
        assert_eq!(env(None), None);
    }

    #[test]
    fn token_never_reaches_the_log() {
        assert_eq!(
            scrub("Authorization: Bearer ghp_secret123", "ghp_secret123"),
            "Authorization: Bearer ***"
        );
        assert_eq!(
            scrub("###   10%\r######  50%\r", "ghp_secret123"),
            "######  50%"
        );
    }

    #[test]
    fn linux_and_windows_kinds() {
        let exe = Path::new("/usr/bin/overseer-app");
        let any = |_: &Path| true;
        assert_eq!(
            install_kind("linux", exe, &[], any, true),
            InstallKind::Bundle
        );
        assert_eq!(
            install_kind("linux", exe, &[], any, false),
            InstallKind::Package
        );
        assert_eq!(
            install_kind(
                "windows",
                Path::new("C:\\Overseer\\overseer-app.exe"),
                &[],
                any,
                false
            ),
            InstallKind::Bundle
        );
        assert_eq!(
            install_kind(
                "windows",
                Path::new(
                    "C:\\Program Files\\WindowsApps\\io.github.basilelt.overseer_1.2.3.0_arm64__x\\Overseer.exe"
                ),
                &[],
                any,
                false
            ),
            InstallKind::Package
        );
    }
}
