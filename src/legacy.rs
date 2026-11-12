use crate::install::{commands, hook_binary};
use serde_json::Value;
use std::path::Path;

pub const NAME: &str = "switchboard";
pub const STATE_ENV: &str = "SWITCHBOARD_STATE_DIR";
pub const HOME_DIR: &str = ".switchboard";
pub const DB_FILE: &str = "switchboard.db";
pub const IMPORT_KEY: &str = "import.switchboard";
pub const HOOK_MARKER: &str = "#switchboard-hook";
pub const STATUSLINE_MARKER: &str = "#switchboard-statusline";
pub const UNIT: &str = "switchboard-daemon.service";
pub const LABEL: &str = "dev.switchboard.daemon";
pub const APP: &str = "fr.armtek.switchboard";
pub const WORKTREES: &str = "~/.switchboard/worktrees";

pub fn orphaned_hooks(v: &Value) -> bool {
    let old: Vec<String> = commands(v)
        .into_iter()
        .filter(|c| c.contains(HOOK_MARKER))
        .collect();
    !old.is_empty()
        && old
            .iter()
            .all(|c| hook_binary(c).is_some_and(|b| !Path::new(b).exists()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hooks(cmd: &str) -> Value {
        json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": cmd}]}]}})
    }

    #[test]
    fn hooks_calling_a_missing_binary_are_orphaned() {
        let gone =
            format!("SWITCHBOARD_STATE_DIR=/s '/nonexistent/sb' hook claude Stop {HOOK_MARKER}");
        assert!(orphaned_hooks(&hooks(&gone)));
    }

    #[test]
    fn hooks_calling_an_existing_binary_or_none_are_not_orphaned() {
        let live = format!("/bin/sh hook claude Stop {HOOK_MARKER}");
        assert!(!orphaned_hooks(&hooks(&live)));
        assert!(!orphaned_hooks(&hooks("notify.sh")));
        assert!(!orphaned_hooks(&json!({})));
    }

    #[test]
    fn names_keep_their_values() {
        assert_eq!(HOOK_MARKER, "#switchboard-hook");
        assert_eq!(STATUSLINE_MARKER, "#switchboard-statusline");
        assert_eq!(IMPORT_KEY, "import.switchboard");
        assert_eq!(LABEL, "dev.switchboard.daemon");
        assert_eq!(APP, "fr.armtek.switchboard");
        assert_eq!(UNIT, "switchboard-daemon.service");
        assert_eq!(WORKTREES, "~/.switchboard/worktrees");
        assert_eq!(STATE_ENV, "SWITCHBOARD_STATE_DIR");
        assert_eq!(HOME_DIR, ".switchboard");
        assert_eq!(DB_FILE, "switchboard.db");
        assert_eq!(NAME, "switchboard");
    }
}
