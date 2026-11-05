use serde::Serialize;
use std::process::{Command, Stdio};
use tokio::sync::OnceCell;

const URL: &str = "\u{0}url";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Browser {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    launch: Vec<String>,
}

static FOUND: OnceCell<Vec<Browser>> = OnceCell::const_new();

async fn found() -> &'static [Browser] {
    FOUND
        .get_or_init(|| async {
            tokio::task::spawn_blocking(detect)
                .await
                .unwrap_or_default()
        })
        .await
}

#[tauri::command]
pub async fn browsers_list() -> Vec<Browser> {
    found().await.to_vec()
}

#[tauri::command]
pub async fn browser_open_with(id: String, url: String) -> Result<(), String> {
    let url = crate::browser::parse_url(&url)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http(s) links open in another browser".into());
    }
    let b = found()
        .await
        .iter()
        .find(|b| b.id == id)
        .ok_or_else(|| format!("browser {id:?} is not installed"))?;
    launch(&b.launch, url.as_str())
}

fn argv(template: &[String], url: &str) -> Vec<String> {
    let mut args: Vec<String> = template.iter().map(|a| a.replace(URL, url)).collect();
    if !template.iter().any(|a| a.contains(URL)) {
        args.push(url.to_string());
    }
    args
}

fn launch(template: &[String], url: &str) -> Result<(), String> {
    let args = argv(template, url);
    let (prog, rest) = args.split_first().ok_or("empty browser command")?;
    let mut cmd = Command::new(prog);
    #[cfg(all(unix, not(target_os = "macos")))]
    for (k, v) in appimage_env(|k| std::env::var(k).ok()) {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    let mut child = cmd
        .args(rest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{prog}: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn xdg_open_outside_appimage(url: &str) -> Option<Result<(), String>> {
    if std::env::var_os("APPIMAGE").is_none() && std::env::var_os("APPDIR").is_none() {
        return None;
    }
    Some(launch(&["xdg-open".into(), URL.into()], url))
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
const APPIMAGE_VARS: &[&str] = &[
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "PATH",
    "XDG_DATA_DIRS",
    "GTK_PATH",
    "GTK_EXE_PREFIX",
    "GTK_DATA_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_MODULE_DIR",
    "GIO_EXTRA_MODULES",
    "GSETTINGS_SCHEMA_DIR",
    "GST_PLUGIN_SYSTEM_PATH",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "GST_PLUGIN_PATH",
    "GST_PLUGIN_PATH_1_0",
    "GST_PLUGIN_SCANNER",
    "GST_PLUGIN_SCANNER_1_0",
    "PYTHONPATH",
    "PYTHONHOME",
    "PERLLIB",
    "QT_PLUGIN_PATH",
];

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn appimage_env(get: impl Fn(&str) -> Option<String>) -> Vec<(String, Option<String>)> {
    let appdir = get("APPDIR").filter(|d| !d.is_empty());
    if appdir.is_none() && get("APPIMAGE").is_none() {
        return Vec::new();
    }
    let mut out: Vec<(String, Option<String>)> = ["APPIMAGE", "APPDIR", "ARGV0", "OWD"]
        .iter()
        .filter(|k| get(k).is_some())
        .map(|k| (k.to_string(), None))
        .collect();
    for key in APPIMAGE_VARS {
        if let Some(orig) = get(&format!("{key}_ORIG")) {
            out.push((key.to_string(), Some(orig).filter(|v| !v.is_empty())));
            continue;
        }
        let (Some(dir), Some(value)) = (appdir.as_deref(), get(key)) else {
            continue;
        };
        let dir = dir.trim_end_matches('/');
        let inside = |p: &str| p == dir || p.starts_with(&format!("{dir}/"));
        let sep = if *key == "LD_PRELOAD" { ' ' } else { ':' };
        let kept: Vec<&str> = value
            .split(sep)
            .filter(|p| !p.is_empty() && !inside(p))
            .collect();
        if kept.len() != value.split(sep).filter(|p| !p.is_empty()).count() {
            out.push((
                key.to_string(),
                Some(kept.join(&sep.to_string())).filter(|v| !v.is_empty()),
            ));
        }
    }
    out
}

fn finish(mut list: Vec<Browser>) -> Vec<Browser> {
    let mut seen = std::collections::HashSet::new();
    list.retain(|b| seen.insert(b.id.to_lowercase()) && !b.name.is_empty());
    let mut names = std::collections::HashSet::new();
    list.retain(|b| names.insert(b.name.to_lowercase()));
    list.sort_by_key(|b| b.name.to_lowercase());
    list
}

#[cfg(target_os = "macos")]
fn detect() -> Vec<Browser> {
    let mut roots: Vec<std::path::PathBuf> = [
        "/Applications",
        "/System/Applications",
        "/System/Volumes/Preboot/Cryptexes/App/System/Applications",
    ]
    .map(Into::into)
    .into();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    finish(mac_browsers(&roots, |p| p.exists()))
}

#[cfg(any(target_os = "macos", test))]
const MAC_KNOWN: &[(&str, &str)] = &[
    ("com.apple.Safari", "Safari"),
    ("com.google.Chrome", "Google Chrome"),
    ("com.google.Chrome.canary", "Google Chrome Canary"),
    ("org.mozilla.firefox", "Firefox"),
    (
        "org.mozilla.firefoxdeveloperedition",
        "Firefox Developer Edition",
    ),
    ("org.mozilla.nightly", "Firefox Nightly"),
    ("company.thebrowser.Browser", "Arc"),
    ("com.brave.Browser", "Brave Browser"),
    ("com.microsoft.edgemac", "Microsoft Edge"),
    ("com.operasoftware.Opera", "Opera"),
    ("com.vivaldi.Vivaldi", "Vivaldi"),
    ("org.chromium.Chromium", "Chromium"),
    ("com.kagi.kagimacOS", "Orion"),
    ("app.zen-browser.zen", "Zen"),
    ("com.duckduckgo.macos.browser", "DuckDuckGo"),
    ("org.torproject.torbrowser", "Tor Browser"),
];

#[cfg(any(target_os = "macos", test))]
fn mac_browsers(
    dirs: &[std::path::PathBuf],
    exists: impl Fn(&std::path::Path) -> bool,
) -> Vec<Browser> {
    MAC_KNOWN
        .iter()
        .filter(|(_, app)| dirs.iter().any(|d| exists(&d.join(format!("{app}.app")))))
        .map(|(id, app)| Browser {
            id: (*id).into(),
            name: (*app).into(),
            launch: vec![
                "/usr/bin/open".into(),
                "-b".into(),
                (*id).into(),
                URL.into(),
            ],
        })
        .collect()
}

#[cfg(windows)]
fn detect() -> Vec<Browser> {
    use std::os::windows::process::CommandExt;
    let mut out = String::new();
    for root in [
        r"HKCU\SOFTWARE\Clients\StartMenuInternet",
        r"HKLM\SOFTWARE\Clients\StartMenuInternet",
        r"HKLM\SOFTWARE\WOW6432Node\Clients\StartMenuInternet",
    ] {
        if let Ok(o) = Command::new("reg")
            .args(["query", root, "/s", "/ve"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .output()
        {
            out.push_str(&String::from_utf8_lossy(&o.stdout));
            out.push('\n');
        }
    }
    finish(parse_reg(&out, |k| std::env::var(k).ok()))
}

#[cfg(any(windows, test))]
fn parse_reg(out: &str, env: impl Fn(&str) -> Option<String>) -> Vec<Browser> {
    const ROOT: &str = "\\startmenuinternet\\";
    let mut key = String::new();
    let mut names: Vec<(String, String)> = Vec::new();
    let mut cmds: Vec<(String, String)> = Vec::new();
    for line in out.lines() {
        if line.starts_with("HKEY_") {
            key = line.trim().to_string();
            continue;
        }
        let Some(value) = ["    REG_SZ    ", "    REG_EXPAND_SZ    "]
            .iter()
            .find_map(|t| line.split_once(t).map(|(_, v)| v.trim()))
        else {
            continue;
        };
        let Some(i) = key.to_ascii_lowercase().find(ROOT) else {
            continue;
        };
        let rest = &key[i + ROOT.len()..];
        let (sub, tail) = rest.split_once('\\').unwrap_or((rest, ""));
        if tail.is_empty() {
            names.push((sub.to_string(), value.to_string()));
        } else if tail.eq_ignore_ascii_case(r"shell\open\command") {
            cmds.push((sub.to_string(), value.to_string()));
        }
    }
    cmds.into_iter()
        .filter_map(|(sub, cmd)| {
            let exe = expand(exe_of(&cmd)?, &env);
            let name = names
                .iter()
                .find(|(s, _)| s.eq_ignore_ascii_case(&sub))
                .map(|(_, n)| n.as_str())
                .filter(|n| !n.is_empty() && !n.starts_with('@'))
                .unwrap_or(&sub);
            Some(Browser {
                id: sub.clone(),
                name: name.to_string(),
                launch: vec![exe, URL.into()],
            })
        })
        .collect()
}

#[cfg(any(windows, test))]
fn exe_of(cmd: &str) -> Option<&str> {
    let cmd = cmd.trim();
    let exe = if let Some(q) = cmd.strip_prefix('"') {
        q.split('"').next()?
    } else if let Some(i) = cmd.to_ascii_lowercase().find(".exe") {
        &cmd[..i + 4]
    } else {
        cmd.split_whitespace().next()?
    };
    (!exe.is_empty()).then_some(exe)
}

#[cfg(any(windows, test))]
fn expand(s: &str, env: &impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(a) = rest.find('%') {
        let Some(b) = rest[a + 1..].find('%') else {
            break;
        };
        let var = &rest[a + 1..a + 1 + b];
        out.push_str(&rest[..a]);
        match env(var) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[a..a + b + 2]),
        }
        rest = &rest[a + b + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect() -> Vec<Browser> {
    let home = dirs::home_dir().unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut roots = vec![data_home, home.join(".local/share/flatpak/exports/share")];
    roots.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(std::path::PathBuf::from),
    );
    roots.extend(
        ["/var/lib/flatpak/exports/share", "/var/lib/snapd/desktop"].map(std::path::PathBuf::from),
    );
    let mut list = Vec::new();
    for root in roots {
        let Ok(dir) = std::fs::read_dir(root.join("applications")) else {
            continue;
        };
        let mut files: Vec<_> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "desktop"))
            .collect();
        files.sort();
        for path in files {
            let (Some(id), Ok(text)) = (
                path.file_name().and_then(|n| n.to_str()),
                std::fs::read_to_string(&path),
            ) else {
                continue;
            };
            if let Some(b) = parse_desktop(id, &text) {
                list.push(b);
            }
        }
    }
    finish(list)
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn parse_desktop(id: &str, text: &str) -> Option<Browser> {
    let (mut entry, mut name, mut exec, mut web, mut app, mut hidden) =
        (false, None, None, false, true, false);
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((k, v)) = line.split_once('=').filter(|_| entry) else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "Name" => name = Some(v.to_string()),
            "Exec" => exec = Some(v.to_string()),
            "MimeType" => {
                web = v
                    .split(';')
                    .any(|m| m == "x-scheme-handler/http" || m == "x-scheme-handler/https")
            }
            "Type" => app = v == "Application",
            "Hidden" | "NoDisplay" => hidden |= v == "true",
            _ => {}
        }
    }
    if !web || !app || hidden || id.to_ascii_lowercase().contains("overseer") {
        return None;
    }
    let launch = exec_argv(&exec?);
    (!launch.is_empty()).then(|| Browser {
        id: id.to_string(),
        name: name.unwrap_or_else(|| id.trim_end_matches(".desktop").to_string()),
        launch,
    })
}

#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn exec_argv(exec: &str) -> Vec<String> {
    let exec = exec.replace("\\\\", "\\");
    let mut words: Vec<String> = Vec::new();
    let (mut cur, mut quoted, mut started) = (String::new(), false, false);
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' if quoted => cur.extend(chars.next()),
            c if c.is_whitespace() && !quoted => {
                if started || !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
                started = false;
            }
            c => cur.push(c),
        }
    }
    if started || !cur.is_empty() {
        words.push(cur);
    }
    words
        .into_iter()
        .filter_map(|w| {
            if matches!(w.as_str(), "%u" | "%U" | "%f" | "%F") {
                return Some(URL.to_string());
            }
            if matches!(w.as_str(), "%i" | "%c" | "%k") {
                return None;
            }
            let mut out = String::new();
            let mut it = w.chars().peekable();
            while let Some(c) = it.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match it.next() {
                    Some('%') => out.push('%'),
                    Some('u' | 'U' | 'f' | 'F') => out.push_str(URL),
                    _ => {}
                }
            }
            Some(out)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn appends_or_substitutes_the_url() {
        assert_eq!(
            argv(&["firefox".into(), URL.into()], "https://a.dev"),
            ["firefox", "https://a.dev"]
        );
        assert_eq!(
            argv(&["chrome".into(), format!("--app={URL}")], "https://a.dev"),
            ["chrome", "--app=https://a.dev"]
        );
        assert_eq!(
            argv(&["C:\\b.exe".into()], "https://a.dev"),
            ["C:\\b.exe", "https://a.dev"]
        );
    }

    #[test]
    fn desktop_entries_that_handle_http_are_browsers() {
        let firefox = "[Desktop Entry]\nName=Firefox Web Browser\nName[fr]=Navigateur Firefox\nExec=/usr/lib/firefox/firefox %u\nType=Application\nMimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;\n\n[Desktop Action new-window]\nName=New Window\nExec=/usr/lib/firefox/firefox --new-window %u\n";
        let b = parse_desktop("firefox.desktop", firefox).unwrap();
        assert_eq!(b.name, "Firefox Web Browser");
        assert_eq!(b.launch, ["/usr/lib/firefox/firefox", URL]);

        let flatpak = "[Desktop Entry]\nName=Chromium\nExec=/usr/bin/flatpak run --branch=stable --command=chromium org.chromium.Chromium @@u %U @@\nMimeType=x-scheme-handler/https\nType=Application\n";
        let b = parse_desktop("org.chromium.Chromium.desktop", flatpak).unwrap();
        assert_eq!(
            b.launch,
            [
                "/usr/bin/flatpak",
                "run",
                "--branch=stable",
                "--command=chromium",
                "org.chromium.Chromium",
                "@@u",
                URL,
                "@@"
            ]
        );

        let quoted = "[Desktop Entry]\nName=Odd\nExec=\"/opt/My Browser/bin\" --class=%c --flag=\"a \\\"b\\\"\" %%x %F\nMimeType=x-scheme-handler/http;\n";
        assert_eq!(
            parse_desktop("odd.desktop", quoted).unwrap().launch,
            [
                "/opt/My Browser/bin",
                "--class=",
                "--flag=a \"b\"",
                "%x",
                URL
            ]
        );
    }

    #[test]
    fn non_browser_or_hidden_entries_are_skipped() {
        let editor = "[Desktop Entry]\nName=Editor\nExec=gedit %U\nMimeType=text/plain;\n";
        assert!(parse_desktop("gedit.desktop", editor).is_none());
        let hidden = "[Desktop Entry]\nName=Old\nExec=old %u\nMimeType=x-scheme-handler/http;\nNoDisplay=true\n";
        assert!(parse_desktop("old.desktop", hidden).is_none());
        let action_only =
            "[Desktop Entry]\nName=App\nExec=app\n[Other]\nMimeType=x-scheme-handler/http;\n";
        assert!(parse_desktop("app.desktop", action_only).is_none());
        let link =
            "[Desktop Entry]\nType=Link\nName=Site\nExec=x\nMimeType=x-scheme-handler/http;\n";
        assert!(parse_desktop("site.desktop", link).is_none());
        let own =
            "[Desktop Entry]\nName=Overseer\nExec=overseer %u\nMimeType=x-scheme-handler/http;\n";
        assert!(parse_desktop("overseer.desktop", own).is_none());
    }

    #[test]
    fn windows_registry_lists_start_menu_browsers() {
        let out = r#"
HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Google Chrome
    (Default)    REG_SZ    Google Chrome

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Google Chrome\Capabilities

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Google Chrome\DefaultIcon
    (Default)    REG_SZ    C:\Program Files\Google\Chrome\Application\chrome.exe,0

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Google Chrome\shell\open\command
    (Default)    REG_SZ    "C:\Program Files\Google\Chrome\Application\chrome.exe"

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Firefox-308046B0AF4A39CB
    (par défaut)    REG_SZ    Firefox

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Firefox-308046B0AF4A39CB\shell\open\command
    (par défaut)    REG_EXPAND_SZ    %ProgramFiles%\Mozilla Firefox\firefox.exe -osint -url "%1"

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\IEXPLORE.EXE
    (Default)    REG_SZ    @C:\Windows\iexplore.exe,-1

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\IEXPLORE.EXE\shell\open\command
    (Default)    REG_SZ    C:\Program Files\Internet Explorer\iexplore.exe

HKEY_LOCAL_MACHINE\SOFTWARE\Clients\StartMenuInternet\Broken
    (Default)    REG_SZ    Broken
"#;
        let env = |k: &str| (k == "ProgramFiles").then(|| r"C:\Program Files".to_string());
        let list = finish(parse_reg(out, env));
        let got: Vec<_> = list
            .iter()
            .map(|b| (b.id.as_str(), b.name.as_str(), b.launch[0].as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "Firefox-308046B0AF4A39CB",
                    "Firefox",
                    r"C:\Program Files\Mozilla Firefox\firefox.exe"
                ),
                (
                    "Google Chrome",
                    "Google Chrome",
                    r"C:\Program Files\Google\Chrome\Application\chrome.exe"
                ),
                (
                    "IEXPLORE.EXE",
                    "IEXPLORE.EXE",
                    r"C:\Program Files\Internet Explorer\iexplore.exe"
                ),
            ]
        );
    }

    #[test]
    fn registry_commands_resolve_to_an_executable() {
        assert_eq!(exe_of(r#""C:\a b\x.exe" -- "%1""#), Some(r"C:\a b\x.exe"));
        assert_eq!(
            exe_of(r"C:\Program Files\B\b.EXE --x"),
            Some(r"C:\Program Files\B\b.EXE")
        );
        assert_eq!(exe_of("   "), None);
        assert_eq!(expand("%Missing%\\a%", &|_: &str| None), "%Missing%\\a%");
    }

    #[test]
    fn macos_lists_known_browsers_that_are_installed() {
        let dirs = [
            PathBuf::from("/Applications"),
            PathBuf::from("/Users/me/Applications"),
        ];
        let present = [
            "/Applications/Safari.app",
            "/Users/me/Applications/Firefox.app",
            "/Applications/Arc.app",
        ];
        let list = finish(mac_browsers(&dirs, |p: &Path| {
            present.iter().any(|x| Path::new(x) == p)
        }));
        let got: Vec<_> = list
            .iter()
            .map(|b| (b.id.as_str(), b.name.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("company.thebrowser.Browser", "Arc"),
                ("org.mozilla.firefox", "Firefox"),
                ("com.apple.Safari", "Safari")
            ]
        );
        assert_eq!(
            list[1].launch,
            ["/usr/bin/open", "-b", "org.mozilla.firefox", URL]
        );
    }

    #[test]
    fn appimage_environment_is_restored_for_launched_browsers() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert!(appimage_env(env(&[
            ("LD_LIBRARY_PATH", "/opt/lib"),
            ("PATH", "/usr/bin")
        ]))
        .is_empty());

        let got = appimage_env(env(&[
            ("APPIMAGE", "/home/me/Overseer.AppImage"),
            ("APPDIR", "/tmp/.mount_SwitcX/"),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_SwitcX/usr/lib:/opt/cuda/lib",
            ),
            ("LD_PRELOAD", "/tmp/.mount_SwitcX/usr/lib/libx.so"),
            ("PATH", "/tmp/.mount_SwitcX/usr/bin:/usr/local/bin:/usr/bin"),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_SwitcX/usr/share:/usr/local/share:/usr/share",
            ),
            ("GTK_PATH", "/tmp/.mount_SwitcX/usr/lib/gtk-3.0"),
            (
                "GDK_PIXBUF_MODULE_FILE",
                "/tmp/.mount_SwitcX/usr/lib/loaders.cache",
            ),
            ("GST_PLUGIN_SYSTEM_PATH_1_0", "/tmp/.mount_SwitcXY/lib"),
            ("PYTHONPATH", "/tmp/.mount_SwitcX/usr/lib/python3"),
            ("PYTHONPATH_ORIG", "/home/me/py"),
            ("QT_PLUGIN_PATH", "/tmp/.mount_SwitcX/usr/plugins"),
            ("QT_PLUGIN_PATH_ORIG", ""),
            ("GIO_MODULE_DIR", "/usr/lib/gio/modules"),
        ]));
        let set = |k: &str| got.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        assert_eq!(set("APPIMAGE"), Some(None));
        assert_eq!(set("APPDIR"), Some(None));
        assert_eq!(set("LD_LIBRARY_PATH"), Some(Some("/opt/cuda/lib".into())));
        assert_eq!(set("LD_PRELOAD"), Some(None));
        assert_eq!(set("PATH"), Some(Some("/usr/local/bin:/usr/bin".into())));
        assert_eq!(
            set("XDG_DATA_DIRS"),
            Some(Some("/usr/local/share:/usr/share".into()))
        );
        assert_eq!(set("GTK_PATH"), Some(None));
        assert_eq!(set("GDK_PIXBUF_MODULE_FILE"), Some(None));
        assert_eq!(set("PYTHONPATH"), Some(Some("/home/me/py".into())));
        assert_eq!(set("QT_PLUGIN_PATH"), Some(None));
        assert_eq!(set("GST_PLUGIN_SYSTEM_PATH_1_0"), None);
        assert_eq!(set("GIO_MODULE_DIR"), None);
    }

    #[test]
    fn duplicates_keep_the_first_entry() {
        let b = |id: &str, name: &str| Browser {
            id: id.into(),
            name: name.into(),
            launch: vec!["x".into()],
        };
        let list = finish(vec![
            b("firefox.desktop", "Firefox"),
            b("FIREFOX.desktop", "Other"),
            b("org.mozilla.firefox.desktop", "Firefox"),
            b("e", ""),
        ]);
        assert_eq!(list, [b("firefox.desktop", "Firefox")]);
    }
}
