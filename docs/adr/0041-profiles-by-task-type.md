# 0041 Agent profiles by task type

Status: accepted

## Context
ADR 0037 resolves a profile by explicit choice, then by the longest workspace path, then by the
default. The right model and effort depend more on the kind of task than on the repo: a small fix
and a large migration in the same repo should not start the same way. Anthropic's model guide
("Which Claude model should you build with?") recommends that code pick the model by task type
instead of a model routing itself (issue #272).

## Decision
- **A fixed list of types** in code (`profiles::TASK_TYPES`): `fix`, `feature`, `engineering`,
  `refactor` (migration, large refactor, architecture), `triage` (search, logs, sorting). A short
  closed list keeps the form and the rules readable and lets the daemon validate both.
- **The type is stored on the task** (`tasks.task_type`, `taskType` on `POST`/`PATCH
  /v1/tasks`). It is set when the task is created or edited, never fetched from the tracker at
  launch: a `gh`/`jira` call per start would fail offline and add work at every start.
- **Rules live in `profiles.json`** as `types: {type: profileId}`, validated like `scopes` (known
  type, known profile or `none`). The map ships empty, so nothing changes until the user maps a
  type. No built-in profile is added for the suggested map; users build those profiles.
- **Resolution**: explicit task profile > type rule > workspace scope > default. A rule pointing
  at a deleted profile, or an unknown type, falls through instead of failing a start. A task
  created with a type whose profile sets an effort stores an empty effort, as an explicit profile
  does, so the profile's effort applies; `PATCH` accepts `"effort": ""` to clear it later.
- **Imported tickets** get a type from a pure mapping (`profiles::task_type_from`): GitHub and
  GitLab labels (scoped `type::bug` included), Jira issue type (`type` column of `jira issue
  list`). The form shows it and the user can change it before creating the task.
- **Explore subagent** (verified in the Claude Code 2.1.322 bundle): the built-in `Explore`
  declares `model: "inherit"`, so `CLAUDE_CODE_SUBAGENT_MODEL` does not reach it; agents are
  merged by name (built-in < plugin < user < project < flag < policy), so a
  `~/.claude/agents/Explore.md` replaces the built-in whole, prompt and tools included.
  `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` does not help: under it an agent model of `inherit` stays
  `inherit`. This is documented in the user guide; Overseer does not generate that file.

## Rejected
- Reading the type from the ticket at each start: network on the launch path, failures when
  offline, and a type that changes under a running board.
- Free-form types or label-to-profile rules: harder to validate and to show, and labels differ
  per tracker; the import maps them once onto the closed list.
- "Low then retry" (start at `low`, re-run failures at `high`): needs a pass/fail signal per task;
  left for later as the issue suggests.

## Consequences
- A resumed task session takes its env-only values (subagent and background models) from the
  workspace or default profile, as it already did for an explicit task profile (ADR 0037):
  `for_resume` knows only the directory.
- Deleting a profile in the editor also drops the type rules that point at it.
