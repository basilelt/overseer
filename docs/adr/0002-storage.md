# 0002 Storage

Status: proposed, amended by 0008 (state dir, driver, Codex copy) and 0009 (API on SQLite)

## Context
Daemon must work offline and survive reboots; API is shared, hosted on Kubernetes. The VM has no
sqlite3 CLI (not needed by a pure-Go driver). The VM's group-writable dirs once broke cmux-tui.

## Decision
- Daemon: SQLite through `modernc.org/sqlite` (no cgo), WAL mode, in
  `~/.local/state/overseer/` (Linux) / `~/Library/Application Support/overseer/` (macOS),
  dirs `0700`, files `0600`, created with explicit modes regardless of umask.
  Tables: `lines` (contract fields), `line_local` (pid, pane, flags, launch args), `events` (hook
  spool ingested), `outbox` (seq, payload), `meta`.
- Agent stores are read read-only: Claude transcripts and `~/.claude/sessions/*.json`;
  Codex `state_5.sqlite` opened with `mode=ro` (never through the agent's own write path).
- API: PostgreSQL. Tables `stations`, `lines`, `commands` (audit), `events` (retention 7 days for SSE replay).

## Consequences
No durable data in `$TMPDIR`. Registry rebuildable from agent files at any time; the DB is a cache
plus the things only Overseer knows (owner, links, pins, held, outbox).
