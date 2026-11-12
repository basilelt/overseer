mod common;
use common::Fx;
use std::path::Path;

#[test]
fn test_path_exposes_no_host_dir_beyond_system_ones_and_a_tmux_link() {
    let fx = Fx::new("hermetic-path", 0);
    let path = fx
        .vars()
        .into_iter()
        .find(|(k, _)| *k == "PATH")
        .map(|(_, v)| v)
        .unwrap();
    let dirs: Vec<_> = std::env::split_paths(&path).collect();
    for d in &dirs {
        let ok = d.ends_with("tests/stubs")
            || ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
                .iter()
                .any(|s| d == Path::new(s))
            || d.ends_with("tmux-bin");
        assert!(ok, "host dir leaked into the test PATH: {}", d.display());
    }
    for d in dirs.iter().filter(|d| d.ends_with("tmux-bin")) {
        let names: Vec<_> = std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| !n.to_string_lossy().starts_with("tmux."))
            .collect();
        assert_eq!(names, ["tmux"]);
    }
}
