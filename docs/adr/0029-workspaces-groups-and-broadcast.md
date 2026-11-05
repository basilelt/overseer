# 0029 Workspaces, groups and broadcast

Status: accepted

## Context
The app grew past one project at a time: users keep several repositories, plain folders (notes,
infra) and many agents open together, and want to arrange them in a sidebar, keep the open tabs
of each, and send the same instruction ("rebase on main", "stop and summarise") to several agents
at once. The daemon already knows projects (repositories seen in lines, config roots, tasks,
added projects), lines and terms, and can type into a term (task prompts) or a tmux pane.

## Decision
- Workspaces, folders and groups live in the daemon registry (new `CREATE TABLE IF NOT EXISTS`
  tables, no migration), so every client of a station sees the same layout. A workspace is any
  directory (canonical path, unique), optionally in a folder; folders nest one level. Pin, archive,
  rank, open tabs and the idle-notification preference are stored but opaque to the daemon.
- Every project gets an `auto` workspace the first time `GET /v1/workspaces` sees it. Deleting a
  workspace leaves a tombstone on its path so it does not come back; an explicit `POST` clears it.
  The route reads the daemon's last scan instead of scanning per request.
- Membership is computed, not stored: task link, else the longest path prefix unless the line's
  repository has its own workspace that does not contain that prefix (a workspace on `$HOME` must
  not swallow every worktree), else none. The daemon and the app implement the same rule.
- Groups are explicit lists of `line:<id>` / `term:<id>` keys, not checked against live objects
  (a line id survives restarts, a term id does not; the app prunes).
- `POST /v1/broadcast` targets ids, a workspace or a group. Each target is typed into through the
  line's daemon term, else its tmux pane (bracketed paste through a named buffer), using the same
  paste as task prompts. Only idle agents are typed into by default: needing input counts as busy
  (typing would answer a permission prompt), the busy state comes from hooks and session files,
  else from 1.5 s of PTY silence. Busy targets are skipped, typed anyway, or queued in daemon
  memory and delivered by the scan tick when they become idle (dropped after 1 h or when the
  target goes away), with a `broadcast.delivered` event.
- `actions.log` records each broadcast with its targets and results and the text length, never
  the text.
- The protocol stays 1: routes, schemas and events are additions.

## Consequences
- Queued broadcasts are lost when the daemon restarts; the caller saw `queued`, not `sent`.
- The idle decision uses the last scan, up to one interval stale; a target that turned busy since
  may receive the text.
- Group members that no longer exist are reported per target (`refused`) rather than rejected.
