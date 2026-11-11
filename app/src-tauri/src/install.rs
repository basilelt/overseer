use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const SERVER_VERSION: &str = env!("OVERSEER_SERVER_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    LinuxAarch64,
    DarwinX86_64,
    DarwinAarch64,
}

impl Platform {
    pub fn from_uname(uname: &str) -> Result<Self, String> {
        let mut it = uname.split_whitespace();
        let (os, arch) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
        let arm = matches!(arch, "aarch64" | "arm64" | "armv8l");
        let x86 = matches!(arch, "x86_64" | "amd64");
        match (os, x86, arm) {
            ("Linux", true, _) => Ok(Self::LinuxX86_64),
            ("Linux", _, true) => Ok(Self::LinuxAarch64),
            ("Darwin", true, _) => Ok(Self::DarwinX86_64),
            ("Darwin", _, true) => Ok(Self::DarwinAarch64),
            _ => Err(format!(
                "unsupported remote platform: {:?} (supported: Linux or macOS on x86_64/aarch64)",
                uname.trim()
            )),
        }
    }

    pub fn local() -> Option<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some(Self::LinuxX86_64),
            ("linux", "aarch64") => Some(Self::LinuxAarch64),
            ("macos", "x86_64") => Some(Self::DarwinX86_64),
            ("macos", "aarch64") => Some(Self::DarwinAarch64),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::DarwinX86_64 => "darwin-x86_64",
            Self::DarwinAarch64 => "darwin-aarch64",
        }
    }

    pub fn target_triple(self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "x86_64-unknown-linux-musl",
            Self::LinuxAarch64 => "aarch64-unknown-linux-musl",
            Self::DarwinX86_64 => "x86_64-apple-darwin",
            Self::DarwinAarch64 => "aarch64-apple-darwin",
        }
    }

    pub fn resource_path(self) -> String {
        format!("server/{}/overseer", self.name())
    }
}

pub fn needs_install(installed: Option<&str>, wanted: &str) -> bool {
    installed != Some(wanted)
}

fn version_key(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

pub fn older(running: &str, wanted: &str) -> bool {
    let (mut a, mut b) = (version_key(running), version_key(wanted));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a < b
}

pub fn wants_fetch(installed: Option<&str>, wanted: &str) -> bool {
    installed.is_none_or(|i| older(i, wanted))
}

pub fn should_upgrade(running: &str, wanted: &str, installed: Option<&str>) -> bool {
    installed == Some(wanted) && older(running, wanted)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerUpdate {
    pub running: String,
    pub installed: String,
    pub terms: usize,
    pub reason: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Fallback {
    Replace,
    Offer(ServerUpdate),
}

pub fn fallback(
    running: &str,
    wanted: &str,
    terms: Option<usize>,
    reason: &str,
    replaced: bool,
) -> Fallback {
    match terms {
        Some(0) if !replaced => Fallback::Replace,
        _ => Fallback::Offer(ServerUpdate {
            running: running.into(),
            installed: wanted.into(),
            terms: terms.unwrap_or(0),
            reason: reason.into(),
        }),
    }
}

pub fn live_terms(body: &serde_json::Value) -> Option<usize> {
    body.as_array().map(|a| {
        a.iter()
            .filter(|t| t.get("exited").and_then(|e| e.as_bool()) != Some(true))
            .count()
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn windows_resource(resource_dir: &Path, arch: &str) -> PathBuf {
    resource_dir
        .join("server")
        .join(format!("windows-{arch}"))
        .join("overseer.exe")
}

pub fn binary_candidates(
    platform: Platform,
    resource_dir: Option<&Path>,
    bin_dir: Option<&Path>,
    exe_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(r) = resource_dir {
        out.push(r.join(platform.resource_path()));
    }
    if let Some(b) = bin_dir {
        out.push(b.join(platform.name()).join("overseer"));
        out.push(b.join(format!("overseer-{}", platform.target_triple())));
        out.push(b.join(format!("overseer-{}", platform.name())));
    }
    if Platform::local() == Some(platform) {
        if let Some(e) = exe_dir {
            out.push(e.join("overseer"));
        }
    }
    out
}

/// An AppImage runs from a FUSE mount (`$APPDIR`, e.g. `/tmp/.mount_OverseXXXX`) that changes
/// on every launch and disappears when the app quits. A daemon started from the bundled binary
/// in there would leave the `~/.overseer/bin/overseer` shim (hooks, MCP, systemd unit, CLI)
/// pointing at a dead path. Copy that binary to the per-version server dir the daemon already
/// upgrades from (`~/.overseer/server/<version>/overseer`) and use the copy instead, so each app
/// update brings a stable daemon of its own version. Any other path, or a failed copy, is
/// returned unchanged.
pub fn stable_local_binary(
    bin: &Path,
    appdir: Option<&Path>,
    home: &Path,
    version: &str,
) -> PathBuf {
    let inside = appdir.is_some_and(|d| !d.as_os_str().is_empty() && bin.starts_with(d));
    if !inside || version.is_empty() || !home.is_absolute() {
        return bin.to_path_buf();
    }
    let dest = home.join(".overseer/server").join(version).join("overseer");
    match stage(bin, &dest) {
        Ok(()) => dest,
        Err(e) => {
            crate::diag::warn(
                "conn",
                format!(
                    "could not copy {} to {}: {e}",
                    bin.display(),
                    dest.display()
                ),
            );
            bin.to_path_buf()
        }
    }
}

fn stage(src: &Path, dest: &Path) -> std::io::Result<()> {
    let len = std::fs::metadata(src)?.len();
    if std::fs::metadata(dest).is_ok_and(|m| m.is_file() && m.len() == len) {
        return Ok(());
    }
    let dir = dest
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent dir"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".overseer.{}.tmp", std::process::id()));
    let res = (|| {
        std::fs::copy(src, &tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&tmp, dest)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ov-stage-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_binary_outside_the_appimage_mount_is_used_as_is() {
        let d = scratch("outside");
        let bin = d.join("overseer");
        std::fs::write(&bin, b"bin").unwrap();
        let home = d.join("home");
        assert_eq!(stable_local_binary(&bin, None, &home, "1.2.3"), bin);
        assert_eq!(
            stable_local_binary(&bin, Some(&d.join("other")), &home, "1.2.3"),
            bin
        );
        assert!(!home.exists(), "nothing is copied");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_binary_inside_the_appimage_mount_is_copied_to_the_versioned_server_dir() {
        let d = scratch("inside");
        let appdir = d.join(".mount_Overse");
        let bin = appdir.join("usr/lib/Overseer/server/linux-x86_64/overseer");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"daemon v1").unwrap();
        let home = d.join("home");
        let got = stable_local_binary(&bin, Some(&appdir), &home, "1.2.3");
        let want = home.join(".overseer/server/1.2.3/overseer");
        assert_eq!(got, want);
        assert_eq!(std::fs::read(&want).unwrap(), b"daemon v1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&want).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let leftovers: Vec<_> = std::fs::read_dir(want.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "no temp file left behind");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_existing_copy_is_kept_and_a_truncated_one_is_replaced() {
        let d = scratch("existing");
        let appdir = d.join(".mount_Overse");
        let bin = appdir.join("overseer");
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::write(&bin, b"daemon v2").unwrap();
        let home = d.join("home");
        let want = home.join(".overseer/server/2.0.0/overseer");
        std::fs::create_dir_all(want.parent().unwrap()).unwrap();
        std::fs::write(&want, b"same size").unwrap();
        assert_eq!(
            stable_local_binary(&bin, Some(&appdir), &home, "2.0.0"),
            want
        );
        assert_eq!(std::fs::read(&want).unwrap(), b"same size", "left alone");
        std::fs::write(&want, b"short").unwrap();
        stable_local_binary(&bin, Some(&appdir), &home, "2.0.0");
        assert_eq!(std::fs::read(&want).unwrap(), b"daemon v2");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_failed_copy_falls_back_to_the_bundled_binary() {
        let d = scratch("fail");
        let appdir = d.join(".mount_Overse");
        let bin = appdir.join("overseer");
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::write(&bin, b"daemon").unwrap();
        let home = d.join("home");
        std::fs::write(&home, b"a file, not a dir").unwrap();
        assert_eq!(
            stable_local_binary(&bin, Some(&appdir), &home, "1.0.0"),
            bin
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn maps_uname_to_platforms() {
        assert_eq!(
            Platform::from_uname("Linux x86_64\n").unwrap().name(),
            "linux-x86_64"
        );
        assert_eq!(
            Platform::from_uname("Linux aarch64")
                .unwrap()
                .target_triple(),
            "aarch64-unknown-linux-musl"
        );
        assert_eq!(
            Platform::from_uname("Darwin arm64").unwrap(),
            Platform::DarwinAarch64
        );
        assert_eq!(
            Platform::from_uname("Darwin x86_64")
                .unwrap()
                .resource_path(),
            "server/darwin-x86_64/overseer"
        );
        let err = Platform::from_uname("FreeBSD amd64").unwrap_err();
        assert!(err.contains("unsupported remote platform"));
        assert!(Platform::from_uname("Linux riscv64").is_err());
    }

    #[test]
    fn install_decision_and_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(needs_install(Some("0.0.9"), "0.1.0"));
        assert!(needs_install(None, "0.1.0"));
        assert!(!needs_install(Some("0.1.0"), "0.1.0"));
    }

    #[test]
    fn version_order_and_upgrade_decision() {
        assert!(older("0.7.0", "0.8.0"));
        assert!(older("0.9.9", "0.10.0"));
        assert!(older("0.8", "0.8.1"));
        assert!(older("v0.7.3", "0.8.0"));
        assert!(!older("0.8.0", "0.8.0"));
        assert!(!older("0.8.0-rc.1", "0.8.0"));
        assert!(!older("0.9.0", "0.8.0"));
        assert!(wants_fetch(None, "0.8.0"));
        assert!(wants_fetch(Some("0.7.9"), "0.8.0"));
        assert!(!wants_fetch(Some("0.8.0"), "0.8.0"));
        assert!(!wants_fetch(Some("0.9.0"), "0.8.0"));
        assert!(should_upgrade("0.7.0", "0.8.0", Some("0.8.0")));
        assert!(!should_upgrade("0.7.0", "0.8.0", Some("0.7.0")));
        assert!(!should_upgrade("0.7.0", "0.8.0", None));
        assert!(!should_upgrade("0.9.0", "0.8.0", Some("0.8.0")));
        assert!(!should_upgrade("0.8.0", "0.8.0", Some("0.8.0")));
    }

    #[test]
    fn fallback_replaces_an_idle_daemon_and_offers_otherwise() {
        assert_eq!(
            fallback("0.7.0", "0.8.0", Some(0), "no hot upgrade", false),
            Fallback::Replace
        );
        let offer = |terms, reason: &str| {
            Fallback::Offer(ServerUpdate {
                running: "0.7.0".into(),
                installed: "0.8.0".into(),
                terms,
                reason: reason.into(),
            })
        };
        assert_eq!(
            fallback("0.7.0", "0.8.0", Some(3), "no hot upgrade", false),
            offer(3, "no hot upgrade")
        );
        assert_eq!(
            fallback("0.7.0", "0.8.0", Some(0), "still 0.7.0", true),
            offer(0, "still 0.7.0")
        );
        assert_eq!(fallback("0.7.0", "0.8.0", None, "x", false), offer(0, "x"));
        let v = serde_json::json!([{"id": "a", "exited": false}, {"id": "b", "exited": true}, {"id": "c"}]);
        assert_eq!(live_terms(&v), Some(2));
        assert_eq!(live_terms(&serde_json::json!({"error": "x"})), None);
        let Fallback::Offer(u) = offer(2, "r") else {
            unreachable!()
        };
        assert_eq!(
            serde_json::to_value(u).unwrap(),
            serde_json::json!({"running": "0.7.0", "installed": "0.8.0", "terms": 2, "reason": "r"})
        );
    }

    #[test]
    fn binary_lookup_order() {
        let c = binary_candidates(
            Platform::LinuxAarch64,
            Some(Path::new("/res")),
            Some(Path::new("/bins")),
            Some(Path::new("/exe")),
        );
        assert_eq!(c[0], Path::new("/res/server/linux-aarch64/overseer"));
        assert_eq!(c[1], Path::new("/bins/linux-aarch64/overseer"));
        assert_eq!(c[2], Path::new("/bins/overseer-aarch64-unknown-linux-musl"));
        let same = binary_candidates(
            Platform::local().unwrap_or(Platform::LinuxX86_64),
            None,
            None,
            Some(Path::new("/exe")),
        );
        assert_eq!(same.len(), usize::from(Platform::local().is_some()));
    }

    #[test]
    fn windows_daemon_ships_in_resources_per_arch() {
        assert_eq!(
            windows_resource(Path::new("/res"), "x86_64"),
            Path::new("/res/server/windows-x86_64/overseer.exe")
        );
        assert_eq!(
            windows_resource(Path::new("/res"), "aarch64"),
            Path::new("/res/server/windows-aarch64/overseer.exe")
        );
    }
}
