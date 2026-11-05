# 0014 Hold and close execution

Status: accepted for development (dry run by default); real use after the supervised trial

## Context
`docs/line-states.md` lists the hold predicates and the order of actions. The clean-exit method
of Claude Code was never tested on a real session; the brief forbids touching real sessions
without approval.

## Decision
- **Predicates** (`src/hold.rs`, one function and one test each): `alive` (live pid, not a
  duplicate session), `inTmux` (agent's pid descends from a pane of the configured tmux server),
  `atPrompt`, `inputEmpty`, `noToolRunning`, `notPinned`, `notNeedsInput`. Unknown = refuse.
- **inputEmpty**: `tmux capture-pane -p -e -J`; the last line (within the last 15) whose first
  non-border character is `>`, `❯` or `›` is the prompt; its rest, plus following lines up to a
  horizontal rule, must contain no non-dim character. Dim / grey text (SGR 2, 90, 256-colour greys,
  grey truecolour) is the placeholder hint. No prompt found (permission dialog, unknown UI,
  Codex footer without a rule) = refuse.
- **noToolRunning**: direct children of the agent; a child whose argv contains `shell-snapshots`
  is a tool shell; a child whose argv contains `mcp` or a string from config `mcpCommands` is an
  MCP server; any other child refuses; open subagents or background tasks (hook counters) refuse.
- **Clean exit**: SIGTERM to the agent pid only (`/exit` is not confirmed). Order: record held
  (`held_by_user = pid`) → `@sb_held=<lineId>` on the pane → `remain-on-exit on` → SIGTERM → wait
  10 s (pid gone or start time changed) → on timeout revert the registry and the pane option and
  report → start the stub with `respawn-pane` (pane dead, or `-k` when the pane's shell has no
  child) → restore `remain-on-exit`. Predicates are evaluated on a fresh scan right before acting.
- **Stub** (ADR 0005 option 1): `sh -c "trap : INT; overseer stub <id>; exec $SHELL"`. Enter
  re-checks the line is not running, clears `@sb_held` and the held mark, then execs the resume
  argv in the line's cwd; Ctrl-C leaves a shell.
- **close**: alive line → full hold first, refused if any predicate fails; then `closed_at` and
  `hung-up`; `@sb_held` cleared so a restore does not bring the stub back. Dead line → mark only.
- **Re-opening**: a live process seen on a hung-up or user-held line clears the marks, unless it
  is the very pid being held (race with the daemon). `resume` of a hung-up line reopens the same
  line instead of creating a new one (`UNIQUE(agent, session_id)`); this amends line-states.md.
- **Dry run**: `hold`/`close` print and log (`<state>/actions.log`, JSONL) what they would do;
  `--yes` (CLI) or `dryRun=false` (API) acts. Exit code 3 on refusal.

## Consequences
- Tested only against a stub agent on an isolated tmux server. A supervised hold of one real
  session chosen by the user is still required (Phase 3 acceptance), and decides whether `/exit`
  replaces SIGTERM.
- The screen regex is version-sensitive; a UI change produces refusals, not wrong holds.
