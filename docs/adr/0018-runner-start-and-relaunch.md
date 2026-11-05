# 0018 Runner lines: start and relaunch

Status: accepted

## Context
feedback-hub asks the API for a `feedback-bot` line on a runner (`POST /v1/lines`) and later
relaunches the same line with the reporter's answer (ADR 0009, BRIEF §6-7). ADR 0016 refused
`start` and `relaunch`. Both put text into an agent, so they need the same gates as the other
relayed commands, and the text must never be stored or logged.

## Decision
- **Same gates as ADR 0016**: `acceptRemoteCommands` must be true on the runner; `args.dryRun`
  defaults to true for `start` and `relaunch` too. `POST /v1/lines` accepts an optional boolean
  `dryRun` (default true) forwarded to the runner: feedback-hub sends `"dryRun": false` to act.
  A dry run answers `would start claude in <worktree> on branch <b> (tmux session sb-…)`.
- **start** (`src/runner.rs`), claude only:
  - clone: `config.repos` (`{"github.com/org/repo": "/path/to/clone"}`, case-insensitive) first,
    else the clones under `roots` whose normalised origin matches;
  - branch: `args.branch`, else `feedback/<ticketKey>`; checked with `check-ref-format`;
  - worktree: `<worktreeDir>/<clone dir>/<branch slug>` (`worktreeDir` default `~/worktrees`),
    `worktree add` from the clone's current HEAD (`-b` for a new branch). No fetch: the runner's
    clone is kept up to date by its operator. An existing path is refused;
  - line: the API's provisional id is kept. A registry row is reserved before launch with that id,
    a fresh session uuid, `owner`, `origin` and ticket from the args; the agent is started with
    `claude <startArgs> --session-id <uuid>`, so the next scan matches the session to that row
    and the API's `pending` fields are replaced by the real ones;
  - terminal: a new detached tmux session `sb-<last 6 of the id>` on the station's tmux server;
  - prompt: written to `<state>/tmp/<id>.prompt` (`0600`), loaded with `tmux load-buffer`, pasted
    with `paste-buffer -d -p` once the pane shows an empty prompt (`inputEmpty`, up to
    `OVERSEER_START_TIMEOUT_MS`, default 30 s), then Enter; the file is deleted. The prompt is
    never in argv, `actions.log`, the command reason, the registry or the API.
  - Idempotent: a second `start` for a known line id answers `already started`.
- **relaunch**: same line, new turn. On a live line, refused unless `alive`, `inTmux`, `atPrompt`
  and `inputEmpty` pass (never clobber typed text, never interrupt a turn); the message is pasted
  like the prompt. On a held or lost line the line is resumed detached (ADR 0016) and the message
  sent once the prompt is back.
- `owner`, `origin` and `envRef` are local marks of the registry (additive columns), surfaced in
  the synced `Line`; `envRef` is set with `overseer link <line> --env <ref|none>`.

## Consequences
- 202 from `POST /v1/lines` means delivered; the outcome is the command's audit entry or the
  `command.completed` event. A dry-run start leaves a provisional line in the API until it ages
  out of the views.
- Codex lines cannot be started (no way to choose the session id up front).
- Runner packaging: `deploy/runner/Dockerfile` (daemon + sync + tmux + Claude Code from the
  official npm package); systemd user units and launchd plists in `deploy/`; none is installed by
  Overseer.
