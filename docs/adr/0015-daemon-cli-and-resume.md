# 0015 Daemon, CLI and local resume

Status: accepted
Superseded: cmux integration removed (2026-10-29).

## Decision
- **CLI in-process**: every command opens the registry and scans directly; the daemon is not
  required. The registry uses WAL, `busy_timeout` 5 s and `BEGIN IMMEDIATE` for a scan, so the CLI,
  the stub and the daemon can run together. New columns are added with guarded `ALTER TABLE`
  (Phase 1 databases upgrade in place): `ticket_override`, `pinned`, `held_by_user` (held pid),
  `closed_at`; new tables `hook_state`, `spool_offsets`.
- **Daemon**: `overseer daemon [--interval 5]`, foreground loop (ingest + scan, writes
  `status.json`) plus HTTP/1.1 on a Unix socket (`<state>/daemon.sock`, `0600`, override
  `OVERSEER_SOCKET`), one request per connection, routes of `contract/daemon.openapi.yaml`.
  `/v1/events` (SSE) answers 501 until the Board (phase 6). Service units come with the installer
  of a later phase.
- **resume**: refuses a running line or a missing cwd. Where: the pane holding the stub
  (`respawn-pane -k`, the stub is ours) → a new window when run inside tmux → a detached session
  when only `OVERSEER_TMUX_SOCKET` is set → otherwise `exec` in the current terminal. The
  daemon's resume route and relayed resumes never exec and never open a window: held pane, else a
  detached session (ADR 0016); `resume --attach` adds a `tmux attach` (ADR 0017). Argv is
  `<agent> <launch flags> --resume <id>` (Codex `codex resume <flags> <id>`), flags from the live
  argv, else the registry, else `claude_args` of `~/.local/state/tmux-snapshot/latest/state.json`
  (read-only, non-guessed entries win, resume flags stripped).
- **Owner**: `git config user.email` of the line's worktree, else config `owner` (else the `$HOME`
  git email, else `$USER`).
- **ls window**: lines active in the last 7 days, plus every live, pinned or user-held line;
  `--all` shows everything. The footer prints the station counters.
- **Station counters**: active / idle / held, summed agent RSS (`/proc/<pid>/status` VmRSS or
  `ps -o rss=`), memory used/total (`/proc/meminfo`, `sysctl hw.memsize` on macOS).
- **Orphan**: live agent in a pane of a tmux session with no attached client. The cmux-tui
  cross-check of the brief is deferred (its terminal list is not trusted yet, see findings/cmux.md).
  **Duplicate**: two live `sessions/<pid>.json` with one session id.
- **macOS pid reuse**: `ps -o etime=` gives the process start; it must be within 60 s of the
  session file's `startedAt`, otherwise the pid belongs to another process.
- **Test hooks into the binary**: `OVERSEER_TMUX_SOCKET` (tmux `-L`), `OVERSEER_TMUX_SNAPSHOT`,
  `OVERSEER_NOW` (clock), `OVERSEER_HOLD_TIMEOUT_MS`, `OVERSEER_SOCKET`.

## Consequences
- The macOS paths (`ps`, `sysctl`, `lsof`) are written but not run on a Mac yet.
- Resuming a Codex line is unverified (no Codex on the VM).
