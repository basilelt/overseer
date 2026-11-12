#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Pending,
    Auto,
    InProgress,
    InTesting,
    Merged,
    Completed,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Auto => "auto",
            Status::InProgress => "in_progress",
            Status::InTesting => "in_testing",
            Status::Merged => "merged",
            Status::Completed => "completed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        ALL.iter().copied().find(|x| x.as_str() == s)
    }

    pub const fn is_done(self) -> bool {
        matches!(self, Status::Merged | Status::Completed)
    }
}

pub const PENDING: &str = Status::Pending.as_str();
pub const AUTO: &str = Status::Auto.as_str();
pub const IN_PROGRESS: &str = Status::InProgress.as_str();
pub const IN_TESTING: &str = Status::InTesting.as_str();
pub const MERGED: &str = Status::Merged.as_str();
pub const COMPLETED: &str = Status::Completed.as_str();

pub const ALL: [Status; 6] = [
    Status::Pending,
    Status::Auto,
    Status::InProgress,
    Status::InTesting,
    Status::Merged,
    Status::Completed,
];

pub const LIST_FILTER: [&str; 6] = [PENDING, AUTO, IN_PROGRESS, IN_TESTING, MERGED, COMPLETED];
pub const SESSION_TASK_SETTABLE: [&str; 4] = [IN_PROGRESS, IN_TESTING, MERGED, COMPLETED];
pub const TASK_PATCH_SETTABLE: [&str; 5] = [PENDING, AUTO, IN_TESTING, MERGED, COMPLETED];
pub const PROPOSAL_SETTABLE: [&str; 4] = [PENDING, IN_TESTING, MERGED, COMPLETED];
pub const TASK_CREATE_SETTABLE: [&str; 2] = [PENDING, AUTO];
pub const AGENT_SETTABLE: [&str; 3] = [IN_TESTING, MERGED, IN_PROGRESS];

pub fn is_unstarted(status: &str) -> bool {
    status == PENDING || status == AUTO
}

pub fn is_done(status: &str) -> bool {
    Status::parse(status).is_some_and(Status::is_done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_strings_are_frozen() {
        let wire: Vec<&str> = ALL.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            wire,
            [
                "pending",
                "auto",
                "in_progress",
                "in_testing",
                "merged",
                "completed"
            ]
        );
        for s in ALL {
            assert_eq!(Status::parse(s.as_str()), Some(s));
        }
        assert_eq!(Status::parse("done"), None);
        assert_eq!(Status::parse(""), None);
    }

    #[test]
    fn each_set_keeps_its_members_and_order() {
        assert_eq!(
            SESSION_TASK_SETTABLE.join(", "),
            "in_progress, in_testing, merged, completed"
        );
        assert_eq!(
            TASK_PATCH_SETTABLE.join(", "),
            "pending, auto, in_testing, merged, completed"
        );
        assert_eq!(
            PROPOSAL_SETTABLE.join(", "),
            "pending, in_testing, merged, completed"
        );
        assert_eq!(AGENT_SETTABLE.join(", "), "in_testing, merged, in_progress");
        assert_eq!(
            LIST_FILTER.join(", "),
            "pending, auto, in_progress, in_testing, merged, completed"
        );
    }

    #[test]
    fn only_merged_and_completed_are_done() {
        let done: Vec<&str> = ALL
            .iter()
            .filter(|s| s.is_done())
            .map(|s| s.as_str())
            .collect();
        assert_eq!(done, ["merged", "completed"]);
        assert!(is_unstarted("pending") && is_unstarted("auto") && !is_unstarted("in_progress"));
        assert_eq!(TASK_CREATE_SETTABLE.join(", "), "pending, auto");
        assert!(is_done("merged") && is_done("completed"));
        assert!(!is_done("in_testing") && !is_done("nonsense"));
    }
}
