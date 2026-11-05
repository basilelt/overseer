# 0012 Board, local event stream and cmux integration

Status: accepted (defaults chosen without the user, to review); other stations completed by ADR 0019
Superseded: cmux integration removed (2026-10-29).

Numbered 0012 because `main` already has a second 0009 (`api-and-sync`); the merge renumbers it.

## Decision
- **Page**: `board/index.html`, `board.js`, `board.css`; vanilla JS, no build step, no external
  resources, compiled into the binary (`include_str!`). Served by the daemon at `GET /board`
  (+ `/board/board.js`, `/board/board.css`). Light/dark from `prefers-color-scheme`. Every
  user-facing string sits in `STRINGS` at the top of `board.js` (`en` default, `fr` provided;
  `?lang=fr` or the header toggle, remembered in `localStorage`).
- **Daemon**: one thread per connection (an SSE client must not block other requests); the
  request parser keeps headers. Routes added beside `contract/daemon.openapi.yaml`:
  `GET /v1/board`, `GET /v1/pr?repo&branch`, `GET /v1/lines/{id}/diff`,
  `POST /v1/lines/{id}/commit|stash` (dry run unless `dryRun=false`, like hold and close).
  `/v1/lines` is unchanged (no network on that path: `ls` and sync use it).
- **Loopback HTTP**: `overseer daemon --http 127.0.0.1:<port>` (port 0 = random). Non-loopback
  addresses are refused. A 128-bit token from `/dev/urandom` is created per start; every request
  needs `?token=` except the two static assets; the `Host` header must be `127.0.0.1`,
  `localhost` or `[::1]` (DNS rebinding). The URL is written to `<state>/board.json` (`0600`) and
  printed by `overseer board`. Token in the query, not a cookie: `EventSource` cannot set
  headers and the page appends it to every call. The socket path keeps file-permission auth only.
- **Live updates**: `GET /v1/events` is SSE. After every scan the daemon diffs the board lines
  (local + other stations) and publishes `line.created`, `line.state_changed` (+ `line.hung_up`),
  `line.updated`, `station.online|offline` in the envelope of `event.schema.json`. Ids are
  monotonic per daemon start; the last 256 events replay after `Last-Event-ID`; `: ping` every
  15 s; a client that fails a write is dropped. The page refetches `/v1/board` on any event
  (debounced 250 ms) and every 30 s.
- **Other stations** (no sync on this branch): the Board merges `<state>/stations/<id>.json`
  (`{stationId, lastSeenAt, lines: [Line | {line}]}`). A station is offline when `lastSeenAt` is
  older than 120 s. Following ADR 0007, its lines are shown greyed with "station X offline since
  T", actions disabled with the reason. Lines of online remote stations are read-only too ("act
  from that station") until remote commands go through the API. **This file is the seam for the
  merge with the sync client**: the sync code writes it from the API's lines and stations.
- **PR / CI**: `gh pr list --repo=<repo> --head=<branch> --state all --limit 1 --json ...`, no
  shell, `GH_PROMPT_DISABLED`, 15 s timeout, cached in memory 60 s (hits and misses). CI =
  failure if any check failed, pending if any runs, success otherwise, `none` without checks.
  Fetched lazily per branch by the page, so a slow `gh` never delays the board. Tests put a fake
  `gh` first on `PATH` (`tests/stubs/gh`).
- **Env link**: `envRef` (from other stations' lines; local lines have none yet) links to
  `<yardUrl>/envs/<envRef>`, `yardUrl` from `config.json`. Yard is faked by a static directory
  (`tests/fakes/yard`). Reading Yard's state is deferred until Yard has an API.
- **Worktree actions**: diff is read-only (`git status --porcelain`, `git diff HEAD`, `--stat`,
  capped at 512 KiB). Commit (`git add -A`, `git commit -m`) and stash (`git stash push -u -m
  "overseer <line>"`) refuse a mid-turn line, a missing worktree, a clean worktree or an
  empty message; the page shows the files and asks to confirm before `dryRun=false`. Both are
  logged to `actions.log`. Git hooks and signing of the worktree apply as usual. The stash stack
  is shared by all worktrees of a repository.
- **Destructive actions** (resume, hold, close, commit, stash) always run the dry run first and
  confirm in a dialog; refusals show the failed predicates and their detail.
- **Resume from the Board** uses the daemon's resume (ADR 0011): it relaunches in the held
  line's stub pane, or in a new tmux session when the daemon runs with `OVERSEER_TMUX_SOCKET`;
  otherwise it refuses ("no tmux available to the daemon") and the page shows the command to run
  in a terminal.
- **cmux browser surface**: Phase 0 findings list `tab create browser --url` only among commands
  "not run because they may mutate"; no non-destructive command is documented. So
  `overseer board --cmux` prints the URL and the `cmux-tui --session cmux tab create browser
  --url ...` line for the user to run; Overseer never calls cmux.
- **cmux sidebar**: `overseer status --cmux-sidebar` prints short lines for a narrow column
  (`SB <n> active`, `<n> idle · <n> held`, orphans). `deploy/cmux/overseer-sidebar/` holds a
  sidebar plugin (`cmux-plugin.toml` + a refresh loop). cmux-tui 0.1.0 reads `cmux-plugin.toml`
  with `plugin.kind` and `plugin.version` and runs sidebar plugins in a sized terminal surface
  (strings of the binary); the other keys are guesses. **Unverified, never installed.**

## Consequences
- Performance (debug build, 50 lines over 3 stations, in-process): `/board` ≈ 2 ms,
  `/v1/lines` ≈ 23 ms, `/v1/board` ≈ 27 ms; 15 state changes reach 50 subscribers in ≈ 6 ms.
- No headless browser on the VM: the browser test skips with a note (it runs with chromium or
  firefox on `PATH`).
- To do with the user: validate the sidebar manifest on a real cmux, decide whether
  `board --cmux` may create the browser tab, local `envRef` source, Yard status API.
