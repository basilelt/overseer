# Line states

`state` in the contract is one of `active | idle | held | hung-up`. Everything else (busy vs at
prompt, needs-input, orphan, pinned) is local registry data, not contract.

## Inputs

| Signal | Claude | Codex |
|---|---|---|
| process alive (`alive`) | pid from `~/.claude/sessions/<pid>.json`, `kill -0`, `procStart` = `/proc/<pid>/stat` starttime | process scan for `codex` with the session id / cwd |
| turn running (`busy`) | `UserPromptSubmit` → `Stop`/`StopFailure`; `status: busy` in `sessions/<pid>.json` | `UserPromptSubmit` → `Stop`; `event_msg` turn start/complete |
| waiting for user (`needsInput`) | `Notification`, `PermissionRequest`, `Elicitation` | `PermissionRequest` |
| activity time (`lastActivityAt`) | max(hook event time, transcript mtime) | max(hook event time, rollout mtime, `threads.updated_at`) |
| held marker | registry `held=true` + tmux pane `@sb_held=<lineId>` | same |
| closed | `overseer close`, PR merged/closed (relayed by API) | same |

`idleAfter` is configurable, default 10 min.

## State machine

```
              discovered/started                     idleAfter elapsed, alive, not busy
   (none) ───────────────────────────▶ active ─────────────────────────────────────▶ idle
                                         ▲  ◀────────── any hook event / new activity ─┘
                                         │
                            resume ok    │      hold (all predicates pass)
       held ─────────────────────────────┘◀──────────────── active | idle
         ▲  └──── process found alive again (user relaunched manually) ──▶ active
         │
         └── process gone, not held on purpose, session file exists ──── (see "dead" below)

   active | idle | held ──── close | PR merged/closed ──▶ hung-up   (terminal)
```

Transitions:

| From | To | Trigger |
|---|---|---|
| — | active | new session file / `SessionStart` / process seen |
| active | idle | alive, not busy, `now - lastActivityAt > idleAfter` |
| idle | active | any hook event, transcript/rollout mtime advance, `busy` |
| active, idle | held | `hold` command after predicates pass; daemon stops the process and writes `held` **before** sending the signal |
| held | active | `resume` (relaunch with `--resume`), or a live process with that session id appears |
| any non-terminal | hung-up | `close`, or API relays PR merged/closed for the line's `prUrl` |
| hung-up | — | none; a new `resume` creates a new line on the same `agentSessionId` (history kept) |

### Process gone without hold ("dead")

A line whose process disappeared without `hold` (crash, OOM, reboot, tab closed) maps to **`held`**
with local flag `heldReason: "lost"` (vs `"user"`), because the session id is intact and resume
works exactly. The Board shows the reason. After a reboot every recent line is `held(lost)` until
`tmux-restore` relaunches it (→ `active`). This keeps the frozen enum.

### Orphan, duplicate (local flags, not states)

- `orphan`: process alive and its tmux session has no attached client. Surfaced, never killed.
- `duplicate`: two live processes on the same `agentSessionId`.

## Hold safety

`hold` runs only if every predicate is true; each is a separate function with its own test.

| Predicate | Check | Refusal message |
|---|---|---|
| `atPrompt` | last event is `Stop` (not `UserPromptSubmit`) **and** Claude `status == idle`; Codex: last turn completed | "agent is mid-turn" |
| `inputEmpty` | terminal content: the prompt line after the input marker is empty (`tmux capture-pane -p -t <pane>`); agent-specific prompt regex, versioned | "input box is not empty" |
| `noToolRunning` | no child process except known MCP servers; tool shells are children whose argv sources `~/.claude/shell-snapshots/`; no `SubagentStart` without `SubagentStop`; no background task (Claude `TaskCreated` without `TaskCompleted`) | "a tool / subagent / background task is running" |
| `notPinned` | registry `pinned == false` | "line is pinned" |
| `notNeedsInput` | no pending `Notification`/`PermissionRequest` | "agent is waiting for you" |

Execution after all pass: record `held` + launch flags in registry → set `@sb_held=<lineId>` on the
pane → send `/exit` + Enter to the agent if the clean-exit method is confirmed (Phase 3), else
SIGTERM to the agent pid only → wait for exit (timeout → abort, report) → run the stub in the
pane. Re-check `inputEmpty` and `atPrompt` immediately before sending keys (race window).

Dry-run is default during development: log the predicate results and the actions.

## As implemented (Phase 3, ADR 0014)

- Two more predicates run first: `alive` (a live pid, not a duplicate session) and `inTmux` (the
  agent descends from a pane of the tmux server). SIGTERM is used; `/exit` awaits the supervised trial.
- `hung-up` is re-opened in place by `resume` or by a live process on the same session id
  (the registry keeps one row per `(agent, agentSessionId)`), instead of creating a new line.
- `held(user)` is kept across reboots by the registry (`held_by_user`), independent of tmux.
