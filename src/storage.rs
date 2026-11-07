use std::path::{Path, PathBuf};

pub fn legacy_dir() -> PathBuf {
    crate::home().join(".overseer")
}

pub fn legacy_worktree_dir() -> Option<PathBuf> {
    Some(legacy_dir().join("worktrees")).filter(|p| p.is_dir())
}

pub fn legacy_reviews_dir() -> PathBuf {
    legacy_dir().join("reviews")
}

pub fn uploads_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("u")
}

pub fn tasks_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("tasks")
}

pub fn reviews_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("reviews")
}

pub fn shim_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("bin")
}

const MOVED: &[&str] = &["u", "tasks"];

pub fn migrate(state_dir: &Path, legacy: &Path) -> Vec<String> {
    let mut log = Vec::new();
    for name in MOVED {
        let from = legacy.join(name);
        let is_link = std::fs::symlink_metadata(&from).is_ok_and(|m| m.file_type().is_symlink());
        if is_link || !from.is_dir() {
            continue;
        }
        let to = state_dir.join(name);
        match move_dir(&from, &to) {
            Ok(n) => log.push(format!(
                "moved {n} entries from {} to {}",
                from.display(),
                to.display()
            )),
            Err(e) => log.push(format!(
                "kept {} where it is, could not move it to {}: {e}",
                from.display(),
                to.display()
            )),
        }
    }
    log
}

fn move_dir(from: &Path, to: &Path) -> std::io::Result<usize> {
    if !to.exists() {
        std::fs::rename(from, to)?;
        link_back(from, to);
        return Ok(1);
    }
    let mut moved = 0;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dest = to.join(e.file_name());
        if dest.symlink_metadata().is_ok() {
            continue;
        }
        std::fs::rename(e.path(), dest)?;
        moved += 1;
    }
    if std::fs::remove_dir(from).is_ok() {
        link_back(from, to);
    }
    Ok(moved)
}

#[cfg(unix)]
fn link_back(from: &Path, to: &Path) {
    let _ = std::os::unix::fs::symlink(to, from);
}

#[cfg(not(unix))]
fn link_back(_from: &Path, _to: &Path) {}

pub fn link_legacy_shim(legacy: &Path, shim: &Path) -> std::io::Result<bool> {
    let old = legacy
        .join("bin")
        .join(shim.file_name().unwrap_or_default());
    if old == shim || old.symlink_metadata().is_err() || !shim.exists() {
        return Ok(false);
    }
    crate::install::ensure_shim(&old, shim, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sb-storage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    fn text(p: &Path) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn paths_live_under_the_state_dir() {
        let s = Path::new("/st");
        assert_eq!(uploads_dir(s), Path::new("/st/u"));
        assert_eq!(tasks_dir(s), Path::new("/st/tasks"));
        assert_eq!(reviews_dir(s), Path::new("/st/reviews"));
        assert_eq!(shim_dir(s), Path::new("/st/bin"));
    }

    #[test]
    fn worktrees_default_to_the_state_dir_unless_configured() {
        let d = tmp("cfg");
        assert_eq!(crate::load_config(&d).worktree_dir, d.join("worktrees"));
        put(&d.join("config.json"), r#"{"worktreeDir": "/srv/wt"}"#);
        assert_eq!(crate::load_config(&d).worktree_dir, Path::new("/srv/wt"));
    }

    #[cfg(unix)]
    #[test]
    fn migrate_moves_uploads_and_tasks_and_keeps_old_paths_readable() {
        let d = tmp("move");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        std::fs::create_dir_all(&state).unwrap();
        put(&legacy.join("u/a1b2c3.png"), "img");
        put(&legacy.join("tasks/t1/note.txt"), "note");
        let log = migrate(&state, &legacy);
        assert_eq!(log.len(), 2, "{log:?}");
        assert_eq!(text(&state.join("u/a1b2c3.png")), "img");
        assert_eq!(text(&state.join("tasks/t1/note.txt")), "note");
        assert_eq!(
            std::fs::read_link(legacy.join("u")).unwrap(),
            state.join("u")
        );
        assert_eq!(text(&legacy.join("u/a1b2c3.png")), "img");
        assert_eq!(text(&legacy.join("tasks/t1/note.txt")), "note");
    }

    #[cfg(unix)]
    #[test]
    fn migrate_is_idempotent() {
        let d = tmp("idem");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        std::fs::create_dir_all(&state).unwrap();
        put(&legacy.join("u/a.txt"), "a");
        assert_eq!(migrate(&state, &legacy).len(), 1);
        assert!(migrate(&state, &legacy).is_empty());
        assert_eq!(text(&state.join("u/a.txt")), "a");
    }

    #[cfg(unix)]
    #[test]
    fn migrate_merges_into_an_existing_destination_without_overwriting() {
        let d = tmp("merge");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        put(&legacy.join("u/old.txt"), "old");
        put(&state.join("u/new.txt"), "new");
        let log = migrate(&state, &legacy);
        assert!(log[0].contains("moved 1 entries"), "{log:?}");
        assert_eq!(text(&state.join("u/old.txt")), "old");
        assert_eq!(text(&state.join("u/new.txt")), "new");
        assert_eq!(text(&legacy.join("u/old.txt")), "old");
        assert!(legacy.join("u").is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn migrate_keeps_conflicting_legacy_files_instead_of_deleting_them() {
        let d = tmp("keep");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        put(&legacy.join("u/same.txt"), "legacy");
        put(&state.join("u/same.txt"), "new");
        migrate(&state, &legacy);
        assert!(!legacy.join("u").is_symlink());
        assert_eq!(text(&legacy.join("u/same.txt")), "legacy");
        assert_eq!(text(&state.join("u/same.txt")), "new");
    }

    #[test]
    fn migrate_ignores_a_missing_legacy_dir_and_leaves_worktrees_and_reviews() {
        let d = tmp("none");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        assert!(migrate(&state, &legacy).is_empty());
        put(&legacy.join("worktrees/r-1/b/f"), "w");
        put(&legacy.join("reviews/o/r/f"), "r");
        assert!(migrate(&state, &legacy).is_empty());
        assert!(legacy.join("worktrees/r-1/b/f").is_file());
        assert!(legacy.join("reviews/o/r/f").is_file());
        assert!(!state.join("worktrees").exists() && !state.join("reviews").exists());
    }

    #[cfg(unix)]
    #[test]
    fn legacy_shim_becomes_a_link_to_the_new_shim() {
        let d = tmp("shim");
        let (legacy, state) = (d.join("home/.overseer"), d.join("state"));
        let real = d.join("real");
        put(&real, "bin");
        let shim = state.join("bin/overseer");
        assert!(!link_legacy_shim(&legacy, &shim).unwrap());
        std::fs::create_dir_all(legacy.join("bin")).unwrap();
        std::os::unix::fs::symlink(&real, legacy.join("bin/overseer")).unwrap();
        assert!(!link_legacy_shim(&legacy, &shim).unwrap());
        crate::install::ensure_shim(&shim, &real, true).unwrap();
        assert!(link_legacy_shim(&legacy, &shim).unwrap());
        assert_eq!(
            std::fs::read_link(legacy.join("bin/overseer")).unwrap(),
            shim
        );
        assert!(!link_legacy_shim(&legacy, &shim).unwrap());
        assert_eq!(text(&legacy.join("bin/overseer")), "bin");
    }
}
