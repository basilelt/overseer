use tauri::AppHandle;

#[tauri::command]
pub async fn mobile_minimize(app: AppHandle) -> Result<(), String> {
    imp::minimize(&app)
}

#[tauri::command]
pub async fn mobile_system_dark(app: AppHandle) -> Result<Option<bool>, String> {
    imp::system_dark(&app)
}

#[tauri::command]
pub async fn mobile_system_bars(app: AppHandle, dark: bool) -> Result<(), String> {
    imp::system_bars(&app, dark)
}

#[tauri::command]
pub async fn mobile_clipboard_image(app: AppHandle) -> Result<Option<String>, String> {
    imp::clipboard_image(&app)
}

#[tauri::command]
pub fn open_notification_settings(app: AppHandle) -> Result<(), String> {
    imp::notification_settings(&app)
}

pub fn notification_settings_url(os: &str) -> Option<&'static str> {
    match os {
        "macos" => Some("x-apple.systempreferences:com.apple.preference.notifications"),
        "windows" => Some("ms-settings:notifications"),
        _ => None,
    }
}

#[tauri::command]
pub fn start_dictation(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::macos::start_dictation(&app)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("dictation is not available on this platform".into())
    }
}

#[cfg(target_os = "android")]
pub use imp::plugin;

#[cfg(target_os = "android")]
mod imp {
    use serde::{Deserialize, Serialize};
    use tauri::plugin::{Builder, PluginHandle, TauriPlugin};
    use tauri::{AppHandle, Manager, Wry};

    struct Native(PluginHandle<Wry>);

    pub fn plugin() -> TauriPlugin<Wry> {
        Builder::new("overseer-mobile")
            .setup(|app, api| {
                let handle =
                    api.register_android_plugin("io.github.basilelt.overseer", "MobilePlugin")?;
                app.manage(Native(handle));
                Ok(())
            })
            .build()
    }

    fn run<T: serde::de::DeserializeOwned>(
        app: &AppHandle,
        cmd: &str,
        args: impl Serialize,
    ) -> Result<T, String> {
        let native = app
            .try_state::<Native>()
            .ok_or("the mobile plugin is not loaded")?;
        native
            .0
            .run_mobile_plugin::<T>(cmd, args)
            .map_err(|e| match e {
                tauri::plugin::mobile::PluginInvokeError::InvokeRejected(r) => {
                    r.message.unwrap_or_else(|| format!("{cmd} failed"))
                }
                e => e.to_string(),
            })
    }

    #[derive(Serialize)]
    struct Bars {
        dark: bool,
    }

    #[derive(Deserialize)]
    struct Dark {
        dark: bool,
    }

    #[derive(Deserialize)]
    struct Clip {
        #[serde(default)]
        path: Option<String>,
    }

    pub fn minimize(app: &AppHandle) -> Result<(), String> {
        run::<()>(app, "minimize", ())
    }

    pub fn system_dark(app: &AppHandle) -> Result<Option<bool>, String> {
        run::<Dark>(app, "systemDark", ()).map(|d| Some(d.dark))
    }

    pub fn system_bars(app: &AppHandle, dark: bool) -> Result<(), String> {
        run::<()>(app, "setSystemBars", Bars { dark })
    }

    pub fn clipboard_image(app: &AppHandle) -> Result<Option<String>, String> {
        run::<Clip>(app, "clipboardImage", ()).map(|c| c.path)
    }

    pub fn notification_settings(app: &AppHandle) -> Result<(), String> {
        run::<()>(app, "notificationSettings", ())
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use tauri::AppHandle;

    pub fn minimize(_: &AppHandle) -> Result<(), String> {
        Ok(())
    }

    pub fn system_dark(_: &AppHandle) -> Result<Option<bool>, String> {
        Ok(None)
    }

    pub fn system_bars(_: &AppHandle, _: bool) -> Result<(), String> {
        Ok(())
    }

    pub fn clipboard_image(_: &AppHandle) -> Result<Option<String>, String> {
        Ok(None)
    }

    pub fn notification_settings(app: &AppHandle) -> Result<(), String> {
        let url = super::notification_settings_url(std::env::consts::OS)
            .ok_or("no notification settings to open on this platform")?;
        #[cfg(desktop)]
        {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        }
        #[cfg(mobile)]
        {
            let _ = (app, url);
            Err("no notification settings to open on this platform".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::notification_settings_url;

    #[test]
    fn notification_settings_per_os() {
        assert!(notification_settings_url("macos")
            .unwrap()
            .starts_with("x-apple.systempreferences:"));
        assert_eq!(
            notification_settings_url("windows"),
            Some("ms-settings:notifications")
        );
        assert_eq!(notification_settings_url("linux"), None);
        assert_eq!(notification_settings_url("ios"), None);
    }
}
