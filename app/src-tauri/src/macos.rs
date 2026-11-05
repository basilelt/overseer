use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel, MainThreadMarker};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSUserDefaults};
use objc2_web_kit::WKWebViewConfiguration;
use tauri::utils::config::WindowConfig;
use tauri::{App, AppHandle, Context, Manager, WebviewWindowBuilder, Wry};

pub const MAIN: &str = "main";

const SUBSTITUTIONS: [&str; 4] = [
    "WebAutomaticQuoteSubstitutionEnabled",
    "WebAutomaticDashSubstitutionEnabled",
    "WebAutomaticTextReplacementEnabled",
    "WebAutomaticSpellingCorrectionEnabled",
];

pub fn defer_main_window(ctx: &mut Context<Wry>) {
    for w in ctx
        .config_mut()
        .app
        .windows
        .iter_mut()
        .filter(|w| w.label == MAIN)
    {
        w.create = false;
    }
}

pub fn create_main_window(app: &App) -> tauri::Result<()> {
    let Some(conf) = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == MAIN)
        .cloned()
    else {
        return Ok(());
    };
    if app.get_webview_window(MAIN).is_some() {
        return Ok(());
    }
    plain_text_defaults();
    build_window(app.handle(), &conf)
}

pub fn build_window(app: &AppHandle, conf: &WindowConfig) -> tauri::Result<()> {
    if let Some(mtm) = MainThreadMarker::new() {
        let built = WebviewWindowBuilder::from_config(app, conf)
            .and_then(|b| b.with_webview_configuration(configuration(mtm)).build());
        match built {
            Ok(_) => return Ok(()),
            Err(e) => crate::diag::warn(
                "app",
                format!(
                    "window {} with plain-text webview settings failed: {e}",
                    conf.label
                ),
            ),
        }
    }
    WebviewWindowBuilder::from_config(app, conf)?.build()?;
    Ok(())
}

fn configuration(mtm: MainThreadMarker) -> Retained<WKWebViewConfiguration> {
    let cfg = unsafe { WKWebViewConfiguration::new(mtm) };
    let has: bool = unsafe { msg_send![&*cfg, respondsToSelector: sel!(setWritingToolsBehavior:)] };
    if has {
        let _: () = unsafe { msg_send![&*cfg, setWritingToolsBehavior: -1isize] };
    }
    cfg
}

fn plain_text_defaults() {
    let keys: Vec<Retained<NSString>> = SUBSTITUTIONS
        .iter()
        .map(|k| NSString::from_str(k))
        .collect();
    let no = NSNumber::new_bool(false);
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let vals: Vec<&AnyObject> = keys.iter().map(|_| -> &AnyObject { &no }).collect();
    let dict = NSDictionary::<NSString, AnyObject>::from_slices(&key_refs, &vals);
    unsafe { NSUserDefaults::standardUserDefaults().registerDefaults(&dict) };
}

pub fn start_dictation(app: &AppHandle) -> Result<(), String> {
    app.run_on_main_thread(|| {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let none: *const AnyObject = std::ptr::null();
        let ns_app: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
        let _: bool =
            unsafe { msg_send![ns_app, sendAction: sel!(startDictation:), to: none, from: none] };
    })
    .map_err(|e| e.to_string())
}
