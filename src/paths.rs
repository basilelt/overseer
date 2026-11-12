use std::path::PathBuf;

pub const OVERSEER_HOME: &str = ".overseer";
pub const USER_BIN: &str = ".local/bin";

pub const BIN_DIR: &str = "bin";
pub const SERVER_DIR: &str = "server";
pub const UPLOADS_DIR: &str = "u";
pub const TASKS_DIR: &str = "tasks";
pub const REVIEWS_DIR: &str = "reviews";
pub const WORKTREES_DIR: &str = "worktrees";

pub const HOME_DIRS: [&str; 5] = [BIN_DIR, SERVER_DIR, UPLOADS_DIR, TASKS_DIR, REVIEWS_DIR];
pub const MIGRATED_HOME_DIRS: [&str; 3] = [UPLOADS_DIR, TASKS_DIR, REVIEWS_DIR];

pub const PROFILES_FILE: &str = "profiles.json";
pub const BOARD_FILE: &str = "board.json";
pub const USER_FILE: &str = "user.json";
pub const SYNC_FILE: &str = "sync.json";
pub const PROJECTS_FILE: &str = "projects.json";

pub const MIGRATED_STATE_FILES: [&str; 5] = [
    PROFILES_FILE,
    BOARD_FILE,
    USER_FILE,
    SYNC_FILE,
    PROJECTS_FILE,
];

pub fn overseer_home() -> PathBuf {
    crate::home().join(OVERSEER_HOME)
}

pub fn bin_dir() -> PathBuf {
    overseer_home().join(BIN_DIR)
}

pub fn server_dir() -> PathBuf {
    overseer_home().join(SERVER_DIR)
}

pub fn reviews_dir() -> PathBuf {
    overseer_home().join(REVIEWS_DIR)
}

pub fn default_worktrees_dir() -> PathBuf {
    overseer_home().join(WORKTREES_DIR)
}

pub fn user_bin() -> PathBuf {
    crate::home().join(USER_BIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrated_dirs_are_a_subset_of_the_home_dirs() {
        assert!(MIGRATED_HOME_DIRS.iter().all(|d| HOME_DIRS.contains(d)));
    }

    #[test]
    fn paths_keep_their_names_under_the_home() {
        let h = crate::home();
        assert_eq!(overseer_home(), h.join(".overseer"));
        assert_eq!(bin_dir(), h.join(".overseer/bin"));
        assert_eq!(server_dir(), h.join(".overseer/server"));
        assert_eq!(reviews_dir(), h.join(".overseer/reviews"));
        assert_eq!(default_worktrees_dir(), h.join(".overseer/worktrees"));
        assert_eq!(user_bin(), h.join(".local/bin"));
        assert_eq!(HOME_DIRS, ["bin", "server", "u", "tasks", "reviews"]);
        assert_eq!(MIGRATED_HOME_DIRS, ["u", "tasks", "reviews"]);
        assert_eq!(
            MIGRATED_STATE_FILES,
            [
                "profiles.json",
                "board.json",
                "user.json",
                "sync.json",
                "projects.json"
            ]
        );
    }
}
