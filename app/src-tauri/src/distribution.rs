pub fn compiles_self_update(distribution: Option<&str>, target_os: &str) -> bool {
    distribution != Some("play") && target_os != "ios"
}

#[cfg(test)]
mod tests {
    use super::compiles_self_update;

    #[test]
    fn the_github_android_and_desktop_builds_compile_the_apk_updater() {
        assert!(compiles_self_update(None, "android"));
        assert!(compiles_self_update(Some("github"), "android"));
        assert!(compiles_self_update(None, "linux"));
        assert!(compiles_self_update(None, "macos"));
    }

    #[test]
    fn the_google_play_and_ios_builds_leave_it_out() {
        assert!(!compiles_self_update(Some("play"), "android"));
        assert!(!compiles_self_update(None, "ios"));
    }

    #[test]
    fn this_test_build_compiles_it() {
        let compiled: fn() -> bool = || cfg!(self_update);
        assert!(compiled());
    }
}
