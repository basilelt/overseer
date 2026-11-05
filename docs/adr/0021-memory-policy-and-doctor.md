# 0021 Memory policy (report only) and the final doctor

Status: accepted
Superseded: cmux integration removed (2026-10-29).

## Context
BRIEF §5.4 asks for an opt-in memory-pressure policy that never violates the hold rules; §13
lists what `doctor` checks.

## Decision
- `config.memoryPolicy = {"enabled": true, "thresholdPercent": 85}`; off by default. When used
  memory is above the threshold, `status` (JSON `memoryPolicy`, plus one text line) and `doctor`
  list the idle live lines, least recently used first, each with the hold predicates evaluated as
  a dry run (`holdable`, `failed`). Nothing is ever held automatically; the user runs `hold`.
- `doctor` checks: state dir, daemon, Claude/Codex hooks, claude/codex/tmux versions, cmux binary
  and socket (connect only, never a command), station enrolment, API reachable (`/healthz`), user
  login, remote commands, runner readiness (claude, tmux, clones) when remote commands are
  accepted, toolkit coexistence (tmux-snapshot/restore, tmux-claude-id.sh), lines, memory policy,
  orphans, duplicates, held lines without a stub.

## Consequences
- Automatic holding stays a future, separate decision.
