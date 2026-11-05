# 0022 Daemon-owned terminals, tasks and activity for the desktop app

Status: accepted

## Context
The desktop app (Tauri 2 + ghostty-web, Mac/Linux/Windows) needs terminals that survive the app
closing without depending on tmux (absent on Windows), a local Kanban whose cards start agents,
and a readable side panel. Contract: `contract/app-daemon.md` (protocol 1).

## Decision
- **Terminals** (`src/term.rs`): the daemon spawns programs in PTYs with `portable-pty` (ConPTY on
  Windows) and keeps them in memory: a 2 MiB output ring, subscribers, writer, master, killer. One
  reader thread per PTY, one waiter thread for the exit code. No tokio: thread per connection like
  the rest of the daemon.
- **WebSocket** (`GET /v1/terms/{id}/io`): `read_request` has already consumed the HTTP head, so the
  101 is written by hand (`derive_accept_key`) and `tungstenite::WebSocket::from_raw_socket` takes
  over. One thread per client alternates a 20 ms read timeout with draining its output channel:
  tungstenite answers pings from the read path, so splitting read and write over two cloned
  sockets could interleave frames. The ring snapshot and the subscription are taken under one lock
  (no gap, no duplicate). The connection's 5 s write timeout drops a stalled client, never the term.
- **Lifetime**: terms survive clients, not the daemon (the PTY master closes, the program gets
  SIGHUP). Kill = SIGHUP to the process group (the child leads its own session), then SIGKILL to the group after 2 s if the waiter has not seen the exit.
- **Lines ↔ terms**: the scan walks each live agent pid up its parents (16 steps) to a term pid
  and sets `local.termId`; the daemon tick sets the term's `lineId`. Resume can target a new term
  (`{"target":"term"}`) and then acts by default, unlike the tmux resume.
- **Tasks** (`src/tasks.rs`): rows in the registry (`tasks`, `task_numbers` so numbers are never
  reused; the counter is an atomic upsert-returning). Start = `git worktree add -b
  task/<n>-<slug> … HEAD` under `worktreeDir/<repo>/task-<n>-<slug>`, then a term running
  `claude [startArgs] --session-id <uuid> [--model] --effort` (a reserved line row, like the
  runner) or `codex [-m] -c model_reasoning_effort=…`, with `OVERSEER_TASK_ID`. The prompt is
  typed into the PTY after 1.5 s of output silence, as a bracketed paste when the program enabled
  it: no temp file, no argv (the scan records agent argv as launch flags), no log.
- **Auto-runner and in_testing** run in the daemon tick after the scan: `in_progress` → `in_testing`
  on a `Stop`/`StopFailure` hook newer than the start, on busy-then-idle, or when the term exits;
  then the oldest `auto` tasks start while fewer than `autoConcurrency` (default 2) run.
- **Activity** (`src/activity.rs`): incremental parse of the Claude transcript / Codex rollout
  (byte offset per path, partial last line kept), returned on request; never logged. A watcher
  thread stats the transcripts of live lines every 100 ms and publishes `line.activity` at most
  every 500 ms per line.
- **MCP** (`overseer mcp`): hand-written JSON-RPC 2.0 over stdio, tools only, talking to the
  daemon socket. An agent may only move its own task to `in_testing`.

## Consequences
- A daemon restart kills its terms; their tasks move to `in_testing` with `error`.
- Codex task flags are unverified (no Codex on the VM).
- Activity reflects transcripts only; tool output is not shown.
