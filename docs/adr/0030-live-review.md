# 0030 Live review with comments sent to the agent

Status: accepted

## Context
Reviewing an agent's work meant reading the flat worktree diff of the side panel or the Kanban
modal, then typing feedback into the agent's terminal by hand. Users want what a pull request
gives — a file tree, a real diff, comments on lines, suggestions — while the agent is still
working, and they want the agent to receive the comments and answer them, whatever the agent
(Claude Code, Codex, Gemini, …). The daemon already owns the agents' terminals, knows when a line
is busy or waiting for input, and serves an MCP server to agents.

## Decision
- **Diff**: the daemon assembles the review itself (`src/review/diff.rs`): `git diff -M` of the
  working tree against `git merge-base HEAD <base>` plus untracked files read from disk. The base
  is the task's project branch, `origin/HEAD`, then `main`/`master`/`develop`/`trunk`; without
  one, only uncommitted work is shown. The answer is structured (files, hunks, numbered lines) so
  the app does not parse patches; caps per file (3000 lines) and in total (4 MiB).
- **Storage**: threads, comments, deliveries and viewed marks live in four registry tables created
  with `CREATE TABLE IF NOT EXISTS` by the review module itself (no column migration, safe when
  several processes open a new registry at once). Comments are drafts (`pending`) until a submit.
- **Anchors**: a thread keeps the commented lines and FNV-1a hashes of the three trimmed lines
  around them. On every change it is located again: same place, same lines elsewhere (nearest),
  or the surrounding lines with new content in between ("changed since comment"); else outdated.
  No line-mapping through diffs: the context hashes are enough and survive commits and rebases of
  the agent's branch.
- **Live updates**: a `notify` watcher (inotify on Linux, ReadDirectoryChangesW on Windows)
  watches each worktree recursively; events under `.git/` are ignored, the others are debounced
  500 ms (2 s at most), then a snapshot (`git status --porcelain -z` with mtimes and sizes, plus
  `HEAD`) is compared with the last one, so changes to ignored files (build output) publish
  nothing. A worktree the watcher cannot watch (inotify limits; `OVERSEER_REVIEW_POLL=1`
  forces it) is polled every 500 ms, and every worktree is re-checked every 5 s in case an
  event was missed. macOS uses FSEvents (`notify`'s `macos_fsevent`): its daemons are
  built on a macOS runner (`make dist-darwin`, release job `servers-macos`) because CoreServices
  needs the Apple SDK, which the zig cross-build lacks. kqueue stays unused: it needs one
  descriptor per file. Submodules:
  an event under a submodule runs `git status` in that submodule only, and a commit inside one is
  seen through its git dir's `HEAD` and `logs/HEAD`, never its `index`. `review.changed` carries the changed paths. Watched: worktrees with open threads and worktrees
  read through `/v1/review*` in the last 30 minutes (32 at most), not every agent's worktree.
- **Delivery**: a submit formats one plain-text instruction (path:lines, quoted code, comments,
  suggestions as a unified diff, thread ids, overall comment) and types it into the worktree's
  agent — its daemon term (bracketed paste + Enter, as task prompts) or its tmux pane
  (`paste-buffer -p` + Enter). It is typed only when the agent is idle: not busy, not waiting for
  input (typing into a permission prompt could approve a tool), and for agents without busy
  tracking after 1.5 s of terminal silence (term) or an unchanged screen between two ticks
  (tmux). Otherwise the delivery is queued and retried every daemon tick. This works for every
  agent of the agent table because it needs only a terminal.
- **Agents answer through MCP**: `list_review_comments` and `resolve_review_comment` in
  `overseer mcp`; resolving marks a thread `addressed` with the agent's note. The user still
  resolves. `request_changes` puts an In testing task back to In progress through the existing
  task review, so the Kanban loop closes on the next turn end.
- The protocol stays 1: routes, events and MCP tools are additions (contract §11).

## Consequences
- A watched worktree costs one `git status` per burst of edits (plus one every 5 s), or twice a
  second when it falls back to polling; the watch list is bounded.
- Delivery shares `src/deliver.rs` with task prompts and broadcast: one set of idle rules, one
  send path (term paste, tmux named buffer), one retry queue. Review queues persist in the
  registry and are re-queued after a daemon restart.
- The instruction is typed, so it ends up in the agent's transcript like any prompt; the comments
  themselves are stored in the registry. The queued instruction text is cleared once delivered.
- A delivery goes to one agent: the line given, else the worktree's task line, else the most
  recent alive line in the worktree. Two agents in one worktree get the review once.
- Comments on removed lines (`old` side) are not re-anchored: the base rarely moves.
