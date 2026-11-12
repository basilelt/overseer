use std::path::PathBuf;

macro_rules! home_names {
    ($home:literal, $uploads:literal, $tasks:literal) => {
        pub const OVERSEER_HOME: &str = $home;
        pub const UPLOADS_DIR: &str = $uploads;
        pub const TASKS_DIR: &str = $tasks;
        pub const SHORT_UPLOADS_REL: &str = concat!($home, "/", $uploads);
        pub const TASK_UPLOADS_REL: &str = concat!($home, "/", $tasks);
    };
}

home_names!(".overseer", "u", "tasks");
pub const USER_BIN: &str = ".local/bin";

pub const BIN_DIR: &str = "bin";
pub const SERVER_DIR: &str = "server";
pub const REVIEWS_DIR: &str = "reviews";
pub const WORKTREES_DIR: &str = "worktrees";

pub const HOME_DIRS: [&str; 5] = [BIN_DIR, SERVER_DIR, UPLOADS_DIR, TASKS_DIR, REVIEWS_DIR];
pub const MIGRATED_HOME_DIRS: [&str; 3] = [UPLOADS_DIR, TASKS_DIR, REVIEWS_DIR];

pub const PROFILES_FILE: &str = "profiles.json";
pub const BOARD_FILE: &str = "board.json";
pub const USER_FILE: &str = "user.json";
pub const SYNC_FILE: &str = "sync.json";
pub const PROJECTS_FILE: &str = "projects.json";

pub const REGISTRY_DB: &str = "overseer.db";
pub const SYNC_DB: &str = "sync.db";
pub const USAGE_DB: &str = "usage.db";
pub const STATE_DBS: [&str; 3] = [REGISTRY_DB, SYNC_DB, USAGE_DB];

pub const COMMANDS_DIR: &str = "commands";
pub const INBOX_DIR: &str = "inbox";
pub const DONE_DIR: &str = "done";
pub const RUNNING_DIR: &str = "running";
pub const STATIONS_DIR: &str = "stations";
pub const TMP_DIR: &str = "tmp";
pub const STATE_UPLOADS_DIR: &str = "uploads";

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
    fn relative_upload_paths_keep_their_values() {
        assert_eq!(SHORT_UPLOADS_REL, ".overseer/u");
        assert_eq!(TASK_UPLOADS_REL, ".overseer/tasks");
    }

    #[test]
    fn state_names_keep_their_values() {
        assert_eq!(STATE_DBS, ["overseer.db", "sync.db", "usage.db"]);
        assert_eq!(
            [
                COMMANDS_DIR,
                INBOX_DIR,
                DONE_DIR,
                RUNNING_DIR,
                STATIONS_DIR,
                TMP_DIR,
                STATE_UPLOADS_DIR
            ],
            ["commands", "inbox", "done", "running", "stations", "tmp", "uploads"]
        );
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
