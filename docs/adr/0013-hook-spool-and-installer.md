# 0013 Hook spool, live state and installer

Status: accepted (implements 0004 option A + C)
Superseded: cmux integration removed (2026-10-29).

## Context
ADR 0004 chose own hooks plus polling. Phase 2 had to fix the details without a real install:
the user's `~/.claude/settings.json` already runs up to four hooks per event, the payload of each
event is only known from documentation, and Codex re-trusts hooks by hash.

## Decision
- **Hook program**: `overseer hook <claude|codex> <Event> "$PPID"`, installed as
  `OVERSEER_STATE_DIR=<abs> <abs bin> hook claude Stop "$PPID" 2>/dev/null||:;echo {};#overseer-hook`.
  The agent runs hooks through `sh -c`, so `$PPID` inside the command is the agent's pid; the binary
  never relies on its own parent. It prints nothing, always exits 0; the command prints `{}` so
  `SessionStart`/`UserPromptSubmit` add nothing to the context.
- **Allow-list**: from stdin only `session_id`, `cwd`, `hook_event_name`, `notification_type` are kept,
  plus `$TMUX_PANE`, `$CMUX_TUI_TERMINAL_ID`. `prompt`, `message`, tool input are never written.
- **Spool**: one JSON line per event appended (`O_APPEND`, `0600`) to `<state>/spool/<agent pid>.jsonl`.
  Every `scan` (so `ls`, `hold`, the daemon loop) ingests new complete lines inside the same
  `BEGIN IMMEDIATE` transaction, tracks a byte offset per file and deletes files of dead pids. Live
  state therefore works with or without the daemon.
- **Events** (Claude): SessionStart, UserPromptSubmit, Stop, StopFailure, Notification,
  PermissionRequest, Elicitation, SubagentStart/Stop, TaskCreated/Completed, SessionEnd.
  (Codex): SessionStart, UserPromptSubmit, Stop, PermissionRequest, SubagentStart/Stop, SessionEnd.
  `Notification` sets needs-input unless `notification_type` is `idle_prompt` or `auth_success`
  (missing type = needs input: a false refusal is safer than a false hold).
- **Busy**: for Claude the newer of the hook state and `sessions/<pid>.json` `status` wins
  (`statusUpdatedAt`); for Codex only hooks give busy, otherwise unknown.
- **Installer**: `overseer hooks install|uninstall`, dry run by default (unified diff of the raw
  file against the merged file); `--apply` backs up to `<file>.bak-YYYYMMDDHHMM`, writes a temp file,
  re-parses it and checks that every pre-existing hook command and top-level key survives and every
  Overseer command is present (the `jq -e` equivalent), then renames. Entries are found by the
  `#overseer-hook` marker: re-install is a no-op, a moved binary replaces the old entry,
  uninstall removes only marked entries. Invalid JSON is never touched. Codex: `~/.codex/hooks.json`
  only, `config.toml` is never edited; the user re-trusts the hooks in Codex.
- **Key order**: `serde_json` feature `preserve_order` (adds `indexmap`, fetched once) so a merge
  only adds lines. Checked read-only on copies of the VM files: 0 removed lines in both diffs.

## Consequences
- Payload fields beyond `session_id` are unverified against a real agent (stub only); a missing
  field degrades to the polling fallback.
- A hook adds one process spawn (~1–2 ms). If the state dir is unwritable the hook silently does nothing.
- The installed command embeds the absolute paths of the binary and the state dir of the checkout
  it was run from. Install from the long-lived checkout's build (not a temporary worktree); after
  moving it, run `hooks install --apply` again (the marker replaces the stale entries). The
  platform-dir installer of a later phase removes this coupling.
