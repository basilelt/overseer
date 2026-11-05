# 0017 Remote ls, remote resume and deep links (Phase 5)

Status: accepted
Superseded: cmux integration removed (2026-10-29).

## Decision
- **User login**: `overseer login --api http://…` reads a user token on stdin, checks it with
  `GET /v1/stations`, then writes `<state>/user.json` (`{api, token}`, `0600`). Nothing is written
  if the API refuses it. `http://` only, as for sync (ADR 0009).
- **ls**: when `user.json` exists, `ls` adds `GET /v1/lines?includeHungUp=true&owner=<owner>`
  rows of other stations to the local scan (this station is always read locally). `owner` defaults
  to the config owner; `--owner all` drops the filter, `--local` skips the API. Rows keep the API
  shape `{line, stationOnline, syncedAt}` in `--json`; the table gains a STATION column and an
  `OFFLINE` flag. An unreachable API prints a warning and lists this station only.
- **resume**: a query that matches no local line is resolved against every API line of other
  stations (same id / session / ULID prefix rules). An offline station is an error before any
  connection: `line … runs on station …, which is offline`.
- **Transport**: `ssh -t <target> -- <remoteCommand> resume <lineId> --attach`, exec'd in the
  current terminal. `<target>` comes from config `ssh: {"<stationId>": "user@host"}`, default the
  station id itself (an `~/.ssh/config` Host alias). `remoteCommand` defaults to `overseer`
  (set an absolute path when the remote PATH of a non-login shell lacks it). Targets starting with
  `-` or containing spaces and ids outside `[A-Za-z0-9._@-]` are refused.
- **Remote side**: `resume --attach` resumes as the daemon does (held pane, else detached session
  `sb-…`), then `tmux attach`es to it; if the line is already running in a pane it just attaches
  there. Without a terminal it prints `attach with: tmux attach -t …` instead.
- **cmux**: Phase 0 confirmed no safe, non-destructive cmux command to open a remote tab (the Mac
  CLI is unverified; cmux-tui `create`/`focus` were deliberately not run). Remote resume therefore
  uses plain ssh; when `cmux` or `cmux-tui` is on PATH a note says so. Switching to
  `cmux ssh <target> --command "overseer resume <id> --attach"` is a one-line change once
  verified on the Mac.
- **Deep links** (amends ADR 0006): `overseer open <url>` accepts only
  `overseer://resume/<lineId>` and asks `resume line …? [y/N]` (refuses without a terminal
  unless `--yes`), then runs the same local-or-remote resume. `line/` and `ticket/` URLs wait for
  the Board (Phase 6). Handler files are shipped in `deploy/url-handler/` and never registered by
  Overseer: Linux `overseer-url.desktop` (`Terminal=true`, `x-scheme-handler/overseer`),
  macOS an AppleScript applet (`on open location`) that runs `overseer open` in Terminal, with
  the `CFBundleURLTypes` to add to its `Info.plist`.

## Consequences
- The user needs key-based ssh to every station and `overseer` on the remote PATH.
- Lines of other owners are hidden by default; ids of any owner still resolve for `resume`.
- The macOS handler and `cmux ssh` path are untested until run on the Mac.
