# 0038 Organize my sessions: reviewed proposals, deterministic or written by an agent

Status: accepted

## Context
After a few weeks a station holds hundreds of sessions: many repositories, worktrees per ticket,
agents idle for a month, sessions whose folder was deleted, tasks whose branch was merged. The
sidebar (ADR 0029) and the board organise them only if the user builds workspaces, folders and
groups by hand. Constraints from the existing model:

- Workspace membership is computed (task link, else longest path prefix, else repository), so
  "placing" a session means creating a workspace on a path, moving a workspace into a folder, or
  adding `line:<id>` to a group; there is nothing to write on the line itself.
- Lines have no archive. Hold is the reversible way to put a session away (Resume brings it back);
  `close` marks it hung-up. A project is archived through `workspace.archived`.
- Sessions older than `sessionTasksDays` have no session task; a new action creates one.
- Merged state comes from the `worktrees::landed` cache, which may not know yet; organizing must
  never wait on git.
- The board organizer (`task_agents::organize_one`) already runs an agent as a task with its own
  origin, and the MCP server already gates tools by the caller task's origin.

## Decision
- **Proposal, then review.** Every plan is a stored **proposal** (registry table `proposals`,
  last 20 kept, event `organize.proposal`). Schema v1: `{version, source, summary, actions,
  pickUp, reasons}` with a closed set of ten action kinds (`create_folder`, `create_workspace`,
  `move_workspace`, `create_group`, `hold`, `close`, `pin`, `archive_workspace`,
  `set_task_status`, `track`). Validation rejects unknown kinds, more than 500 actions, dangling
  ids or folder refs, and `close` of a busy or waiting line. Nothing runs until the user ticks
  items and calls `apply`.
- **Apply** runs only the ticked actions, in plan order, through the existing functions
  (workspaces, folders, groups, the hold and close checks, the pin mark, task patch, session task
  insert). It is idempotent (already in effect → `skipped`), reports one result per item, never
  stops on a failure, and logs to `actions.log`. It reads the cached lines, except one fresh scan
  per Apply that holds or closes, so the hold checks see the current state. Lines of another station are held or closed
  through the relay and refused while it is offline.
- **Deterministic path** (`POST /v1/organize/deterministic`): offline rules over the cached lines,
  the workspace index, tasks, the `landed` cache (`unknown` counts as not merged) and the activity
  cache: workspace per repository, folder per owner, group per shared ticket key, hold after 30
  idle days, close when the folder is gone, completed when the branch landed, archive of idle
  workspaces, and up to 40 "pick up" sessions ranked by edited files, then open todos, then
  recency.
- **Agent path** (`POST /v1/organize/start`): a task with origin `configurator`, effort high, run
  in place in `<state>/organize`, like the board organizer. Its prompt reuses the GitHub review
  wording ("stop and wait, never act") and the heuristics above, and asks it to call
  `propose_organization` exactly once, never edit files, never run git write commands. Read-only
  MCP tools give it the data (`list_sessions`, `get_session_brief`, `list_projects`,
  `list_worktrees`, `list_layout`); `propose_organization` is accepted only from a configurator
  task, which is in turn refused `create_task`, `set_task_status` and `prioritize_task`. Claude
  gets `--mcp-config` with `overseer mcp` on its command line per launch, so nothing is
  written to the user's config; other agents need the server registered, and the app offers the
  deterministic path when no proposal arrives within 5 minutes.
- **App**: one entry ("Organize my sessions…") in the palette, Settings > Help and the phone More
  sheet, a choice between Quick and the default agent, a plan review checklist (all ticked except
  `close`, reasons shown, per-item results) and a "Pick up where you left off" sheet. A sidebar
  nudge appears when more than 15 recent sessions have no workspace, counted from the membership
  index the sidebar already has; a tour step announces it under What's new.

## Rejected
- Letting the agent act directly (MCP write tools for workspaces, holds): one wrong call on a busy
  agent loses work; a reviewed plan costs one click.
- Storing a workspace or archive flag on lines: membership is derived on purpose (ADR 0029).
- Rescanning git (status, merge base) to rank sessions: the deterministic path must stay instant
  and idle-cheap; edited files from the activity cache stand in for "dirty".
- Writing an MCP entry into `~/.claude.json` or `~/.codex/config.toml`: global side effect.

## Consequences
- Plans can be stale by the time they are applied; apply re-checks each item (hold checks, busy
  state, existing names) and reports instead of failing the batch.
- A merge the `landed` cache has not seen yet is missed until the next run.
- The configurator task appears on the board like the organizer, hidden from the columns, and
  completes when its proposal is recorded.
