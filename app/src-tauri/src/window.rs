use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Manager};

pub const MAIN: &str = "main";

static NEXT: AtomicU32 = AtomicU32::new(2);

pub fn next_label(taken: impl Fn(&str) -> bool) -> String {
    loop {
        let label = format!("w-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        if !taken(&label) {
            return label;
        }
    }
}

#[tauri::command]
pub async fn window_new(app: AppHandle) -> Result<String, String> {
    let mut conf = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .cloned()
        .ok_or("main window config missing")?;
    conf.label = next_label(|l| app.get_webview_window(l).is_some());
    conf.create = true;
    let label = conf.label.clone();
    crate::diag::info("app", format!("opening window {label}"));
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        app.run_on_main_thread(move || {
            if let Err(e) = crate::macos::build_window(&handle, &conf) {
                crate::diag::warn("app", format!("window {} failed: {e}", conf.label));
            }
        })
        .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    tauri::WebviewWindowBuilder::from_config(&app, &conf)
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_unique_and_skip_taken_ones() {
        let a = next_label(|_| false);
        let b = next_label(|l| l == format!("w-{}", a[2..].parse::<u32>().unwrap() + 1));
        assert!(a.starts_with("w-") && b.starts_with("w-"));
        assert_ne!(a, b);
        assert_ne!(a, MAIN);
        assert_ne!(b, format!("w-{}", a[2..].parse::<u32>().unwrap() + 1));
    }
}
