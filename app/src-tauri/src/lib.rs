#[cfg(self_update)]
#[cfg_attr(desktop, allow(dead_code))]
mod apk_update;
pub mod askpass;
#[cfg(desktop)]
mod browser;
#[cfg(desktop)]
mod browsers;
mod commands;
mod conn;
/// Held by tests that change the process environment (PATH, OVERSEER_SOCKET) and by tests that
/// spawn processes needing it intact.
#[cfg(test)]
pub(crate) static TEST_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
#[cfg(all(test, unix))]
mod daemon_it;
pub mod diag;
mod direct;
#[cfg(test)]
mod distribution;
mod forward;
mod http;
mod install;
#[cfg_attr(mobile, allow(dead_code))]
mod localnet;
#[cfg(target_os = "macos")]
mod macos;
mod mux;
#[cfg_attr(desktop, allow(dead_code))]
mod native;
pub mod relay;
mod release_http;
#[cfg(all(test, unix))]
mod remote_it;
mod server_fetch;
mod sse;
mod ssh;
#[cfg_attr(mobile, allow(dead_code))]
mod sshconfig;
mod sshg;
mod transport;
#[cfg(desktop)]
mod updater;
#[cfg(desktop)]
mod window;
mod ws;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    diag::install_panic_hook(env!("CARGO_PKG_VERSION").to_string());
    let builder = tauri::Builder::default()
        .setup(|app| {
            if let Some(dir) = diag::resolve_dir(app.handle()) {
                diag::init(dir, &app.package_info().version.to_string());
            }
            #[cfg(target_os = "macos")]
            macos::create_main_window(app)?;
            diag::log_start(app.handle());
            diag::start_watchdog(app.handle().clone());
            #[cfg(all(unix, desktop))]
            diag::quit_on_signals(app.handle().clone());
            Ok(())
        })
        .on_window_event(|w, e| match e {
            tauri::WindowEvent::Focused(f) if w.label() == "main" => diag::focus(*f),
            tauri::WindowEvent::Destroyed => {
                if w.label() == "main" {
                    diag::focus(false);
                    diag::visibility(false);
                }
                #[cfg(desktop)]
                browser::forget(tauri::Manager::app_handle(w), w.label());
                if let Some(state) = tauri::Manager::try_state::<commands::AppState>(w) {
                    state.close(w.label());
                }
            }
            _ => {}
        })
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .manage(commands::AppState::default());
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::UpdaterState::default())
        .manage(browser::BrowserState::default())
        .invoke_handler(tauri::generate_handler![
            commands::daemon_request,
            commands::daemon_bytes,
            commands::daemon_events,
            commands::daemon_events_stop,
            commands::term_attach,
            commands::term_write,
            commands::term_resize,
            commands::term_background,
            commands::term_detach,
            commands::conn_connect,
            commands::conn_disconnect,
            commands::conn_answer,
            commands::conn_drop_slots,
            commands::conn_wake,
            commands::conn_restart_server,
            commands::ssh_hosts,
            commands::ports_watch,
            commands::ports_watch_stop,
            commands::ports_configure,
            commands::ports_forward,
            commands::ports_unforward,
            commands::upload_file,
            commands::save_paste,
            commands::save_download,
            browser::browser_open,
            browser::browser_bounds,
            browser::browser_navigate,
            browser::browser_back,
            browser::browser_forward,
            browser::browser_reload,
            browser::browser_hide,
            browser::browser_show,
            browser::browser_close,
            browser::browser_pages_reset,
            browser::open_external,
            browsers::browsers_list,
            browsers::browser_open_with,
            window::window_new,
            window::window_handoff,
            updater::update_status,
            updater::update_check,
            updater::update_install,
            localnet::local_network_status,
            localnet::open_local_network_settings,
            updater::update_relaunch,
            direct::app_platform,
            direct::direct_device_key,
            direct::direct_hosts,
            direct::direct_host_save,
            direct::direct_host_delete,
            direct::direct_host_forget_key,
            diag::diag_log,
            diag::diag_pong,
            diag::diag_visibility,
            diag::diag_session_end,
            diag::diag_previous,
            diag::diag_dismiss,
            diag::diag_tail,
            diag::diag_info,
            diag::diag_save,
            diag::diag_open_dir,
            native::start_dictation,
            native::open_notification_settings,
        ]);
    #[cfg(all(target_os = "android", self_update))]
    let builder = builder.plugin(apk_update::installer_plugin());
    #[cfg(target_os = "android")]
    let builder = builder.plugin(native::plugin());
    #[cfg(all(mobile, self_update))]
    let builder = builder.manage(apk_update::ApkState::default());
    #[cfg(mobile)]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::daemon_request,
        commands::daemon_bytes,
        commands::daemon_events,
        commands::daemon_events_stop,
        commands::term_attach,
        commands::term_write,
        commands::term_resize,
        commands::term_background,
        commands::term_detach,
        commands::conn_connect,
        commands::conn_disconnect,
        commands::conn_answer,
        commands::conn_wake,
        commands::conn_restart_server,
        commands::upload_file,
        commands::save_paste,
        commands::save_download,
        commands::ports_watch,
        commands::ports_watch_stop,
        commands::ports_configure,
        commands::ports_forward,
        commands::ports_unforward,
        direct::app_platform,
        direct::direct_device_key,
        direct::direct_hosts,
        direct::direct_host_save,
        direct::direct_host_delete,
        direct::direct_host_forget_key,
        direct::open_external,
        #[cfg(self_update)]
        apk_update::apk_update_status,
        #[cfg(self_update)]
        apk_update::apk_update_check,
        #[cfg(self_update)]
        apk_update::apk_update_download,
        #[cfg(self_update)]
        apk_update::apk_update_install,
        #[cfg(self_update)]
        apk_update::apk_update_allow_install,
        native::mobile_minimize,
        native::mobile_system_dark,
        native::mobile_system_bars,
        native::mobile_clipboard_image,
        native::open_notification_settings,
        diag::diag_log,
        diag::diag_pong,
        diag::diag_visibility,
        diag::diag_session_end,
        diag::diag_previous,
        diag::diag_dismiss,
        diag::diag_tail,
        diag::diag_info,
        diag::diag_save,
    ]);
    #[allow(unused_mut)]
    let mut ctx = tauri::generate_context!();
    #[cfg(target_os = "macos")]
    macos::defer_main_window(&mut ctx);
    let app = builder.build(ctx).unwrap_or_else(|e| {
        diag::error("app", format!("failed to start: {e}"));
        panic!("error while running overseer: {e}")
    });
    app.run(|_, event| {
        if let tauri::RunEvent::Exit = event {
            diag::session_end();
        }
    });
}
