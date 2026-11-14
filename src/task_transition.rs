use crate::registry::Registry;
use crate::status::{self, COMPLETED, IN_TESTING, MERGED};
use crate::tasks::{Res, Task};
use crate::Ctx;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Patch,
    Review,
    Reconcile,
    Landed,
}

impl Origin {
    pub(crate) fn leaves_merged_cleanup(self) -> bool {
        matches!(self, Origin::Patch | Origin::Review)
    }

    pub(crate) fn clears_session_state_on_completed(self) -> bool {
        self == Origin::Patch
    }

    pub(crate) fn runs_ready_hooks(self) -> bool {
        matches!(self, Origin::Patch | Origin::Reconcile)
    }

    pub(crate) fn ends_reviewer_on_completed(self) -> bool {
        self == Origin::Reconcile
    }

    pub(crate) fn stops_on_save_error(self) -> bool {
        matches!(self, Origin::Patch | Origin::Review)
    }
}

pub(crate) fn enter(ctx: &Ctx, reg: &Registry, t: &mut Task, was: &str, origin: Origin) -> Res<()> {
    if origin.leaves_merged_cleanup() {
        leave_merged(t, was);
    }
    if origin.clears_session_state_on_completed() && t.status == COMPLETED && was != COMPLETED {
        t.session_state = None;
    }
    let saved = crate::tasks::save(ctx, reg, t);
    if origin.stops_on_save_error() {
        saved?;
    }
    if origin.runs_ready_hooks() && t.status == IN_TESTING && was != IN_TESTING {
        crate::task_agents::spawn_overlap_alert(ctx, t);
        crate::quorum::on_ready(ctx, t);
    }
    if origin.ends_reviewer_on_completed() && t.status == COMPLETED {
        crate::quorum::reviewer_ended(ctx, reg, t);
    }
    Ok(())
}

fn leave_merged(t: &mut Task, was: &str) {
    if was == MERGED && t.status != MERGED {
        if let Some(b) = &t.branch {
            crate::worktrees::forget_landed(b);
        }
        if !t.from_session() && !status::is_done(&t.status) {
            t.session_state = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_origin_keeps_its_own_side_effects() {
        let table = [
            (Origin::Patch, [true, true, true, false, true]),
            (Origin::Review, [true, false, false, false, true]),
            (Origin::Reconcile, [false, false, true, true, false]),
            (Origin::Landed, [false, false, false, false, false]),
        ];
        for (o, want) in table {
            let got = [
                o.leaves_merged_cleanup(),
                o.clears_session_state_on_completed(),
                o.runs_ready_hooks(),
                o.ends_reviewer_on_completed(),
                o.stops_on_save_error(),
            ];
            assert_eq!(got, want, "{o:?}");
        }
    }

    #[test]
    fn leaving_merged_clears_the_marker_for_every_exit() {
        for to in ["in_progress", "pending", "in_testing"] {
            let mut t = Task {
                status: to.into(),
                session_state: Some("merged".into()),
                ..Default::default()
            };
            leave_merged(&mut t, "merged");
            assert_eq!(t.session_state, None, "{to}");
        }
        let mut t = Task {
            status: "completed".into(),
            session_state: Some("merged".into()),
            ..Default::default()
        };
        leave_merged(&mut t, "merged");
        assert_eq!(t.session_state.as_deref(), Some("merged"));
        let mut t = Task {
            status: "in_progress".into(),
            session_state: Some("x".into()),
            ..Default::default()
        };
        leave_merged(&mut t, "in_testing");
        assert_eq!(t.session_state.as_deref(), Some("x"));
    }
}
