use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const MAIN: &str = "main";

static NEXT: AtomicU32 = AtomicU32::new(2);
static HANDOFF: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

fn hand_off(label: &str, target: Value) {
    HANDOFF
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(label.to_string(), target);
}

fn take_handoff(label: &str) -> Option<Value> {
    HANDOFF.lock().unwrap().as_mut()?.remove(label)
}

/// The host a window was opened for, once: a reload then starts like any other window.
#[tauri::command]
pub fn window_handoff(window: tauri::Window) -> Option<Value> {
    take_handoff(window.label())
}

pub fn next_label(taken: impl Fn(&str) -> bool) -> String {
    loop {
        let label = format!("w-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        if !taken(&label) {
            return label;
        }
    }
}

#[tauri::command]
pub async fn window_new(app: AppHandle, connect: Option<Value>) -> Result<String, String> {
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
    if let Some(target) = connect.filter(|t| !t.is_null()) {
        hand_off(&label, target);
    }
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

    #[test]
    fn a_handoff_is_taken_once_by_its_window() {
        let target = serde_json::json!({"kind": "ssh", "host": "prod", "label": "prod"});
        hand_off("w-handoff", target.clone());
        assert_eq!(take_handoff("w-other"), None);
        assert_eq!(take_handoff("w-handoff"), Some(target));
        assert_eq!(take_handoff("w-handoff"), None);
    }
}
