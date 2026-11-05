include!("src/distribution.rs");

fn main() {
    let manifest = std::path::Path::new("../../Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let version = std::fs::read_to_string(manifest)
        .ok()
        .and_then(|s| {
            s.lines()
                .skip_while(|l| l.trim() != "[package]")
                .find_map(|l| {
                    l.trim()
                        .strip_prefix("version = ")
                        .map(|v| v.trim_matches('"').to_string())
                })
        })
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=OVERSEER_SERVER_VERSION={version}");
    println!("cargo:rerun-if-env-changed=OVERSEER_DISTRIBUTION");
    println!("cargo:rustc-check-cfg=cfg(self_update)");
    let distribution = std::env::var("OVERSEER_DISTRIBUTION").ok();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if compiles_self_update(distribution.as_deref(), &target_os) {
        println!("cargo:rustc-cfg=self_update");
    }
    std::fs::create_dir_all("resources/server").expect("create resources/server");
    tauri_build::build()
}
