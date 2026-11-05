#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = overseer_app_lib::relay::client_main() {
        std::process::exit(code);
    }
    if let Some(code) = overseer_app_lib::askpass::client_main() {
        std::process::exit(code);
    }
    overseer_app_lib::run()
}
