use crate::registry::db;
use crate::registry::Registry;
use crate::status;
use crate::tasks::{self, bad, Res, Task};
use crate::util::ulid;
use crate::Ctx;
use serde_json::{json, Value};
use std::sync::Mutex;

pub const REVIEWER: &str = "reviewer";
pub const PREFIX: &str = "Quorum: ";
const MAX_REASON: usize = 2000;
const NO_VOTE: &str = "ended without voting";
static LOCK: Mutex<()> = Mutex::new(());

fn ids(q: &Value, key: &str, max: usize, err: &str) -> Res<Vec<&'static str>> {
    let list = q[key]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= max && a.iter().all(Value::is_string))
        .ok_or_else(|| bad(err))?;
    let mut out: Vec<&'static str> = vec![];
    for v in list {
        let id = v.as_str().unwrap_or_default();
        let a = crate::agents::get(id).ok_or_else(|| bad(format!("unknown agent {id}")))?;
        if !a.coding() {
            return Err(bad("shell cannot join a quorum"));
        }
        if out.contains(&a.id) {
            return Err(bad(format!("quorum.{key} lists {id} twice")));
        }
        out.push(a.id);
    }
    Ok(out)
}

pub fn parse(v: &Value, orchestrator: &str) -> Res<Option<Value>> {
    match v {
        Value::Null => return Ok(None),
        Value::Object(_) => {}
        _ => return Err(bad("quorum must be an object or null")),
    }
    let workers = ids(
        v,
        "workers",
        usize::MAX,
        "quorum.workers must be a non-empty array of agent ids",
    )?;
    let reviewers = ids(
        v,
        "reviewers",
        2,
        "quorum.reviewers must list 1 or 2 agent ids",
    )?;
    check_orchestrator(orchestrator)?;
    Ok(Some(json!({"workers": workers, "reviewers": reviewers})))
}

pub fn check_orchestrator(agent: &str) -> Res<()> {
    if crate::agents::get(agent).is_some_and(|a| a.orchestrator) {
        Ok(())
    } else {
        Err(bad("the quorum orchestrator must be claude or codex"))
    }
}

fn list(q: &Value, key: &str) -> Vec<String> {
    q[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

pub fn is_reviewer(t: &Task) -> bool {
    t.origin.as_deref() == Some(REVIEWER)
}

pub fn needed(voters: usize) -> usize {
    voters / 2 + 1
}

pub fn outcome(votes: &[Value], voters: usize) -> Option<&'static str> {
    let yes = votes.iter().filter(|v| v["approve"] == true).count();
    let no = votes.len() - yes;
    if yes >= needed(voters) {
        Some("accepted")
    } else if no > voters - needed(voters) {
        Some("rejected")
    } else {
        None
    }
}

pub fn child_agent(parent: &Task, asked: Option<&'static str>) -> Res<Option<&'static str>> {
    let Some(q) = &parent.quorum else {
        return Ok(asked);
    };
    let workers = list(q, "workers");
    match asked {
        None => Ok(workers
            .first()
            .and_then(|w| crate::agents::get(w))
            .map(|a| a.id)),
        Some(a) if workers.iter().any(|w| w == a) => Ok(Some(a)),
        Some(a) => Err(bad(format!(
            "agent {a} is not a worker of this quorum (workers: {})",
            workers.join(", ")
        ))),
    }
}

pub fn orchestrator_prompt(q: &Value) -> String {
    let workers = list(q, "workers").join(", ");
    let reviewers = list(q, "reviewers");
    let voters = reviewers.len() + 1;
    format!(
        "Quorum mode: you are the orchestrator of this task.\n\
1. Split the task into subtasks. Create each with create_task and set its agent to one of the workers ({workers}), \
spreading them across different workers so that different agents check each other. Use dependsOn where order matters. \
Subtasks start on their own, each in its own worktree and branch.\n\
2. When a subtask is ready, Overseer messages you and starts its reviewers ({}). Inspect the subtask's worktree too \
and vote with cast_vote (taskId, approve, reason). The result is accepted on {} of {voters} votes (the reviewers and you).\n\
3. Overseer messages you the outcome. Accepted: merge the subtask's branch into your own branch in your worktree. \
Rejected: send_message the subtask with the reasons to fix; it is reviewed again when it is ready.\n\
4. Wait for Overseer's messages instead of polling list_tasks. When every subtask is accepted and merged into your \
branch, run the tests, then call set_task_status in_testing.",
        reviewers.join(", "),
        needed(voters),
    )
}

fn reviewer_prompt(root: &Task, w: &Task, round: u64, base: &str) -> String {
    let path = w.worktree_path.as_deref().unwrap_or(&w.project);
    let branch = w.branch.as_deref().unwrap_or("its branch");
    format!(
        "You are a quorum reviewer for task #{} \"{}\". Review the result of subtask #{} \"{}\" made by {} (round {round}).\n\
Subtask description:\n{}\n\n\
Its worktree is {path} on {branch}, branched from {base}. Inspect the committed and uncommitted changes \
(git -C {path} status, git -C {path} diff, git -C {path} log {base}..HEAD -p), run the relevant tests, and check \
that the change does what the subtask asks without breaking anything. Never edit files.\n\
Then vote exactly once: with the cast_vote tool (approve, reason) if you have it, else run \
overseer vote approve \"<reason>\" or overseer vote reject \"<reason>\" in a shell. \
The reason is one or two sentences naming the concrete problems, if any. Stop after voting.",
        root.number,
        root.title,
        w.number,
        w.title,
        w.agent,
        w.description.trim(),
    )
}

fn root_of(reg: &Registry, t: &Task) -> Option<Task> {
    if is_reviewer(t) {
        return None;
    }
    let p = tasks::get(reg, t.parent_id.as_deref()?).ok()?;
    p.quorum.is_some().then_some(p)
}

pub fn reviewer_cwd(reg: &Registry, t: &Task) -> Option<String> {
    if !is_reviewer(t) {
        return None;
    }
    let w = tasks::get(reg, t.parent_id.as_deref()?).ok()?;
    w.worktree_path.filter(|p| std::path::Path::new(p).is_dir())
}

fn tell(ctx: &Ctx, to: &str, body: String) {
    let body: String = body.chars().take(7900).collect();
    let _ = crate::task_agents::send_message(ctx, to, &json!({ "body": body }));
}

fn stop_reviewers(ctx: &Ctx, reg: &Registry, worker: &str) {
    for mut r in tasks::all(reg).unwrap_or_default().into_iter().filter(|r| {
        is_reviewer(r) && r.parent_id.as_deref() == Some(worker) && !tasks::is_done(&r.status)
    }) {
        if let Some(term) = r.live_term() {
            let _ = term.kill();
        }
        r.status = status::COMPLETED.into();
        let _ = tasks::store(reg, &mut r, &ctx.config.station_id);
    }
}

fn effort_for(agent: &str) -> &'static str {
    match crate::agents::get(agent) {
        Some(a) if a.valid_efforts().contains(&"high") => "high",
        _ => "",
    }
}

pub fn on_ready(ctx: &Ctx, worker: &Task) {
    let Ok(reg) = db(ctx) else {
        return;
    };
    let Some(root) = root_of(&reg, worker) else {
        return;
    };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(mut w) = tasks::get(&reg, &worker.id) else {
        return;
    };
    let reviewers = list(root.quorum.as_ref().unwrap_or(&Value::Null), "reviewers");
    let round = w
        .votes
        .as_ref()
        .and_then(|v| v["round"].as_u64())
        .unwrap_or(0)
        + 1;
    stop_reviewers(ctx, &reg, &w.id);
    let voters = reviewers.len() + 1;
    w.votes = Some(
        json!({"round": round, "voters": voters, "needed": needed(voters),
                         "votes": [], "outcome": null}),
    );
    if tasks::store(&reg, &mut w, &ctx.config.station_id).is_err() {
        return;
    }
    let base = crate::git::git(&w.project, &["symbolic-ref", "-q", "--short", "HEAD"])
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "the base branch".into());
    let mut started = vec![];
    for (i, agent) in reviewers.iter().enumerate() {
        let now = crate::procs::now_ms();
        let mut r = Task {
            id: ulid(now + i as i64),
            title: format!("Review #{} (round {round})", w.number),
            description: reviewer_prompt(&root, &w, round, &base),
            project: w.project.clone(),
            status: status::AUTO.into(),
            agent: agent.clone(),
            effort: effort_for(agent).into(),
            origin: Some(REVIEWER.into()),
            parent_id: Some(w.id.clone()),
            created_ms: now,
            updated_ms: now,
            rank: now as f64,
            ..Default::default()
        };
        if tasks::insert(&reg, &mut r, &ctx.config.station_id).is_ok() {
            started.push(format!("#{} ({agent})", r.number));
        }
    }
    tell(
        ctx,
        &root.id,
        format!(
            "{PREFIX}subtask #{} \"{}\" ({}) is ready for review, round {round}. Reviewers: {}. \
Inspect its worktree {} and vote with cast_vote (taskId {}, approve, reason).",
            w.number,
            w.title,
            w.agent,
            started.join(", "),
            w.worktree_path.as_deref().unwrap_or(&w.project),
            w.id,
        ),
    );
}

fn voter_role(w: &Task, from: &Task) -> Res<&'static str> {
    if w.parent_id.as_deref() == Some(from.id.as_str()) && from.quorum.is_some() {
        return Ok("orchestrator");
    }
    if is_reviewer(from)
        && from.parent_id.as_deref() == Some(w.id.as_str())
        && !tasks::is_done(&from.status)
    {
        return Ok(REVIEWER);
    }
    Err((
        403,
        format!("task #{} is not a voter on task #{}", from.number, w.number),
    ))
}

pub fn vote(ctx: &Ctx, id: &str, b: &Value) -> Res<Task> {
    let approve = b["approve"]
        .as_bool()
        .ok_or_else(|| bad("approve must be a boolean"))?;
    let reason = b["reason"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("reason required"))?;
    if reason.chars().count() > MAX_REASON {
        return Err(bad(format!(
            "reason must be at most {MAX_REASON} characters"
        )));
    }
    let from = b["from"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| bad("from must be the voter's task id"))?;
    let reg = db(ctx)?;
    let from = tasks::get(&reg, from).map_err(|_| bad(format!("from {from} is not a task")))?;
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let w = tasks::get(&reg, id)?;
    record(ctx, &reg, w, &from, approve, reason)
}

fn record(
    ctx: &Ctx,
    reg: &Registry,
    mut w: Task,
    from: &Task,
    approve: bool,
    reason: &str,
) -> Res<Task> {
    let role = voter_role(&w, from);
    let Some(state) = w.votes.as_mut() else {
        return Err((409, format!("no vote is open on task #{}", w.number)));
    };
    if let Some(o) = state["outcome"].as_str() {
        return Err((
            409,
            format!("the vote on task #{} is closed ({o})", w.number),
        ));
    }
    let role = role?;
    let votes = state["votes"].as_array().cloned().unwrap_or_default();
    if votes.iter().any(|v| v["from"] == from.id.as_str()) {
        return Err((409, format!("task #{} already voted", from.number)));
    }
    let mut votes = votes;
    votes.push(
        json!({"from": from.id, "number": from.number, "agent": from.agent,
                      "role": role, "approve": approve, "reason": reason}),
    );
    let voters = state["voters"].as_u64().unwrap_or(1) as usize;
    let decided = outcome(&votes, voters);
    state["votes"] = json!(votes);
    state["outcome"] = json!(decided);
    let round = state["round"].as_u64().unwrap_or(1);
    if decided == Some("rejected") && w.status == status::IN_TESTING && w.live_term().is_some() {
        w.status = status::IN_PROGRESS.into();
        w.started_ms = Some(crate::procs::now_ms());
    }
    tasks::store(reg, &mut w, &ctx.config.station_id)?;
    if let Some(o) = decided {
        stop_reviewers(ctx, reg, &w.id);
        if let Some(root) = w.parent_id.as_deref() {
            tell(ctx, root, verdict(&w, round, o, &votes, voters));
        }
    }
    Ok(w)
}

fn verdict(w: &Task, round: u64, o: &str, votes: &[Value], voters: usize) -> String {
    let yes = votes.iter().filter(|v| v["approve"] == true).count();
    let lines: Vec<String> = votes
        .iter()
        .map(|v| {
            format!(
                "- #{} {} ({}): {} - {}",
                v["number"],
                v["agent"].as_str().unwrap_or_default(),
                v["role"].as_str().unwrap_or_default(),
                if v["approve"] == true {
                    "approve"
                } else {
                    "reject"
                },
                v["reason"].as_str().unwrap_or_default()
            )
        })
        .collect();
    let next = if o == "accepted" {
        format!(
            "Merge its branch {} into your branch.",
            w.branch.as_deref().unwrap_or("(none)")
        )
    } else {
        format!(
            "Send the fixes to it with send_message (to: {}); it is reviewed again when its next turn ends.",
            w.id
        )
    };
    format!(
        "{PREFIX}subtask #{} \"{}\" is {o} in round {round} ({yes} of {voters} approve, {} needed).\n{}\n{next}",
        w.number,
        w.title,
        needed(voters),
        lines.join("\n")
    )
}

pub fn reviewer_ended(ctx: &Ctx, reg: &Registry, r: &Task) {
    if !is_reviewer(r) {
        return;
    }
    let Some(wid) = r.parent_id.as_deref() else {
        return;
    };
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(w) = tasks::get(reg, wid) else {
        return;
    };
    let voted = w.votes.as_ref().is_some_and(|s| {
        s["outcome"].is_string()
            || s["votes"]
                .as_array()
                .is_some_and(|vs| vs.iter().any(|v| v["from"] == r.id.as_str()))
    });
    if voted {
        return;
    }
    let mut open = r.clone();
    open.status = status::IN_PROGRESS.into();
    let _ = record(ctx, reg, w, &open, false, NO_VOTE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_a_quorum_and_normalizes_it() {
        let q = parse(
            &json!({"workers": ["claude", "codex"], "reviewers": ["gemini"]}),
            "claude",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            q,
            json!({"workers": ["claude", "codex"], "reviewers": ["gemini"]})
        );
        assert_eq!(parse(&Value::Null, "gemini").unwrap(), None);
    }

    #[test]
    fn parse_refuses_bad_shapes_with_the_contract_messages() {
        let err = |v: Value, o: &str| parse(&v, o).unwrap_err().1;
        assert_eq!(
            err(json!("x"), "claude"),
            "quorum must be an object or null"
        );
        assert_eq!(
            err(json!({"workers": [], "reviewers": ["codex"]}), "claude"),
            "quorum.workers must be a non-empty array of agent ids"
        );
        assert_eq!(
            err(json!({"workers": [1], "reviewers": ["codex"]}), "claude"),
            "quorum.workers must be a non-empty array of agent ids"
        );
        assert_eq!(
            err(json!({"workers": ["codex"]}), "claude"),
            "quorum.reviewers must list 1 or 2 agent ids"
        );
        assert_eq!(
            err(
                json!({"workers": ["codex"], "reviewers": ["a", "b", "c"]}),
                "claude"
            ),
            "quorum.reviewers must list 1 or 2 agent ids"
        );
        assert_eq!(
            err(
                json!({"workers": ["nope"], "reviewers": ["codex"]}),
                "claude"
            ),
            "unknown agent nope"
        );
        assert_eq!(
            err(
                json!({"workers": ["shell"], "reviewers": ["codex"]}),
                "claude"
            ),
            "shell cannot join a quorum"
        );
        assert_eq!(
            err(
                json!({"workers": ["codex", "codex"], "reviewers": ["codex"]}),
                "claude"
            ),
            "quorum.workers lists codex twice"
        );
        assert_eq!(
            err(
                json!({"workers": ["codex"], "reviewers": ["gemini", "gemini"]}),
                "claude"
            ),
            "quorum.reviewers lists gemini twice"
        );
        assert_eq!(
            err(
                json!({"workers": ["codex"], "reviewers": ["claude"]}),
                "gemini"
            ),
            "the quorum orchestrator must be claude or codex"
        );
    }

    #[test]
    fn a_strict_majority_of_all_voters_decides() {
        let v = |a: bool| json!({"approve": a});
        assert_eq!(needed(2), 2);
        assert_eq!(needed(3), 2);
        assert_eq!(outcome(&[v(true)], 2), None);
        assert_eq!(outcome(&[v(true), v(true)], 2), Some("accepted"));
        assert_eq!(outcome(&[v(false)], 2), Some("rejected"));
        assert_eq!(outcome(&[v(true)], 3), None);
        assert_eq!(outcome(&[v(true), v(false)], 3), None);
        assert_eq!(outcome(&[v(true), v(false), v(true)], 3), Some("accepted"));
        assert_eq!(outcome(&[v(false), v(false)], 3), Some("rejected"));
    }

    #[test]
    fn child_agent_defaults_to_the_first_worker_and_refuses_others() {
        let mut p = Task::default();
        assert_eq!(child_agent(&p, Some("gemini")).unwrap(), Some("gemini"));
        p.quorum = Some(json!({"workers": ["codex", "gemini"], "reviewers": ["claude"]}));
        assert_eq!(child_agent(&p, None).unwrap(), Some("codex"));
        assert_eq!(child_agent(&p, Some("gemini")).unwrap(), Some("gemini"));
        assert_eq!(
            child_agent(&p, Some("claude")).unwrap_err().1,
            "agent claude is not a worker of this quorum (workers: codex, gemini)"
        );
    }

    #[test]
    fn the_orchestrator_prompt_names_workers_reviewers_and_the_majority() {
        let p = orchestrator_prompt(
            &json!({"workers": ["codex", "gemini"], "reviewers": ["claude", "codex"]}),
        );
        assert!(p.contains("(codex, gemini)"));
        assert!(p.contains("reviewers (claude, codex)"));
        assert!(p.contains("2 of 3 votes"));
        assert!(p.contains("instead of polling"));
    }

    #[test]
    fn the_verdict_lists_each_vote_and_the_next_step() {
        let w = Task {
            id: "W".into(),
            number: 4,
            title: "Parser".into(),
            branch: Some("task/4-parser".into()),
            ..Default::default()
        };
        let votes = [
            json!({"number": 5, "agent": "codex", "role": "reviewer", "approve": true, "reason": "ok"}),
            json!({"number": 1, "agent": "claude", "role": "orchestrator", "approve": true, "reason": "fine"}),
        ];
        let t = verdict(&w, 1, "accepted", &votes, 3);
        assert!(t.starts_with(PREFIX));
        assert!(t.contains("2 of 3 approve, 2 needed"));
        assert!(t.contains("- #5 codex (reviewer): approve - ok"));
        assert!(t.contains("Merge its branch task/4-parser"));
        let t = verdict(&w, 2, "rejected", &votes[..1], 2);
        assert!(t.contains("send_message (to: W)"));
    }
}
