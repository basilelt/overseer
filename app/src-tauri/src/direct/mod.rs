pub mod client;
pub mod hosts;
#[cfg(test)]
mod it;
pub mod keys;
pub mod known;
pub mod secrets;

use crate::askpass;
use crate::conn::Reporter;
use client::{Ask, Asker, Passwords, Session};
use hosts::{Hosts, SavedHost};
use known::KnownHosts;
use russh::keys::PrivateKey;
use secrets::Secrets;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub const DEVICE_KEY: &str = "device-key";

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn hosts(&self) -> Hosts {
        Hosts::new(self.dir.join("hosts.json"))
    }

    pub fn known(&self) -> KnownHosts {
        KnownHosts::new(self.dir.join("known_hosts"))
    }

    pub fn secrets(&self) -> Secrets {
        Secrets::new(&self.dir)
    }

    pub fn device_key(&self) -> Result<PrivateKey, String> {
        let secrets = self.secrets();
        if let Some(pem) = secrets.get(DEVICE_KEY)? {
            return keys::from_pem(&pem);
        }
        let key = keys::generate(&keys::comment())?;
        secrets.set(DEVICE_KEY, &keys::to_pem(&key)?)?;
        Ok(key)
    }

    pub fn delete_host(&self, id: &str) -> Result<Vec<SavedHost>, String> {
        if let Some(h) = self.hosts().get(id) {
            self.secrets()
                .delete(&password_name(&hosts::describe(&h.endpoint())));
        }
        self.hosts().delete(id)
    }

    pub fn forget_host_key(&self, id: &str) -> Result<(), String> {
        let h = self.hosts().get(id).ok_or("unknown host")?;
        self.known().forget(&h.host, h.port)
    }
}

pub fn password_name(who: &str) -> String {
    format!("password:{who}")
}

struct PasswordCache {
    remember: bool,
    secrets: Secrets,
}

static TYPED: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

impl Passwords for PasswordCache {
    fn known(&self, who: &str) -> Option<String> {
        if let Some(p) = TYPED
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(who).cloned())
        {
            return Some(p);
        }
        if self.remember {
            return self.secrets.get(&password_name(who)).ok().flatten();
        }
        None
    }

    fn accepted(&self, who: &str, password: &str) {
        TYPED
            .lock()
            .unwrap()
            .get_or_insert_with(HashMap::new)
            .insert(who.to_string(), password.to_string());
        if self.remember {
            let _ = self.secrets.set(&password_name(who), password);
        }
    }

    fn rejected(&self, who: &str) {
        if let Some(m) = TYPED.lock().unwrap().as_mut() {
            m.remove(who);
        }
        self.secrets.delete(&password_name(who));
    }
}

pub fn prompt_for(ask: &Ask) -> (&'static str, String, Option<String>, Option<String>) {
    match ask {
        Ask::HostKey { host, fingerprint } => (
            "hostkey",
            format!("The authenticity of host '{host}' can't be established.\nKey fingerprint is {fingerprint}.\nTrust it and connect?"),
            Some(fingerprint.clone()),
            Some(host.clone()),
        ),
        Ask::Password { who } => ("secret", format!("Password for {who}:"), None, None),
    }
}

pub fn asker(rep: Arc<Reporter>) -> Asker {
    Arc::new(move |ask| {
        let rep = rep.clone();
        Box::pin(async move {
            let (kind, text, fp, host) = prompt_for(&ask);
            let (id, p, rx) = askpass::ask_as(kind, text, fp, host);
            rep.prompt(Some(p));
            let answer = rx.await.ok().flatten();
            askpass::forget(id);
            rep.prompt(None);
            answer
        })
    })
}

pub async fn connect(
    dir: &Path,
    cache: Option<&Path>,
    id: &str,
    rep: Arc<Reporter>,
    replace: bool,
    on_step: &(dyn Fn(&'static str, Option<String>) + Send + Sync),
) -> Result<(crate::ssh::Tunnel, String), String> {
    let store = Store::new(dir);
    let host = store
        .hosts()
        .get(id)
        .ok_or_else(|| format!("no saved host {id}"))?;
    let key = store.device_key()?;
    let session = Session {
        key: Arc::new(key),
        known: Arc::new(store.known()),
        ask: asker(rep),
        passwords: Arc::new(PasswordCache {
            remember: host.remember_password,
            secrets: store.secrets(),
        }),
        server: cache.map(|c| crate::server_fetch::source(c.to_path_buf())),
    };
    let described = host.describe();
    let t = client::connect(&host, &session, replace, on_step)
        .await
        .map_err(|e| format!("{e}\n({described})"))?;
    Ok((t, described))
}

fn store(app: &AppHandle) -> Result<Store, String> {
    app.path()
        .app_data_dir()
        .map(Store::new)
        .map_err(|e| format!("no app data folder: {e}"))
}

#[derive(Serialize)]
pub struct PlatformInfo {
    os: &'static str,
    mobile: bool,
}

#[tauri::command]
pub fn app_platform() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        mobile: cfg!(any(target_os = "android", target_os = "ios")),
    }
}

#[tauri::command]
pub async fn direct_device_key(app: AppHandle) -> Result<keys::DeviceKey, String> {
    let s = store(&app)?;
    tokio::task::spawn_blocking(move || s.device_key().map(|k| keys::describe(&k)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn direct_hosts(app: AppHandle) -> Result<Vec<SavedHost>, String> {
    Ok(store(&app)?.hosts().list())
}

#[tauri::command]
pub fn direct_host_save(app: AppHandle, host: SavedHost) -> Result<Vec<SavedHost>, String> {
    store(&app)?.hosts().save(host)
}

#[tauri::command]
pub fn direct_host_delete(app: AppHandle, id: String) -> Result<Vec<SavedHost>, String> {
    store(&app)?.delete_host(&id)
}

#[tauri::command]
pub fn direct_host_forget_key(app: AppHandle, id: String) -> Result<(), String> {
    store(&app)?.forget_host_key(&id)
}

#[cfg(mobile)]
#[tauri::command]
pub fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only http(s) links open outside the app".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_key_is_created_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("sb-direct-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Store::new(&dir);
        let a = s.device_key().unwrap();
        let b = Store::new(&dir).device_key().unwrap();
        assert_eq!(a.public_key(), b.public_key());
        let hosts = s
            .hosts()
            .save(SavedHost {
                host: "box".into(),
                user: "u".into(),
                port: 22,
                ..Default::default()
            })
            .unwrap();
        s.known().trust("box", 22, a.public_key()).unwrap();
        s.forget_host_key(&hosts[0].id).unwrap();
        assert_eq!(
            s.known().check("box", 22, a.public_key()),
            known::Verdict::Unknown
        );
        assert!(s.delete_host(&hosts[0].id).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompts_reuse_the_askpass_kinds() {
        let (kind, text, fp, host) = prompt_for(&Ask::HostKey {
            host: "[box]:2222".into(),
            fingerprint: "SHA256:abc".into(),
        });
        assert_eq!(
            (kind, fp.as_deref(), host.as_deref()),
            ("hostkey", Some("SHA256:abc"), Some("[box]:2222"))
        );
        assert_eq!(askpass::classify(&text, "").0, "hostkey");
        assert_eq!(
            prompt_for(&Ask::Password {
                who: "u@box:22".into()
            })
            .0,
            "secret"
        );
    }

    #[test]
    fn typed_passwords_are_kept_for_reconnects_and_dropped_when_refused() {
        let dir = std::env::temp_dir().join(format!("sb-direct-pw-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = PasswordCache {
            remember: false,
            secrets: Secrets::new(&dir),
        };
        assert_eq!(c.known("t@x:22"), None);
        c.accepted("t@x:22", "pw");
        assert_eq!(c.known("t@x:22").as_deref(), Some("pw"));
        assert!(!dir.join("secrets").exists());
        c.rejected("t@x:22");
        assert_eq!(c.known("t@x:22"), None);
        let r = PasswordCache {
            remember: true,
            secrets: Secrets::new(&dir),
        };
        r.accepted("r@x:22", "pw2");
        TYPED.lock().unwrap().as_mut().unwrap().clear();
        assert_eq!(r.known("r@x:22").as_deref(), Some("pw2"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
