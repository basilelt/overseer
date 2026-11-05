# 0004 Hook transport

Status: accepted, detailed in 0013
Superseded: cmux integration removed (2026-10-29).

## Context
Observed on the VM: `~/.claude/settings.json` already runs up to four hooks per event (toolkit,
matrix, cmux-tui, rtk); cmux-tui hooks every Claude and Codex event and keeps per-agent state in its
journal (`cmux-tui --session cmux session <sel> journal subscribe`). Codex hooks need re-trust by
hash after any edit. Hooks must be under 50 ms and never fail the agent.

## Options
A. Own hook `overseer-hook <agent> <event>` added next to the others on 5 events per agent
   (SessionStart, UserPromptSubmit, Stop, Notification/PermissionRequest, SessionEnd). It appends one
   JSON line to `$STATE_DIR/spool/<pid>.jsonl` (O_APPEND, no socket round-trip), adds `$TMUX_PANE`,
   `$CMUX_TUI_TERMINAL_ID`, `$PPID`; always exits 0 and prints `{}`.
B. No own hooks where cmux-tui is present: subscribe to its journal; fall back to A elsewhere (Mac,
   runners).
C. No hooks: poll `~/.claude/sessions/*.json` (`status busy|idle`), transcript/rollout mtimes, processes.

## Decision (recommended)
A as the source of truth, with C always on as a fallback and reconciliation (it already gives
busy/idle for Claude 2.1.289). B is optional enrichment once the journal format is documented:
it ties Overseer to an unversioned 0.1.0 interface and is absent on the Mac and runners.

## Consequences
One more hook per event (≈ 2 ms: a static binary appending a line). Installer merges into
`hooks.<Event>[]`, keeps every existing entry, backs up, validates with `jq -e`. Codex installs
require the user to re-trust the hooks.
