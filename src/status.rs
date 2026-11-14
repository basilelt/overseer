macro_rules! statuses {
    ($($variant:ident => $wire:literal),+ $(,)?) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Status {
            $($variant),+
        }

        impl Status {
            pub const ALL: &'static [Status] = &[$(Status::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Status::$variant => $wire),+
                }
            }
        }
    };
}

statuses! {
    Pending => "pending",
    Auto => "auto",
    InProgress => "in_progress",
    InTesting => "in_testing",
    Merged => "merged",
    Completed => "completed",
}

impl Status {
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|x| x.as_str() == s)
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
        let wire: Vec<&str> = Status::ALL.iter().map(|s| s.as_str()).collect();
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
        for &s in Status::ALL {
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
        let done: Vec<&str> = Status::ALL
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

    #[test]
    fn every_status_is_in_each_set_or_deliberately_excluded() {
        let sets: [(&str, &[&str], &[Status]); 5] = [
            ("LIST_FILTER", &LIST_FILTER, &[]),
            (
                "SESSION_TASK_SETTABLE",
                &SESSION_TASK_SETTABLE,
                &[Status::Pending, Status::Auto],
            ),
            (
                "TASK_PATCH_SETTABLE",
                &TASK_PATCH_SETTABLE,
                &[Status::InProgress],
            ),
            (
                "PROPOSAL_SETTABLE",
                &PROPOSAL_SETTABLE,
                &[Status::Auto, Status::InProgress],
            ),
            (
                "AGENT_SETTABLE",
                &AGENT_SETTABLE,
                &[Status::Pending, Status::Auto, Status::Completed],
            ),
        ];
        for (name, set, excluded) in sets {
            for s in Status::ALL {
                assert_eq!(
                    set.contains(&s.as_str()),
                    !excluded.contains(s),
                    "{name}: decide whether {s:?} belongs"
                );
            }
        }
        let create_excluded = [
            Status::InProgress,
            Status::InTesting,
            Status::Merged,
            Status::Completed,
        ];
        for s in Status::ALL {
            assert_eq!(
                TASK_CREATE_SETTABLE.contains(&s.as_str()),
                !create_excluded.contains(s),
                "TASK_CREATE_SETTABLE: decide whether {s:?} belongs"
            );
        }
    }
}
