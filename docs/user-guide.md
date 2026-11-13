# Overseer user guide (0.2.0)

## 1. What Overseer is

Overseer keeps track of AI coding agent sessions (Claude Code and Codex) across several
machines. Its unit is the **line**: one agent session, in one worktree, on one station, for one
owner. A station (a dev Mac, a VM, a runner container) runs the `overseer` daemon/CLI; a shared
`overseer-api` merges the lines of every station, relays commands between them and serves
read endpoints, events and webhooks. The original design and its reasons are in [`BRIEF.md`](../BRIEF.md)
and later decisions are in [`docs/adr/`](adr/); line states are detailed in [`docs/line-states.md`](line-states.md).

## 2. Build and install

```
make build      # target/release/overseer and target/release/overseer-api (offline)
make test       # both crates, offline
make lint       # clippy -D warnings on both crates
```

`bin/overseer` builds the release binary on first use and then runs it (handy inside the
checkout). Nothing is installed automatically: copy `target/release/overseer` somewhere on your
PATH yourself, for example `~/.local/bin/overseer`, and `overseer-api` wherever the API runs.

The state dir defaults to `$XDG_STATE_HOME/overseer` (`~/.local/state/overseer`) on Linux
and `~/Library/Application Support/overseer` on macOS; `OVERSEER_STATE_DIR` overrides it.
`bin/overseer` (the in-checkout wrapper) uses `<checkout>/.state` unless the variable is set.
Make sure the CLI, the services and the hooks resolve the same dir.

**Updating the desktop app.** The app never runs Homebrew. **Settings, Updates** (and the update
pill) offer one **Download & install** button that downloads the release asset matching how the app
was installed and installs it: on macOS (`.dmg` or Homebrew cask) the signed `.app.tar.gz` replaces
the app in place and **Restart to finish** relaunches it; an AppImage is replaced in place; a `.deb`
or `.rpm` install downloads the signed package and installs it with `dpkg -i` or `rpm -U` through
`pkexec` (a system password prompt), then restarts; a Windows `-setup.exe` install runs the signed
installer in passive mode; a Windows `.msix` install downloads the matching x64 or arm64 `.msix`,
checks its SHA-256 against the release `SHA256SUMS` and opens it in Windows App Installer, where you
click **Update**. The Homebrew cask declares `auto_updates true`, so `brew upgrade` skips an app that
updated itself; `brew install` still works for a first install.

## 3. Per-station setup

### 3.1 State dir and `config.json`

State dir: `$OVERSEER_STATE_DIR` (mode `0700`). It holds `overseer.db` (`0600`), `spool/`
(hook events), `actions.log`, `status.json`, `daemon.sock`, `daemon.lock`, `board.json`, `sync.json`,
`sync.db`, `user.json`, `stations/`, `commands/`, `tmp/` and the optional `config.json`. Files
sent from the app go to `~/.overseer/u/` (see 14.1), not to the state dir; an `uploads/` dir
left by older versions is emptied after 7 days and removed.

```json
{
  "stationId": "vm-node2-dev1",
  "owner": "dev@example.com",
  "roots": ["~/src/work", "~/src/perso"],
  "idleAfterSeconds": 600,
  "mcpCommands": ["linear-server"],
  "acceptRemoteCommands": false,
  "ssh": { "mac-dev": "dev@mac.example.com" },
  "remoteCommand": "overseer",
  "yardUrl": "http://yard.example.com",
  "repos": { "github.com/acme/live": "~/src/work/live" },
  "worktreeDir": "~/.overseer/worktrees",
  "discoverRoots": ["~/Documents/Github", "~/src"],
  "startArgs": ["--permission-mode", "acceptEdits"],
  "memoryPolicy": { "enabled": false, "thresholdPercent": 85 }
}
```

| Key | Default | Meaning |
|---|---|---|
| `stationId` | short hostname | Station id, also the prefix of line ids. |
| `owner` | `git config user.email` run from `$HOME`, else `$USER` | Owner of lines whose worktree has no `user.email`; filter for other stations' lines. |
| `roots` | none | Extra directories whose repos (the root itself and its direct children) are recorded and searched for clones. |
| `idleAfterSeconds` | `600` | A live line without activity for longer is `idle`. |
| `mcpCommands` | none | Substrings of MCP server command lines that `hold` may stop (anything containing `mcp` is already recognised). |
| `acceptRemoteCommands` | `false` | Run commands relayed by the API (section 6). |
| `ssh` | none | Station id to ssh target for remote resume / patch; default target is the station id (an `~/.ssh/config` Host alias). |
| `remoteCommand` | `overseer` | The `overseer` to run on the other station over ssh. |
| `yardUrl` | none | Board env links: `envRef` opens `<yardUrl>/envs/<envRef>`. |
| `repos` | none | Normalised repo (`github.com/acme/live`, lowercase, no `.git`) to local clone path; used by `start` and `patch --local`. |
| `worktreeDir` | `~/.overseer/worktrees` | The worktree store: task and app sessions go to `<worktreeDir>/<repo>-<hash>/<branch>` (14.9); `start` and `patch --local` also create theirs under it. Was `~/worktrees`. |
| `discoverRoots` | those of `~/Documents/Github`, `~/src`, `~/code`, `~/projects`, `~/dev` that exist | Folders scanned three levels deep for git repositories (at daemon start, every 10 minutes, **Rescan folders** in the app); each one becomes a project with a workspace. `[]` turns discovery off. |
| `worktreeFetch` | `true` | `git fetch origin <default branch>` (10 s timeout) before creating a worktree from the base. |
| `worktreeStaleDays` | `14` | A clean, unused worktree without activity for longer is `stale` (cleanup suggested). |
| `sessionTasksDays` | `7` | Agent sessions alive or active within this many days get a Kanban task of their own (14.2a). `0` turns it off. |
| `startArgs` | none | Extra `claude` arguments for lines started by the runner. |
| `memoryPolicy` | off | `{"enabled": true, "thresholdPercent": 85}` turns on the report (section 8). `enabled` must be JSON `true`; threshold defaults to 85. |

Paths may start with `~/`. A missing or unreadable `config.json` means all defaults.

### 3.2 Hooks and agent notifications

```
overseer hooks install                 # dry run: diff of every agent's hook file
overseer hooks install --apply         # backup to <state>/backups/, merge, validate, atomic replace
overseer hooks install gemini --apply  # only the named agents
overseer hooks uninstall --apply       # removes only Overseer's entries and files
```

The app offers the same thing once per host when it connects ("Enable agent notifications?":
Install, Not now for this session, Never for this host), and Settings > Notifications has one
switch per agent. The daemon routes are `GET /v1/hooks/status` and
`POST /v1/hooks/install|uninstall {"agents": [...], "dryRun": true|false}`; they return one result
per file with its diff and backup.

| Agent | Mechanism | File | Events used |
|---|---|---|---|
| Claude Code | hooks | `$CLAUDE_CONFIG_DIR/settings.json` (`~/.claude`) | SessionStart, UserPromptSubmit, Stop, Notification, PermissionRequest, PostToolUse*, Subagent*, Task*, SessionEnd |
| Codex | hooks | `$CODEX_HOME/hooks.json` (`~/.codex`) | SessionStart, UserPromptSubmit, Stop, PermissionRequest, PostToolUse, Subagent*, SessionEnd |
| Gemini CLI (0.26+) | hooks | `~/.gemini/settings.json` | SessionStart, BeforeAgent, AfterAgent, Notification (ToolPermission), SessionEnd |
| Copilot CLI | hooks | `$COPILOT_HOME/hooks/overseer.json` (`~/.copilot`) | sessionStart, userPromptSubmitted, agentStop, notification, sessionEnd |
| Cursor Agent | hooks | `~/.cursor/hooks.json` | sessionStart, beforeSubmitPrompt, stop, sessionEnd |
| Mistral Vibe (3.0+) | hooks | `$VIBE_HOME/hooks.toml` (`~/.vibe`) | session_start, pre_agent and post_agent (main agent only), post_tool, notification (approval_requested, question_asked, turn_completed, turn_failed), session_end |
| opencode | plugin | `$XDG_CONFIG_HOME/opencode/plugins/overseer.js` | session.status, session.idle, permission.asked/replied, session.error |
| Amp | plugin | `$XDG_CONFIG_HOME/amp/plugins/overseer.js` | session.start, agent.start, agent.end |
| Antigravity (`agy`) | terminal activity | none | none |
| Shell | none | none | none |

Every agent's events map onto the same line states: a submitted prompt makes the line busy, a
finished turn makes it idle, and a permission or question prompt sets "needs input". The app turns
those transitions into OS notifications: "<agent> finished in <workspace>" and
"<agent> needs your input"; clicking one focuses that tab (see 14.4a.1 to tune or mute them). Agents without hooks, and agents whose
hooks did not fire, fall back to terminal activity: in a daemon terminal, a line is busy while the
agent keeps writing after you pressed Enter, and its turn ends after 4 s without output. Amp has no
needs-input event; Cursor Agent fires only some hooks in the CLI; Mistral Vibe fires no hook
when you interrupt a turn, so its line stays busy until the next turn ends.

Hooks call `~/.overseer/bin/overseer`, a symlink the daemon points at its own binary on
every start and hot upgrade, so a server upgrade never breaks them. When the daemon starts and
finds Overseer hook entries that name another binary (older installs wrote
`~/.overseer/server/<version>/overseer`), it rewrites them in place with the same merge,
backup and validation. Files without Overseer entries are never touched.

Codex runs new or edited hooks only after you trust them: start `codex` and choose "Review hooks" or
"Trust all and continue" at its "Hooks need review" prompt, or run `/hooks` inside it (`codex exec` cannot
review hooks). Overseer never edits `config.toml`; `overseer doctor` shows which events still lack a trust entry. Gemini CLI before 0.26 needs hooks enabled in its settings.
Files that Overseer owns (the Copilot hook file, the Mistral Vibe `hooks.toml`, the opencode and
Amp plugins) are refused if a file of the same name was not written by Overseer. Mistral Vibe
before 3.0 used other hook names behind an experimental flag and is not supported by the hooks.

The hook program only appends `{agent, event, at, sessionId, cwd, pid, tmuxPane, termId}` to
`<state>/spool/<pid>.jsonl`. Prompts and messages are never stored.

### 3.3 Daemon and sync as services

The app can install the daemon's own service for you (14.4, **Keep the server running**): a
`systemd --user` unit `~/.config/systemd/user/overseer-daemon.service` on Linux, a LaunchAgent
`~/Library/LaunchAgents/dev.overseer.daemon.plist` on macOS, both running the stable shim
`~/.overseer/bin/overseer daemon`. Otherwise Overseer installs no service. `deploy/`
contains ready-made files to copy and edit, for setups that want different flags or the sync
service:

| File | Runs |
|---|---|
| `deploy/systemd/overseer-daemon.service` | `%h/.local/bin/overseer daemon --interval 5 --http 127.0.0.1:7878`, `Restart=on-failure` |
| `deploy/systemd/overseer-sync.service` | `%h/.local/bin/overseer sync`, after the network and the daemon, `Restart=always` |
| `deploy/launchd/io.github.basilelt.overseer.daemon.plist` | same daemon command, `RunAtLoad`, `KeepAlive`, stderr to `<state>/daemon.log` |
| `deploy/launchd/io.github.basilelt.overseer.sync.plist` | `overseer sync`, stderr to `<state>/sync.log` |

All four set `OVERSEER_STATE_DIR` to `~/.local/state/overseer`. The systemd files are
`systemd --user` units:

```
mkdir -p ~/.config/systemd/user ~/.local/state/overseer
cp deploy/systemd/overseer-*.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now overseer-daemon.service overseer-sync.service
```

On macOS, replace every `USERNAME` in the plists (and adjust the `PATH` they set, Homebrew first),
then:

```
cp deploy/launchd/io.github.basilelt.overseer.*.plist ~/Library/LaunchAgents/
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.github.basilelt.overseer.daemon.plist
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/io.github.basilelt.overseer.sync.plist
```

Start the sync service only after the station is enrolled (3.4). The CLI works without the
daemon; the daemon adds the hook ingestion loop, the local API, the Board and relayed command
execution.

### 3.4 Enrol and sync

An admin creates the station on the API (section 9.3) and hands over its token once:

```
printf '%s\n' "$STATION_TOKEN" | overseer enroll --api http://api.example.com:8080
overseer sync --once      # one pass: scan, push, wait for the ack, exit (non-zero if not fully acked)
overseer sync             # keep running: rescan + heartbeat every 15 s, reconnect with backoff (1 s to 60 s)
```

`enroll` accepts `http://` (or `ws://`) URLs only and writes `<state>/sync.json` (`0600`). The
station id and owner come from `config.json`. Sync state lives in `<state>/sync.db` (outbox of
deltas, last pushed lines): an outage is replayed from the outbox, a lost outbox costs one
snapshot. Stations reach the API over a private network; there is no TLS in the daemon (ADR 0009).

### 3.5 Login (user token)

```
printf '%s\n' "$USER_TOKEN" | overseer login --api http://api.example.com:8080
```

`login` takes an `http://` URL, checks the token with `GET /v1/stations`, then writes
`<state>/user.json` (`0600`); a refused token writes nothing. Login is what lets this station use
other stations' lines: `ls` across stations, remote `resume`, `patch` of a remote line, and Board
actions relayed through the API. Enrolment (station token) and login (user token) are
independent.

### 3.6 Dev Mac

1. Build, copy `overseer` to `~/.local/bin`, export `OVERSEER_STATE_DIR`.
2. Write `config.json` (`stationId`, `owner`, `roots`, `ssh` targets of your VMs, `yardUrl`).
3. `overseer hooks install`, then `--apply`.
4. Load the launchd daemon plist; open the Board with `overseer board`.
5. Ask the admin to create the station with `"kind": "mac"`, `overseer enroll`, load the sync plist, `overseer login`.
6. Optionally register the `overseer://` URL handler (see `open`).

Leave `acceptRemoteCommands` off unless you want the API to hold/close/resume lines here.

### 3.7 VM

Same as the Mac with the systemd user units (station created with `"kind": "vm"`). To keep the user services running after
logout: `loginctl enable-linger $USER`. Set `acceptRemoteCommands: true` if you want to act on this
VM's lines from the Board of another station. Make sure the Mac can reach the VM with the ssh
target in its `ssh` map (or a Host alias equal to the station id) and that `overseer` is on the
VM's non-interactive PATH (or set `remoteCommand`).

### 3.8 Runner station

A runner starts lines on request from feedback-hub (section 6). `deploy/runner/Dockerfile` builds
an image with the daemon, `git`, `tmux`, Node.js and Claude Code from the official npm package
`@anthropic-ai/claude-code`. Build it yourself; the build context is the repo root:

```
docker build -f deploy/runner/Dockerfile --build-arg CLAUDE_CODE_VERSION=latest -t overseer-runner .
```

The image runs as user `runner` (uid 10001) with
`OVERSEER_STATE_DIR=/home/runner/.local/state/overseer`, has the hooks installed into its
`~/.claude` at build time, declares `VOLUME /home/runner`, and its command runs
`overseer daemon --interval 5 &` then `exec overseer sync` under `tini`. Claude credentials
(`ANTHROPIC_API_KEY` or a login stored in the volume) are supplied at run time, never baked into the
image.

Runner `config.json` (in the volume, at `<state>/config.json`):

```json
{
  "stationId": "runner-1",
  "owner": "feedback@example.com",
  "acceptRemoteCommands": true,
  "repos": { "github.com/acme/live": "/home/runner/repos/live" },
  "worktreeDir": "/home/runner/worktrees",
  "startArgs": ["--permission-mode", "acceptEdits"]
}
```

Clone the repos under `/home/runner/repos` (with credentials to fetch them). Create the station
with `"kind": "runner"` on the API, then enrol from a one-off container on the same volume:

```
docker run -i --rm -v runner-home:/home/runner overseer-runner \
  overseer enroll --api http://api.example.com:8080 < station-token.txt
docker run -d --name runner-1 -v runner-home:/home/runner -e ANTHROPIC_API_KEY overseer-runner
```

`overseer doctor` shows a `runner` item when `acceptRemoteCommands` is on: `claude` on PATH,
`tmux`, every `repos` entry is a clone, at least one of `repos`/`roots` set.

## 4. Commands

```
overseer ls [--all] [--json] [--local] [--owner EMAIL|all] [--ticket AS-N] [--repo TEXT] [--state active|idle|held|hung-up]
overseer status [--json]
overseer board
overseer resume <line> [--print] [--attach]
overseer open overseer://resume/<line> [--yes]
overseer patch <AS-N> [--local] [--print]
overseer link <line> <AS-N|none>
overseer link <line> --env <envRef|none>
overseer pin <line> [--off]
overseer hold <line> [--yes] [--json]
overseer close <line> [--yes] [--json]
overseer doctor
overseer daemon [--interval SECONDS] [--once] [--http 127.0.0.1:PORT] [--detach]
overseer socket-path
overseer version [--json]
overseer mcp                       (stdio MCP server for agents, section 14.5)
overseer hooks install|uninstall [AGENT...] [--apply]
overseer enroll --api URL          (station token on stdin)
overseer sync [--once]
overseer login --api URL           (user token on stdin)
overseer hook <claude|codex> <Event> [agent-pid]   (internal)
overseer stub <line>                               (internal)
overseer titles <line>                             (internal)
```

`<line>` is a line id, a session id, or any unique prefix of either (or of the ULID part of the
id). A usage error exits with code 2.

### `ls`

Lists Claude Code and Codex sessions of this station, newest activity first: lines active in the
last 7 days plus every live, pinned or user-held line. `--all` lists everything. The last line
shows the station counters (active / idle / held, agents' RSS, memory used / total, orphans,
duplicates) and, when enabled, the memory policy line.

- `--ticket` is case-insensitive; `--repo` matches a substring of the normalised repo; `--state`
  filters on the state.
- STATE shows `held(lost)` (process gone, resumable), `held(user)` (held on purpose) and the flags
  `!input` (agent waits for you), `pinned`, `ORPHAN` (its tmux session has no client attached),
  `DUP` (two processes on one session id), `OFFLINE` (station offline). WORKTREE is suffixed
  `[missing]` or `[tmp]`.
- `--json` prints `[{line, local}]`: `line` follows `contract/schemas/line.schema.json`; `local`
  holds `pid`, `rssBytes`, `busy`, `needsInput`, `pinned`, `orphan`, `duplicate`, `heldReason`,
  `cwd`, `cwdMissing`, `volatile`, `launchArgs`, `resumeCommand`, `resumeVerified`, `title`,
  `tmuxSession`, `tmuxPane`.
- `owner` is the `git config user.email` of the line's worktree, else the config `owner`.
- After `login`, `ls` also lists other stations' lines from the API (this station is always read
  locally). Those rows are filtered on the config `owner`; `--owner EMAIL` picks another owner,
  `--owner all` lists everyone's, `--local` skips the API. `--json` keeps the API shape
  `{line, stationOnline, syncedAt}` for them. If the API is unreachable, `ls` warns and lists this
  station only.
- Launch flags of a dead session come from the last time Overseer saw it running, else from
  `claude_args` in `~/.local/state/tmux-snapshot/latest/state.json` (read-only).

States: archived Codex thread or closed line: `hung-up`; no live process: `held`; turn running or
activity within `idleAfterSeconds`: `active`; otherwise `idle`. Activity and "turn running" come
from the hooks when installed, else from `~/.claude/sessions/<pid>.json` and file times.

### `status`

Station counters as one text line. `--json` prints the full status object (`stationId`,
`version`, counts, `orphans`, `duplicates`, `agentRssBytes`, `memTotalBytes`, `memUsedBytes`, and
`memoryPolicy` when enabled). With the memory policy on, the text output has a second line starting with `memory policy:` (section 8).

### `board`

Prints the Board URL (with its token) of the running daemon; see section 5.

### `resume`

Relaunches a line that is not running, in its cwd, with its original flags:
`claude <flags> --resume <id>` / `codex resume <flags> <id>`. Where it runs:

1. in the pane that shows the held stub of that line;
2. else, when run inside tmux, in a new window of the current session;
3. else in the current terminal (`exec`).

`--print` only prints the command. A running line is refused (attach to its pane instead).

`--attach` (what a remote resume runs on the far side): resume in the held pane or a new detached
session `sb-...`, then `tmux attach` to it; a line already running in a pane is attached to.
Without a terminal it prints `attach with: tmux attach -t ...`.

**Another station.** When `<line>` matches no line here and you are logged in, it is looked up in
the API and resumed on its own station over ssh, in this terminal:

```
ssh -t <target> -- overseer resume <lineId> --attach
```

`<target>` is `ssh.<stationId>` from `config.json`, else the station id; `overseer` is
`remoteCommand`. `--print` prints that ssh command. A line whose station is offline is refused
(`... runs on station X, which is offline`).

### `open` (deep links)

`overseer open overseer://resume/<lineId>` asks `resume line ...? [y/N]` and then resumes it
like `resume` (locally or over ssh). Other URLs are refused; without a terminal it needs `--yes`.
The handler files are in `deploy/url-handler/`; Overseer never registers them:

- Linux: `cp deploy/url-handler/overseer-url.desktop ~/.local/share/applications/` then
  `xdg-mime default overseer-url.desktop x-scheme-handler/overseer`.
- macOS: `osacompile -o ~/Applications/OverseerOpen.app
  deploy/url-handler/OverseerOpen.applescript`, merge `Info.plist.fragment` into the app's
  `Contents/Info.plist` (`CFBundleURLTypes`), then open the app once so LaunchServices registers
  the scheme. It runs `overseer open <url>` in Terminal, so `overseer` must be on the login
  PATH.

### `patch`

Takes over the line of a ticket; see section 7.

### `link`

`link <line> AS-1234` sets the ticket by hand (wins over the branch name); `link <line> none` goes
back to the ticket inferred from the branch. A ticket key is a Jira-style `PROJECT-123`: an
uppercase project key of two or more letters, digits or `_`, a dash and a number, found anywhere
in the branch name (`feature/PROJ-123-login`). Lowercase keys in branch names (`fix/proj-123`)
are recognised only for the project keys listed in `OVERSEER_TICKET_KEYS` (comma-separated, e.g.
`OVERSEER_TICKET_KEYS=PROJ,OPS` in the daemon's environment). `link <line> --env <envRef>` sets the line's env
reference (shown as a Yard link on the Board); `--env none` (or `-`) clears it. An env ref is
1 to 128 characters without whitespace. Both act on local lines only.

### `pin`

`pin <line>` protects a line from `hold` (the memory policy reports it as not holdable); `--off`
removes it.

### `hold`, `close` (dry run by default)

`hold` stops an agent to free memory while keeping its session. It acts only when every check
passes, and only with `--yes`; otherwise it prints the checks and the planned steps. Every run is
logged to `<state>/actions.log`. Exit code 3 = refused; `--json` prints the report as JSON.

| Check | Refuses when |
|---|---|
| alive | no live process, or two processes on the same session |
| inTmux | the agent is neither in a tmux pane nor in an Overseer (daemon) terminal |
| atPrompt | a turn is running (hook `UserPromptSubmit` without `Stop`, or `status: busy`) |
| inputEmpty | text is typed in the prompt, or the prompt cannot be recognised on screen |
| noToolRunning | a Bash tool, a subagent, a background task or an unknown child process runs |
| notPinned | the line is pinned |
| notNeedsInput | the agent waits for a permission / answer |

With `--yes`: the line is recorded `held(user)`, the pane gets `@sb_held=<lineId>`, the agent gets
SIGTERM (if it has not exited within `OVERSEER_HOLD_TIMEOUT_MS`, default 10 s, the hold is
aborted and reverted), and the pane shows
`held - AS-1234 - claude ... press Enter to resume`. Enter resumes in place;
`overseer resume <line>` does the same from elsewhere. A held line stays held after a reboot;
to get the stub back in its pane after a reboot apply [`docs/toolkit-patch.md`](toolkit-patch.md).

A pause queued for the end of the turn (`afterTurn`) also waits for running sub-agents. A
background task does not wait: after `OVERSEER_HOLD_GRACE_MS` (default 10 s) the pause is dropped
and the app is told. If an agent lost its sub-agent end event, the pause waits until the session
ends or restarts, the line is closed, or you cancel the pause.

`close` marks a line `hung-up` (finished). A live line is held first, with the same checks.

An agent running in an Overseer terminal (daemon term) is held through the daemon
(`POST /v1/lines/<id>/hold|close?dryRun=false`, or Hold / Close in the app's chat header): the
screen checks read the term's own screen, the agent gets SIGTERM, and the same term then shows the
held stub; Enter in it, or Resume in the app, restarts the agent there. Close also ends the term.
The `overseer hold|close` commands run in their own process and only see tmux panes.

### `doctor`

Prints one line per check: level, name, detail. Exits non-zero when any item is `fail`. Items are
described in section 12.

### `daemon`

Foreground loop: ingests hook events and rescans every `--interval` seconds (default 5), writes
`<state>/status.json`, executes relayed commands found in the inbox, and serves the local API of
`contract/daemon.openapi.yaml` on `<state>/daemon.sock` (`0600`, or `$OVERSEER_SOCKET`).
`--once` runs a single pass. `--http 127.0.0.1:PORT` also serves the Board on a loopback port
(`PORT` 0 picks a free one); a non-loopback address is refused. One daemon per state dir: a
second one exits with an error while `<state>/daemon.lock` is held, whatever its socket path.

```
curl --unix-socket "$OVERSEER_STATE_DIR/daemon.sock" http://unix/v1/lines
curl --unix-socket "$OVERSEER_STATE_DIR/daemon.sock" -X POST 'http://unix/v1/lines/<id>/hold'   # dry run
curl --unix-socket "$OVERSEER_STATE_DIR/daemon.sock" -X POST -d '{"ticketKey":"AS-1234"}' http://unix/v1/lines/<id>/link
```

`?dryRun=false` makes hold, close and resume act. The CLI does not need the daemon.

`--detach` starts the daemon in the background (its own session, output in `<state>/daemon.log`,
pid in `<state>/daemon.pid`), waits until it answers, prints the socket path and exits 0; when a
daemon already runs on this state dir it only prints the path. `overseer socket-path` prints
the path without starting anything; `overseer version --json` prints
`{version, commit, os, arch, protocol}` (also `GET /v1/version`). Terminals, tasks and the
activity panel of the desktop app are served by the daemon only: see section 14.

Upgrading the server keeps your terminals. When the desktop or mobile app connects to a host
whose daemon is older than the server it ships, it installs the new server and asks the running
daemon to hot-upgrade (`POST /v1/upgrade`, ADR 0034): the daemon checks the new binary, then
re-executes it in place with the same pid, so terminals, their scrollback and the programs in
them keep running; the app only sees its connections drop and come back. `GET /v1/version` then
reports `epoch` 1 and `upgradedFrom`. By hand:

```
curl --unix-socket "$OVERSEER_STATE_DIR/daemon.sock" -X POST -d '{"version":"0.9.0"}' http://unix/v1/upgrade
```

Daemons from before this feature, Windows, or a new binary that fails the check fall back to a
restart: `overseer daemon --detach --replace` stops a running daemon of another version (pid
from `daemon.pid`) and starts this one, which closes its terminals. The app does that by itself
when no terminal is open and otherwise shows a banner with a Restart button. A failed hot upgrade
is logged in `<state>/daemon.log` and leaves the old daemon serving.

### `hooks install|uninstall`

See 3.2.

### `enroll`, `sync`, `login`

See 3.4 and 3.5.

### Internal commands

- `hook <claude|codex> <Event> [agent-pid]`: called by the agents' hooks; appends one record to the
  spool and always exits 0.
- `stub <line>`: runs in a held pane, shows the held message and resumes on Enter.
- `titles <line>`: prints the last 5 `ai-title` titles of a local Claude line as a JSON array; used
  by `patch --local` over ssh. Never conversation content.

## 5. Board

```
overseer daemon --http 127.0.0.1:0     # or a fixed port (the services use 7878); loopback only
overseer board                         # prints the Board URL (with its token)
```

The daemon serves the Board at `GET /board` on its socket and, with `--http`, on a loopback port.
A new token is generated at each start; open the URL printed by `overseer board` (also in
`<state>/board.json`, `0600`). Without the token, or with a `Host` other than `127.0.0.1`,
`localhost` or `[::1]`, the daemon answers 403.

- Lines are grouped by ticket, then "No ticket", with state, flags (needs input, pinned, orphan,
  duplicate, missing worktree), agent, station, repo, branch, worktree, last activity, PR + CI
  (read with `gh pr list`, cached 60 s; nothing shown if `gh` is missing or not logged in) and the
  env link (`envRef` to `<yardUrl>/envs/<envRef>`).
- Live: the page listens to `GET /v1/events` (SSE) and refreshes on each change; the daemon sees
  changes at each scan (`--interval`).
- Actions: resume, hold, close, link, pin, diff, commit, stash. Hold, close, resume, commit and
  stash first run as a dry run; a refusal shows the failed checks, otherwise a dialog shows what
  will happen and asks to confirm. Diff is read-only. Commit (`git add -A` + `git commit`) and
  stash (`git stash push -u`) refuse a mid-turn line or a clean worktree; they use the worktree's
  git config and hooks. All of them are logged to `actions.log`.
- Other stations: read from `<state>/stations/<stationId>.json`, which `overseer sync` writes
  from the `stations` frame the API sends in reply to each heartbeat. Only lines whose owner equals
  this station's `owner` are kept. A station whose last heartbeat seen by the API is older than
  2 minutes is offline: its lines are greyed and their actions disabled with the reason.
- Lines of other online stations are actionable for resume, hold, close and link when this
  station is logged in (`overseer login`): the daemon relays the command through the API
  (`POST /v1/lines/{id}/commands`) and waits up to `OVERSEER_RELAY_TIMEOUT_MS` (default 15 s)
  for the outcome. Hold, close and resume are relayed as a dry run first, then for real after
  confirmation; link is applied directly. The target station must have
  `acceptRemoteCommands: true`. Without login those lines show the reason
  `run overseer login here to act through the api`. Pin, diff, commit and stash stay local-only.
- Language: English by default, French with the header toggle or `?lang=fr`. Strings live in
  `STRINGS` at the top of `board/board.js`.

## 6. Relayed commands, feedback-hub and runners

### 6.1 Relayed commands (ADR 0016)

A user (or admin) sends a command for a line to the API, which relays it to the line's station over
the sync WebSocket:

```
POST /v1/lines/{lineId}/commands  {"id":"<idempotency key>","kind":"hold","args":{"dryRun":true}}
GET  /v1/lines/{lineId}/commands  # audit: requestedBy, status, reason, completedAt
```

Kinds accepted here: `resume`, `hold`, `close`, `link` (`args.ticketKey`), `relaunch`
(`args.message`). `start` is internal: the API creates it from `POST /v1/lines`. Allowed args are
`dryRun`, `ticketKey` and `message`. The API answers 202 with the pending command, 404 for an
unknown line, 409 when the station is offline (nothing is queued) or the id was already used for
another line. Reusing an id for the same line returns the existing command.

`sync` writes each command to `<state>/commands/inbox/<id>.json` and reports `delivered`. The
daemon or sync (whichever claims the file first, by an atomic rename into `commands/running/`)
executes it and reports `succeeded`, `refused` or `failed` with a reason; a command interrupted
mid-run is reported `failed: interrupted before completion`. The outcome is visible in the audit
and in the `command.completed` event.

- Off unless `"acceptRemoteCommands": true` on the station; otherwise `refused`,
  `disabled: acceptRemoteCommands is false on this station`.
- `args.dryRun` defaults to `true` for every kind: send `"dryRun": false` to act.
- `hold`/`close` run every predicate: a failing one gives `refused`, e.g.
  `inputEmpty: input box is not empty`.
- `resume` uses the held pane or a new detached tmux session.
- `relaunch`: same line, new turn. On a live line it is refused unless `alive`, `inTmux`,
  `atPrompt` and `inputEmpty` all pass, then the message is pasted into its pane. On a held or lost
  line it resumes it detached, waits for the prompt, then sends the message.
- Each outcome is logged in `actions.log` (`"action":"remote-command"`, `requestedBy`).

Message and prompt texts are never stored by the API and never logged.

### 6.2 feedback-hub: starting a line on a runner

```
POST /v1/lines
{"owner":"feedback-bot","repo":"github.com/acme/live","branch":"feedback/AS-1234",
 "agent":"claude","origin":"feedback","prompt":"...","ticketKey":"AS-1234",
 "idempotencyKey":"fh-123","dryRun":false}
```

Required: `owner`, `repo`, `branch` (or `ticketKey`: `feedback/<ticketKey>`), `agent` (`claude` or `codex`), `origin` (`dev` or
`feedback`), `prompt`. Optional: `ticketKey`, `stationId` (any connected station; default the most
recently seen online runner), `idempotencyKey`, `dryRun`.

**`dryRun` defaults to `true`**: feedback-hub must send `"dryRun": false` to actually start
anything. A dry run still registers the provisional line in the API (with `worktreePath` and
`agentSessionId` set to `pending`); it stays in the API's line list and there is no endpoint
to remove it.

The API creates a provisional line `<stationId>-<ULID>`, relays a `start` command and waits up to
10 s:

| Answer | Meaning |
|---|---|
| 202 + provisional line | delivered to the runner; the outcome comes later in `GET /v1/lines/{id}/commands` or the `command.completed` event |
| 409 `runner refused the line` | the runner refused or failed within the wait (e.g. no clone) |
| 503 `no runner online` | no connected runner (or the given `stationId` is not connected) |
| 504 `runner did not accept in time` | retry with the same `idempotencyKey` |

Retrying with the same `idempotencyKey` never starts a second line.

### 6.3 What the runner does on `start`

- Agent `claude` only (the API accepts `codex`, the runner refuses it). An empty prompt is refused.
- Clone: `repos[<repo>]` (normalised, lowercase, without `.git`), else a repo under `roots` whose
  normalised remote matches. Otherwise `refused: no clone of <repo> on this station`.
- Branch: `args.branch`, else `feedback/<ticket>` (`POST /v1/lines` fills the same default when
  `branch` is omitted and `ticketKey` is set); it must be a valid branch name.
- Worktree: `<worktreeDir>/<clone dir name>/<branch slug>` with `git worktree add` (with `-b` when
  the branch does not exist locally; nothing is fetched). An existing path is refused
  (`worktree path ... already exists`).
- A new detached tmux session `sb-<last 6 characters of the line id>` runs
  `claude <startArgs> --session-id <uuid>`.
- The line keeps the API's provisional id, owner and origin. A second `start` for a known line id
  succeeds with `already started in <worktree>`.
- The prompt is written to a `0600` file under `<state>/tmp`, pasted with tmux
  `load-buffer`/`paste-buffer` once the agent shows an empty prompt, then deleted. It is never
  logged, never stored by the API and never on a command line. If the prompt does not appear within
  `OVERSEER_START_TIMEOUT_MS` (default 30000) the command fails with
  `the agent in ... did not reach its prompt`.

Relaunch uses the same delivery and timeout.

## 7. `overseer patch AS-NNNN`

ADR 0007, decision 2: a dev takes over an agent's line, by ssh attach by default, or as a new
local line on demand.

`patch` picks the most recent open (not `hung-up`) line of the ticket among this station's lines
and, when logged in, the API's lines of every owner, preferring lines not owned by you (the
agent's). The argument must be a ticket key.

- Default (attach): same as `resume <line> --attach`. For a remote line this is
  `ssh -t <target> -- overseer resume <id> --attach`. `--print` shows what it would run.
- `--local`: a new local line on the same branch:
  - worktree `<worktreeDir>/<repo name>/<branch slug>-patch`, from the local clone found via
    `repos`/`roots`; the branch is fetched from that clone's `origin` first, and tracked from
    `origin/<branch>` when it does not exist locally;
  - new tmux session `sb-patch-<branch slug>` running `claude --session-id <uuid>`;
  - seeded with a short summary: line id, station, repo, branch, ticket, PR URL (via `gh`, if any)
    and the last 5 `ai-title` titles of the previous agent (read locally, or over
    `ssh -T <target> -- overseer titles <id>` on the agent's station). Never conversation
    content. The summary goes through a `0600` temp file, is pasted to the new agent, then deleted.
  - `--local --print` describes the plan without doing anything.

Attach with `tmux attach -t sb-patch-...` (the command prints it).

## 8. Memory policy

```json
"memoryPolicy": { "enabled": true, "thresholdPercent": 85 }
```

Off by default. It only **reports**; it never holds anything. When used memory is above the
threshold, the idle live lines are listed least recently used first, each evaluated against the
hold predicates:

- `status --json`: `memoryPolicy` = `{thresholdPercent, usedPercent, over, wouldHold: [{lineId,
  holdable, failed, rssBytes}]}`; `wouldHold` is empty when not over.
- `status`, `ls`: a line `memory policy: 62.5% used, under 85%`, or
  `memory policy: 91.2% used, over 85%; idle lines that would be held (report only, run overseer hold): ...`.
- `doctor`: `skip` when off, `ok` under the threshold, `warn` over it.

Nothing is reported when used memory cannot be read. Act with `overseer hold <line> --yes`.

## 9. overseer-api

### 9.1 Run it

```
OVERSEER_ADMIN_TOKEN=$(openssl rand -hex 32) \
OVERSEER_API_ADDR=127.0.0.1:8080 OVERSEER_API_DB=./api.db target/release/overseer-api
```

- `OVERSEER_API_ADDR` (default `0.0.0.0:8080`; port `0` picks a free one, printed on stdout).
- `OVERSEER_API_DB` (SQLite file, default `overseer-api.db` in the working directory,
  created `0600`).
- `OVERSEER_ADMIN_TOKEN`: admin endpoints are disabled without it.
- `GET /healthz` answers `ok`. A station is marked offline 45 s after its last frame.

### 9.2 Docker compose and Kubernetes

- Local container: `OVERSEER_ADMIN_TOKEN=... docker compose -f deploy/docker-compose.yml up --build`
  (binds `127.0.0.1:8080`, data in the `api-data` volume). Stop with `... down` (`-v` drops the
  data).
- Kubernetes: create the Secret out of band, then apply the manifests (one replica, PVC, Service):

```
kubectl create secret generic overseer-api --from-literal=admin-token="$(openssl rand -hex 32)"
kubectl apply -k deploy/k8s
```

### 9.3 Tokens and enrolment

Every call sends `Authorization: Bearer <token>`. Tokens are shown once and stored hashed.

```
curl -H "Authorization: Bearer $ADMIN" -H 'Content-Type: application/json' \
  -d '{"stationId":"vm-node2-dev1","kind":"vm"}' $API/v1/stations      # {"stationId","token"}
curl ... -d '{"name":"bob"}' $API/v1/users                          # {"name","token"}
curl -X DELETE ... $API/v1/stations/vm-node2-dev1/token                  # revoke, closes its socket
curl -X DELETE ... $API/v1/users/bob/token
```

`kind` is `mac`, `vm` or `runner` (`runner` is what `POST /v1/lines` picks); `owner` is optional.
Enrolling an active station again is refused (409); enrolling a revoked one issues a new token.

### 9.4 Reads (user or admin token)

```
GET /v1/lines?owner=&ticketKey=&ticket=&stationId=&state=&includeHungUp=true
GET /v1/lines/{lineId}
GET /v1/stations           # online, lastSeenAt, activeCount, memory, revoked
```

`ticket` is an alias of `ticketKey`. Each line comes as `{line, stationOnline, syncedAt}`;
`hung-up` lines are hidden unless `includeHungUp=true` or `state=hung-up`.

### 9.5 Sync WebSocket

`GET /v1/stations/{id}/sync` (station token) carries `hello`/`ack`, `snapshot`/`delta`,
`heartbeat`, `command`/`command_result` frames (`contract/schemas/sync-message.schema.json`). In
reply to each heartbeat the API also sends a `stations` frame: the other stations with their
`lastSeenAt` and their open lines, which `sync` writes to `<state>/stations/` for the Board.

### 9.6 Events

- SSE: `GET /v1/events` (`?types=line.state_changed,station.offline`); send `Last-Event-ID` to
  replay up to 7 days after that id. Types: `line.created`, `line.updated`, `line.state_changed`,
  `line.hung_up`, `station.online`, `station.offline`, `command.requested`, `command.completed`.
- Webhooks (admin): `POST /v1/webhooks {"url":"http://...","types":["line.state_changed"]}` returns
  a `secret` once; `DELETE /v1/webhooks/{id}` removes one. URLs are `http://` only. Each delivery
  carries `X-Overseer-Signature: t=<unix>,v1=<hex>` where
  `v1 = HMAC-SHA256(secret, "<t>.<raw body>")`; check it and reject old `t`.

### 9.7 feedback-hub

`POST /v1/lines` (section 6.2), then follow the line with `GET /v1/lines/{id}`, its commands, SSE
or webhooks. To follow up on the same line, `POST /v1/lines/{id}/commands` with
`{"kind":"relaunch","args":{"message":"...","dryRun":false}}`.

## 10. TwinFactory

TwinFactory consumes the read endpoints (`/v1/lines`, `/v1/lines/{id}`, `/v1/stations`), the SSE
stream and signed webhooks. The contract is `contract/api.openapi.yaml` and the JSON schemas in
`contract/schemas/` (`line`, `command`, `event`, `sync-message`); the daemon's local API is
`contract/daemon.openapi.yaml`. Compatibility is covered by `tests/twinfactory.rs`, which runs a
fake consumer against the API and checks every payload against the schemas.

## 11. Testing and demos

- `make test`, `make lint`: everything offline. Tests that need tmux use an isolated server
  (`tmux -L sb-test-*`), never your own.
- Stub agents in `tests/stubs` (claude, codex); fakes in `tests/fakes`: feedback-hub client,
  TwinFactory consumer, JSON-schema checker, static Yard (`tests/fakes/yard`); fixtures (fake `gh`,
  `ssh`, Claude data) in `tests/fixtures`.
- Demo scripts, each isolated (temp HOME and state dir, own tmux server, stub agents):
  - `scripts/demo-phase1.sh`: read-only `ls` (under bwrap when available);
  - `scripts/demo-phase2.sh`: hooks, live state, link, resume, daemon;
  - `scripts/demo-phase3.sh`: hold refusals, hold, reboot, resume, close, doctor;
  - `scripts/demo-phase4.sh`: api on a temp dir and random port, two fake stations, merged read;
  - `scripts/demo-phase5.sh`: relayed hold, login, `ls` across stations, remote resume (fake ssh);
  - `scripts/demo-phase6.sh`: Board with two fake stations, fake `gh` and Yard; prints the URL and
    waits for Enter (`DEMO_WAIT=<seconds>` when not on a terminal);
  - `scripts/demo-phase7.sh`: feedback-hub start and relaunch on a runner, `patch`.

## 12. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| Hooks stopped reaching the installed daemon after running a dev build | A daemon on a non-default state dir (checkout `.state`) used to rewrite the global hooks and `~/.overseer/bin/overseer`. Now only the daemon on the default state dir repairs them, and a daemon on another state dir logs `not repairing hooks` and leaves hooks that call another state dir alone; the default daemon heals hijacked hooks on its next start. Run dev daemons with their own `HOME` (`scripts/dev.sh`, `scripts/qa/sandbox.sh`). |
| CLI, services or hooks see different lines | They resolve different state dirs (e.g. `bin/overseer` uses `<checkout>/.state`). Set the same `OVERSEER_STATE_DIR` everywhere and rerun `hooks install --apply`. |
| `board`: `daemon not reachable` | Start `overseer daemon --http 127.0.0.1:0` (or the service). `no loopback board`: restart it with `--http`. |
| Board answers 403 | Missing or stale token (new at each daemon start): open the URL from `overseer board`; use `127.0.0.1`/`localhost` as host. |
| States lag or `!input` missing | Hooks not installed: `overseer hooks install --apply`. |
| Codex lines not tracked live | Codex needs you to trust new or edited hooks: start codex and review them at the "Hooks need review" prompt, or run `/hooks`. `overseer doctor` lists the events without a trust entry. |
| `api unreachable, showing this station only` | API down or wrong URL in `user.json`; `doctor` shows `api reachable`. `ls` falls back to local lines. |
| `the api refused the user token` | Token revoked: `overseer login` again. |
| `refused: disabled: acceptRemoteCommands is false on this station` | Set `"acceptRemoteCommands": true` in that station's `config.json`. |
| Relayed command `succeeded` but nothing happened | It was a dry run: send `"dryRun": false`. |
| `refused: inputEmpty: ...`, `atPrompt: ...`, `noToolRunning: ...` | A predicate failed: wait for the turn to end, clear the prompt, or act from the pane. |
| 409 `station offline; not queued` / `... which is offline` | The station has not been seen for 45 s by the API: check its `sync` service. |
| Board: other stations greyed | No heartbeat seen by the API for 2 minutes. |
| Board: other stations' lines not actionable | This station is not logged in: `overseer login`. |
| 504 from the Board relay | The station did not answer within `OVERSEER_RELAY_TIMEOUT_MS`; is its daemon or sync running? |
| `start`: `no clone of <repo> on this station` | Add it to `repos` (normalised repo) or put the clone under a `roots` entry. |
| `start`: `worktree path ... already exists` | Remove the old worktree (`git worktree remove`) or use another branch. |
| `did not reach its prompt` | Claude did not start (auth, PATH, trust dialog) within `OVERSEER_START_TIMEOUT_MS`; attach to the `sb-...` session to see why. |
| `patch`: `branch ... is neither in ... nor on its origin` | The branch was never pushed: push it from the agent's station or use the default attach. |
| `patch`: `... already exists` | A previous `--local` patch worktree is still there. |
| `patch`: `no open line for AS-...` | Nothing open for that ticket here or through the API (login?). |
| Memory policy shows nothing | Not enabled (`"enabled": true`), under the threshold, no idle live line, or memory figures unavailable. |

### Desktop app logs and crash reports

The app logs to `~/Library/Logs/io.github.basilelt.overseer/` on macOS,
`~/.local/share/io.github.basilelt.overseer/logs/` on Linux and `%LOCALAPPDATA%\io.github.basilelt.overseer\logs\`
on Windows (`app.log`, 5 files of 5 MB). The daemon logs to `daemon.log` in its state dir, with the
same rotation. After a crash, a freeze or a forced quit, the next launch opens "Previous session
ended unexpectedly" with the crash report, the end of the app log and, once connected, the end of
the server log: press **Copy all** and paste the result into your report. Palette > "Show logs"
opens the same view at any time. Secrets are masked and terminal contents are never logged
(`app/README.md`, Troubleshooting).

### Sending feedback

The speech-bubble button in the title bar (also palette > "Send feedback", Settings > Feedback,
phone More > Send feedback, and **Report this crash** on the "Previous session ended
unexpectedly" dialog) opens the feedback sheet: type (Bug, Idea, Other), an optional title, a
description and attachments (paste a screenshot, drop files or browse; they are uploaded to the
active host). **Include diagnostics** adds the app and server versions, OS and architecture, the
connection mode, the last 200 app and 100 server log lines, secrets masked; **Preview what will
be sent** shows the exact text. **Let the agent investigate first** asks the agent to read the
logs and code, read-only, and add a likely cause.

**Send with <agent>** opens a new tab on the active host running your default agent CLI, in a
local clone of the feedback repository when one is a known project (else in
`~/.overseer/feedback/work`). The agent searches existing issues with `gh issue list`, comments
on a matching one or creates one issue with `gh issue create`, as you (your own `gh` login), with
labels `bug`, `enhancement` or `feedback` when it may. Once it prints `ISSUE_URL=…`, the app shows
"Issue created" with the link. The tab stays open so you can follow and answer the agent. Claude
Code and Codex get the request as their initial prompt, so if they first ask whether to trust the
folder, accept and the request runs; other agents get it pasted once they are ready.

**Send directly** creates the issue with `gh` on the host without an agent. **Open on GitHub**
copies the report and opens a prefilled new-issue page, for when you are offline or have no
`gh`. The repository is `basilelt/overseer` unless you change Settings > Feedback >
Feedback repository.

Attachments are uploaded into the issue with `gh --attach` (gh 2.99 or later), the same storage as
a drag and drop on github.com: images show inline and videos play. GitHub accepts PNG, JPEG, GIF,
WebP and SVG up to 10 MB, and MP4, MOV and WebM up to 100 MB (10 MB on a free GitHub plan, which
the sheet warns about). The sheet refuses other types and larger files, and the daemon refuses
them too. With an older `gh`, or when GitHub refuses a file, the issue is still created and lists
that file as not uploaded with the reason, and the app shows GitHub's message. GitHub may require
write access to the repository to attach files.

### Recording a bug

Palette > **Record a bug** (phone: More > Record a bug) records what you do while you show a
problem. The setup dialog offers **Record the screen too** where the webview can capture the
screen (`getDisplayMedia` and `MediaRecorder`; a browser asks which screen or window to share);
elsewhere, such as iOS, Android and the Linux desktop app (WebKitGTK cannot capture), the option
is not offered and only the action log is recorded. If the webview rejects the
request, Overseer asks once more in the plain form; if the screen still cannot be captured, the
recording goes on with the action log only and says so. Declining
the share prompt cancels the recording as before. While it runs, a red pill at
the bottom shows the time and the number of actions, with **Stop** and a discard button; a
notification shows above the pill, never under it. The log
has the start, clicks (the control's label, never a field's content), shortcuts (Ctrl/Cmd/Alt
combinations, Escape, function keys, arrows), "typed in <field>" once per field without the text,
view and dialog changes, uncaught errors and error toasts, each with its time since the start.
Recordings stop on their own after 2 minutes (about 10 MB of video, the free GitHub limit) or when
the screen share ends.

**Stop** opens the feedback sheet on Bug with the recording: the video is uploaded to the active
host as a `recording-<date>-<time>.webm` (or `.mp4`) attachment, and the log goes into the report
as a "Recorded actions" block whose times match the video. **Replay** plays the video in the sheet
with the action list beside it: the current action is highlighted as it plays and clicking one
seeks the video there. The x on the Recording row removes both the video and the log. When the
upload fails (offline, daemon restarting), the log and Replay still work and **Attach video** on
the Recording row tries again.

### `doctor` items

Levels: `ok` (fine), `warn` (works, but something is missing or suspicious), `fail` (broken;
non-zero exit), `skip` (not configured, not checked), `info` (a setting worth knowing).

| Item | Checks |
|---|---|
| state dir | exists with mode 700 |
| daemon | answers on its socket (CLI works without it; hooks spool to disk) |
| claude hooks | how many Overseer events are in `settings.json` |
| codex hooks | same for `hooks.json` (only when `~/.codex` exists) |
| codex trust | read-only look at `config.toml`: `warn` listing the Overseer events with no trust entry, or disabled (`enabled = false`), with the step to take. When every event has an entry the item is `info`, not `ok`: Codex compares a hash of each hook, doctor does not, so an entry left from before Overseer rewrote `hooks.json` looks the same; run `/hooks` in codex to see the hooks needing review |
| claude, codex, tmux | version found on PATH (hold needs tmux) |
| daemon PATH | `info`: the running daemon's `PATH` and its source (`login-shell`, `fallback`, `inherited`; 14.1a) |
| station enrolment | `sync.json` present (`skip` when not enrolled) |
| api reachable | `GET /healthz` on the enrolment or login API (`fail` when not) |
| user login | `user.json` present |
| remote commands | `info`: accepted or refused (`acceptRemoteCommands`) |
| runner | only when remote commands are accepted: claude, tmux, clones in `repos`, some `repos`/`roots` |
| toolkit | `tmux-snapshot`/`tmux-restore` patched for `@sb_held` (only when they exist) |
| existing hooks | `tmux-claude-id.sh` still declared (only when it exists) |
| tmux-snapshot | snapshot state file present for launch flags of dead sessions |
| lines | the scan works; station counters |
| memory policy | section 8 |
| orphan | a live agent whose tmux session has no client (never killed) |
| duplicate | two live processes on one session id |
| held without stub | a user-held line with no stub pane: resume it with `overseer resume` |

## 13. Environment variables

Station (mostly for tests and services):

| Variable | Default | Effect |
|---|---|---|
| `OVERSEER_STATE_DIR` | `~/.local/state/overseer` (Linux), `~/Library/Application Support/overseer` (macOS) | State dir. |
| `OVERSEER_SOCKET` | `<state>/daemon.sock` | Daemon socket. |
| `OVERSEER_TMUX_SOCKET` | default tmux server | tmux `-L` server to use. |
| `OVERSEER_TMUX_SNAPSHOT` | `~/.local/state/tmux-snapshot/latest/state.json` | Snapshot file read for launch flags. |
| `OVERSEER_NOW` | current time | Fixed RFC 3339 "now". |
| `OVERSEER_HOLD_TIMEOUT_MS` | `10000` | Wait for the agent to exit after SIGTERM in `hold` (abort and revert after). |
| `OVERSEER_HOLD_GRACE_MS` | `10000` | How long a lasting block (e.g. a background task) must last before a pending `afterTurn` hold is dropped. |
| `OVERSEER_START_TIMEOUT_MS` | `30000` | Wait for the agent's prompt in `start`, `relaunch`, `patch --local`. |
| `OVERSEER_RELAY_TIMEOUT_MS` | `15000` | Board relay wait for the outcome. |
| `OVERSEER_TASK_ID` | set by the daemon | In an agent started for a task: its task id (MCP `id` default). |
| `OVERSEER_TERM_ID` | set by the daemon | In every daemon term: its term id. |
| `OVERSEER_SUPERVISED` | set by the service file | `1`: the daemon runs under systemd or launchd; when another daemon already holds the lock it exits 0 so the service does not loop. |
| `OVERSEER_SSH` | `ssh` (Windows: `System32\OpenSSH\ssh.exe`) | Desktop app: the ssh client used for remote hosts. |
| `OVERSEER_SSH_CONFIG` | none | Desktop app: an ssh config passed as `-F` (also used for the host list). |
| `OVERSEER_LOGIN_ENV` | `1` | `0`: the daemon keeps the environment it was started with instead of adopting the login shell's (14.1a). |
| `OVERSEER_DOCKER_TESTS` | unset | `1` runs the Docker sshd test of `make test-remote`. |
| `CLAUDE_CONFIG_DIR` | `~/.claude` | Claude Code data and settings. |
| `CODEX_HOME` | `~/.codex` | Codex data and hooks. |

API: `OVERSEER_API_ADDR`, `OVERSEER_API_DB`, `OVERSEER_ADMIN_TOKEN` (section 9.1).

## 14. Desktop app: terminals, tasks, activity, MCP, server mode

The app talks to the daemon with the protocol of [`contract/app-daemon.md`](../contract/app-daemon.md)
(protocol 1; ADRs 0022–0024, 0026). Everything below needs a running daemon.

### 14.0 Welcome and guided tour

On first launch, once the app is connected (immediately in local mode, after the first successful
connection on a phone), a welcome card offers **Take the tour** or **Skip**. The tour dims the
window and highlights the real controls one at a time: host chip, workspaces sidebar, tabs and the
new-session menu, the Terminal/Chat toggle, the side panel, Tasks, Review, the usage, system and ports
chips, notifications and hooks, the command palette and Feedback, in 0.25 the Cmd/Ctrl+P palette shortcut, and in 0.27 the demo mode (on the **Demo mode** pill while in the demo, else on the host chip), and in 0.28 the GPU rendering option for terminals, and in 0.29 the clean up of unused worktrees and the import from Switchboard (on its button in Settings, Help, or on the Settings button), and in 0.30 click to move the cursor in a terminal (on the tabs), and in 0.31 the Light and Dark palettes (on Settings, or More on a phone), and in 0.33 the one-click install of skills and plugins (on Settings), and in 0.34 the auto-approval defaults (on Settings, Agents), the Review, Merged and Closed columns (on the Merged column when the board is open, else on the Tasks tab) and the clean removal of everything Overseer added (on Settings, Connection, **Remove**), and in 0.35 one-click in-app updates for every install (on Settings, Updates, else on the Settings button), and in 0.36 subjects inside a workspace (on **+ New subject** in the open workspace, else on the sidebar), and the live review behind the git button (on the git button of a session's tab bar, else on the command palette), and the review map (on List | Map in a review, else on the Review tab), and keeping agents running with the lid closed (on its switch in Settings, Connection, else on the Settings button), and Record a bug (centred, on the command palette), and the Quorum option of a new task (on the Quorum control in the New task dialog, else on the Tasks tab), and installing a missing agent from the app (on the title bar **+**, else on the palette), and agents kept inside their project (on **Agents message each other** in Settings, Defaults, else on the Settings button), and Mistral Vibe as a new agent (on the title bar **+**, else on the palette), and the real cost of each session and task (on the cost line of the side panel, else on the side panel), and profiles by task type (on the **Type** field of the New task dialog, else on the Tasks tab), and the choice between agents filing what they find as tasks or fixing it themselves (on **Agents file what they find as tasks** in Settings, Defaults, else on the Settings button), and several hosts in one window (on the host sections of the sidebar when a window shows several hosts, else on the host chip), and sticky notes on sessions (on the note icon of a session that has notes, else on the sidebar). Steps whose control is not on
screen (no session selected, a feature the server lacks, phone layout) are skipped. On a phone the
card is a sheet at the bottom (or top, when the control sits low) and points at the bottom
navigation, the session strip and the menu.

Keys: Right arrow for next, Left arrow for back, Esc to close. Skipping or finishing records, in the
app settings (`tourSeen`), the newest release whose tour you saw, so it is not shown again. After an
update, every step added since that release shows under **What's new**, oldest release first, each
labelled **New in x.y**: going from 0.22 to 0.26 tours the features of 0.23 to 0.26 at once. Only
minor and major releases add steps. To see it again: Settings > Help > **Take the
tour**, the palette command **Start the tour**, or **Show the tour again** in More on a phone.

Settings (gear in the title bar, or More on a phone) has a sidebar of categories: **General**
(appearance, links), **Terminal** (font, scrollback, GPU rendering, keyboard), **Agents** (task defaults,
auto-approval, profiles, hooks, usage), **Notifications**, **Connection** (hosts, server), **Updates** and **Help** (tour,
logs, feedback). The search box at the top of the sidebar has focus when Settings opens: type part of a
setting's name or description, in the app language or in English, and only the matching settings stay,
highlighted, with a count per category; Esc clears the search. On a phone the categories are a list
that opens a section, with a back button.

**Theme and palettes** (Settings > General > Appearance). **Theme** is System, Light or Dark; System
follows your operating system live, so Overseer switches when your computer does. Each of Light and Dark
has its own **palette**, used whenever that mode is on screen. The Dark palettes are **Default dark**,
**Graphite** (softer gray panels and a pale blue accent) and **Solarized Soft** (deep teal panels and warm
terminal colors); the Light palette is **Default light**. Picking a palette never changes the theme, so
System keeps following your OS.

Press **New** next to a palette to make your own for that mode. It starts from the palette selected and has
six colors (accent, background, panels, borders, text and secondary text) chosen with color pickers; the
other shades are derived from them. A preview updates as you pick, and a warning appears when text is
below the 4.5:1 contrast needed to read it comfortably. Change the name in the field, **Edit** to adjust a
custom palette later and **Delete** to remove it (the mode returns to its default palette). Terminals follow the background and text colors.
Palettes are saved with your other settings. Older settings migrate on first launch: a saved Graphite or
Solarized Soft theme becomes the Dark theme with that Dark palette, and colors you had changed become a
custom palette, so nothing looks different.

**GPU rendering** (Settings > Terminal, off by default, experimental) draws terminals with WebGL
instead of the normal renderer, which keeps long, fast output smooth. It needs hardware graphics
acceleration; where WebGL is not available the terminals keep the normal renderer. Selection,
scrolling and colours work the same with either renderer, and turning it off goes back to the normal
renderer.

**Import from Switchboard.** Overseer was called Switchboard before. When the daemon finds the old
Switchboard data on its machine and it is not imported yet, a dialog appears once after the app
connects. It shows how many sessions, tasks and workspaces will come over, and tells you when the
old Switchboard daemon is still running: it is stopped and its sessions are handed over to
Overseer, and the agent hooks move too. **Import** runs it, applies the settings of the old app
(theme, fonts, layout and so on, overwriting the current ones) and reloads Overseer. **Not now**
closes the dialog until the next launch; **Don't ask again** stops the offer for good. If the
import fails, the reason is shown in the dialog and nothing else changes. Until the import is done,
Settings, Help, **Import from Switchboard** opens the same dialog, also after **Don't ask again**.
Once imported the entry is gone, so newer Overseer data is never overwritten by the old one.

### 14.0a Demo mode

**Try the demo** lets you use the whole app without a server, for example to look around before
installing the daemon. It is on the connect screen: at the bottom of the host list on the desktop
(**Connect to host**), under **Add server** on a phone, and next to **Retry** when a connection fails.
The app reloads on sample data: sessions with live terminals, a task board, reviews with diffs,
chats, ports and saved hosts. Everything is simulated inside the app; nothing is installed, no SSH
connection is made and no notification or update check reaches the system.

While the demo runs, a **Demo mode** pill sits in the title bar (next to the host chip on a phone)
with **Exit demo**. The demo keeps its own settings, layouts and drafts apart from yours, and it
survives closing or backgrounding the app: it stays on until you exit it. **Exit demo** (on the pill
or on the connect screen) deletes the demo's data and reloads the app on your own hosts, settings
and sessions, exactly as you left them.

### 14.1 Terminals

The daemon owns pseudo-terminals: programs keep running when the app closes, any number of clients
attach, and `tmux` is not needed (Windows uses ConPTY). Terms live in daemon memory: restarting the
daemon ends them, but the next start brings them back (14.4, **After a server reboot or restart**). tmux
sessions are still detected as before.

Opening a tab never starts an agent. A restored tab, a click on a session task or **Attach** only
attaches to a process that is already running; a tab whose agent is not running shows an idle
**Resume** button, and only Resume (tab, side panel, menu, chat header, Resume all) runs
`claude --resume`. The daemon enforces it: `POST /v1/lines/<id>/resume` with `"attachOnly": true`
answers 409 instead of starting anything.

An agent can ask something before it starts, such as Claude Code asking whether to trust the
folder. Until it is answered the session reads **Starting**:
Resume is disabled and the chat offers **Open terminal** to answer it. Resuming again returns the
same terminal instead of starting a second agent, and Close ends it.

```
S=$(overseer socket-path)
curl --unix-socket "$S" -X POST -d '{"cwd":"/home/me/src/app","cmd":["claude"],"cols":120,"rows":32}' http://unix/v1/terms
curl --unix-socket "$S" http://unix/v1/terms
curl --unix-socket "$S" -X POST -d '{"confirm":true}' http://unix/v1/terms/<id>/kill
```

When an agent quits on its own in a tab (Ctrl+C, `/quit`, `exit`), the tab stays open: it prints
the session the agent ran and the command that resumes it, then starts your shell there. Killing the
term, closing the tab or holding the session ends it as before, and task terms are not kept.
**Close** on a session (side panel or chat) does the same: the agent stops, and its tab keeps a
shell with `<agent> closed. Session <id>, resume with: …`; an agent started from a shell leaves that
shell. This is `POST /v1/lines/<id>/close?dryRun=false&shell=true`; without `shell=true` (closing
the tab, the CLI, the organizer) the term ends with the agent.

`GET /v1/terms/<id>/io` is a WebSocket: the last 2 MiB of output first, then live output (binary
frames); binary frames sent are typed into the program, `{"resize":{"cols":…,"rows":…}}` text
frames resize it. An agent started in a term is linked to its line (`local.termId` in
`GET /v1/lines`). `POST /v1/lines/<id>/resume` with `{"target":"term"}` resumes a held or lost line
in a new term (it acts by default; `?dryRun=true` only shows the plan).

Every term gets `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=overseer`,
`TERM_PROGRAM_VERSION=<version>` and `OVERSEER_TERM=1` (a `POST /v1/terms` `env` can override
them), and none of the variables of the SSH session or multiplexer the daemon was started from:
`SSH_CONNECTION`, `SSH_CLIENT`, `SSH_TTY`, `TMUX`, `TMUX_PANE`, `STY`, and `SSH_AUTH_SOCK` when it
is a forwarded agent socket (`/tmp/ssh-*/agent.*`) that dies with that session. So an rc file
that starts tmux or a session picker for SSH logins does not fire in Overseer terminals; to
skip something explicitly, test the marker:

```
[[ -n $OVERSEER_TERM || $TERM_PROGRAM == overseer ]] || t --loop
```

In the app:

- **Shift+Enter** sends `ESC` + `CR` (Meta+Enter), which Claude Code, Codex and most TUIs read
  as "new line" and tmux forwards as `M-Enter`. Turn it off in Settings ("Shift+Enter inserts a
  newline") to send a plain Enter. zsh needs one line in `~/.zshrc` to insert a newline in a
  multi-line command instead of running it:

  ```
  bindkey '^[^M' self-insert-unmeta
  ```

  (bash and fish do this by default or ignore it; Overseer never edits your dotfiles.)
- **Images and files**: paste an image (Cmd+V, or Ctrl+Shift+V on Linux and Windows, with a screenshot
  on the clipboard; right-click a terminal for Copy and Paste; the desktop app shows no other
  webview menu with Back, Forward or Reload, only the native menu in text fields and an app Copy
  over selected text) or drop files on a terminal. The app uploads them to the daemon (`POST /v1/uploads`, 50 MB per file)
  which stores each one as `~/.overseer/u/<6-character id>.<ext>` (mode `0600`, deleted after 7
  days), then types the path followed by a space as a bracketed paste when the program asked for
  it. Other files are typed short, `~/.overseer/u/a1b2c3.pdf`; images keep their absolute path
  so Claude Code recognises them and shows `[Image #1]` instead of the path. Nothing is submitted:
  press Enter yourself. Dropping a file on a local terminal types its own path without copying it.
  A toast shows the upload (with progress for large files). Folders cannot be uploaded. The
  daemon's answer carries both forms: `{"path": "/home/me/.overseer/u/a1b2c3.png", "short":
  "~/.overseer/u/a1b2c3.png", "name", "size"}`.
- **Font**: Settings → Terminal → Font family takes any installed font by name (suggestions:
  MesloLGS NF, JetBrainsMono Nerd Font, FiraCode Nerd Font, Hack Nerd Font, SF Mono, Menlo;
  several comma-separated), with the font size (8–28) and the line height (1–2). The default is the
  bundled JetBrains Mono. The bundled "Symbols Nerd Font Mono" (Nerd Fonts 3.5.1, MIT, 1.2 MB
  woff2) always ends the chain, so Nerd Font and powerline icons of oh-my-zsh, powerlevel10k or
  starship prompts render with any font.
- **Mouse wheel** works like in a native terminal: when the program asks for mouse reports
  (tmux with `set -g mouse on`, vim with `mouse=a`, htop…) the wheel is sent to it as wheel
  events, so a Claude Code session inside tmux scrolls tmux's history; a full-screen program
  without mouse reports (less, man) gets Up/Down arrows unless it turned that off
  (`ESC [?1007l`); otherwise the wheel scrolls the terminal's own scrollback. Shift+wheel always
  scrolls the local scrollback. Drags always select text locally (they are not sent to the
  program).
- **Click to move the cursor**: a single click in the line being typed (a shell prompt, the
  input of Claude Code, Codex or another agent) moves the cursor there by sending Left/Right
  arrows, like in a text editor. Only the cursor's own row is handled; clicks on other rows,
  with a modifier, while scrolled back, on a phone, or in a program that reads the mouse itself
  (tmux with mouse on, vim with `mouse=a`, htop) do nothing. It only works where a line editor
  reads the input with bracketed paste on (bash 5.1+, zsh, fish, Claude Code): a shell without
  one, such as `sh` on Debian (dash), would type the arrows, so clicks there do nothing.

### 14.1a Agents

`GET /v1/agents` lists the agent CLIs the daemon knows — Claude Code, Codex, Gemini CLI,
Antigravity (`agy`), opencode, Amp, Copilot CLI, Cursor Agent (`cursor-agent`), Mistral Vibe
(`vibe`) and the login shell — with `installed` and `version` (from `<bin> --version`, cached a minute), the models and
efforts it maps, and whether it can resume. Start one in a term without writing its argv:

```
curl --unix-socket "$S" -X POST -d '{"cwd":"/home/me/src/app","agent":"gemini","model":"gemini-2.5-pro"}' http://unix/v1/terms
```

The flags per agent are in `contract/app-daemon.md` §2a. Claude, Codex, Gemini and Mistral Vibe
sessions are found in their own files (with the activity panel and the chat view); a term running another agent is a line of
its own while the term exists, and resuming it continues the agent's latest session in that
directory (Copilot resumes the exact session: the daemon gives it a session id at start). Only
Claude, Codex and Mistral Vibe report the end of a turn (hooks): a task on another agent moves to
`in_testing` when the agent exits or calls the MCP tool `set_task_status`.

Mistral Vibe 2 sessions are not read: a Vibe 2 started from Overseer runs in its terminal but gets
no line (no chat, resume or restore); run `vibe update` to get Vibe 3. Mistral Vibe 3 sessions are read from `$VIBE_HOME/logs/session/unified/<id>/` (`meta.json` for
the folder, title and model, the session store for the chat); a session is live while Vibe holds
its lease in `logs/session/active/`. Forks (`/branch`) are lines of their own, subagents are not.
Vibe has no launch flag for the model: a profile sets it with `VIBE_ACTIVE_MODEL`. A task started
on Vibe gets `--trust`, since its new worktree would otherwise stop at Vibe's trust prompt.

Detection uses the daemon's `PATH`. A daemon started over SSH by the app gets a minimal
environment, so at start it asks the login shell for the real one (`$SHELL -l -i -c`, 5 s
timeout), the way VS Code Remote-SSH does: agents in `~/.local/bin`, `~/.nvm/…/bin`,
`~/.bun/bin` and so on are found, and every term it starts gets the same environment. If the
shell does not answer, the usual user directories that exist are added to `PATH` instead.
`overseer doctor` and `GET /v1/env` show the `PATH` in use and where it came from. A running
daemon keeps its environment: the new one applies from its next start (the next server update),
it is never restarted for this since that would end its terms. **Rescan** in the agent picker
runs the detection again (`POST /v1/agents/rescan`), for instance after installing an agent.

**Auto-approve** in Settings → Agents chooses which supported CLIs Overseer starts with their
native automatic permission mode. It is on by default for Claude Code, Codex, Gemini CLI,
Antigravity, opencode, Copilot CLI, Cursor Agent and Mistral Vibe (`--auto-approve`). The settings are stored on the connected
daemon host and apply to new sessions, task agents and feedback agents; changing a setting does
not change sessions that are already running. Amp currently auto-approves by default unless its
own permission configuration enables a policy, so it has no Overseer toggle. The switch on the
**Auto approve agent requests** row turns it on or off for every agent at once; it shows a mixed
state when only some agents auto-approve.

**Skills and plugins** (Settings > Agents, or **Install skills and plugins** in the command
palette) installs a tool for one agent in one click, on the host the app is connected to. Each
install runs in a new terminal titled `<tool> · <agent>` that stays open on `Done. Press Enter to
close.`, so the output and any prompt are visible. The picker lists only installed agents the tool
supports.

| Tool | What it is | Claude Code | Gemini CLI | Codex, opencode, Copilot, Cursor, Amp, Antigravity, Mistral Vibe |
|---|---|---|---|---|
| RTK | a CLI proxy (binary + hook), not a skill | `rtk init -g --auto-patch` | `rtk init -g --gemini --auto-patch` | `rtk init -g` with `--codex`, `--opencode --auto-patch`, `--copilot`, `--agent cursor`, `--agent antigravity`, `--agent vibe`; no Amp |
| Caveman | a plugin with skills | `claude plugin marketplace add JuliusBrussee/caveman && claude plugin install caveman@caveman` | `gemini extensions install https://github.com/JuliusBrussee/caveman` | `npx -y skills add JuliusBrussee/caveman --skill '*' -g -a <agent> -y` |
| Ponytail | a plugin with skills | same with `DietrichGebert/ponytail`, `ponytail@ponytail` | same with `DietrichGebert/ponytail` | same with `DietrichGebert/ponytail` |

RTK first runs its installer (`install.sh` from `rtk-ai/rtk`, into `~/.local/bin`) when `rtk` is not
on `PATH`. The skills CLI names Copilot `github-copilot` and Antigravity `antigravity`.

### 14.1c File preview

Click a file path in a terminal, a chat message or a tool step to open it in the app. Images, video
and audio play inline (video never autoplays), PDFs render on desktop and iOS (not on Android),
Markdown is rendered and text-like files (txt, log, JSON, YAML, TOML, CSV, XML) are shown as text,
up to 1 MiB displayed. Relative paths resolve against the terminal's current directory; `~` is the
host's home. The daemon serves any regular file the host user can read, up to 50 MiB, through
`GET /v1/file`; directories, devices and larger files are refused with a message. **Copy path** puts
the full path on the clipboard, and **Download** saves a copy of the file into your Downloads
folder on the device running the app (a file of the same name is kept and the copy gets a ` (1)`
suffix).

On a phone, long-press a tab in the session strip for **Pin/Unpin** (agent tabs) and **Close**.

### 14.1b Agent profiles (models and reasoning)

A profile is a named preset that says, per agent CLI, which model and reasoning effort to use,
which model subagents use, and which background model to use. It is how you trade quality
against token spend without editing each CLI's config. Three profiles are built in and
read-only: **Economy**, **Balanced** and **Max**; duplicate one to make your own.

Open it from **Settings → Agents & models → Edit…**, or **Agent profile…** in a workspace's
context menu. Each installed agent has a card that only shows the fields that CLI accepts, with
how the value is passed (launch flag, launch environment or subagent file) and an estimated cost
relative to the CLI defaults (list prices; an estimate, not a bill).

Which profile applies, most specific first:

1. the profile picked for a task (Add Task → **Profile**) or for the next session (**+** menu →
   **Next session**, used once);
2. for a task, the profile mapped to its **Type** (see below);
3. the workspace's profile (the longest workspace path containing the session's directory or the
   task's project);
4. the default profile (**Settings → Agents & models**); out of the box this is **CLI defaults**,
   so nothing changes until you pick one.

**By task type.** The right model and effort depend more on the kind of work than on the repo,
so a task can carry a **Type**: **Bug fix**, **Feature**, **Engineering** (everyday work and
debugging), **Migration / refactor** or **Search / triage**. Under **Agents & models → By task
type**, map each type to a profile (or to **CLI defaults**); an unmapped type falls through to the
workspace and default profiles. Nothing is mapped out of the box. A sensible start for Claude,
after the model guide's advice to pick the model in code by task type: bug fixes and features on
`sonnet` or `opus` at `medium`, engineering on `opus` at `medium`, migrations on `fable` at
`medium`, triage on `haiku`. Make those profiles, then map them. A task imported from GitHub or
GitLab takes its type from its labels (`bug`, `enhancement`, `refactor`, `question`…), and one
from Jira from its issue type (Bug, Story, Task, Epic, Spike…); you can change it before
creating the task. A task whose mapped profile was deleted falls back to the workspace one.

A model or effort typed explicitly (task form, API) wins over the profile. The session tab shows
the profile name, and the activity panel shows it in full (`Balanced · sonnet · effort medium`).

How each CLI gets the values (agent files change only when you explicitly apply them):

| CLI | Model | Effort | Default subagent model / effort | Per named subagent | Background model |
|---|---|---|---|---|---|
| Claude Code | `--model` | `--effort` | `CLAUDE_CODE_SUBAGENT_MODEL` (env); "Override every subagent" adds `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` | `model:` / `effort:` in the subagent's `.md` frontmatter, via **Apply to agent files** | `ANTHROPIC_DEFAULT_HAIKU_MODEL` (env) |
| Codex | `-m` | `-c model_reasoning_effort=` | `-c agents.default_subagent_model=` / `-c agents.default_subagent_reasoning_effort=` | not offered (role files) | — |
| opencode | `-m` | — | — | `OPENCODE_CONFIG_CONTENT` (env, `agent.<name>.model`) | `small_model` in the same env |
| Copilot CLI | `--model` | `--effort` (max → `xhigh`) | — | — | — |
| Gemini CLI | `-m` | — (the CLI has no launch effort flag) | — | `model:` in `.gemini/agents/*.md`, via **Apply to agent files** | — |
| Antigravity | `--model` | `--effort` | — | `model:` tier in `.agents/agents` or `~/.gemini/config/agents`, via **Apply to agent files** | — |
| Cursor Agent | `--model` | — | — | — | — |
| Amp | — | `--mode` (max → `ultra`) | — | — | — |
| Mistral Vibe | `VIBE_ACTIVE_MODEL` (env) | — | — | — | — |

To change a named subagent, set it under **Named subagents**, save, then choose **Apply to agent
files…**. Gemini CLI definitions come from `~/.gemini/agents` and `<project>/.gemini/agents`;
Antigravity definitions come from `~/.gemini/config/agents` and `<project>/.agents/agents`;
Claude uses `~/.claude/agents` and `<project>/.claude/agents`. Project files take precedence over
user files. Overseer previews the frontmatter lines it would change, and writes only the files
you tick, keeping a `.bak-<stamp>` copy of each in `<state>/backups/`. Gemini CLI does not expose a native launch
effort flag, so profiles cannot set Gemini effort.

Profiles live in the daemon's state dir (`profiles.json`), so the desktop and phone apps share
them. API: `GET/PUT /v1/profiles`, `GET /v1/profiles/subagents?cwd=`, `POST
/v1/profiles/apply-files` (dry run unless `"dryRun": false` with the confirmed `files`), and
`profile` on `POST /v1/terms` and `POST /v1/tasks` (`none` for CLI defaults). Task types:
`types` (`{"fix": "<profile id>"}`) on `PUT /v1/profiles`, the list in `taskTypes` on `GET`,
and `taskType` on `POST`/`PATCH /v1/tasks` (`null` clears it; `"effort": ""` on `PATCH` clears
an explicit effort so the profile's applies).
Resuming a held session keeps its model and effort flags; environment-only values (Claude
subagent and background models, opencode config) are taken from the profile that applies to
the session's directory at resume time, without the task type: a resumed task session takes them
from its workspace or default profile, as it already does for an explicit task profile.

**Claude's built-in Explore subagent** declares `model: inherit`, so it runs on the session's
model whatever `CLAUDE_CODE_SUBAGENT_MODEL` says, and **Override every subagent**
(`CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1`) does not change that either: it keeps `inherit` (checked in
Claude Code 2.1.322). To move it to a cheaper model, add your own `Explore` agent in
`~/.claude/agents/Explore.md` with `model: haiku`. Claude merges agents by name, and a user or
project file replaces the built-in one whole, so that file must carry its own description,
tools and prompt; once it exists, **Named subagents** and **Apply to agent files** manage its
`model:` like any other.

### 14.2 Tasks (Kanban)

Tasks are local to the station, stored in the registry, numbered per project (`#1`, `#2`, … never
reused), optionally linked to an `AS-` ticket. Statuses, one board column each: `pending`
(Pending), `auto` (Auto), `in_progress` (In progress), `in_testing` (Review), `merged` (Merged)
and `completed` (Closed). Merged and Closed are both done; they start collapsed on the board.

- `POST /v1/tasks/<id>/start` creates the worktree `<worktreeDir>/<repo>-<hash>/task-<n>-<slug>` on
  a new branch `task/<n>-<slug>` from the project's base branch (14.9; a restart reuses it; with
  `{"inPlace": true}` the agent runs in the project checkout instead), starts the agent in a
  term — `claude [startArgs] --session-id <uuid> [--model M] --effort E`,
  `codex [-m M] -c model_reasoning_effort=E` (`max` → `xhigh`), or another agent of 14.1a — and
  types the title and description into it once its screen is quiet (Gemini, Antigravity,
  opencode and Copilot get them with their prompt flag instead). The prompt is never written to
  disk or logs.
- Tasks in `auto` start by themselves in column order (`rank`), while fewer than `autoConcurrency` tasks run
  (Settings, Defaults, **Auto concurrency**, 1 to 16, applied at once and kept by the daemon; `config.json` gives the starting value, default 2).
- A running task moves to `in_testing` when the agent finishes a turn (`Stop` hook) or exits.
- An `in_testing` task, or a `completed` one whose worktree is still on disk, moves to `merged` once its branch has landed in the project's default
  branch (`origin/HEAD`, else `main`/`master`; `origin/<default>` when it is ahead of the local
  one): merged with a merge commit, fast-forwarded, rebased (same patches) or squashed (one
  commit with the branch's whole diff). A branch deleted after the merge is checked through
  `origin/<branch>`. While such tasks exist, the daemon fetches the default branch at most every
  5 minutes per repository, in the background (skipped when `worktreeFetch` is off, backed off
  for 10 minutes when offline). A task moved back by hand after that is left alone.
- Agents move their own task through MCP `set_task_status`: `in_testing` when the work is ready
  for review, `merged` once they merged it themselves, and `in_progress` when work resumes on a
  task in Review, Merged or Closed (its agent must be running). Other statuses are refused.
- `POST /v1/tasks/<id>/review` with `approve` (→ `completed`), `reject` (→ `in_progress`, an
  optional `message` is typed into the agent) or `commit` (`add -A` and `commit` in the worktree
  with `message` or `task #<n>: <title>`, → `completed`).
- `GET /v1/projects` lists the repositories the app can create tasks in (from lines, `roots`,
  `repos`, existing tasks and added projects); `POST /v1/projects {"path": …}` adds one (it must
  be in a git repository).
- `rank` orders the cards of a column; the app updates it when a card is dragged.
- Dependencies: a task can depend on other tasks (`dependsOn`, a list of task ids). Set them in
  the "Depends on" picker of the new task dialog or the task details (search by number or title,
  same project unless "All projects" is ticked). A task stays blocked until every dependency is
  `merged` or `completed` (`in_testing` still blocks): the card shows a "Blocked" chip and "Waiting on #12,
  #14", the auto runner and the title bar RUN button skip it, and it starts in queue order once
  unblocked. Starting a blocked task by hand asks for confirmation. A dependency that is deleted is
  dropped from its dependents; self-dependencies and cycles are refused (400). The link chip on a
  card shows how many tasks it depends on; hovering a card (or tapping its link chip on the
  phone) highlights the whole chain. The "Chain" card action opens the new task dialog with the
  current task as dependency. With exactly one dependency, "Start from #N's branch" (on by
  default, `inheritBranch`) creates the worktree from that dependency's branch instead of the base
  branch.

```
curl --unix-socket "$S" -X POST -d '{"title":"Fix login","description":"…","project":"/home/me/src/app","status":"auto"}' http://unix/v1/tasks
curl --unix-socket "$S" 'http://unix/v1/tasks?status=in_progress,in_testing'
```

#### 14.2b Agent-created tasks, messages, matrix and organizer

- **Agent-created tasks**: an agent that finds an unrelated bug creates a task for it (MCP
  `create_task`, with optional `urgent` and `important`) instead of fixing it off-scope. These
  cards carry a "by #N" chip naming the task whose agent created them (`origin: "agent"`,
  `parentId`). They always land in Pending: an agent never queues work to `auto` or starts it.
- **Messages**: agents running at the same time talk to each other with `send_message`,
  `read_messages` and `find_overlaps` (which other running tasks changed the same files), and
  messages arrive in the receiving agent's input prefixed `[Overseer]`. You can write to the
  agent of a task too: the message button on a card (or **Details and messages** in the phone
  menu) opens the task detail with its message thread and a send box (Ctrl/Cmd+Enter sends).
  A message to a task whose agent is not running is kept and added to its prompt when it starts.
  When a task moves to Review and other running tasks of the project changed some of the
  same files, Overseer messages both agents by itself (`[Overseer] Overlap: task #A ... and
  task #B ... both change: ...`) so they agree who changes what. The same pair is not alerted
  again unless the set of shared files changes.
- **Agent messaging control**: agents only message tasks of their own project; a message to a
  task of another project is refused (403, "belongs to another project"), and `list_tasks`
  lists the agent's own project unless it names another. **Settings → Defaults → Agents
  message each other** (on by default, `GET`/`PUT /v1/tasks/agent-messaging {"enabled": bool}`)
  turns agent-to-agent messages off: a task's agent can then still message its own subtasks
  and its parent (quorum keeps working), your messages and Overseer's own messages still
  arrive, overlap alerts stop, and the organizer no longer messages agents.
- **Side tasks or autonomous agents**: **Settings → Defaults → Agents file what they find as
  tasks** (on by default, `GET`/`PUT /v1/tasks/agent-tasks {"enabled": bool}`) chooses what an
  agent does with a bug or improvement it finds outside its task while working, merging or
  reviewing. On, the Overseer MCP tells it to `create_task` it (urgent and important) instead of
  fixing it off-scope. Off, it tells the agent to fix it itself in its own task and branch, and
  `create_task` says to create a task only when you ask for one or a quorum orchestrator splits
  its work; `create_task` itself still works. Agents read the setting when they start, so it
  applies to agents started after the change.
- **Matrix**: the **Board | Matrix** switch in the Tasks header shows the open tasks (not
  merged or closed, not session tasks) on an Eisenhower matrix: Do first (urgent and important),
  Schedule (important), Delegate (urgent) and Eliminate (neither), plus an Unsorted strip for
  tasks without a classification. Drag a card to another quadrant to change its `urgent` and
  `important` flags; on a phone the quadrants stack and the long-press menu of a card sets the
  priority. The task detail has **Urgent** and **Important** switches too, and the board cards
  show their quadrant as a chip.
- **Organizer**: **Organize** starts an agent that classifies every task of the project filter
  (or of every project) by urgency and importance, orders the cards of each column accordingly
  and warns the agents of overlapping tasks. It runs in the project checkout, does not edit
  files, create tasks or start tasks, and its task stays off the board: an "Organizing…" label
  replaces the button while it runs. **Settings → Auto organize** runs it by itself, shortly
  after new tasks arrive (off by default).

#### 14.2c Quorum

Tick **Quorum** in the New task dialog to have several agent CLIs share one task and check each
other. It is only offered when the task's agent is Claude Code or Codex (the orchestrator, which
needs the Overseer MCP); with another agent the switch is disabled with a hint.

- **Workers** (one or more installed agents) get the subtasks the orchestrator cuts the task into.
  **Reviewers** (one or two installed agents) review each worker's result. Neither list can hold
  `shell` or the same agent twice. The defaults are every installed agent as a worker and two
  other agents as reviewers.
- The orchestrator creates the subtasks, which start by themselves on their own branches, with one
  worker agent each. The root card carries a **Quorum** chip (hover for the agents); a subtask's
  reviewer tasks carry a **Reviewer #N** chip and start by themselves when the worker reaches Review.
- Every reviewer and the orchestrator vote approve or reject, with a reason. A result is accepted on a
  strict majority of all voters: with one reviewer both must approve (2 of 2), with two reviewers 2 of 3.
  The worker card shows the tally (for example **2/3 accepted · Round 1**, **1/3 rejected**, or
  **1/3 voting** until the daemon decides); the task detail lists each vote with its reason.
- The daemon sends the outcome and every reason to the orchestrator, which merges accepted branches
  into its own branch. After a rejection a worker that still runs goes back to In progress; the
  orchestrator sends it the fixes, and when its next turn ends it is reviewed in a new round. A
  reviewer that ends without voting counts as a rejection.
- While it waits for subtasks, the orchestrator's card can sit in Review between its turns; its
  messages still reach it.
- Agents that have the Overseer MCP (Claude Code, Codex) vote with the `cast_vote` tool. Any other
  reviewer votes from its task terminal with the CLI: `overseer vote approve "<reason>"` or
  `overseer vote reject "<reason>"`.
- The quorum is set when the task is created and can only change while the task is Pending or Auto.
  A subtask cannot run its own quorum.

#### 14.2a Session tasks

Every agent session (Claude, Codex, Gemini, opencode, …) running in a git repository gets a task
of its own on the board, in its project, without doing anything: these cards carry a small
"session" glyph (`origin: "session"`). Only sessions alive or active within `sessionTasksDays`
(default 7) get one, so old history does not flood the board; closed sessions never do. A session
started from a task keeps that task and gets no second one.

- **Title**: the session's AI title, else the first prompt cut to about 60 characters, else
  "<agent> session in <branch>". The description is the first prompt. Branch, worktree, ticket,
  agent and model come from the session. Edit the title freely: it is kept.
- **Progress**: the bar follows the agent's todo list (Claude's TodoWrite): done / total, with
  the todo in progress as the note.
- **Status follows the session**:

  | Session | Column | Badge |
  |---|---|---|
  | working | In progress | |
  | waiting for you (permission, question, first prompt) | In progress | needs input |
  | turn finished, idle, or its process ended, after Overseer saw it working, with changes | Review (ready for review) | |
  | same, but it edited no file and has no worktree of its own (questions, searches, reports) | Closed | |
  | already finished or ended when Overseer first finds it (sessions from before Overseer) | Closed | |
  | held by you | stays in Review / Merged / Closed, else In progress | paused |
  | finished, idle or gone with its branch landed in the default branch (see 14.2) | Merged | |
  | closed (hung up) | Closed | |

- **Moving a card by hand** wins: the task stays where you put it while the session stays in the
  same state, and never goes back to an earlier column on its own unless the session starts
  working again. Session tasks only move between In progress, Review, Merged and Closed; they
  cannot be started or deleted (close the session instead).
- **Clicking a card** opens the session's tab (it attaches when the agent runs, otherwise the tab
  shows **Resume**; nothing starts on its own); a Review
  card opens the review. Dropping a card on Closed offers to hold its session, with
  the usual safety checks shown first.
- **Hide session tasks** in the board's toolbar (the glyph button on phones) hides them; the
  choice is remembered.

### 14.3 Activity panel

`GET /v1/lines/<id>/activity[?since=RFC3339]` summarises the line's transcript: title, goal (first
prompt), work log (edits, commands, reads, searches with running/done/failed), the agent's todo
list, edited files and the last assistant text. A line with no transcript (a stub agent, a
session being closed) answers 200 with empty lists and `"transcript": false`. `GET /v1/lines/<id>/diff` gives the worktree diff.
The SSE stream `GET /v1/events` adds `term.created|updated|exited`, `task.created|updated|deleted`
`line.activity` (the transcript grew; at most one per line every 500 ms), `git.changed` and
`worktree.created|updated|removed` (14.9, 14.10).

### 14.4 Server mode

The app drives a remote machine through the **system `ssh`** (ADR 0028), so a host that works with
`ssh <host>` in a terminal works in the app: `~/.ssh/config` and `/etc/ssh/ssh_config(.d)`,
`Match` blocks, `ProxyJump`, `ProxyCommand` (NetBird, Tailscale, Cloudflare…),
`CanonicalizeHostname`, `IdentityAgent` (1Password, Secretive), certificates and security keys.
One `ssh` session per host prints the server's platform, receives the binary for it when
`~/.overseer/server/<version>/overseer` is missing or another version (sha256 checked),
runs `… daemon --detach`, then becomes `overseer proxy`, a multiplexer that carries every API
request, the event stream, every terminal and every forwarded port over that one connection
(contract §8a). Nothing listens on a TCP port on the server.

- **Prompts**: an unknown host key, a key passphrase or a password is asked in the app (ssh calls
  the app as its `SSH_ASKPASS`). A new host key is written to `known_hosts` by ssh only after you
  click "Trust and connect"; `StrictHostKeyChecking accept-new` in your config skips the
  question; a changed key is refused.
- **Errors** show ssh's own message and how the host resolved, from `ssh -G`:
  `alice-workstation → workstation.example.net:22 as alice via ProxyJump bastion`.
- **Reconnect on launch**: the desktop app reconnects to the last host it was connected to (the
  phone to its last host) and shows the connecting state; the connect dialog opens only if that
  fails. **Disconnect** is remembered: after it, the next launch stays local (the phone shows the
  host list) until you connect again.
- **Several hosts at once** (desktop): **New Window** (File menu on macOS, palette, Cmd+Option+N
  or Ctrl+Alt+N) opens another window on the host picker; each window has its own connection
  (local or any SSH host), terminals, port forwards and browser, and its title shows the host.
  Closing a window closes its connection only (the daemon and its terminals keep running). Only
  the first window decides which host reopens at launch; the selected workspace is remembered per
  host. Two windows on the same host both show its notifications.
- **Several hosts in one window** (desktop, opt-in): Settings → Connection → **Windows and hosts**
  picks **One window per host** (the default, as above) or **One window, all hosts**. In the second
  mode, connecting to a host adds it to the window instead of replacing the current one; the
  palette's **Add a host to this window…** and the sidebar's **Add a host to this window** do it in
  either mode. **Ask each time I connect to a host** makes the connect dialog ask "Where should
  <host> open?" with **Add to this window** and **Open in a new window** (the mode's choice first,
  Enter picks it); **Remember for <host>** keeps the answer for that host, and **Forget the
  remembered choices** clears them. A window with several hosts shows one sidebar section per host:
  its name, address or connection state, session count and a **…** menu (**Show in the main area**,
  **Collapse**, **Move to its own window**, **Disconnect, sessions keep running**, and **Retry** after a
  failure). The main area (tabs, panel, tasks, review) shows one host at a time: clicking a session
  or a workspace of another section brings that host into the main area at once, without
  reconnecting, with its own tabs and layout; the palette's **Show <host> in the main area** and
  the host chip menu (the chip reads "N hosts") does the same. Every host stays connected and its
  section stays live; a session waiting for input on a background host still notifies, titled
  "<host>: …", and opening the notification switches to that host. With **Show the host on tabs**
  (on by default) tabs carry the host's name and colour. Disconnecting the host in the main area
  brings another host of the window into it. **Reconnect every host at launch** (on by default)
  reopens the hosts of the first window; disconnected hosts stay closed. Phones keep one host.
- Session rows in the sidebar do not repeat the host name: the host chip, and the host sections
  when a window shows several hosts, already say where the sessions run.
- **Keep the server running**: after the first connection to a host the app offers once to start
  the server at boot. **Keep running** (or Settings → Server → **Keep server running after
  reboot**) writes and enables `~/.config/systemd/user/overseer-daemon.service`
  (`ExecStart=~/.overseer/bin/overseer daemon`, `Restart=on-failure`, `OOMPolicy=continue`) on Linux, or the
  LaunchAgent `dev.overseer.daemon` on macOS (which starts at login). It does not start a second
  daemon; `daemon --detach` starts the unit when it is installed. The unit points at the shim, so
  hot upgrades keep it valid. A user unit starts at boot only when lingering is on: Settings shows
  a warning with the command (`loginctl enable-linger <user>`) and **Enable lingering**, which runs
  `loginctl enable-linger --no-ask-password` (no sudo; when the system asks for an administrator,
  run the command on the server yourself). Turning the switch off disables and removes the unit.
  API: `GET /v1/lifecycle`, `PATCH /v1/lifecycle {"autostart": true|false, "linger": true,
  "afterReboot": "agents"|"shells"|"nothing"}`.
- **Keep agents running with the lid closed** (Settings → Connection → Server, desktop app on a
  local server only, off by default): closing the laptop lid no longer puts the computer to sleep,
  so agents keep working with the network on. It drains the battery and the laptop can get hot:
  plug it in, and turn the option off before putting the laptop in a bag. On Linux the daemon holds
  a `systemd-inhibit --what=handle-lid-switch` lock while it runs (taken again when it restarts,
  gone when it stops). polkit allows it from your desktop session but may refuse it when the
  daemon runs as a boot service (**Keep server running after reboot**) with no active session;
  Settings then shows the reason. Check with `systemd-run --user --wait systemd-inhibit
  --what=handle-lid-switch --mode=block --who=t --why=t true`. A desktop that handles the lid
  itself (KDE's power settings, for example) ignores this lock: set its lid action there;
  `systemd-inhibit --list` shows who holds the lid switch. On macOS it runs `pmset -a disablesleep 1`
  after asking for an administrator password, and `0` when you turn it off. On Windows it sets the
  lid close action of the current power plan to **Do nothing** (plugged in and on battery) and puts
  the previous actions back when you turn it off. If the system refuses, the switch stays off and
  the reason is shown under it. API: `GET`/`PUT /v1/power/lid {"enabled": true|false}`.
- **After a server reboot or restart**: the daemon keeps its terminal list (folder, title, size,
  line, task, and for a running agent its current session and flags) in `overseer.db`, written
  only when it changes. When it starts after a machine reboot, a crash or a plain restart (anything
  but a hot upgrade, which hands its terminals over alive), it reopens the terminals with their old
  ids, so tabs keep working: shells start in their folder, and agents that were running are resumed
  with their own command (`claude --model opus --resume <session>`). The log says which it was
  (`machine rebooted` when the boot id changed, `/proc/sys/kernel/random/boot_id` or
  `kern.boottime` on macOS, `daemon restarted` otherwise). Settings → Server → **After a server
  reboot or restart**: **Restore agent sessions** (default), **Restore shells only**, **Nothing**.
  A hot upgrade or an app relaunch never restores or resumes anything.
- **Out of memory**: the agents run inside the daemon's service, so the service sets
  `OOMPolicy=continue`: when the kernel kills one process for memory (a build, a test run), only
  that process dies, not the daemon and every other session. The daemon rewrites an installed unit
  that predates a setting like this one when it starts, then runs `systemctl --user daemon-reload`.
- **Reconnect**: when the ssh process ends or the link stops answering keepalives (15 s pings;
  `ServerAliveInterval` 15 s unless your config sets one), the app shows "Reconnecting (attempt
  n)" and retries with backoff up to 30 s. Terminals re-attach and replay their scrollback; the
  programs never stopped, since the daemon owns them.
- **macOS "No route to host"** while `ssh` works in Terminal: macOS Local Network privacy. The app
  connects in-process and relays to `ssh` over loopback, so only Overseer needs the permission.
  If it still fails, the "Allow local network access" panel opens Privacy & Security > Local
  Network; switch Overseer on (off and on again after an update), quit and reopen, Try again.
- `OVERSEER_SSH` (ssh binary) and `OVERSEER_SSH_CONFIG` (passed as `-F`) override the
  defaults. Windows uses `%SystemRoot%\System32\OpenSSH\ssh.exe` (Settings → Apps → Optional
  features → OpenSSH Client).
- Mosh is not used: it carries one screen over UDP, not the API, events, several terminals and
  ports; daemon-owned sessions plus reconnect give the "survives sleep and roaming" part.

Static Linux binaries: `make dist` (`dist/overseer-<version>-x86_64-unknown-linux-musl`, and
`aarch64-…` when `aarch64-linux-musl-gcc` is installed or on an arm64 runner); macOS binaries are
built on the macOS CI runner. On Windows the daemon listens on loopback TCP with a token in
`<state>\daemon.json` instead of a socket (ADR 0023).

### 14.4a Ports

The daemon notices TCP ports opened in `LISTEN` state by any process started in one of its
terminals (Linux `/proc/net/tcp*`, macOS `lsof`), reports them in `GET /v1/ports` and as
`port.opened` / `port.closed` events (contract §7a):

```
curl --unix-socket "$S" http://unix/v1/ports
[{"port":5173,"addr":"127.0.0.1","pid":4242,"process":"node","termId":"…","project":"/home/me/src/web","since":"…"}]
```

Connected to a remote host, the app forwards each new port through the SSH tunnel to the same
local port (the next free one when taken), quietly: the title bar ports chip flashes once and the
forward lands in the notification center ("Port 5173 forwarded", click to open it), with no toast.
A dev server that restarts keeps one entry per port and host (forwarded, closed, forwarded again
within 10 minutes update the same entry and its ×count). The
Ports panel (title bar chip, or "Ports" in the palette) lists remote port → local address,
process, project and whether the forward is automatic or manual, and opens the URL in the
built-in or system browser, copies it, stops a forward or adds one by hand. Each row also has a
stop button that ends the process behind the port (SIGTERM), for the dev servers an agent left
running in the background:

```
curl --unix-socket "$S" -X POST -d '{"pid":4242,"confirm":true}' http://unix/v1/ports/5173/kill
```

Only the pid listed for that port can be stopped this way. Settings: auto-forward
on/off, ignore list (`22, 5432, 9000-9010`), bind to localhost only (default; off binds
`0.0.0.0` so other machines can reach the forward). Locally the same panel lists the ports, which
are reachable directly. Windows daemons do not detect ports yet.

### 14.4a Notification center

The bell in the title bar (Notifications in More on the phone) lists what happened in the
background on the connected host: an agent needs input, finished or failed, a task reached
testing, a workspace went idle, ports forwarded or closed, an app or server update, uploads and
errors. Each session, task, workspace and port has one entry that shows its latest event: a new
event replaces the old one, moves it to the top and makes it unread again. When an agent moves its
task to testing, the "finished" that follows within a minute is folded into that entry (no second
toast or system notification). Opening a session marks its entry read. Each entry shows its time,
its source and an unread dot; click it to act (open the line, open the port in the browser, open
the update). Mark all read and Clear sit at the top. The list is kept per host in the app (last
100 entries) and survives a reload; a replayed event after a reconnect never adds a second entry,
and lists saved by older versions are reduced to one entry per session. The bell counts unread
entries; port and upload entries arrive already read, so they never light it.

Toasts are kept for errors and for direct answers to what you just did (copied, saved, a failed
forward). In-app events only go to the center, unless Settings > Notifications > "Show toasts for
in-app notifications" is on. The gear in the center header opens the notification board below;
the bell-off button on a session's entry mutes that session.

### 14.4a.1 Notification board

Settings > Notifications decides, for each kind of event, where it goes. Every event goes
through the same rules, whatever produced it (agents, all idle, ports, app and server updates,
uploads, errors):

- **Events**: per kind, **System** (a native notification of the OS, plus the center), **In-app
  only** (the center, and a toast if toasts are on) or **Off** (nothing, not even the center).
  Agents and all-idle default to System; ports, updates, uploads and errors to In-app only.
- **System notifications**: the permission state, **Request permission** (phone and browser),
  **Open system settings** (macOS, Windows, Android) and **Send test**. Overseer never asks for
  the permission at launch: the phone asks the first time a notification would show, or when you
  press Request permission. Once refused, it only asks again from the board.
- **Play a sound**, **Show previews** (off: the system notification shows only its title, so the
  lock screen does not show session names or questions), **Only when Overseer is not focused**
  (while you use the app, events stay in the center), **Don't notify for the session I'm looking
  at**, and **Minimum gap per session** (0 to 120 s, 20 s by default) between two system
  notifications of the same session.
- **Quiet hours**: a start, an end (it can span midnight) and days; during them system
  notifications become in-app only.
- **Workspaces and groups**: turn one off to mute it, and mute a single session from its context
  menu or from its entry in the center ("Mute notifications" is also in the workspace and group
  menus). Muted events stay in the center, already read, with no alert. Mutes are kept per host.

One session keeps one notification: a newer one replaces the older on Android and iOS (they also
group a session's notifications together), and on Linux the older one is closed first. Clicking a
notification brings the window forward and opens its session.

Per platform:

- **macOS**: native notifications of the installed app; allow or silence them in System Settings >
  Notifications > Overseer. macOS cannot remove a shown notification, so a session's older
  notifications stay in Notification Center (the minimum gap keeps them few).
- **Windows**: toasts in the Action Center, named after the app, for the installed build (MSI or
  NSIS). Windows reports a click only while the toast is on screen; when you click it later in the
  Action Center, Overseer opens the notified session if the window comes forward within 30 s.
  Older toasts are not replaced.
- **Linux**: the desktop's notification server (GNOME, KDE, dunst, mako...). A click opens the
  session where the server supports actions; otherwise focusing the window within 30 s does.
- **Android**: Android 13 and later ask for the permission. Notifications use the channels Agents,
  Ports, Updates and uploads, Errors, and Silent (used when Play a sound is off); each channel can
  be tuned in Android's own settings, which Open system settings opens. A notification that
  replaces a session's earlier one updates silently, without a second sound or vibration.
- **iOS**: the system asks for the permission the first time; a session's notifications are
  grouped together.

Notifications come from the app: with the app closed, nothing is notified.

### 14.4b System monitor

The title bar shows `CPU 23% · RAM 61%` for the machine the app is connected to (the remote
server, or this machine when local). The chip turns amber at 80 % and red at 95 % (CPU or RAM,
whichever is worse). Click it, or "System monitor" in the palette, or "System" in the new-tab
menu (a workspace tab) or in More on the phone, to open the System panel:

- CPU (total, per-core bars, load average, Linux pressure), memory (used, cached, swap), network
  (in/out per interface) and disk I/O (read/write per physical disk) as 10-minute charts;
- filesystems with a usage bar per real mount;
- Overseer sessions: each terminal's process tree summed, e.g. "Claude in overseer:
  3 % CPU, 450 MB" (click to open it);
- the top 10 processes by CPU or by memory.

It costs nothing when nobody looks: the daemon samples only while a client is watching (the
panel polls every 2 s, the chip alone every 15 s, each poll keeps the sampler alive 5 s, so about 6 s of sampling
per 15 s while the window is focused), and the
app stops polling when its window is hidden or not focused. Measured on Linux with ~80 processes,
the sampler thread used 0.14 s of CPU per minute of sampling (0.2 % of one core, debug build).
macOS reports no per-core CPU and no disk I/O; Windows daemons report nothing yet. The history
only covers the time something was watching.

```
curl --unix-socket "$S" http://unix/v1/system | jq '.current | {cpu, mem}'
curl -N --unix-socket "$S" http://unix/v1/system/stream
```

### 14.4c Agent usage

The title bar shows a usage chip next to the other chips: one segment per agent active today.
When plan windows are known, it shows them, for example, "Claude 5h 83% · wk 47%", "Codex 5h
29% · wk 38%", or Codex Go's monthly `mo` allowance. Otherwise it shows the tokens of the day
("Claude 12.3M"). A segment turns amber at 80 % and red at 95 % of a
window, or when the agent's last request hit a usage limit; the tooltip lists reset times. The
chip is hidden when no agent ran today.

Clicking it (or "Agent usage" in the palette, or Usage in the phone's More tab) opens the Usage
panel: plan gauges per agent with a reset countdown, totals for today, 7 or 30 days (tokens with
input / output / cache, estimated cost marked ≈, requests, active time), tokens per day, and a
breakdown by project, session, model or agent; a session row opens that session. "Open in a tab"
keeps it as a workspace tab.

**Cost per session and task.** The side panel shows what the selected session cost, with its
main model and effort ("≈$1.23 · claude-opus-5-5 · effort high"), and a **Cost** section that
splits it per model (subagents included, each at its own model's price) into uncached input,
cache writes, cache reads and output. A switch to another model mid-session is listed as its own
line with what rebuilding the cache cost, since the new model writes the whole context again. The session tab
shows the same total, and each task card the total of the task's sessions. Tasks keep that number
after the transcripts are gone. At the bottom of the Usage panel, **Cost per task** groups the
tasks by profile (and agent), by ticket or lists each task, with the average cost per finished
task (in testing, merged or completed) for each profile: run the same kind of work under two
profiles and compare. The numbers are real token counts from the transcripts (Claude Code,
Codex, Gemini CLI) priced at API list prices dated in the hint; a model without a known price
keeps its tokens but adds nothing to the cost (marked "+"), and agents whose logs have no token
counts show no cost at all rather than an estimate. Subscriptions are not billed per token, so
read it as what the same work would cost on the API.

Where the numbers come from (contract §7c): Claude Code transcripts, Codex rollouts and Gemini
CLI sessions (when they record token counts) on the machine the app is connected to, read
incrementally by the daemon. Settings → Usage lists each of these providers: Claude Code with its two
plan-limit switches below, Codex (tokens, cost and its plan limits, always read from its session
logs) and Gemini CLI (tokens and cost only; it records no plan limits). Claude Code does not store its 5-hour and weekly percentages on
disk, but it passes them to its status line command (`rate_limits` in the status line's JSON input,
for Pro and Max plans). Settings → Usage → "Plan limits from the status line" (off until you turn it
on, per host) wraps your status line command in `~/.claude/settings.json` (a backup is written
next to it) with `overseer statusline-tap`, which saves those percentages for the daemon and
runs your original command with the same input, so what Claude Code shows does not change. Turning
it off restores the original command and forgets the recorded limits. From a shell on the host: `overseer statusline-tap install`
shows the change, `--apply` writes it, `uninstall --apply` reverts it. If the Overseer binary is
gone, the wrapped command falls back to running yours directly. The limits then update on each
Claude reply and show "from the status line". Independently, with Settings → Usage → "Fetch Claude plan limits" (off by default) the daemon asks
Anthropic for them with the local Claude Code login, the same call Claude Code makes: at most every
5 minutes, only while the app shows usage and a Claude session replied in the last hour. When the
status line tap is on, its limits are used while they are under 5 minutes old, and the call to
Anthropic fills in after that (for example for usage on claude.ai). Nothing
else leaves the machine, and the login is never refreshed or logged. When the login has expired
the panel says "Limits unavailable — open Claude Code to refresh login"; opening Claude Code
renews it. With the setting off, Claude shows tokens and cost only, plus "limit" when a request
was refused. Costs are estimates from public per-token API prices (`PRICES` in `src/usage.rs`); a
subscription is not billed that way. The app fetches when the daemon reports a change (the
chip at most every 10 s, an open panel at most every 2 s). While plan limits are shown and the
window is focused, it also refetches every 5 minutes and right after the nearest window resets,
so the percentages stay current when Claude runs outside Overseer or a window resets. It stops
while the window is hidden or unfocused and catches up when you come back.

### 14.5 MCP server for agents

`overseer mcp` is a stdio MCP server (tools only) that lets an agent read and update tasks
through the daemon: `list_tasks`, `get_task`, `create_task` (always `pending`; optional
`dependsOn` ids or task numbers), `set_task_dependencies {taskId, dependsOn}` (refuses cycles),
`update_task_progress {done,total,note}` and `set_task_status` (only its own task, only to
`in_testing`, `merged` or `in_progress`; `in_progress` only while its agent runs). The daemon sets `OVERSEER_TASK_ID` in agents it starts for a task. Codex tasks get
the server on their command line (`-c mcp_servers.overseer.*`, with the task id and state dir in
its env, since Codex clears the environment of MCP servers) and Claude Code tasks get it through
`--mcp-config` (its tools pre-approved with `--allowedTools mcp__overseer`), so those agents can
message other agents without any setup. Every task terminal also has the directory of the
`overseer` shim (`~/.overseer/bin`) on its `PATH`, so the `overseer` CLI works in it. Overseer never edits agent configuration; to enable it yourself
elsewhere:

- Claude Code (user scope): `claude mcp add --scope user overseer -- /path/to/overseer mcp`,
  or in a project's `.mcp.json`:
  `{"mcpServers":{"overseer":{"command":"/path/to/overseer","args":["mcp"]}}}`.
- Codex, in `~/.codex/config.toml`:

  ```toml
  [mcp_servers.overseer]
  command = "/path/to/overseer"
  args = ["mcp"]
  ```

Set `OVERSEER_STATE_DIR` in the server's `env` when the daemon does not use the default state
dir.

The same server has two review tools (§14.8): `list_review_comments {status?, worktree?}` (default
`sent`: the threads waiting for the agent, on its task's worktree or the current repository) and
`resolve_review_comment {id, note}` (marks a thread `addressed` with the note as the agent's reply).

Organize tools (14.7b), read-only: `list_sessions {project?, sinceDays?, limit?}` (up to 300),
`get_session_brief {id}` (goal, todos, edited files, last message), `list_projects`,
`list_worktrees {project}` (with merged and dirty), `list_layout` (workspaces, folders, groups).
`propose_organization {plan}` records a plan for your review; only the configurator agent may call
it, and that agent may not create tasks or change their status or priority.

### 14.6 Workspaces, groups and broadcast (daemon API)

Workspaces (`/v1/workspaces`), folders (`/v1/folders`) and groups (`/v1/groups`) are stored in the
registry, so every client of the station sees the same sidebar (ADR 0029). Each project gets a
workspace automatically; any other directory can be added. Deleting a project's workspace keeps it
deleted until you add it again. `POST /v1/broadcast` types one text into several agents — given
lines or terms, every agent of a workspace, or a group — and only into idle ones unless asked:

```
curl --unix-socket "$S" -X POST -d '{"text":"rebase on main","workspaceId":"<id>","queue":true}' http://unix/v1/broadcast
```

Each target answers `sent`, `skipped` (busy or waiting for a permission answer), `queued`
(typed when the agent becomes idle, within an hour, unless the daemon restarts) or `refused`
(not running, no terminal). `actions.log` records the outcome and the text length, never the text.

A workspace can be **nested** in a top-level directory workspace with `parentId`: a sub-folder or a
submodule (`{"path":"/home/u/endurance/20_processus/onboarding","parentId":"<id>","cwd":"/home/u/endurance"}`),
or a subject (`{"kind":"subject","name":"Archi V3","parentId":"<id>"}`, whose `cwd` defaults to the
parent's path). The path must be inside the parent's, the parent must be a top-level `dir` workspace
(one level only), and a nested workspace has no folder and cannot be pinned (each refused with a 400).
`cwd` on a nested folder is where its sessions start (`null` starts them in its own folder) and can be
patched; on a top-level directory it is refused. Creating a nested folder on a path that already has a
top-level workspace adopts that workspace, keeping its id, tabs and sessions. Every checked-out
submodule of a top-level workspace (the `path =` entries of its `.gitmodules` whose folder holds a
`.git`) becomes a nested workspace on its own, when the workspace is created, at start, every 10 minutes
and on **Rescan folders**; one you delete stays deleted. Deleting a workspace deletes its nested ones
(the answer lists them under `children`). A session goes to the innermost workspace whose path holds
it, so a session in a sub-folder or a submodule joins the nested workspace, and a worktree of a
submodule kept outside the repository joins the submodule's when that submodule is checked out in
one clone only; with several clones of the same repository (as for any repository cloned several
times), such a worktree belongs to none of them unless it was started from the subject or moved
there by hand.

A **subject** is a workspace without a directory of its own, for work organised by topic rather than
by folder. Create it with `POST /v1/workspaces` and `{"kind":"subject","name":"Acme deal","cwd":"~/code/app"}`
(`folderId` and `pinned` optional); `cwd` is its default working directory, `~` accepted, and several
subjects may share one. The list returns `kind` (`dir` or `subject`), `cwd`, and a `null` path for subjects;
`PATCH` accepts `name` and `cwd`. Discovery never creates subjects. A session joins one through the usual
`PUT /v1/lines/<id>/workspace` and `PUT /v1/terms/<id>/workspace`, which beat its directory, so a session
inside a folder workspace assigned to a subject shows only in the subject. `POST /v1/terms` takes an
optional `workspaceId`: the term is assigned at creation, starts in the workspace's `cwd` when none is
given, and the agent line created from it inherits the assignment. Deleting a subject unassigns its
sessions and kills nothing.

### 14.7 Workspaces in the app

A workspace is one project on one host: a repository or any folder, with every agent, shell,
browser and review tab that belongs to it. The sidebar shows **Workspaces** (default) or
**By status** (the session list grouped by Needs input, Needs testing, Working, Pushed, Idle,
Stopped: no live agent process, resumable); the toggle is at the top of the sidebar.

- **Sticky notes**: hover an agent session in the sidebar (on a phone or a touch screen the icon is always there)
  and click the note icon to open its sticky notes. Write in the box and press **Add note** (or
  Ctrl/Cmd+Enter); click a note to change it (saved when you leave it, or with Ctrl/Cmd+Enter;
  clearing its text deletes it) and the bin deletes it. Escape or a click outside closes them,
  saving what you changed and adding what is left in the box. A
  session with notes keeps its note icon lit, with the count in its tooltip. Notes are yours only:
  they are never sent to the agent, live in the daemon's registry (they survive restarts and show on
  every client of the station), and stay with the session until it is forgotten. Up to 50 notes of
  4000 characters each per session. Plain shells have no notes.

- **Spaces**: pinned workspaces on top, then your folders (one level of sub-folders) and the
  workspaces outside any folder. Each workspace is one block with its sessions under it: click its
  name to open it (its tab strip and `+` show), the chevron only folds it. Its sessions are listed in
  the order of its tabs (split panes left or top first), then the ones without a tab by urgency and
  age; drag a session above or below another of the same workspace to move its tab there too. Running
  plain shells are listed after its sessions, in tab order as well, muted with a terminal icon, and are
  not counted in its pills. A group section at the
  bottom lists your groups; click one to open it. Every project the daemon knows gets a workspace: the
  repositories its agents ran in, and the git repositories it finds under `discoverRoots`
  (3.1; by default `~/Documents/Github`, `~/src`, `~/code`, `~/projects`, `~/dev`), scanned at
  start, every 10 minutes and with **Rescan folders** in the `+` menu or the palette. `+` adds one
  from any folder (`~` is fine), the folder button adds a folder, the archive button shows
  archived workspaces. When a workspace's folder is renamed or deleted, a workspace created
  automatically is hidden (it comes back with the folder; the new name is found by the next
  scan) and one you added stays, marked "Folder missing". Drag a workspace header onto another to reorder, onto a folder to move it in, onto
  **Pinned** to pin it; drag folders the same way. Right-click a workspace, folder, group, the
  **Other** row or a session for rename, pin, move, broadcast,
  hold all, resume all, "notify me when all agents are idle", archive and delete. On a touch
  screen (iPad, tablet) press and hold the same rows to open the same menus; a short tap still
  selects. Sessions picked with Cmd/Ctrl+click are tinted with the accent colour. A folder
  remembers whether it is collapsed.
- **Block**: the most urgent state of its sessions (Needs input, then Running, then Needs testing,
  then Idle; shells do not count) with how many agents are in it, the last status line ("Claude
  finished", "Asking: …"), the host and its connection state, and the folder path.
- **Subjects**: a workspace can hold subjects, listed inside its block after its own sessions:
  sub-folders (folder icon), submodules (git icon, added on their own for every checked-out
  submodule) and subjects without a folder (# icon). A session running inside a subject's folder
  joins it; drag any session onto a subject, or use **Move to workspace…** in its context menu, to
  put it there by hand. **+ New subject** at the bottom of the block (also in the workspace's
  context menu and the palette, **New subject in the current workspace**) asks for a path inside the
  workspace (Browse starts there), a name, and where its sessions start: the workspace root, so
  agents read the instructions at the root, or the subject's own folder. The `+` of a subject
  (shown on hover, always on a touch screen) opens the new-session menu and starts the session in
  the subject; a subject's context menu toggles **Start sessions at the workspace root**. A subject
  has no folder or pin of its own; deleting a workspace deletes its subjects, and a deleted
  submodule subject does not come back. The attention banner shows `workspace › subject`.
- **Membership**: a line belongs to the workspace of its task's project, else to the workspace
  whose folder contains its worktree, unless the line's repository has its own workspace that
  does not contain that folder (so a workspace on your home folder does not take every
  worktree). Shells follow their working directory. Sessions that match nothing are under
  **Other sessions**.
- **Tabs**: selecting a workspace shows its own tab strip (agents, shells, browser, review), with
  the agent glyph and a status dot per tab. Tabs keep a compact width with long titles cut, and
  the selected tab shows its full title; when they do not fit, the strip scrolls with the mouse
  wheel or trackpad, and hovering a tab shows its full title. `+` starts any installed agent or a shell in the
  workspace folder. Views (browser, **Review changes**, Git graph, worktrees, System, Usage, PR reviews)
  open from the **Views** section of the command palette (Cmd/Ctrl+P). The palette and the phone
  **More** sheet list only the commands that can run now: a command that needs a connection (or a
  project, or no recording in progress) is hidden, not greyed out. A destructive button (end a terminal, delete a task, restart the server
  from the phone **More** sheet) arms on the first press and acts on a second press within 3 seconds. Drag tabs
  to reorder (the sidebar follows), middle-click or `×` to close (this ends the agent or shell; right-click > **Hide tab
  (keep running)** only hides it), split right or down. The
  order is saved in the daemon, so it comes back on another machine.
  Keyboard: see 14.7a (close with Cmd+W or Ctrl+W, reopen with Cmd/Ctrl+Shift+T, new tab with
  Cmd/Ctrl+T).
- **Web pages of a session**: a link opened from a session (Cmd/Ctrl+click on a URL in its
  terminal, a link in its chat, **Open in browser** on a port its process listens on) opens
  beside that session, inside its tab, instead of in a separate tab. The pages of a session have
  their own strip (close with `×` or a middle-click), back, forward, reload, address bar, open in
  the system browser and **Move to its own tab**; drag the edge to resize. Switching to another
  session hides its pages and shows that session's; the globe button in the tab strip (with the
  page count) shows or hides them. Pages are kept per host across restarts (8 per session, 32
  sessions). The desktop app keeps the 4 most recently shown pages loaded and hidden pages
  paused; older ones reload from their last address when shown again. On the phone a page opens
  full screen over the session, and the globe chip switches back to the session. Links opened
  elsewhere (notifications, PR buttons, the Browser view) keep opening in the Browser view.
- **Where links open**: Settings > Links > **Open links** picks the target for chat links,
  terminal links, ports, PR buttons and notifications: **In Overseer** (the session's web pane
  or the Browser view, the default), **Default browser** (the OS one) or one of the installed
  browsers (found once per launch, when first needed: known browsers in `/Applications` and
  `~/Applications` on macOS, `StartMenuInternet` in the registry on Windows, `.desktop` files
  handling `http` in the XDG data dirs, Flatpak and Snap on Linux). **Ask every time** shows the
  picker for each link. Cmd/Ctrl+click opens a link in the default browser and Shift+click shows
  the picker (in a terminal, where Cmd/Ctrl+click opens links, use Cmd/Ctrl+Shift+click).
  Right-click a link (long-press on Android) for Open in Overseer, Open in the default
  browser, Open in each installed browser, and Copy link. The phone offers Overseer or the
  system browser only. Buttons that say "system browser" always use the OS default.
- **Groups**: Ctrl/Cmd+click sessions (in By status or in the tab strip) to select several, then
  **Add to group** in the bar at the bottom of the sidebar, the context menu or the palette. A
  line can be in several groups ("release 5.8", "infra"); a group shows its members as tabs.
- **Broadcast**: from a workspace, a group, a selection (context menu, the megaphone in the tab
  strip, or the palette). Write the message once; the agents at their prompt are ticked, busy
  ones and those waiting for a permission answer are left out unless **Queue for busy agents**
  is ticked (they get it when their turn ends; the dialog shows "Delivered" when it happens).
  The daemon types it like a task prompt (bracketed paste when the program asks for it) and logs
  the outcome, not the text.
- **Hold all / Resume all**: runs the hold checks of every agent first (dry run) and lists which
  pass; only those are held when you confirm. Resume all reopens each held agent in a terminal.
- **Notify when all idle**: per workspace or group, an OS notification when every agent in it is
  back at its prompt.
- **Tasks**: the board opens filtered on the current workspace's project.

### 14.7b Organize my sessions

**Organize my sessions…** (palette, Settings > Help, phone **More**) proposes a tidy layout and
changes nothing until you approve it (ADR 0038). When more than 15 recent sessions belong to no
workspace, the sidebar suggests it once; dismiss the hint to hide it. Pick:

- **Quick (by project, offline)**: the daemon builds the plan from what it already knows, in
  about a second and without calling git: a workspace per repository whose sessions have none, a
  folder per owner (GitHub organisation or parent folder) when you work for several, a group per
  ticket key (`PROJ-123`) shared by two or more sessions, **hold** for agents idle more than 30
  days, **close** for sessions whose folder is gone, **completed** for tasks whose branch was
  merged (when the merge state is already known), **archive** for workspaces idle 30 days with
  no open task.
- **With <your default agent>**: a configurator agent reads your sessions, worktrees and layout
  through the MCP tools (14.5) and writes one proposal with a reason per item. It never edits
  files, never runs git write commands and cannot create or move tasks. Claude gets the
  Overseer MCP server on its command line automatically; other agents need `overseer mcp`
  registered (14.5), and if no plan arrives within 5 minutes the app offers the quick path.

**Plan review** lists the actions by section (Layout, Clean up, Tasks) with their reasons, all
ticked except **close**. **Apply** runs only the ticked items, in order, and shows a result per
item: done, skipped (already the case) or failed (for example an agent became busy). Applying
again is safe. Holding is the reversible "archive" of a session: **Resume** brings it back.
Sessions on another station are held or closed through it, and fail while it is offline.

**Pick up where you left off** shows up to 40 recent sessions, first those whose agent edited
files, then those with open todos: title, a one-line summary, branch and badges. **Resume**
reopens the agent in a terminal, **Open in workspace** jumps to it, **Dismiss** hides the card.

### 14.7a Keyboard shortcuts

macOS uses Cmd, Linux and Windows use Ctrl. The palette (Cmd/Ctrl+P) shows the shortcut next to
each action, and **Keyboard shortcuts** (palette, or `?` in Settings > Keyboard, or
Cmd+/ / Ctrl+Shift+/) lists them all for the current platform. Shortcuts are caught before the
terminal, but on Linux and Windows the Ctrl+letter keys a shell needs (Ctrl+D EOF, Ctrl+F, Ctrl+B,
Ctrl+J, Ctrl+/) stay with the terminal; use the Ctrl+Shift variant there. The phone app has no
shortcuts.

| Action | macOS | Linux / Windows |
| --- | --- | --- |
| New tab (`+` menu, default agent selected; Enter starts it) | Cmd+T | Ctrl+T |
| Close tab and end its session (the last tab leaves the empty workspace) | Cmd+W | Ctrl+W (setting) |
| Reopen the last closed tab (last 10 per workspace) | Cmd+Shift+T | Ctrl+Shift+T |
| Next / previous tab | Ctrl+Tab / Ctrl+Shift+Tab, Cmd+Shift+] / [ | Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+Shift+] / [ |
| Tab 1 to 8, last tab | Cmd+1…8, Cmd+9 | Ctrl+1…8, Ctrl+9 |
| Split right | Cmd+D | Ctrl+Shift+E |
| Split down | Cmd+Shift+D | Ctrl+Shift+D |
| Focus the pane left / right / up / down | Cmd+Option+arrows | Ctrl+Alt+arrows |
| Find in terminal (Enter: older match, Shift+Enter: newer, Esc closes) | Cmd+F | Ctrl+Shift+F (Ctrl+F outside a terminal) |
| Copy the selection / paste | Cmd+C / Cmd+V | Ctrl+Shift+C / Ctrl+Shift+V, or Ctrl+Insert / Shift+Insert |
| Terminal font larger / smaller / default | Cmd+= / Cmd+- / Cmd+0 | Ctrl+= / Ctrl+- / Ctrl+0 |
| Command palette | Cmd+P | Ctrl+P (Ctrl+Shift+P in a terminal) |
| Settings | Cmd+, | Ctrl+, |
| Keyboard shortcuts | Cmd+/ | Ctrl+Shift+/ (Ctrl+/ outside a terminal) |
| Show or hide the sidebar | Cmd+B | Ctrl+Shift+B (Ctrl+B outside a terminal) |
| Show or hide the activity panel | Cmd+J | Ctrl+Shift+J (Ctrl+J outside a terminal) |
| Kanban board | Cmd+Shift+K | Ctrl+Shift+K |
| New workspace | Cmd+Shift+N | Ctrl+Shift+N |
| New window for another host | Cmd+Option+N | Ctrl+Alt+N |
| Close the window | Cmd+Shift+W | (window manager) |

- **Cmd+W never closes the window** on macOS: the File menu's Close Tab takes it, Close Window is
  Cmd+Shift+W.
- **Ctrl+W on Linux and Windows** is also the shell's "delete the previous word". Settings >
  Keyboard > **Ctrl+W closes the tab** (on by default): turn it off to send Ctrl+W to the terminal.
- **New tab opens a shell** (Settings > Keyboard, off by default): Cmd/Ctrl+T starts a shell in the
  workspace folder instead of opening the `+` menu.
- **Reopen** brings back agents, shells, browser, review, graph and other tabs; a running agent
  re-attaches, an agent whose
  session ended reopens on its Resume button, and an ended shell starts again in the same folder
  (a new shell: its scrollback is gone). Closing a pane records all of its tabs.

### 14.8 Live review

A GitHub-style review of what an agent changed, while it works (ADR 0030, contract §11).

- **Open it** from the activity panel (Changes → Open review), from a Kanban card in In testing
  (Review: the review replaces the plain diff and keeps Approve, Reject and Commit), or from the
  palette (Review changes). It opens as a tab of the workspace; tabs show a badge with the number
  of unresolved threads of their worktree.
- **Beside the agent**: the review button in the tab bar of an agent (chat or terminal) opens a
  menu: **Review beside** splits the pane and shows the review of that agent's worktree on the
  right (an existing split is reused), **Review in a tab** opens it as its own tab, **Close
  review** closes it. The side panel's Open review and the palette (Review beside the agent) do
  the same. The side review follows the agent you select; pin it in its header to keep it on one
  worktree. On the phone the review stays a separate view.
- **What it shows**: every file changed on the branch since it left its base, plus staged,
  unstaged and untracked files. The header reads **Comparing `<branch>` ← `<base>`**; the base
  is, in order: the branch it was created from (Overseer records it for the worktrees it
  creates; otherwise git's branch reflog), the task's project branch, the upstream on the default
  branch, the local branch it was first checked out from when that is closer than the default,
  then the default branch (`origin/HEAD`, `main` or `master`). Click the base to pick another one
  (a searchable list of local and remote branches, most recent first, plus Default); the choice
  is remembered per worktree.
- **Submodules**: the review covers one repository. An agent working inside a submodule, or in a
  worktree of a submodule, reviews that submodule only (the header shows which superproject it
  belongs to). Reviewing a superproject lists each submodule whose commit changed as one
  collapsed entry (`old → new` commits) with **Review this submodule**, which opens its own
  review from the old commit; files inside submodules are never listed, and a submodule with
  only uncommitted changes is skipped, so large submodules cost nothing. File tree, unified or split diff, syntax highlighting, unchanged lines folded
  (click to expand), a Viewed box per file (unticked again when the file changes). J and K walk
  the files after a click anywhere in the review pane, its toolbar included, except on a text field. The view
  follows the agent's edits live: the daemon watches the worktree (file system events, polling as a fallback) and sends
  `review.changed`.
- **Triage**: every changed line is sorted into moved code, formatting, reordered imports,
  renamed identifiers, generated or lock files, tests, comments and logic. The header sums it up
  ("62% moves/format"; Legend explains the colours), changed lines carry a tint of their kind in
  the gutter, and hunks with nothing but low-risk changes fold into one row ("Moved from
  src/x.rs", "Formatting only"): click Show to read them. Folding is separate from Viewed. The
  file tree shows a risk score per file (what is left to read, weighted up for changed
  signatures and down for tests) and can sort by it. Copied files show as `C` when the repository
  sets `git config diff.renames copies`.
- **Map**: List | Map in the header (the choice is remembered) draws every worktree of the review
  on one map: the review's worktree, the worktrees the session's agent also edited, and their
  checked-out submodules, each a region, files sized by bytes, unchanged folders folded into
  grey blocks. Changed files take the colour of the indicator picked above the map: **Real
  logic** (changed lines that are not moves, formatting, renames, imports or generated code),
  **Blast radius** (changed files that use this one, double for another repository), **Contract
  change** (public signatures and shared types), **Agent struggle** (times the agent rewrote the
  file, and test runs that failed after it did), **Untested** (logic no changed test reaches),
  **Not yet seen** (lines not marked viewed, open comments) and **Priority**, their weighted
  mean (real logic 1.2, agent struggle 1.1, blast radius and contract 1, untested 0.9, not yet
  seen 0.6). Hovering a file draws what it uses (blue) and what uses it (orange); a link to
  another repository is dashed violet, and one found only through a shared name is marked so.
  **Every link of the change** shows them all faintly. The side lists the files in that order,
  and the picked file's six scores with plain reasons ("used by 3 changed files, 1 in another
  repository"); **Open the diff** (or a double click) jumps to it in the list. In the list, a heat
  band along the files shows the same colours top to bottom, and **Next hottest file** (or J and
  K) walks the files hottest first across every worktree. On a phone the ranking comes first and
  the map below it. The daemon serves it at `GET /v1/review/map?worktree=…` (or `line=`, `task=`),
  plus `with=` for each extra worktree the review already shows.
- **Comment**: click a line number (shift-click or drag for a range), write, Add review comment
  (Ctrl/Cmd+Enter). Suggest inserts a ```` ```suggestion ```` block prefilled with the lines; the
  thread then offers Apply suggestion, which writes it into the worktree if the lines did not
  change since. Comments stay pending (drafts) until you submit.
- **Submit**: Comment, Request changes or Approve sends every pending comment as **one**
  instruction to the agent of that worktree: file and line range, the quoted code, your comment,
  suggestions as a unified diff, the thread ids, then your overall comment. It is typed into the
  agent's terminal (daemon term or tmux pane) only while the agent is idle at its prompt — never
  while it works or waits for a permission answer; otherwise it is queued and typed as soon as the
  agent is idle (the submit bar shows Queued, then Delivered). Request changes also moves an In
  testing task back to In progress.
- **Threads**: Pending → Sent → Addressed (the agent answered through MCP) → Resolved (you).
  Resolve or reopen at any time, reply, edit or delete drafts. Comments follow the code: a thread
  moves with its lines; if the agent edits the commented lines it is marked "Changed since
  comment" so you can check the edit; if the lines are gone it is "Outdated" and shows the code it
  was written on.
- Every agent gets the typed instruction; agents with the Overseer MCP server can also list the
  threads and answer each one.

### 14.8a PR reviews (GitHub)

The **PR reviews** tab lists the open pull requests where your GitHub account is a requested
reviewer, directly or through a team. Open it from the command palette ("Open
PR reviews") or, on the phone, More → PR reviews.

- It uses the GitHub CLI already signed in on the host running Overseer (`gh`), so nothing
  else to configure. If gh is missing, the tab says so with the install link; if it is not signed
  in, it shows the command to run on the host: `gh auth login`.
- PRs are grouped by repository, newest first, with the author, age, size (`+added −deleted`),
  a Draft badge and the review state (Review required, You reviewed, Changes requested,
  Approved). "Local clone" means the repository is one of your Overseer projects.
- The list loads when the tab is shown and when the window regains focus; the daemon keeps it for
  60 s. **Refresh** fetches it again right away. Nothing runs in the background.
- **Review with agent** checks the PR out in a fresh worktree (`review/pr-<number>`) of your local
  clone, or of a partial clone under `~/.overseer/reviews/<owner>/<repo>` when you have none,
  then starts your default agent there and opens it next to a live review of that worktree. A
  first clone of a large repository can take a while: the tab says it is preparing and the agent
  opens by itself when it is ready. Running it again on the same PR reuses the worktree and
  fast-forwards it to the latest head.
- The agent reads the description and the diff, reviews correctness, security and tests, and
  writes its findings in the chat. It is told never to post, approve, request changes, commit or
  push on its own: it waits for you to say what to post, then uses `gh pr review` for the summary
  and `gh api` for inline comments.
- **Open on GitHub** (or a click on the title) opens the PR in your browser.

### 14.9 Worktrees

Every agent session or task started from the app runs in its own git worktree (tick "Run in
current checkout" in the workspace `+` menu, or pick "Start in the current checkout" on a task, to
opt out). One place, outside your repositories:

```
~/.overseer/worktrees/<repo-name>-<hash>/<branch with / as ->
  sb-claude-1029-0930        branch sb/claude/1029-0930   (an agent session)
  task-7-fix-login           branch task/7-fix-login      (task #7)
```

- Branches: `sb/<agent>/<slug>` for sessions (slug = `MMDD-HHMM` unless given), `task/<n>-<slug>`
  for tasks, `sb/<slug>` for a worktree made from the graph. A taken name gets `-2`, `-3`, …
- Base: the default branch (`origin/HEAD`, else `main`, `master`, else the checkout's branch),
  fetched first (`worktreeFetch`; after a failed fetch, offline for instance, not again for 10 minutes). The worktree starts from the local branch when it contains
  `origin/<default>`, else from `origin/<default>`. New branches have no upstream; their start
  point is kept in `git config branch.<branch>.overseer-base` so the review compares against it.
- Lifecycle, as the Worktrees panel shows it: `in use` (an agent line, a running terminal or an
  in-progress task is in it) → `idle` → `stale` (no uncommitted change, and merged into the base,
  its PR merged, or no activity for `worktreeStaleDays`) → cleaned up. `missing` when the
  directory was deleted by hand.
- **Clean up** always shows a dry run first: exactly the directory and the branch it removes and
  what it keeps. It refuses, with the reason, the main checkout, a worktree created outside
  Overseer (listed as "orphan", never touched), a locked one, one in use, one with
  uncommitted changes (untracked files included; ignored build output does not count) and one
  whose branch has commits that are neither pushed nor in the base. The branch is deleted only
  when every commit of it is in the base (or its PR is merged and the tip pushed); otherwise it
  is kept. Nothing is ever forced.
- **Clean up merged (n)** in the Worktrees toolbar picks the worktrees nobody needs any more: merged
  into the base (or missing on disk), with no agent session, no terminal and no uncommitted change.
  The dialog lists each one with a checkbox, all ticked; untick the ones to keep, then confirm.
  **Clean up stale** and the row's **Clean up** use the same checkboxes.

```
P=$(python3 -c 'import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=""))' ~/src/app)
curl --unix-socket "$S" "http://unix/v1/projects/$P/worktrees"
curl --unix-socket "$S" -X POST -d '{"project":"/home/me/src/app","agent":"claude","slug":"fix login"}' http://unix/v1/worktrees
W=$(python3 -c 'import urllib.parse,sys;print(urllib.parse.quote(sys.argv[1],safe=""))' ~/.overseer/worktrees/app-1a2b3c4d/sb-claude-fix-login)
curl --unix-socket "$S" -X POST -d '{}' "http://unix/v1/worktrees/$W/cleanup"                 # dry run
curl --unix-socket "$S" -X POST -d '{"dryRun":false}' "http://unix/v1/worktrees/$W/cleanup"
```

`POST /v1/worktrees/<id>/merge-base` merges the base into a worktree and
`POST /v1/worktrees/<id>/cherry-pick {"sha"}` picks a commit into it; both refuse uncommitted
changes and abort on conflicts (the error names the files).

### 14.10 Live review and the Git tab

The git button in a session's tab bar (or the palette's "Open or close the live review") turns the
window into a live review of that session: the agent stays where it is, its review opens beside it,
the sidebar becomes a rail of the workspace's sessions (click one to switch; the review follows),
the side panel steps aside, and a bar at the bottom runs git. Press the button again to go back.

The bar acts on one repository only: the worktree named on its left. Its menu lists every worktree
of the session with its changes, and the submodules under their parent, plus the branches to switch
to and **New branch…**. Next to it, **N changed** opens the files: stage, unstage, discard (asks
first and cannot be undone); a submodule shows as one row, **Stage pointer** when its commit moved,
**Open <name>** when it only has changes inside. Write a message and press Enter: **Commit** takes the
staged files, or every change when nothing is staged. When there is nothing to commit, the button
becomes **Push n**, **Publish branch** or **Pull n**, whichever is next. The ⋯ menu holds fetch,
pull, push, commit and push, amend, stage all, unstage all, **Discard all changes…** (confirmed),
**Merge <base> into it**, **Merge…**, stash and pop, and **Open the Git graph**. Comments and **Send to
agent** stay at the bottom of the review, for the whole session. A pull or merge that conflicts is
aborted and names the files; a merge started elsewhere shows as in progress; a push is refused while
a submodule has commits that are not pushed. While the agent is running, the bar warns that files can
still change. Everything refreshes on file and ref changes, never by polling.

The sidebar has a **Git** tab next to **Workspaces** and **By status** (Ctrl+Shift+G, or the palette's
"Show or hide source control in the sidebar"; it reopens a hidden sidebar). It keeps source control
next to the terminal, like VS Code: the worktree of the selected session (its menu lists the session's
other worktrees and submodules), the branch to switch or create, Fetch, Pull and Push or Publish, the
commit message (Ctrl/Cmd+Enter) with Commit and its options, conflicts, staged files and changes with
stage, unstage and discard (confirmed), outgoing and incoming commits, and merge and stash. A file
opens its diff beside the session. It follows the selected session, and the tab is remembered.

The **Git tab** of the project (command palette, "Open Git (graph and source control)", ⋯ › **Open the
Git graph**; on a phone **More**, **Git**; both list it only while connected with a project) shows the graph with an **Uncommitted changes** row on top,
and source control in a column the toolbar button shows (hidden by default), for the worktree picked
at the top. A commit's detail adds **Merge into <branch>** and **Revert in <branch>**. On a phone the
tab has **Changes** and **Graph** sub-tabs.

```
curl --unix-socket "$S" "http://unix/v1/git/status?worktree=$W"
curl --unix-socket "$S" -X POST http://unix/v1/git/commit -d '{"worktree":"'$W'","message":"fix: x","all":true,"push":true}'
```

Routes (`worktree` or `lineId` in the body, `worktree=` or `line=` in the query; every reply holds
the new `status`): `GET /v1/git/status`, `POST /v1/git/stage|unstage|discard` (`files`: paths or
`"all"`), `commit` (`message`, `amend`, `all`, `push`), `fetch`, `pull`, `push`, `switch`
(`branch`, `create`), `merge` (`rev`), `stash` (`pop`), `revert` (`sha`). Each change sends
`git.changed` and `worktree.updated`.

### 14.10a Git graph

The Git tab draws a project's history with lanes (branches, remote branches, tags, HEAD,
merges), with Overseer's knowledge on top: the agent and its state on the tip of its branch,
the task number, the ticket and the PR state. Filters: all branches, current, agents only (the
branches of lines, tasks and worktrees), author; search by sha or subject. A commit shows its
message, changed files and diff, with actions: check out in a new worktree, create a branch,
cherry-pick into a worktree (confirmed), copy the sha, open on GitHub. The view refreshes by
itself when refs move: the daemon polls `HEAD`, `packed-refs` and `refs/` of the projects shown in
the last 10 minutes every second and sends `git.changed`. It works the same on a remote host.

```
curl --unix-socket "$S" "http://unix/v1/projects/$P/graph?limit=300&refs=agents"
curl --unix-socket "$S" "http://unix/v1/projects/$P/commits/8af2740"
```

### 14.11 Chat view (ADR 0036)

Agents always run as their real CLI in a daemon terminal. Their tabs (Claude Code, Codex,
Gemini) have two views: **Terminal** (the CLI itself) and **Chat** (the conversation, rebuilt
live from the agent's session file). Phones open agent tabs in Chat; desktops in Terminal. Switch
with the Chat | Terminal toggle in the tab bar (desktop) or the icon in the chip row (phone); the
choice is remembered per tab. Other agents always show the terminal.

- **Terminal size belongs to the desktop.** A phone never resizes an agent's terminal: it shows
  the desktop's width, scrolls sideways and keeps pinch-to-zoom. Shell terminals keep the shared
  size and the "Fit" button.
- **Messages**: your messages on the right; the agent's replies as Markdown with code blocks
  (copy button, highlighting loaded on demand); timestamps and the model. Thinking is hidden.
- **Select and copy**: message text, code blocks, tool output and sub-agent transcripts can be
  selected with the mouse (or a long-press on the phone) and copied with Cmd/Ctrl+C; buttons and
  chips stay out of the selection. Each message has a copy button next to its time (on hover on
  desktop, always shown on the phone) that copies the whole message as Markdown.
- **Tool steps** are collapsed to one line with a status dot; open one for the small diff of an
  Edit/Write, the command and truncated output of a Bash, the path of a Read/Search. Todo lists
  show as checklists.
- **Sub-agents** (Claude's Agent/Task tool, foreground or background) show as a card: type,
  description, state (running, done, failed, stopped), elapsed time, tokens, tool calls and, while
  it runs, its latest step. Open the card to see the sub-agent's own conversation (its prompt,
  tool steps and final report) nested inside, loaded on demand and kept live while it runs. While
  sub-agents run, an **Agents** strip above the composer lists them like the CLI footer; click one
  to jump to its card and open it. The elapsed time only ticks while a sub-agent runs and the
  window is visible. Codex and Gemini have no sub-agent cards.
- The view follows new messages unless you scrolled up ("N new" pill). "Load earlier" fetches
  older entries; only a window of entries is kept on screen.
- **Composer**: Enter sends on desktop (Shift+Enter for a new line), the send button on phone.
  Paste or pick images: they are uploaded and their path is inserted. `/` opens the agent's
  commands (built-ins plus your custom ones from `~/.claude/commands`, the project's
  `.claude/commands` and `~/.codex/prompts`). When the agent is idle the text is typed into the
  CLI (bracketed paste + Enter); when it is busy you choose **Queue** (typed when it is idle) or
  **Send now**. Nothing interrupts the agent by itself: **Interrupt** in the header sends Esc.
- **Dictation**: talk to the agent with the OS speech-to-text. The mic button in the composer
  starts macOS dictation (same as Edit > Start Dictation; no microphone permission for the app,
  macOS handles it) and the words land in the composer, ready to review and send. On Windows the
  button reminds you of Win+H. Linux has no OS dictation, so the button is hidden; on phones use
  the keyboard's mic. OS dictation also works straight in a terminal (Fn twice on macOS, Win+H
  on Windows): the text is typed into the agent's prompt as you speak.
- **Questions**: when the agent waits for you (permission, menu), a card shows the question read
  from the terminal screen with quick buttons (for example `1 Yes`, `2 Yes, don't ask again`,
  `3 No`; `y`/`n` for Codex; arrows + Enter for menus), and **Open terminal** as a fallback.
- **Hold / Close** in the header stop the agent through the daemon with the usual checks
  (section 4, `hold`). A held or stopped line shows its conversation read-only with **Resume**.
- **Which conversation**: a running line is one Claude process. Its chat and activity read the
  transcript of the session that process reports now in `~/.claude/sessions/<pid>.json`, re-read
  on every scan, so a `/resume` inside the terminal switches the chat too. A process started with
  `--session-id N` keeps its line (and tab) even when it resumed another session, also after it
exits: Resume then reopens the session it was last in (`lines.session_now`). Two processes on
  the same session stay two lines, each marked `duplicate`, and are never merged.
- Daemon API: `GET /v1/lines/<id>/chat[?after=<cursor>|before=<seq>]`,
  `GET /v1/lines/<id>/chat/agents/<agentId>` (a sub-agent's transcript), SSE `line.chat`,
  `GET /v1/lines/<id>/screen`, `POST /v1/lines/<id>/keys`, `GET /v1/lines/<id>/commands`
  (contract §6b). The daemon parses a chat only after someone opened it, incrementally, and
  forgets it after 15 minutes without readers.

## Uninstall

Overseer writes outside its own folders, so deleting the app alone leaves things behind: hook
entries in the agents' config files (for example `~/.claude/settings.json`), the statusline
wrapper, the boot service (a systemd user unit, or `~/Library/LaunchAgents/dev.overseer.daemon.plist`)
and its linger setting, `~/.overseer` (the `bin/` shim, `server/`, `u/`, `tasks/`, `reviews/`), the
config backups it made, worktrees and `task/*` branches it created, the state dir
(`~/.local/state/overseer` on Linux, `~/Library/Application Support/overseer` on macOS,
`%LOCALAPPDATA%\overseer` on Windows, or `$OVERSEER_STATE_DIR`) and the app's own data.

Removing it takes one step, in two equivalent forms:

- In the app: **Settings, Connection, Remove Overseer from this computer**. For a remote server
  the row reads **Remove Overseer from {host}** and acts on that server. The app first runs a dry
  run and lists what will be removed and what is kept with the reason (a worktree with uncommitted
  changes is always kept). **Remove Overseer** applies it; **Cancel** does nothing. When it is done
  the app says you can delete it, and on the desktop offers **Quit Overseer**.
- On the command line: `overseer uninstall` prints the same list (a dry run), `overseer uninstall
  --apply` does it. It works with the daemon stopped; when a daemon is running it asks the daemon
  (`POST /v1/uninstall`), which answers, deletes its state dir and exits. The state dir is only deleted when it holds Overseer's databases (or is empty). Before editing an agent config, uninstall saves a copy next to it as `<file>.overseer-uninstall.bak` and lists it under kept.

Per platform:

| Platform | How |
| --- | --- |
| Windows | Uninstalling from Apps and features runs `overseer uninstall --apply` first only when you tick the uninstaller's **Delete the application data** box; a plain uninstall or reinstall keeps your tasks and state. An update never does: the updater's installer runs the old uninstaller in update mode, which skips it. |
| macOS, Homebrew | `brew uninstall --cask overseer` removes the app; add `--zap` (`brew uninstall --zap --cask overseer`) to also run `overseer uninstall --apply` and delete the app's data. Plain `brew uninstall` leaves hooks and data alone on purpose, and an in-app update never touches them. |
| macOS, dragged to the Trash | Click the in-app button first (or run `/Applications/Overseer.app/Contents/Resources/server/darwin-<arch>/overseer uninstall --apply`), then delete the app. |
| Linux `.deb`, `.rpm`, AppImage | The packages ship no removal script (it would also run on upgrades, as root, without your home). Click the in-app button or run `~/.overseer/bin/overseer uninstall --apply` first, then remove the package or the AppImage. |
| Remote server | Connect to it and use the button, or run `~/.overseer/bin/overseer uninstall --apply` on the server. |
| Android, iOS | Remove the host from the app (or use the button while connected to it to clean the server), then delete the app. |

## What Overseer reads and writes

- Reads (read-only): `$CLAUDE_CONFIG_DIR` or `~/.claude` (`projects/*/*.jsonl` head and tail,
  `sessions/<pid>.json`, never `*.key`; `.credentials.json` or the macOS Keychain item
  `Claude Code-credentials` only for the opt-in Claude plan limits; whole transcripts and `subagents/` once, then appended
  lines, for usage), `$CODEX_HOME` or `~/.codex` (copy of `state_N.sqlite`, or rollouts), `/proc` or `ps`/`lsof`, `git` with `GIT_OPTIONAL_LOCKS=0`, `tmux list-panes`,
  `~/.local/state/tmux-snapshot/latest/state.json`.
- Writes: the state dir (including `usage.db`), files sent from the app in `~/.overseer/u/`,
  the daemon's service file when you turn on **Keep server running after reboot**
  (`~/.config/systemd/user/overseer-daemon.service` or
  `~/Library/LaunchAgents/dev.overseer.daemon.plist`), the lid close setting when you turn on
  **Keep agents running with the lid closed** (`pmset disablesleep` on macOS, the power plan's lid
  action on Windows; restored when turned off), plus `hooks install --apply` and the app's hook switches (the agents' hook files, with backups, and Overseer's own plugin files), the `~/.overseer/bin/overseer` symlink,
  `hold`/`close --yes` (SIGTERM to the agent, `@sb_held` and the stub in its pane), `resume`
  (relaunches the agent), Board commit/stash, relayed commands when `acceptRemoteCommands` is on,
  and the worktrees and tmux sessions created by `start` and `patch --local`; the worktree store
  (`worktreeDir`), `task/*` and `sb/*` branches, task commits, merges and cherry-picks asked from
  the app, branches created from the graph, and the programs started in daemon terms; Claude
  subagent files (`.claude/agents/*.md`) only through **Apply to agent files**, per confirmed file
  and with a `.bak-<stamp>` copy.
- Network: only `enroll`/`sync`/`login` and relays (the API), remote `resume`/`patch` (`ssh`),
  and `gh` for PR status.
