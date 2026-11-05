use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Rect, State, Url, Webview, WebviewBuilder,
    WebviewUrl, Window,
};
use tauri_plugin_opener::OpenerExt;

type Nav = Arc<Mutex<HashMap<String, (String, Channel<Value>)>>>;

#[derive(Default)]
pub struct BrowserState {
    nav: Nav,
}

pub fn label(window: &str) -> String {
    if window == "main" {
        "browser".into()
    } else {
        format!("browser-{window}")
    }
}

pub fn page_label(window: &str, id: Option<&str>) -> Result<String, String> {
    match id {
        None => Ok(label(window)),
        Some(id)
            if !id.is_empty()
                && id.len() <= 32
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') =>
        {
            Ok(format!("page:{window}:{id}"))
        }
        Some(id) => Err(format!("invalid page id {id:?}")),
    }
}

pub fn forget(app: &AppHandle, window: &str) {
    if let Some(s) = app.try_state::<BrowserState>() {
        s.nav
            .lock()
            .unwrap()
            .retain(|_, (owner, _)| owner != window);
    }
}

pub fn parse_url(input: &str) -> Result<Url, String> {
    let s = input.trim();
    let with_scheme = if s.contains("://") || s.starts_with("about:") {
        s.to_string()
    } else if s.starts_with("localhost") || s.starts_with("127.0.0.1") {
        format!("http://{s}")
    } else {
        format!("https://{s}")
    };
    let url = Url::parse(&with_scheme).map_err(|e| format!("invalid URL {s:?}: {e}"))?;
    match url.scheme() {
        "http" | "https" | "about" => Ok(url),
        other => Err(format!("unsupported URL scheme {other:?}")),
    }
}

fn webview(window: &Window, id: Option<&str>) -> Result<Webview, String> {
    window
        .get_webview(&page_label(window.label(), id)?)
        .ok_or_else(|| "browser is not open".into())
}

fn send(nav: &Nav, label: &str, v: Value) {
    if let Some((_, ch)) = nav.lock().unwrap().get(label) {
        let _ = ch.send(v);
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        position: LogicalPosition::new(x, y).into(),
        size: LogicalSize::new(width.max(1.0), height.max(1.0)).into(),
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn browser_open(
    window: Window,
    state: State<'_, BrowserState>,
    url: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    on_nav: Channel<Value>,
    id: Option<String>,
) -> Result<(), String> {
    let url = parse_url(&url)?;
    let owner = page_label(window.label(), id.as_deref())?;
    state
        .nav
        .lock()
        .unwrap()
        .insert(owner.clone(), (window.label().to_string(), on_nav));
    if let Ok(wv) = webview(&window, id.as_deref()) {
        wv.set_bounds(rect(x, y, width, height))
            .map_err(|e| e.to_string())?;
        wv.show().map_err(|e| e.to_string())?;
        return wv.navigate(url).map_err(|e| e.to_string());
    }
    let (load, title) = (state.nav.clone(), state.nav.clone());
    let (load_owner, title_owner) = (owner.clone(), owner.clone());
    let builder = WebviewBuilder::new(owner.clone(), WebviewUrl::External(url))
        .on_navigation(|u| matches!(u.scheme(), "http" | "https" | "about"))
        .on_page_load(move |_, p| {
            let loading = matches!(p.event(), tauri::webview::PageLoadEvent::Started);
            send(
                &load,
                &load_owner,
                json!({"url": p.url().as_str(), "loading": loading}),
            );
        })
        .on_document_title_changed(move |wv, t| {
            let url = wv.url().map(|u| u.to_string()).unwrap_or_default();
            send(
                &title,
                &title_owner,
                json!({"url": url, "title": t, "loading": false}),
            );
        });
    window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(1.0), height.max(1.0)),
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn browser_bounds(
    window: Window,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    id: Option<String>,
) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .set_bounds(rect(x, y, width, height))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_navigate(
    window: Window,
    url: String,
    id: Option<String>,
) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .navigate(parse_url(&url)?)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_back(window: Window, id: Option<String>) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .eval("history.back()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_forward(window: Window, id: Option<String>) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .eval("history.forward()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_reload(window: Window, id: Option<String>) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .reload()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_hide(window: Window, id: Option<String>) -> Result<(), String> {
    match webview(&window, id.as_deref()) {
        Ok(wv) => wv.hide().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub async fn browser_show(window: Window, id: Option<String>) -> Result<(), String> {
    webview(&window, id.as_deref())?
        .show()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn browser_close(
    window: Window,
    state: State<'_, BrowserState>,
    id: Option<String>,
) -> Result<(), String> {
    state
        .nav
        .lock()
        .unwrap()
        .remove(&page_label(window.label(), id.as_deref())?);
    match webview(&window, id.as_deref()) {
        Ok(wv) => wv.close().map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub async fn browser_pages_reset(
    window: Window,
    state: State<'_, BrowserState>,
) -> Result<(), String> {
    let prefix = format!("page:{}:", window.label());
    state
        .nav
        .lock()
        .unwrap()
        .retain(|label, _| !label.starts_with(&prefix));
    for wv in window.webviews() {
        if wv.label().starts_with(&prefix) {
            wv.close().map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    let url = parse_url(&url)?;
    if url.scheme() == "about" {
        return Err("cannot open about: URLs externally".into());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(r) = crate::browsers::xdg_open_outside_appimage(url.as_str()) {
        return r;
    }
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{label, page_label, parse_url};

    #[test]
    fn each_window_has_its_own_browser_webview() {
        assert_eq!(label("main"), "browser");
        assert_eq!(label("w-2"), "browser-w-2");
        assert_ne!(label("w-2"), label("w-3"));
    }

    #[test]
    fn page_webviews_are_keyed_by_window_and_page() {
        assert_eq!(page_label("main", None).unwrap(), "browser");
        assert_eq!(page_label("w-2", None).unwrap(), "browser-w-2");
        assert_eq!(page_label("main", Some("p1")).unwrap(), "page:main:p1");
        assert_ne!(
            page_label("main", Some("w-2")).unwrap(),
            page_label("w-2", None).unwrap()
        );
        assert_ne!(
            page_label("main", Some("p1")).unwrap(),
            page_label("w-2", Some("p1")).unwrap()
        );
        assert!(page_label("main", Some("")).is_err());
        assert!(page_label("main", Some("a:b")).is_err());
        assert!(page_label("main", Some("a/b")).is_err());
        assert!(page_label("main", Some(&"x".repeat(33))).is_err());
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            parse_url("github.com/acme").unwrap().as_str(),
            "https://github.com/acme"
        );
        assert_eq!(
            parse_url("localhost:5173").unwrap().as_str(),
            "http://localhost:5173/"
        );
        assert_eq!(
            parse_url(" http://x.dev ").unwrap().as_str(),
            "http://x.dev/"
        );
        assert!(parse_url("file:///etc/passwd").is_err());
        assert!(parse_url("javascript://alert(1)").is_err());
    }
}
