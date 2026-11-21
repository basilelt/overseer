# 0042 Mistral Vibe as a full agent

Status: accepted

## Context
Users asked for Mistral Vibe (`vibe`) with the same support as Claude Code, Codex and Gemini CLI:
sessions found on disk, chat view, activity, hooks and one-click installs. Vibe 3.0 rewrote the CLI
(a Rust TUI over a Python app-server): sessions moved from `meta.json` + `messages.jsonl` to a
content-addressed session store, hooks lost their experimental flag and were renamed
(`post_agent_turn` → `post_agent`, …), and the builtin tools became `file_system.*`. Vibe 2 is
being replaced by 3 through its own update prompt.

## Decision
- Support Vibe 3 only for sessions, chat, activity and hooks. A Vibe 2 install still launches from
  Overseer, but its sessions are not discovered, so it gets no line: no chat, no resume from
  Overseer and no restore after a daemon restart; the hook file it would read has other event
  names.
- Lines come from `$VIBE_HOME/logs/session/unified/<id>/meta.json` (folder, title, model, start).
  A session with a parent is listed only when `identity.kind` of its runtime state is `fork`, so
  subagents never become lines. A session is alive while `logs/session/active/<id>.lock.json`
  names a live process under a `vibe`/`vibe-rs` process; that process gives the resume flags and
  the term link. Vibe cannot take a session id up front, so a task's line is linked once the
  session appears, as for Codex.
- The chat and activity read the store, not a log: `CURRENT` → generation `manifest.json` → the
  projection's chunks, then the current journal segment's projection records. After the first
  load only appended journal records are parsed; anything that is not an append (a removal, a full
  history set, an advance, a generation whose history does not extend the shown one) makes the
  chat reset. A newer `store_format_minor` than 3.1.2 writes (10) is refused rather than guessed.
- Hooks are an Overseer-owned `$VIBE_HOME/hooks.toml` (refused if the user already has one):
  `session_start`, `pre_agent`/`post_agent` with `match = "main"`, `post_tool`, `notification`
  (`approval_requested`, `question_asked`, `turn_completed`, `turn_failed`) and `session_end`.
  Turns are detected, so a Vibe task moves to review on its own.
- The prompt goes in argv after `--` (Vibe's positional prompt); task launches add `--trust`.
  Vibe has no interactive model flag, so the new-session picker offers no model and profiles set
  `VIBE_ACTIVE_MODEL`. RTK uses `rtk init --agent vibe`, Caveman and Ponytail the `skills` CLI
  target `mistral-vibe`.

## Consequences
- Every format was read from the 3.1.2 source; no live 3.x session was recorded, so the fixtures
  are built from the writers' models. A store format change in a later Vibe shows as no chat
  (409) rather than a wrong one.
- An interrupted turn fires no Vibe hook: the line stays busy until the next turn ends (or the
  session ends). A session ends when its lease is gone — `logs/session/active/<id>.lock.json`
  missing or naming a dead process — so the daemon then records the `session_end` the hook would
  have sent and the line stops being mid-turn even without a hook. The store also says when a
  turn is over: `latestTurn.status` in the last journal envelope (or in the snapshot once the
  journal was rotated). A line the hooks or the terminal still show mid-turn after the store
  reports its turn over, and no new prompt in the last few seconds, is shown idle.
- Usage (tokens) is not read: the store keeps only per-session totals without timestamps.
- Vibe writes a session to the store only when the first message is sent, but holds its lease from the
  start. A lease with no store, on a live process no other session of the line list uses, is listed
  as a session without title or chat, so the tab is a Vibe line at once; a lease file appearing or
  going wakes the scanner (no polling). Its folder comes from the process.
- Vibe's TUI repaints only what changed, so the terminal screen the close dialog reads is the
  accumulated state of every byte since the terminal started (the last 2 MiB), advanced only when
  read, not the last 256 KiB. Its `vibe-app-server` child is the harness, not a tool.
