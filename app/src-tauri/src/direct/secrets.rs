use std::path::PathBuf;

pub struct Secrets {
    dir: PathBuf,
}

impl Secrets {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, name: &str) -> PathBuf {
        self.dir
            .join("secrets")
            .join(name.replace(['/', '\\', ':'], "_"))
    }

    pub fn get(&self, name: &str) -> Result<Option<String>, String> {
        if let Some(v) = platform::get(name)? {
            return Ok(Some(v));
        }
        match std::fs::read_to_string(self.file(name)) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", self.file(name).display())),
        }
    }

    pub fn set(&self, name: &str, value: &str) -> Result<(), String> {
        if platform::set(name, value) {
            let _ = std::fs::remove_file(self.file(name));
            return Ok(());
        }
        let path = self.file(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        write_private(&path, value).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn delete(&self, name: &str) {
        platform::delete(name);
        let _ = std::fs::remove_file(self.file(name));
    }
}

#[cfg(unix)]
fn write_private(path: &std::path::Path, value: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(value.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(tmp, path)
}

#[cfg(not(unix))]
fn write_private(path: &std::path::Path, value: &str) -> std::io::Result<()> {
    std::fs::write(path, value)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod platform {
    use keyring_core::Entry;

    const SERVICE: &str = "io.github.basilelt.overseer";
    use std::sync::OnceLock;

    fn ready() -> bool {
        static READY: OnceLock<bool> = OnceLock::new();
        *READY.get_or_init(|| {
            #[cfg(target_os = "android")]
            let store = android_native_keyring_store::Store::new();
            #[cfg(target_os = "ios")]
            let store = apple_native_keyring_store::protected::Store::new();
            match store {
                Ok(s) => {
                    keyring_core::set_default_store(s);
                    true
                }
                Err(_) => false,
            }
        })
    }

    fn entry(name: &str) -> Option<Entry> {
        if !ready() {
            return None;
        }
        let mods = std::collections::HashMap::from([("access-policy", "after-first-unlock")]);
        if cfg!(target_os = "ios") {
            Entry::new_with_modifiers(SERVICE, name, &mods).ok()
        } else {
            Entry::new(SERVICE, name).ok()
        }
    }

    pub fn get(name: &str) -> Result<Option<String>, String> {
        Ok(entry(name)
            .and_then(|e| e.get_secret().ok())
            .map(|v| String::from_utf8_lossy(&v).into_owned()))
    }

    pub fn set(name: &str, value: &str) -> bool {
        entry(name).is_some_and(|e| e.set_secret(value.as_bytes()).is_ok())
    }

    pub fn delete(name: &str) {
        if let Some(e) = entry(name) {
            let _ = e.delete_credential();
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod platform {
    pub fn get(_: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn set(_: &str, _: &str) -> bool {
        false
    }

    pub fn delete(_: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_a_private_file_off_mobile() {
        let dir = std::env::temp_dir().join(format!("sb-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let s = Secrets::new(&dir);
        assert_eq!(s.get("device-key").unwrap(), None);
        s.set("device-key", "pem").unwrap();
        assert_eq!(s.get("device-key").unwrap().as_deref(), Some("pem"));
        s.set("password:a/b", "x").unwrap();
        assert!(dir.join("secrets/password_a_b").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("secrets/device-key"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        s.delete("device-key");
        assert_eq!(s.get("device-key").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
