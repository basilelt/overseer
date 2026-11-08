# App ⇄ daemon contract (protocol 1)

What the desktop app (`app/`) talks to. Everything here is served by `overseer daemon`; the
machine-readable form is in `daemon.openapi.yaml` (paths `/version`, `/agents`, `/agents/settings`, `/terms*`,
`/tasks*`, `/projects`, `/workspaces*`, `/folders*`, `/groups*`, `/broadcast`,
`/lines/{id}/activity`, `/lines/{id}/resume` with `target: term`, `/lines/{id}/workspace`, `/review*`).
Existing routes (`/lines`, `/lines/{id}/diff`, `/events`, hold/close/…) are unchanged.

Additions since the first version of protocol 1 (all additive, so the protocol stays 1; ADR
0026): `GET /v1/agents` (§2a), `agent`/`model`/`effort` on `POST /v1/terms` and `agent` on Term,
any agent id in `Task.agent` and `line.agent`, `Task.rank`, `POST /v1/projects`. A client must
accept agent ids it does not know (show them with a generic icon). Workspaces, folders, groups
and `POST /v1/broadcast` (§6a, ADR 0029) with their events (§7) are additions too, and so is the
live review (§11: `/v1/review*`, `review.*` events, two MCP tools; ADR 0030), the git graph
(§12), worktrees (§13), `worktree` on `POST /v1/terms`, `inPlace` on task start and the
`git.changed` / `worktree.*` events (ADR 0031). Uploads (§3a), repository discovery and
`missing` on projects and workspaces (§5, §6a), `GET /v1/env` and `POST /v1/agents/rescan` (§2a)
are additions too, and so are session tasks (§5: `Task.origin`, `Task.manualStatus`,
`Task.badges`, `config.json` `sessionTasksDays`), and host-wide agent auto-approval settings (§2a).

## 1. Transport

| Where | How |
|---|---|
| Linux / macOS (local) | HTTP/1.1 on a Unix socket, mode `0600`. Path: `overseer socket-path` (default `<state>/daemon.sock`, `<state>` = `$XDG_STATE_HOME/overseer` or `~/.local/state/overseer` on Linux, `~/Library/Application Support/overseer` on macOS; `OVERSEER_SOCKET` overrides). No auth beyond file permissions. |
| Windows (local) | HTTP/1.1 on `127.0.0.1:<random port>`. `<state>/daemon.json` = `{"addr":"127.0.0.1:PORT","token":"<hex>","pid":N}` (`<state>` = `%LOCALAPPDATA%\overseer`). Every request needs `Authorization: Bearer <token>` **or** `?token=<token>` (WebSocket clients that cannot set headers use the query). 403 otherwise. ADR 0023. |
| Remote (server mode) | The app runs the system `ssh` once per host; the remote command ends in `overseer proxy`, which multiplexes every HTTP request / SSE stream / WebSocket / forwarded port over that one connection's stdio. See §8 and §8a. |

- One request per connection (`Connection: close`), except `GET /v1/events` (SSE, long-lived)
  and `GET /v1/terms/{id}/io` (WebSocket, long-lived).
- Bodies are JSON (`Content-Type: application/json`), max 1 MiB, except `POST /v1/uploads`
  (raw bytes, max 50 MiB, §3a); `GET /v1/file` answers raw bytes (§3a-bis). Errors: `{"error": "<message>"}`.
- Times are RFC 3339 UTC (`2026-10-29T09:00:00Z`).

## 2. Version

`GET /v1/version` and `overseer version --json`:

```json
{"version":"0.2.0","commit":"8af2740","os":"linux","arch":"x86_64","protocol":1}
```

`os` ∈ `linux|macos|windows`, `arch` ∈ `x86_64|aarch64` (Rust `std::env::consts`). `protocol`
increases on any breaking change to this document; the app refuses a daemon whose protocol differs.

`GET /v1/version` (not the CLI) also returns `epoch` (0 on a fresh start, +1 per hot upgrade),
`pid`, and `upgradedFrom` (the previous version, only after a hot upgrade).

`POST /v1/upgrade` `{"path"?: "<binary>", "version"?: "x.y.z", "force"?: bool}` hot-upgrades the
daemon in place (ADR 0034). `version` means `$HOME/.overseer/server/<version>/overseer`.
`202 {"from","to","epoch"}` once the new binary passed its pre-flight; the daemon then re-executes
itself with the same pid, terms and socket, and the response's `epoch` appears in `/v1/version`.
`400` bad or missing binary, `409` incompatible binary (protocol, takeover format, not newer
without `force`) or an upgrade already running, `501` on Windows. Daemons that predate it answer
`404`.

`GET /v1/logs?tail=N` → `200 {"path": "<absolute path of daemon.log>", "lines": ["..."]}`: the
last `N` lines (default 300, clamped to 1..=5000) of the daemon's log, oldest first, read across
`daemon.log` and `daemon.log.1`. Lifecycle lines look like
`2026-10-29T09:00:00.123Z INFO overseer daemon: listening on …`. Every line is redacted
before it leaves the daemon: bearer tokens, `token`/`password`/`passwd`/`secret`/`api_key`/
`apikey`/`authorization` values (`key=value`, `key: value`, JSON), GitHub tokens (`ghp_` …,
`github_pat_`), `sk-` keys and URL passwords become `***`. `daemon.log` lives in the state dir
and is rotated by copy-truncate at 5 MB (checked at start and every 60 s), keeping
`daemon.log` plus `daemon.log.1`..`daemon.log.4`. Same auth as every other `/v1` route. Daemons
that predate it answer `404`.

## 2a. Agents

`GET /v1/agents` → 200, in display order:

```json
[{"id":"claude","name":"Claude Code","installed":true,"version":"2.1.289 (Claude Code)",
  "models":["opus","sonnet","fable"],"efforts":["low","medium","high","max"],"resume":true,
  "verified":true,"sessions":true,"turns":true,"install":"curl -fsSL https://claude.ai/install.sh | bash"}]
```

- `id` ∈ `claude | codex | gemini | agy | opencode | amp | copilot | cursor | shell`. `shell` is
  the login shell (`$SHELL`); it is never a task agent.
- `installed`: the binary is on the daemon's `PATH` and `<bin> --version` exits 0 within 5 s
  (results cached 60 s); `version` is the first line it printed. A broken wrapper reports
  `installed: false`.
- `models`: absent → free text; `[]` → the agent takes no model (the field is ignored);
  otherwise suggestions (free text still allowed). `efforts` is the subset of
  `low|medium|high|max` the agent maps (empty → effort ignored).
- `resume`: the line can be resumed (by session id when the agent has ids, else "continue the
  latest session in the cwd"). `verified`: the flags were checked against the CLI's `--help`.
  `sessions`: lines are discovered from the agent's own session files and `/activity` works;
  otherwise lines exist only for daemon terms running the agent (`/activity` answers 409).
  `turns`: the end of a turn is detected (hooks) so a task moves to `in_testing` on its own;
  otherwise only when the agent exits (or calls MCP `set_task_status`).
- `install`: a hint to show next to an agent that is not installed.

`POST /v1/agents/rescan` → 200, same body as `GET /v1/agents`, after dropping the 60 s cache and
running the detection again.

`GET /v1/agents/settings` → 200 `{"autoApprove":{"claude":true,"codex":true,...}}`. The map
includes CLIs with launch-time auto-approval controls; unset values default to `true`.
`PUT /v1/agents/settings` with `{"agent":"codex","autoApprove":false}` persists one host-wide
setting in the daemon registry and returns the full map. New sessions started by Overseer use
these defaults, including task and feedback launches. Existing sessions keep their launch mode.
Supported flags are Claude Code `--permission-mode auto`, Codex `--sandbox danger-full-access
--ask-for-approval never`, Gemini CLI `--approval-mode=yolo`, Antigravity `--mode=auto`, opencode
`--auto`, Copilot CLI `--allow-all`, and Cursor Agent `--force`. Amp currently auto-approves by
default unless its own permission configuration enables a policy, so Amp is not in the map.
400 when either required field is missing or the agent has no launch-time control.

`GET /v1/env` → 200 `{"path":"/home/u/.local/bin:/usr/bin:…","source":"login-shell"}`: the `PATH`
the daemon uses for detection and for every term it spawns. Only `PATH` is exposed, never other
variables. At start (before any thread), on Unix, the daemon runs `$SHELL -l -i -c` once
(5 s timeout, output read after a marker so rc-file noise is skipped) and adopts the login
shell's environment minus `TMUX*`, `TERM`, `SHLVL`, `PWD`, `OLDPWD`, `_`, `CMUX_*` and
`OVERSEER_*`; the inherited `PATH` entries are kept after the login ones. `source`:
`login-shell`; `fallback` when the probe failed or timed out (the inherited `PATH` plus the
common user dirs that exist: `~/.local/bin`, `~/.cargo/bin`, `~/.bun/bin`, `~/.deno/bin`, the
newest `~/.nvm/versions/node/*/bin`, `~/.volta/bin`, `~/.asdf/shims`, `/opt/homebrew/bin`,
`/usr/local/bin`); `inherited` on Windows or with `OVERSEER_LOGIN_ENV=0`. A running daemon
keeps its environment: the new `PATH` applies from its next start (never restarted for this,
since a restart ends its terms).

Before that probe the daemon drops the variables bound to the SSH session or multiplexer it was
started from (`SSH_CONNECTION`, `SSH_CLIENT`, `SSH_TTY`, `TMUX`, `TMUX_PANE`, `STY`, and
`SSH_AUTH_SOCK` when it matches `/tmp/ssh-*/agent.*`), and never adopts them from the login
shell. Every term gets `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=overseer`,
`TERM_PROGRAM_VERSION=<daemon version>` and `OVERSEER_TERM=1`; `env` of `POST /v1/terms`
overrides them.

| id | binary | model | effort (`max` →) | session id up front | prompt | resume |
|---|---|---|---|---|---|---|
| claude | `claude` | `--model` | `--effort` | `--session-id <uuid>` | typed | `--resume <id>` |
| codex | `codex` | `-m` | `-c model_reasoning_effort=` (`xhigh`) | — | typed | `codex resume <flags> <id>` |
| gemini | `gemini` | `-m` | — | `--session-id <uuid>` | `-i <prompt>` | `--resume <id>` |
| agy | `agy` | `--model` | — | — | `-i <prompt>` | `--continue` |
| opencode | `opencode` | `-m` | — | — | `--prompt <prompt>` | `--continue` |
| amp | `amp` | — | `--mode` (`ultra`) | — | typed | `amp threads continue --last` |
| copilot | `copilot` | `--model` | `--effort` (`xhigh`) | `--resume=<uuid>` | `-i <prompt>` | `--resume=<id>` |
| cursor | `cursor-agent` | `--model` | — | — | typed | `cursor-agent resume` |

`cursor` is unverified (no binary on the development machine). "typed" = the prompt is pasted
into the PTY as described in §5; with a prompt flag it goes in argv instead, and `Term.cmd`
shows `<prompt>` in its place. Claude and Codex keep the paste: their argv is read back from
`/proc` to build resume commands, so a prompt in argv would be replayed on resume.

## 3. Terminals

The daemon owns PTYs (`portable-pty`; ConPTY on Windows). A term survives the app closing and
any client disconnecting; it lives in daemon memory and **does not survive a daemon restart**
(the PTY closes, the program gets SIGHUP). tmux sessions are still detected as before; terms are
an addition, not a replacement.

### Term

```json
{"id":"01JB...","pid":4242,"cwd":"/home/u/src/x","cmd":["claude","--session-id","…"],
 "title":"claude","cols":120,"rows":32,"createdAt":"2026-10-29T09:00:00Z",
 "exited":false,"exitCode":null,"lineId":"vm-01J…","taskId":"01JB…","agent":"claude"}
```

`exitCode`, `lineId`, `taskId`, `agent` are omitted when unknown (`agent` is set when `cmd[0]`
is one of the agents' binaries). `sizedBy` (addition, ADR 0033) is the kind of client that set the
current size (`"phone"` or `"desktop"`, from the `client` of the last resize that named one);
omitted until a client names itself. A resize naming `"client": "phone"` is ignored (200, size
unchanged) for agent terms (`lineId` or `agent` set): only desktops size an agent's PTY (ADR
0036). `lineId` is set at creation (resume,
task start) or discovered by the scan loop within one daemon interval (default 5 s) when an
agent line's pid is the term's pid or one of its descendants. Lines get the reverse link as
`local.termId` in `GET /v1/lines`. `workspaceId` (addition) is the workspace the term was
explicitly assigned to (§6a, Membership), omitted otherwise; lines carry it as
`local.workspaceId`.

Custom titles (addition): Terms carry `titleOverride` (string or null) and lines carry
`local.titleOverride`, both set by a user rename and shown instead of `title` / `local.title`
when not null. `PUT /v1/lines/{id}/title` `{"title": str | null}` stores it in the registry
(persists across restarts), answers 200 `{"lineId","titleOverride"}` · 400 · 404 and publishes
`line.updated` with `local.titleOverride`. The auto `title` fields are never changed.

### Routes

| Route | Body | Result |
|---|---|---|
| `POST /v1/terms` | `{cwd?, cmd?:[str] \| agent?:str, model?, effort?, env?:{str:str}, cols?, rows?, title?, lineId?, taskId?}` | **201** Term. `cwd` defaults to `$HOME`, `cmd` to the login shell (`$SHELL`, else `/bin/sh`; `%COMSPEC%` on Windows), `cols`×`rows` to 80×24 (each 1..=1000). `title` defaults to the basename of `cmd[0]`. The child gets the daemon's environment plus `env`, `TERM=xterm-256color`, `COLORTERM=truecolor`, `OVERSEER_TERM_ID=<id>`. With `agent` (instead of `cmd`) the daemon builds the argv from §2a with `model`/`effort`, a fresh session id when the agent takes one, and `title` = the agent's name. With `worktree: true` (or `{branch?, base?, slug?}`) and a `cwd` inside a git repository, the daemon first creates a worktree as `POST /v1/worktrees` does (branch `sb/<agent>/<slug>`, `shell` for a plain shell) and starts the term in it. 400: bad body, `cwd` not a directory, `cmd[0]` not found, both `agent` and `cmd`, unknown or not installed agent, bad `effort`, `worktree` with a `cwd` outside a repository; 409: the worktree could not be created. |
| `GET /v1/terms` | | 200 `[Term]` (running and exited, oldest first) |
| `GET /v1/terms/{id}` | | 200 Term · 404 |
| `POST /v1/terms/{id}/resize` | `{cols, rows, client?}` | 200 Term · 400 · 404 · 409 exited |
| `PUT /v1/terms/{id}/title` | `{"title": str \| null}` | 200 Term with `titleOverride` (trimmed, control characters as spaces, ≤ 80 chars; null or blank clears it; kept across daemon upgrades, gone with the term) · 400 · 404. Publishes `term.updated`. |
| `POST /v1/terms/{id}/kill` | `{"confirm":true}` | **202** Term (SIGHUP to its process group, then SIGKILL after 2 s on Unix; `exited` flips with `term.exited`) · 400 without `confirm:true` · 404 · 409 already exited |
| `DELETE /v1/terms/{id}` | | 200 `{"deleted":"<id>"}` · 404 · 409 still running |
| `GET /v1/terms/{id}/io` | WebSocket upgrade | see below · 404 · 400 not an upgrade |

### `GET /v1/terms/{id}/io` (WebSocket, RFC 6455)

- On connect the server first sends the scrollback ring (last 2 MiB of output, possibly cut
  mid-escape-sequence: feed it to the emulator as is) as **binary** frames of at most 64 KiB, then
  live output as **binary** frames. Replay and subscription are atomic (no gap, no duplicate).
- Client → server **binary** frame = bytes written to the PTY (keystrokes, pastes).
- Client → server **text** frame = JSON control: `{"resize":{"cols":120,"rows":32},"client":"phone"}`
  (`client` optional: 1–16 letters, digits or `-`; stored as the Term's `sizedBy`). Unknown
  text frames are ignored.
- Any number of clients may attach to one term; all see the same output, all may type, the last
  resize wins.
- When the program exits the server sends the remaining output then a Close frame (1000). An
  exited term still accepts connections: replay, then Close.
- Closing the socket never affects the program.

## 3a. Uploads

Lets the app hand a local file (a pasted screenshot, a dropped file) to a program running in a
remote term: the app uploads it, then types the returned path into the term.

`POST /v1/uploads?name=<percent-encoded file name>` with the raw file as the body
(`Content-Type: application/octet-stream`) and a `Content-Length` header:

- **201** `{"path":"/home/u/.overseer/u/a1b2c3.png","short":"~/.overseer/u/a1b2c3.png","name":"screenshot.png","size":48213}`.
- **400** name empty, `.`/`..`, or containing `/`, `\` or NUL; or the body ended before
  `Content-Length` bytes (the partial file is removed). **411** no `Content-Length` (chunked
  bodies are refused). **413** more than 50 MiB (52 428 800 bytes), refused before reading.
- The body is streamed to disk, not buffered. The file is stored as
  `~/.overseer/u/<id>.<ext>` (directory `0700`, file `0600`): `<id>` is 6 random characters
  `[a-z0-9]`, `<ext>` the lower-cased extension of the sanitised name (≤ 10 alphanumerics, none
  when absent). `name` is the name sanitised to `[A-Za-z0-9._-]` (other characters → `_`, leading
  dots dropped, ≤ 80 characters). `short` is `path` with the home directory written `~`.
- The daemon deletes uploads older than 7 days (at start, then every 10 minutes), and empties and
  removes the `<state>/uploads/<yyyymmdd>/` directories of older versions.

The app types the path followed by a space, shell-quoted (a `~/` prefix stays unquoted), through
the term input as a bracketed paste when the program enabled mode 2004, and never sends Enter.
Images (`png`, `jpg`, `jpeg`, `gif`, `webp`) are typed with `path` so Claude Code attaches them as
`[Image #N]`; other files with `short`. Pasted clipboard files are uploaded in local mode too; a
dropped file on a local terminal types its own path.

## 3a-bis. File preview

Lets the app preview a file the user clicks in a term (an image, a video, a PDF, a log): the
daemon answers the file's raw bytes.

`GET /v1/file?path=<percent-encoded path>[&term=<termId>]`:

- `path` is resolved on the daemon's host: `~` and `~/…` expand to the home directory, absolute
  paths are used as they are, relative paths are joined to the term's current directory (the live
  working directory of the term's process, else the `cwd` it was started in). The path is then
  canonicalised (symlinks followed).
- **200** the bytes, with `Content-Type` from the lower-cased extension: `png`, `jpg`/`jpeg`,
  `gif`, `webp`, `svg` (`image/svg+xml`), `bmp`, `ico`, `avif` as `image/*`; `mp4`/`m4v`
  (`video/mp4`), `webm`, `mov` (`video/quicktime`), `ogv`; `mp3`, `wav`, `ogg`/`opus`
  (`audio/ogg`), `m4a` (`audio/mp4`), `flac`; `pdf`; `md`, `markdown`, `txt`, `log`, `csv`, `tsv`,
  `json`, `jsonl`, `yaml`, `yml`, `toml`, `xml`, `html` as `text/plain; charset=utf-8`; anything
  else `application/octet-stream`. `Cache-Control: no-store`.
- **400** `path` missing, relative without `term` or with an unknown `term`, or not a regular file
  (a directory, a device). **403** not readable. **404** does not exist. **413** larger than
  50 MiB (52 428 800 bytes); the error names the file's size.
- Errors are `{"error": "..."}`. Any regular file the daemon's user can read is served; the route
  sits behind the same token check as every other route.

## 3a-ter. Folder browser

Lets the **New workspace** dialog browse the folders of the connected host (local or remote alike).

`GET /v1/fs/dirs[?path=<percent-encoded path>][&hidden=1]`:

- `path` absent, empty or `~` lists the home directory; `~/…` expands to it; other paths must be
  absolute. The path is canonicalised (symlinks followed).
- **200** `{"path","parent","home","dirs":[{"name","path","git"}],"truncated"}`: `path` is the
  canonical directory, `parent` its parent (`null` at the root), `home` the canonical home
  directory. `dirs` holds only directories (a symlink to a directory counts, its `path` is the
  link's), sorted case-insensitively by name; names starting with `.` are left out unless
  `hidden=1`. `git` is true when the folder has a `.git` entry (a directory or a worktree's file;
  no git process runs). At most 2000 entries; `truncated` is true when more were cut.
- **400** a relative path, or not a directory. **403** not readable. **404** does not exist.
  Errors are `{"error": "..."}`.

## 3b. Feedback

Used by the app's **Send feedback** sheet. Both routes write the report body to
`<state>/feedback/report-<ms>-<n>.md` (directory `0700`, file `0600`); reports older than 7 days
are removed on the next write. Neither route logs the prompt or the body.

`POST /v1/feedback/start` `{"agent","model"?,"effort"?,"cwd"?,"cols"?,"rows"?,"prompt","body"}`
starts the agent CLI in a new term: the prompt goes through the agent's prompt flag, or as the
initial-prompt argument for `claude` and `codex` (both shown as `<prompt>` in `cmd`), else it is
pasted once the program is ready. `prompt` must contain
`@REPORT@`, replaced by the report file path. `cwd` is used when it is a directory, else
`<state>/feedback/work`.

- **201** `{"term": Term, "report": "<path>"}`.
- **400** missing body (or over 60 000 bytes), prompt without `@REPORT@` or over 16 000 bytes,
  unknown agent, `shell`, bad effort. **409** agent not installed or the term failed to start.

`POST /v1/feedback/direct` `{"repo":"owner/name","title","body","labels"?:[...]}` runs
`gh issue create` on the host with the user's own gh login (45 s timeout). When gh rejects the
labels, it retries once without them.

- **201** `{"url":"https://github.com/owner/name/issues/N","labelled":true|false}`.
- **400** bad repo, empty or multi-line title (max 256), missing body. **424** gh not installed.
  **502** gh failed (first stderr line, secrets masked) or printed no issue URL. **504** timeout.

### GitHub review requests

Both routes use the host's own `gh` login. Errors carry `{"error","code"}` with `code` one of
`missing` (gh not installed), `auth` (gh not logged in: `gh auth status` fails), `failed`,
`timeout`, `bad`.

`GET /v1/github/review-requests[?refresh=1]` runs one `gh api graphql` search
`is:pr is:open archived:false review-requested:@me` (direct and team requests, max 100, 30 s
timeout). The result is cached 60 s; `refresh=1` bypasses the cache.

- **200** `{"login","fetchedAt":<ms>,"prs":[{"repo":"owner/name","number","title","url","author",
  "headRefName","baseRefName","updatedAt","isDraft","additions","deletions","changedFiles",
  "reviewDecision":"APPROVED"|"CHANGES_REQUESTED"|"REVIEW_REQUIRED"|null,"reviewed":bool,
  "myState":"COMMENTED"|"APPROVED"|"CHANGES_REQUESTED"|"DISMISSED"|null,"localPath":string|null}]}`.
  `reviewed` is true when the viewer has a submitted review on the PR; `localPath` is the known
  project (`GET /v1/projects`) whose origin is `github.com/<repo>`.
- **424** `missing` or `auth`. **502** gh failed or printed invalid JSON. **504** timeout.

`POST /v1/github/review-requests/{owner}/{repo}/{number}/review`
`{"agent","model"?,"effort"?,"cols"?,"rows"?}` prepares the PR and starts the agent on it:

1. Clone: the known project for the repo, else `~/.overseer/reviews/<owner>/<repo>`, created
   with `gh repo clone <owner>/<repo> <dir> -- --filter=blob:none` (10 min timeout).
2. Worktree: `git fetch origin pull/<number>/head`, then a worktree of branch `review/pr-<number>`
   at that commit in the usual store (`POST /v1/worktrees` rules). When that worktree already
   exists it is reused and fast-forwarded to the new head (best effort); a leftover branch without
   a worktree is moved to the head first.
3. Agent: same validation and prompt delivery as `POST /v1/feedback/start`, cwd = the worktree,
   title `PR #<number> <owner>/<repo>`. The prompt is built by the daemon: read the description
   (`gh pr view`) and the diff (`gh pr diff`), review correctness, security, error handling,
   performance and tests, write the findings in the chat, then stop. It forbids posting,
   approving, requesting changes, committing, pushing or merging until the user explicitly
   confirms in the chat, and then names `gh pr review` and `gh api .../pulls/<n>/comments`.

Steps 1 to 3 run on a background thread; the request waits up to 20 s for them (the app's request
timeout is 30 s).

- **201** `{"term": Term, "worktree", "project", "branch"}`.
- **202** `{"pending": true, "title": "PR #<number> <owner>/<repo>"}`: still cloning or fetching. The
  term appears later through the usual term events with that title (the app opens it then); a
  failure after this point is only written to the daemon log.
- **400** bad owner/repo/number, unknown agent, `shell`, bad effort. **409** agent not installed,
  worktree or term failed. **424** gh not installed. **502** clone or fetch failed. **504** timeout.

## 4. Lines ↔ terms

- `POST /v1/lines/{id}/resume` with body `{"target":"term", "cols"?, "rows"?}` starts the
  line's resume argv (`claude <flags> --resume <session>`, `codex resume <flags> <id>`) in a new
  daemon term in the line's cwd and answers **201** Term (with `lineId`). **With `target: term`
  the call acts by default**; `?dryRun=true` returns the plan (`where: "term"`) instead. Without
  a body (or `target: "tmux"`) the existing tmux behaviour is unchanged (dry run by default).
  409: line running, cwd missing; 400: the line belongs to another station.
- `"attachOnly": true` in that body only attaches: it answers the held stub's term (without
  pressing Enter), the line's running term, or a term attached to its tmux pane, and **409**
  `the agent is not running` instead of starting the agent. The app sends it for every open or
  attach; only an explicit Resume omits it.
- Agent sessions started from the app in a term (`POST /v1/terms` with `agent` or `cmd:
  ["claude", …]`) become lines through the usual discovery (Claude transcripts, Codex rollouts,
  Gemini chats, hooks) and are linked by pid as above. Starting `claude` with `--session-id
  <uuid>` makes the line appear sooner.
- A term running another agent (`sessions: false` in §2a) is itself a line: `agent` = the agent
  id, `agentSessionId` = the session id given at start (copilot) or the term id, `cwd` the term's,
  `active`/`idle` from its last output, `held` once the program exited. Resuming it with `target:
  term` runs the resume argv of §2a in a new term that keeps the same line. Such lines disappear
  with the term (daemon restart, `DELETE /v1/terms/{id}`).

## 5. Tasks (Kanban)

Local to the station (SQLite in the registry, `tasks` table). Optionally synced later; not synced
in protocol 1.

### Task

```json
{"id":"01JB…","number":7,"title":"Fix login","description":"…","project":"/home/u/src/x",
 "status":"in_progress","agent":"claude","model":"opus","effort":"high","station":"vm-dev",
 "ticketKey":"AS-1234","lineId":"vm-dev-01J…","termId":"01JB…","branch":"task/7-fix-login",
 "worktreePath":"/home/u/worktrees/x/task-7-fix-login","progress":{"done":2,"total":5,"note":"tests"},
 "error":"…","createdAt":"…","updatedAt":"…"}
```

- `number` increments per `project` and is never reused (deleted numbers stay burnt).
- `project` is the absolute path of a git repository's main worktree root.
- `status` ∈ `pending | auto | in_progress | in_testing | merged | completed`. `merged`: the
  task's branch landed in the base (set by the daemon, which checks it; an agent can only set it
  through `set_task_status` once the branch has landed, else the tool errors). `merged` counts as
  done like `completed` for `dependsOn`. A task already `completed` is only moved to `merged` when it
  was closed by `approve` or `commit` review, within 6 hours of it; a task closed by hand stays
  `completed`. Apps older than this change do not know `merged` and show such tasks as open.
- `agent` ∈ the ids of §2a except `shell` (default `claude`); `model` is a free string, `""` =
  the agent's default (default `""`); `effort` ∈ `low | medium | high | max` (default `high`;
  ignored by agents without efforts).
- `rank` (number): order inside a Kanban column, ascending; default the creation time in ms. The
  app sets it to the midpoint of the neighbours when a card is dropped between them.
- `station` is the daemon's station id (read-only).
- Optional, omitted when unset: `ticketKey` (`AS-<n>`), `lineId`, `termId`, `branch`,
  `worktreePath`, `progress`, `error` (last start/agent failure, cleared by the next start —
  an addition to the original spec).
- `origin` ∈ `user | session | agent | organizer | configurator` (always present; additive).
  `configurator`: the "Organize my sessions" agent (§6c); treated like `organizer` (hidden from
  the columns, `completed` at turn end, refused task-writing MCP tools). `user`: created
  through `POST /v1/tasks`. `session`: created by the daemon for an agent session (below).
  `agent`: created by a task's agent through the MCP `create_task` tool (always `pending`).
  `organizer`: the board organizer run (below); it goes to `completed`, not `in_testing`, when its
  turn ends, and does not count toward `autoConcurrency`.
- `urgent`, `important` (booleans), `parentId` (id of the task whose agent created this one):
  omitted when unset. Eisenhower quadrant, derived by the app: both true = do, `important` only =
  plan, `urgent` only = delegate, both false = drop, either unset = unclassified.
- `dependsOn` (array of task ids, always present, `[]` when none): the tasks this one waits on.
  Requests may also give task numbers of the same project (`12` or `"#12"`); they are stored as ids.
  A task is blocked while one of them exists and is not `completed`; the auto runner skips
  blocked `auto` tasks and starts them, in queue order, once all their dependencies are
  `completed` (`in_testing` still blocks). `POST /v1/tasks/{id}/start` does not check
  dependencies: the app asks the user to confirm a blocked start. Deleting a task removes its id
  from every `dependsOn` (each dependent gets a `task.updated`).
- `inheritBranch` (boolean, omitted when unset): with exactly one dependency, the task's worktree
  starts from that dependency's branch unless `inheritBranch` is `false` (falls back to the base
  branch when the dependency has no branch or it is gone).
- `manualStatus` (optional, session tasks only): the status the user last set by hand, kept
  while the session stays in the same state (below).
- `badges` (optional, session tasks only): `["needs_input"]` or `["paused"]`.

`Message`: `{id, taskId (recipient), from (task id or null for the user), fromNumber?, fromTitle?,
body, createdAt, delivered}`.

The organizer agent classifies tasks with the MCP `prioritize_task` tool (urgent, important, rank:
do before plan before delegate before drop inside a column) and flags duplicates with
`send_message`. With `config.json` `autoOrganize` (default `false`) or the app toggle stored in the registry
(`GET`/`PUT /v1/tasks/auto-organize`, body `{enabled}`, 400 otherwise), the tick starts it for a
project when its newest open `user`/`agent` task is newer than the last organizer run and at least
30 s old, and none of its organizers runs.

### Routes

| Route | Body | Result |
|---|---|---|
| `GET /v1/tasks?project=&status=` | | 200 `[Task]` ordered by `project`, then `number`. `status` may be a comma list. |
| `POST /v1/tasks` | `{title, project, description?, status?: pending\|auto, agent?, model?, effort?, ticketKey?, rank?, urgent?, important?, origin?: user\|agent, parentId?, dependsOn?: [taskId], inheritBranch?, attachments?: [{path, name?}]}` | **201** Task · 400 (missing title, project not a git repo, bad enum, `parentId` or a `dependsOn` id not a task, an attachment not a file from `POST /v1/uploads`, more than 20 attachments). Attachments move to `~/.overseer/tasks/{id}/` and their paths are appended to the description under `Attachments:`. |
| `GET /v1/tasks/{id}` | | 200 Task · 404 |
| `PATCH /v1/tasks/{id}` | any of `{title, description, status, agent, model, effort, rank, urgent (bool \| null), important (bool \| null), ticketKey (null clears), progress: {done,total,note?} \| null, dependsOn: [taskId], inheritBranch (bool \| null)}` | 200 Task · 400 · 404. `dependsOn` replaces the list: 400 when an id is unknown, is the task itself, or closes a cycle. `status` may be set to `pending`, `auto`, `in_testing`, `merged`, `completed`; `in_progress` only through `/start` (400). Session tasks: `in_progress`, `in_testing`, `merged`, `completed` (sets `manualStatus`). |
| `DELETE /v1/tasks/{id}` | | 200 `{"deleted":"<id>"}` · 404 · 409 while its term runs. The worktree and branch are kept; its attachment folder is removed. |
| `POST /v1/tasks/{id}/start` | `{cols?, rows?, inPlace?}` | 200 Task (`in_progress`, `termId`, `branch`, `worktreePath`; `lineId` at once for agents that take a session id up front, after the next scan otherwise) · 404 · 409 its term is still running, the agent is not installed, or git failed. |
| `GET /v1/tasks/{id}/messages` | | 200 `[Message]`, oldest first: every message sent to or from the task · 404 |
| `POST /v1/tasks/{id}/messages` | `{body (1 to 8000 chars), from?: taskId}` | **201** Message · 400 (empty or long body, `from` equals the task or is unknown) · 404. `from` absent = the user. When the task has a live term the text is pasted and Enter pressed at once and `delivered` is true; otherwise it stays undelivered and is appended to the prompt when the task starts. Publishes `task.message`. |
| `GET /v1/tasks/{id}/overlaps` | | 200 `[{task: {id, number, title, status, branch}, files: [path]}]`: other `in_progress`/`in_testing` tasks of the project, with their own worktree, whose changed files (diff against the merge-base with the branch checked out at the project root, plus `git status --porcelain`) intersect this task's. Computed on demand. · 404 |
| `POST /v1/tasks/organize` | `{project?}` | 200 `{started: [Task]}` · 409 nothing to organize (or the agent could not start). Creates a task with origin `organizer`, title "Organize board", and starts it in place with the organizer prompt, for the project or for every project with open non-session tasks; a project whose organizer already runs is skipped. |
| `POST /v1/tasks/{id}/review` | `{action: approve\|reject\|commit, message?}` | 200 `{"task": Task, "commit"?: "<sha>"}` · 400 · 404 · 409 |

**Start.** Without a worktree yet, the daemon creates one through §13: branch
`task/<number>-<slug>` in `<worktreeDir>/<repo-name>-<hash>/task-<number>-<slug>`, from the
project's base branch (`slug` = title lowercased, non-alphanumerics → `-`, max 40 chars). A
restarted task reuses its worktree and branch. With `inPlace: true` the agent runs in the project
checkout itself (no worktree; `branch` = its current branch, `worktreePath` = the project). Then
a new daemon term in the worktree runs the agent:

- `claude [startArgs…] --session-id <uuid> [--model <model>] --effort <effort>`
- `codex [-m <model>] -c model_reasoning_effort=<low|medium|high|xhigh>` (`max` → `xhigh`)
- the other agents per the table of §2a (`config.json` `startArgs` apply to Claude only)

with `OVERSEER_TASK_ID=<task id>` in its environment. Agents with a prompt flag get
`"<title>\n\n<description>"` in argv. For the others, when the agent is ready (1.5 s of
output silence after its first output, at most 30 s) the prompt `"<title>\n\n<description>"`
(trimmed) is typed into the PTY followed by Enter: as a bracketed paste when the program enabled
bracketed-paste mode (`ESC[?2004h`, both TUIs do), else with newlines turned into spaces. A
typed prompt never touches disk, argv or logs; a prompt given by flag is in the process's argv
(visible to local `ps`) but `Term.cmd`, events and logs show `<prompt>`.

**Auto-runner.** Each daemon tick, while the number of `in_progress` tasks with a running term
is below `config.json` `autoConcurrency` (default 2), the first `auto` task in column order (lowest `rank`, then `createdAt`)
is started. A failed start puts the task back to `pending` with `error` set.

**→ in_testing.** An `in_progress` task moves to `in_testing` when its line finishes a turn after
the start (a `Stop`/`StopFailure` hook after the start, or the line seen busy then not busy), or
when its term exits (`error` = `agent exited with code N` when N ≠ 0). Agents can also move their
own task there through MCP (§7).

**Session tasks.** Each daemon tick, every line of this station with a project (its worktree or
`cwd` is inside a git repository) that no task links to (`lineId`) gets one task with
`origin: "session"` and `lineId` = the line id, in the project of its main worktree root. One task
per line, ever (a unique index on `lineId` for session tasks). Only lines alive or with activity
in the last `config.json` `sessionTasksDays` days (default 7; `0` turns creation off) get one;
hung-up, duplicate, volatile (`/tmp`) and missing-`cwd` lines never do.

- `title`: the agent's latest AI title (Claude `ai-title`, else `line.local.title`), else the
  latest summary, else the first real user prompt, else the latest recorded prompt (Claude
  `last-prompt`), else `"New <Agent name> session"`. Prompts are cleaned first: `<tag>…</tag>`
  blocks (system reminders, command wrappers), pasted-content placeholders, slash commands, and
  absolute paths and URLs are dropped, then the first sentence is kept, cut to ~60 characters at a
  word boundary (`…`) and capitalised. Generated titles follow the session live; a title set
  through `PATCH /v1/tasks/{id}` is the user's and is kept.
- `description`: the activity `goal` (first cleaned user prompt, 500 chars). `branch`,
  `worktreePath`, `ticketKey`, `agent` come from the line; `model` from `--model`/`-m` in its
  launch args, else the model of the latest assistant message (else `""`); `effort` is `high`
  and has no meaning.
- `progress`: from the agent's todo list (Claude `TodoWrite`): `done` = completed todos,
  `total` = all, `note` = the todo in progress, else what the agent did last (`Editing a.rs`,
  `Reading b.rs`, a command's description). Without todos it is `{done: 0, total: 0, note}`.

The status follows the session:

| Session | Status | Badge |
|---|---|---|
| `hung-up` (closed) | `completed` | |
| gone or `held` (lost) and its branch merged into the base (or deleted) | `merged` | |
| `held` (lost: the process ended) | `in_testing` | |
| `held` by the user | unchanged if `in_testing`/`completed`, else `in_progress` | `paused` |
| needs input (permission prompt, question) | `in_progress` | `needs_input` |
| alive without any prompt yet | `in_progress` | `needs_input` |
| busy | `in_progress` | |
| turn finished (not busy after having worked; `idle`) | `in_testing` | |

A user move (`PATCH status`, `/review`) on a session task sets `manualStatus`; the daemon keeps
it while the session stays in the state it had then. When the session changes state, the task
follows again, except that it never moves back to an earlier column on its own unless the session
starts working again (busy or needs input). A session task cannot be started (409), deleted
(409: close the session instead) or set to `pending`/`auto` (400); `PATCH status: in_progress` is
allowed for it. `reject` on its review moves it to `in_progress` (409 with a `message`: type in
the session instead).

**Review.**
- `approve` → `completed`.
- `reject` → `in_progress`; a non-empty `message` is typed into the running agent (bracketed
  paste + Enter). 409 if `message` is given and the term is not running; without a message and
  without a running term the task goes back to `pending` (start it again: the worktree is reused).
- `commit` → `git add -A && git commit -m <message or "task #<number>: <title>">` in the
  worktree, then `completed`; answers the commit sha. 409: no worktree or nothing to commit.

### Projects

`GET /v1/projects` → 200 `[{"path":"/home/u/src/x","name":"x","repo":"github.com/Acme/x"}]`:
the main worktree roots of repositories seen in lines, `config.json` `roots` (each root and its
direct children) and `repos`, task projects, added and discovered projects; `repo` is
`local:<path>` without a remote. Every project has `missing` (bool): an added or discovered
project whose directory no longer exists is listed with `"missing": true` and `repo: ""`;
other sources only list existing repositories.

`POST /v1/projects/discover` → 200 `{"added": 2, "projects": [Project]}` scans for git
repositories now; `added` counts the new ones. The daemon also scans at start and every
10 minutes in the background. It walks `config.json` `discoverRoots` (default: those of
`~/Documents/Github`, `~/src`, `~/code`, `~/projects`, `~/dev` that exist; `[]` disables) three
levels deep; a directory with a `.git` directory or file is a repository and is not descended
into; dot-directories, `node_modules`, `target`, `.venv`, symlinks and the worktree store are
skipped. Each repository's main worktree root is stored like an added project, so it gets an
`auto` workspace (§6a).

`POST /v1/projects` `{"path": "/abs/path"}` (or `~/…`) adds a project, stored in the registry
(`projects` table): **201** `{path, name, repo}` with `path` = the repository's main worktree
root, **200** when it was already added, **400** when the path is missing, relative, not a
directory or not in a git repository.

## 6. Activity (side panel)

`GET /v1/lines/{id}/activity?since=<RFC3339>` → 200 · 404 unknown line · 409 transcript not found.

```json
{"lineId":"…","title":"Fix login","goal":"first user prompt (≤500 chars)","status":"active",
 "busy":true,"needsInput":false,
 "workLog":[{"id":"toolu_01…","kind":"edit","label":"Edit src/a.rs","path":"/w/src/a.rs",
             "state":"done","at":"…","startedAt":"…"}],
 "todos":[{"text":"Write tests","state":"in_progress"}],
 "editedFiles":["/w/src/a.rs"],"lastAssistantText":"≤500 chars","at":"…"}
```

- Parsed from the Claude transcript (`tool_use`/`tool_result`, the last `TodoWrite`), the
  Codex rollout (`function_call`/`custom_tool_call` + outputs, the last `update_plan`) or the
  Gemini chat (`~/.gemini/tmp/<project>/chats/session-*.json[l]`: `toolCalls` with their
  `status`, the last `write_todos`). Other agents have no activity (409).
- `kind`: `edit` (Edit, MultiEdit, Write, NotebookEdit, apply_patch, write_file, replace),
  `bash` (Bash, shell, exec_command, run_shell_command), `read` (Read, read_file,
  list_directory), `search` (Grep, Glob, WebSearch, glob, search_file_content,
  google_web_search), `other`.
- `state`: `running` until a result arrives, then `done` or `failed` (`is_error`, non-zero exit).
- `id` is the tool-call id (stable); `at` is the time of the entry's **last change** (start, then
  result) and `startedAt` its start. `since` returns only entries with `at > since`; the client
  merges by `id`. `todos`, `editedFiles` and the scalar fields are always complete. `at` (top
  level) is the newest entry time (second precision, so the next call may repeat entries of that
  second); pass it as the next `since`. At most the last 500 entries. `title` is the line's
  title (Claude `ai-title`), else the first prompt cut to 80 chars; `goal`, `title` and
  `lastAssistantText` are `""` when unknown.
- `status` is the line state (`active|idle|held|hung-up`). Content is shown to the user in the
  app; the daemon never writes it to a log.
- `GET /v1/lines/{id}/diff` (existing) gives the worktree diff.

## 6b. Chat view (ADR 0036)

`GET /v1/lines/{id}/chat[?after=<cursor>|before=<seq>][&limit=<n>]` → 200 · 400 bad query ·
404 unknown line · 409 agent without a chat parser (anything but `claude`, `codex`, `gemini`)
or no transcript yet.

```json
{"lineId":"…","agent":"claude","cursor":1730196000123456,"reset":false,"more":true,
 "model":"claude-opus-4-1",
 "entries":[
  {"id":"9f…:1","seq":3,"rev":1730196000123401,"at":"…","kind":"assistant","text":"markdown","model":"claude-opus-4-1"},
  {"id":"toolu_01","seq":4,"rev":1730196000123456,"at":"…","kind":"tool",
   "tool":{"name":"Edit","kind":"edit","label":"Edit auth.rs","path":"/w/src/auth.rs",
           "diff":{"old":"a == b","new":"a != b"},"output":"…","state":"done"}},
  {"id":"toolu_03","seq":5,"rev":…,"at":"…","kind":"todos","todos":[{"text":"Write tests","state":"in_progress"}]}]}
```

- `kind`: `user` (`text`, `images`: number of attached images, `imagesAt`: byte offset of
  their transcript line, set for Claude and Codex main chats), `assistant` (`text` Markdown,
  `model`), `tool`, `todos` (TodoWrite, `update_plan`, `write_todos`), `notice` (local command
  output, "Interrupted", "Conversation compacted"). Thinking, meta/system reminders,
  task notifications and subagent sidechains are never sent as entries.
- `tool.kind`: `edit` (`diff` `{old,new}`, or `{patch}` for Codex `apply_patch`), `write`
  (`diff` with `old: ""`), `bash` (`command`), `read`, `search`, `web`, `task` (subagent;
  `detail` = subagent type, `agent` below), `other`. `state` `running|done|failed`; `output` is
  the tool result cut to 4 KiB (`truncated: true` when cut); for a subagent it is its final
  report, cut to 16 KiB. Texts are capped (assistant 64 KiB, user 16 KiB,
  diffs 8 KiB per side).
- `seq` orders entries; `rev` is a revision from a daemon-wide counter that starts at the
  daemon's start time × 1000, bumped when an entry is created **or changed** (a tool result
  arriving). `cursor` is the newest `rev`.
- No cursor: the last `limit` entries (default 200, max 500), `more` when older ones exist.
  `after=<cursor>`: every entry with `rev > cursor`, ascending `seq`; the client upserts by
  `id`. `before=<seq>`: up to `limit` entries before that `seq` ("load earlier"). `reset: true`
  means the client must drop what it has (transcript truncated or rewritten, cursor from an
  earlier daemon or chat cache) and the entries are a fresh tail.
- The daemon parses incrementally from the last byte offset (complete lines only), keeps at
  most 2000 entries per chat and at most 8 chats (subagent transcripts are evicted first), and
  forgets a chat nobody read for 15 minutes.
- Subagents (Claude `Agent`/`Task`): the `task` tool carries `agent`:
  `{"agentId":"a16…"|null,"type":"Explore"|null,"description":"…","state":"running|done|failed|stopped",
  "background":true,"startedAt":"…","endedAt":"…"|null,"tokens":76606,"toolCount":24,
  "lastActivity":"Read lib.rs"|null}`. Claude Code writes each subagent to
  `<session>/subagents/agent-<agentId>.jsonl` (all lines `isSidechain: true`) with
  `agent-<agentId>.meta.json` (`toolUseId`, `agentType`, `description`). The link is the
  background launch result (`toolUseResult.status: "async_launched"`, `agentId`) or, for a
  foreground agent still running, the meta file's `toolUseId`. The agent is `running` until its
  tool result (foreground), its `<task-notification>` (`completed` → `done`, `failed`,
  `killed`/`stopped` → `stopped`; matched by `<tool-use-id>` or `<task-id>`), its
  `SubagentHandback` report, a `TaskStop` on it, or "Background agent … was stopped by the user";
  a `SendMessage` to a finished agent makes it `running` again. A running agent whose transcript
  has not changed for 30 minutes is `stopped`. While running, `tokens` is the last assistant
  turn's input + cache + output tokens, `toolCount` the tool calls so far and `lastActivity` the
  label of the latest tool call or text; once finished they come from the notification
  (`subagent_tokens`, `tool_uses`) or the foreground result (`totalTokens`,
  `totalToolUseCount`). The tool's `state` is `running` while the agent runs, `failed` when it
  failed, `done` otherwise. Only the running agents' transcripts are read (incrementally), and
  their growth fires `line.chat` like the main transcript. Codex and Gemini have no `agent`.
- `GET /v1/lines/{id}/chat/image?at=<imagesAt>&i=<index>` → the raw bytes of the `i`-th image
  of the user entry whose `imagesAt` is `at`, with its `Content-Type` (`image/png`, `image/jpeg`,
  `image/gif` or `image/webp`). 400 `at and i must be non-negative integers` · 404 unknown line or
  `no image {i} at {at}` (usually also when the transcript was rewritten, until the client resets) · 409 no transcript ·
  415 other media type · 422 bad base64. The daemon reads only that line, on demand; image data
  never goes through the chat page.
- `GET /v1/lines/{id}/chat/agents/{agentId}[?after=…|before=…][&limit=…]` → the subagent's own
  transcript in the same page shape plus `"agentId"`, with the same cursors (its `cursor` is
  independent of the main chat's). Its first entry is the prompt (`user`), the
  `SubagentHandback` report is an `assistant` entry, and nested subagents carry `agent` too.
  404 when `agentId` is not alphanumeric, the line is not Claude or the file does not exist.
- `GET /v1/lines/{id}/commands` → 200 `{"commands":[{"name":"/review-pr","description":"…","source":"project"|"user"}]}`:
  custom slash commands (Claude: `<cwd>/.claude/commands/**/*.md` then
  `~/.claude/commands/**/*.md`, a sub-directory gives `dir:name`; Codex:
  `~/.codex/prompts/*.md` as `/prompts:<name>`). `description` is the frontmatter
  `description:` or the first non-empty line. Built-in commands are a static list in the app.
- `GET /v1/lines/{id}/screen` → 200 `{"lineId","text","cols","rows","source":"term"|"tmux"}`:
  the visible screen of the line's terminal as plain text (a daemon term is replayed through a
  small VT grid; a tmux pane is captured) · 409 no live terminal. Used for the prompt card.
- `POST /v1/lines/{id}/keys` `{"keys":["esc"]}` → 200 `{"sent":n}`: named keys (`esc`,
  `enter`, `up`, `down`, `left`, `right`, `tab`, `shift-tab`, `ctrl-c`, `space`, `backspace`) or
  single printable characters, at most 32, typed in order · 400 bad key · 409 no live terminal.
- Sending a message reuses `POST /v1/broadcast` with one `lineIds` entry (§6a).

## 6a. Workspaces, folders, groups, broadcast

Persisted in the registry (`workspaces`, `workspace_tombstones`, `folders`, `groups`). Names are
trimmed, 1..=80 characters (400 otherwise). Unknown keys in bodies are ignored; a non-object body
is 400.

### Workspace

```json
{"id":"01J…","name":"overseer","path":"/home/u/src/overseer","station":"vm-dev","folderId":null,
 "pinned":false,"archived":false,"auto":true,"rank":1730000000000,
 "tabs":["line:…","term:…","browser:https://…"],"notifyIdle":false,"createdAt":"…","updatedAt":"…"}
```

- `path` is unique, absolute and canonical (symlinks resolved, no trailing slash); any directory
  works, not only a repository root. `station` is the daemon's station id (read-only).
- `auto`: created by the daemon from a project of `GET /v1/projects`.
- `missing`: the directory no longer exists (renamed, deleted, unmounted). Read-only.
- `tabs` and `notifyIdle` are opaque to the daemon: it stores them and never acts on them (the
  app prunes tab keys it cannot resolve and raises the idle notification). The app writes
  `line:<id>`, `term:<id>`, `browser:<url>` and `review:<lineId>`; other clients must keep keys
  they do not know.

Routes:

- `GET /v1/workspaces` → 200 `[Workspace]` ordered `pinned` desc, `rank`, `name`. It reads the
  daemon's last scan (a scan only when that cache is empty). Before answering, every project of
  `GET /v1/projects` without a workspace and without a tombstone gets an `auto` workspace
  (`name` = project name, `rank` = now in ms + index), each one announced by `workspace.created`;
  missing projects get none. `auto` workspaces whose directory is missing are left out of the
  answer (kept in the registry, back when the directory is); workspaces created by the user are
  returned with `"missing": true` and the app shows them as "folder missing".
- `POST /v1/workspaces` `{path, name?, folderId?, pinned?}` (`path` absolute or `~/…`) → **201**
  Workspace; **200** the existing workspace with the same path (unarchived if it was archived);
  **400** path missing, relative or not a directory, unknown `folderId`, bad name. Clears a
  tombstone on that path.
- `PATCH /v1/workspaces/{id}` any of `{name, folderId (null = top level), pinned, archived, rank,
  tabs: [string], notifyIdle}` → 200 · 400 · 404.
- `DELETE /v1/workspaces/{id}` → 200 `{"deleted": id}` · 404. It leaves a tombstone on the path,
  so the workspace is not re-created automatically. Every deleted workspace leaves one, not only
  `auto` ones: deleting a manual workspace of a project would otherwise bring it back as `auto`.
  It also clears every explicit assignment to it (lines and terms, see Membership).
- `PUT /v1/lines/{id}/workspace` and `PUT /v1/terms/{id}/workspace` `{"workspaceId": id | null}`
  assign a line (stored in the registry) or a term (kept across hot upgrades) to a workspace,
  overriding the derived membership; `null` goes back to it. Lines → 200
  `{"lineId", "workspaceId"}`, terms → 200 Term (with a `term.updated` event); 400 bad body; 404
  unknown line, term or workspace.

### Folder

`{"id","name","parentId":null|"<top-level folder id>","rank","collapsed":false,"createdAt","updatedAt"}`,
one level of nesting.

- `GET /v1/folders` → 200 `[Folder]` ordered `rank`, `name`.
- `POST /v1/folders` `{name, parentId?, rank?}` → 201; 400 when `parentId` is unknown or not a
  top-level folder.
- `PATCH /v1/folders/{id}` any of `{name, parentId, rank, collapsed}` → 200 · 404 · 400 when the
  parent is not a top-level folder, is the folder itself, or the folder has sub-folders.
- `DELETE /v1/folders/{id}` → 200 `{"deleted": id}` · 404. Its workspaces and sub-folders move to
  its parent (or the top level), each with a `workspace.updated` / `folder.updated` event.

### Group

`{"id","name","rank","members":["line:<id>","term:<id>"],"notifyIdle":false,"createdAt","updatedAt"}`.

- `GET /v1/groups` → 200 `[Group]` ordered `rank`, `name`.
- `POST /v1/groups` `{name, members?, rank?}` → 201 · 400.
- `PATCH /v1/groups/{id}` any of `{name, rank, members (replaces), add: [key], remove: [key],
  notifyIdle}` → 200 · 400 · 404; `members`, then `add`, then `remove`. Keys are `line:<id>` or
  `term:<id>` (400 otherwise); duplicates are dropped, order is kept. Members are not checked
  against live lines or terms.
- `DELETE /v1/groups/{id}` → 200 `{"deleted": id}` · 404.

### Membership

The same rule in the daemon (broadcast to a workspace) and in the app:

0. A line or term with an explicit assignment (`local.workspaceId` on the line, `workspaceId`
   on the term) belongs to that workspace while it exists; an unknown id is ignored and the rules
   below apply.
1. A line linked to a task (`task.lineId == line.id`) belongs to the workspace whose `path` ==
   `task.project`, when there is one.
2. Else let P be the workspace (archived or not) whose `path` is the longest path-boundary prefix
   of the line's `worktreePath` (else `cwd`), and R the workspace whose `path` is the main root of
   a project (`GET /v1/projects`) with the same `repo` as the line.
3. If R exists and P is not inside R (P is none, or `P.path` is neither `R.path` nor under it),
   the line belongs to R; else to P (possibly none). A workspace on `$HOME` therefore does not
   capture the worktrees of a repository that has its own workspace.

Only lines of this station. A term without `lineId`: `taskId` → the task's project workspace,
else the longest prefix of its `cwd`. A task: the workspace whose `path` == `task.project`.

### Broadcast

`POST /v1/broadcast`
`{text, lineIds?: [id], termIds?: [id], workspaceId?, groupId?, onlyIdle?: true, queue?: false}`

- Exactly one source: `lineIds`/`termIds` (either or both), `workspaceId` or `groupId` (400
  otherwise). `text` is trimmed, non-empty, at most 64 KiB (400). Unknown `workspaceId` or
  `groupId` → 404.
- Targets: the given ids; a group's members; for a workspace, its **alive** lines and its
  non-exited terms without a line. Duplicates are dropped.
- → 200 `{"id":"<broadcast id>","results":[{"target":"line:<id>"|"term:<id>","result":"sent"|"skipped"|"queued"|"refused","reason"?:"…"}]}`
  in target order.

Per target:

- Channel: a line is typed into through its live daemon term (`local.termId`, or a term whose
  `lineId` is the line), else its tmux pane (`tmux load-buffer` into a buffer named after the
  broadcast, `paste-buffer -p -d`, then `send-keys Enter`), else refused `no terminal`. A term
  target is typed into directly; refused `exited` once it has exited. Unknown ids: refused
  `unknown line` / `unknown term`.
- State, from the daemon's last scan (up to one interval stale): a held, hung-up or not alive line
  → refused `not running`. `needsInput` → not idle, reason `needs input` (typing would answer a
  permission prompt). Agents with a known busy state (hooks, session files): `busy` → reason
  `busy`. Otherwise (no known busy state, shell terms, term-only agents): busy when the term
  printed something in the last 1.5 s; no known state and no term (tmux) → idle.
- Idle → typed like a task prompt (bracketed paste when the program enabled it, else newlines
  become spaces; Enter 300 ms later) → `sent`. A failed write → refused with the error.
- Not idle: `onlyIdle: false` → sent anyway; else `queue: true` → `queued`; else `skipped` with
  the reason.
- Queued deliveries live in daemon memory (lost on restart). Each scan tick types the ones whose
  target became idle and drops the ones older than 1 h or whose target is now refused (gone,
  exited, not running); each outcome is a `broadcast.delivered` event.
- Deliveries run in parallel and the request returns after all of them (about 300 ms).
- `actions.log` gets `{"action":"broadcast","id","chars":N,"results":[…]}`, never the text.

## 6c. Organize sessions (ADR 0038)

A **proposal** is a reviewed plan to tidy sessions, workspaces, folders, groups and tasks. Nothing
is applied until the user checks actions and calls `apply`. Stored in the registry table
`proposals` (last 20 kept).

### Proposal (version 1)

```json
{"id":"…","createdAt":"…","status":"pending_review","version":1,"source":"deterministic",
 "taskId":"…","summary":"…",
 "actions":[{"id":"a1","kind":"create_folder","name":"acme","ref":"f1","label":"…"},
            {"id":"a2","kind":"move_workspace","workspaceId":"…","folder":"f1"},
            {"id":"a3","kind":"hold","lineId":"…"}],
 "pickUp":[{"lineId":"…","summary":"…","why":"dirty","title":"…","branch":"…","repo":"…",
            "workspaceId":"…","edited":3,"todos":1}],
 "reasons":{"a3":"idle for 41 days"},
 "results":[{"actionId":"a3","kind":"hold","status":"done","detail":"…"}]}
```

- `status` ∈ `pending_review | applied | dismissed`; `source` ∈ `agent | deterministic`;
  `taskId` (agent plans only), `results` (after an apply) and the pick-up details are optional.
- Kinds (closed set): `create_folder {name, ref}`, `create_workspace {path, folder?, pinned?}`,
  `move_workspace {workspaceId, folder}`, `create_group {name, lines: [lineId]}`, `hold {lineId}`,
  `close {lineId}`, `pin {lineId}`, `archive_workspace {workspaceId}`,
  `set_task_status {taskId, status ∈ pending | in_testing | merged | completed}`, `track {lineId}` (creates
  the session task of a line older than `sessionTasksDays`). `folder` is the `ref` of an earlier
  `create_folder` or an existing folder id. `label` is filled by the daemon for display.
- `pickUp.why` ∈ `dirty` (the agent edited files) `| todos` (open todo items) `| recent`.
- Validation → 400 `{"error","problems":["…"]}`: unknown kind, more than 500 actions or 40
  pick-ups, duplicate action ids, unknown line/workspace/task/folder ids, a `ref` that is not an
  earlier `create_folder`, `close` of a busy or `needsInput` line.

### Routes

- `POST /v1/organize/deterministic` → 201 Proposal built offline from cached lines and cached
  merged state (never waits on git): a workspace per repository whose recent lines have none; a
  folder per owner (repo owner or parent directory) when there are at least two owners; a group per
  ticket key shared by two or more recent lines; alive lines idle more than 30 days → `hold`;
  `cwdMissing` → `close` (unchecked by default in the app); tasks whose branch landed →
  `set_task_status completed`; workspaces with no activity for 30 days and no open task →
  `archive_workspace`; up to 40 pick-ups, ranked `dirty` > `todos` > `recent`, then last activity.
- `POST /v1/organize/proposals {plan, taskId?}` → 201 Proposal (`source: "agent"`): records an
  agent plan (the MCP `propose_organization` tool) and completes the configurator task `taskId`.
- `GET /v1/organize/proposals` → the newest 20, newest first. `GET /v1/organize/proposals/{id}`.
  `DELETE /v1/organize/proposals/{id}` → the proposal, `dismissed`.
- `POST /v1/organize/proposals/{id}/apply {actionIds: [id]}` → 200
  `{"proposalId","results":[{"actionId","kind","status":"done"|"skipped"|"failed","detail"?}]}`.
  Runs only the given actions, in plan order, through the existing functions (workspace, folder and
  group create/patch, hold and close with their checks, the pin mark, task patch, session task
  insert). Idempotent: an action already in effect is `skipped`; a failure does not stop the
  others. A `folder` ref whose `create_folder` was not applied fails. Lines of another station:
  hold and close go through the relay, `failed` when the station is offline; pin and track are
  refused. `actions.log` gets `{"action":"organize","proposalId","results":[…]}`.
- `POST /v1/organize/start {agent, model?, effort?}` → 201 `{"task": Task, "mcp": "configured"|"manual"}`:
  starts a configurator task (`origin: "configurator"`, effort `high` by default, run in place in
  `<state>/organize`, not a git repository), like the board organizer. Claude gets
  `--mcp-config` with the `overseer mcp` server on its command line (nothing written to disk);
  other agents need `overseer mcp` registered by the user (`mcp: "manual"`). 409 when the agent
  is not installed or a configurator already runs.
- `GET /v1/organize/sessions?project=&sinceDays=&limit=` (limit ≤ 300, default 100) →
  `{"total": n, "sessions": [...]}`, newest first, trimmed sessions from the cached scan, old
  ones included: `{id, agent, title?, repo, branch, worktreePath, cwdMissing,
  state, lastActivity, ticket?, pinned, workspaceId?, taskId?, taskStatus?}`.
- `GET /v1/organize/layout` → `{"workspaces":[Workspace],"folders":[Folder],"groups":[Group]}`.
- `GET /v1/organize/projects` → `[Project]` like `GET /v1/projects`, from the cached scan
  (MCP list_projects).

Event `organize.proposal` `{"proposal": Proposal}` on create, apply and dismiss.

## 7. Events

`GET /v1/events` (SSE, existing envelope `{"id","type","at","data"}`) adds:

| type | data |
|---|---|
| `term.created` | `{"term": Term}` |
| `term.updated` | `{"term": Term}` (resize, line linked) — addition to the original spec |
| `term.exited` | `{"term": Term}` |
| `task.created` | `{"task": Task}` |
| `task.updated` | `{"task": Task}` |
| `task.deleted` | `{"id": "<task id>"}` |
| `task.message` | `{"message": Message}` |
| `profiles.changed` | `{}`: refetch `GET /v1/profiles` |
| `line.activity` | `{"lineId": "…"}` — the line's transcript grew; debounced to at most one per line per 500 ms. Fetch `/activity?since=`. |
| `line.chat` | `{"lineId": "…", "cursor": <rev>}` — the transcript of a line whose chat was opened (§6b) grew; same debounce as `line.activity`. Fetch `/chat?after=<your cursor>` when `cursor` is newer; `cursor: 0` means the daemon dropped its cache for that chat (unread for 15 minutes or evicted): refetch and expect `reset`. |
| `git.changed` | `{"project": "<path>"}` — refs, `HEAD` (of any worktree) or `packed-refs` changed, or a background PR lookup finished (§12) |
| `worktree.created` | `{"worktree": Worktree}` (§13) |
| `worktree.updated` | `{"project": "<path>"}` — sizes, PR states or a branch of the project changed; refetch the list |
| `worktree.removed` | `{"project": "<path>", "id": "<worktree id>"}` |
| `workspace.created` / `workspace.updated` | `{"workspace": Workspace}` |
| `workspace.deleted` | `{"id": "<workspace id>"}` |
| `folder.created` / `folder.updated` | `{"folder": Folder}` |
| `folder.deleted` | `{"id": "<folder id>"}` |
| `group.created` / `group.updated` | `{"group": Group}` |
| `group.deleted` | `{"id": "<group id>"}` |
| `organize.proposal` | `{"proposal": Proposal}` (§6c) — created, applied or dismissed |
| `broadcast.delivered` | `{"id": "<broadcast id>", "target": "line:…" or "term:…", "result": "sent" or "dropped", "reason"?: "…"}` — a queued delivery was typed or dropped (`expired`, `not running`, …) |
| `port.opened` | `Port` (§7a) — a process in a daemon term's process tree started listening on a TCP port |
| `port.closed` | `Port` (§7a) — that listener went away (or its term exited) |
| `usage.updated` | `{}` — usage totals or plan limits changed (§7c); debounced, only while a client asked for usage recently. Refetch `/v1/usage`. |
| `daemon.upgraded` | `{"from","to","epoch"}` — emitted by the new daemon right after a hot upgrade; best effort, since clients are disconnected during the exec: watch `epoch` in `/v1/version` instead |

## 7a. Ports

The daemon watches the process trees of its live terms (each term's pid and all descendants) for
TCP sockets in `LISTEN` state, at most every 2 s: Linux reads `/proc/net/tcp` and
`/proc/net/tcp6` (state `0A`) and maps socket inodes to pids through `/proc/<pid>/fd`; macOS runs
`lsof -nP -a -iTCP -sTCP:LISTEN -p <pids> -F pcn`. Windows reports nothing (no detector yet). One
entry per port number; when a port is bound on several addresses the widest one is reported
(`0.0.0.0`/`::` over loopback).

### Port

```json
{"port": 5173, "addr": "127.0.0.1", "pid": 4242, "process": "node",
 "termId": "01J…", "lineId": "…", "project": "/home/me/src/web", "since": "2026-10-29T09:00:00Z"}
```

- `addr`: `127.0.0.1`, `::1`, `0.0.0.0`, `::` or a specific address.
- `process`: short command name of the listening pid. `lineId` is omitted when the term has no
  line; `project` is the repository root of the term's cwd when there is one, else the cwd.

### Routes

| Method | Path | Result |
|---|---|---|
| GET | `/v1/ports` | `Port[]` sorted by port |
| POST | `/v1/ports/{port}/kill` | Body `{"pid":N,"confirm":true}`. **202** Port (SIGTERM sent to `pid`; `port.closed` follows) · 400 bad port, no `pid` or no `confirm:true` · 403 not allowed · 404 `pid` is not the process listed for that port · 410 already exited · 501 Windows |

Additive: protocol stays 1. A client must ignore `port.*` events it does not handle.

## 7b. System monitor

The daemon samples its machine every 2 s, **only while someone is watching**: a client holding
`GET /v1/system/stream` open, or one that called `GET /v1/system` in the last 5 s. Otherwise no
sampler thread exists. Samples go into a ring of the last 300 (10 minutes); the ring survives a
pause, so the history can have gaps (the app breaks its charts on gaps).

Linux reads `/proc/stat`, `/proc/meminfo`, `/proc/net/dev`, `/proc/diskstats`,
`/proc/self/mounts` + `statvfs` (every 30 s), `/proc/loadavg`, `/proc/uptime`,
`/proc/pressure/*` when present and `/proc/<pid>/stat` once per process. macOS runs `sysctl`,
`vm_stat`, `ps`, `netstat -ibn` and `df -kP` (no per-core CPU, no disk I/O; CPU is the sum of the
processes' `%cpu`). Other platforms answer `{"supported": false}`.

Filtered out by default: loopback and virtual interfaces (`docker*`, `veth*`, `br-*`, `virbr*`,
`cni*`, `tap*`…) and interfaces that never moved a byte; loop/ram/zram/dm devices and partitions;
pseudo, overlay, squashfs, tmpfs and network filesystems (a hung NFS server cannot block the
sampler), mounts under `/proc`, `/sys`, `/dev`, `/run`, `/snap`, `/var/lib/docker`; one row per
device.

### Sample

```json
{"t": 1793308481579, "platform": "linux", "supported": true, "uptimeSec": 60784,
 "load": [0.52, 0.58, 0.59],
 "cpu": {"total": 23.4, "cores": [30.1, 16.0]},
 "pressure": {"cpu": 1.2, "memory": 0.0, "io": 0.4},
 "mem": {"total": 17179869184, "used": 10522669056, "available": 6657200128,
         "cached": 4009752576, "swapTotal": 4294967296, "swapUsed": 160563200},
 "net": [{"name": "eth0", "rx": 18837, "tx": 119644}],
 "disks": [{"name": "nvme0n1", "read": 2041, "write": 28572}],
 "fs": [{"mount": "/", "device": "/dev/nvme0n1p2", "fstype": "ext4",
         "total": 512000000000, "used": 431000000000, "avail": 81000000000}],
 "top": {"cpu": [{"pid": 4242, "name": "claude", "cpu": 23.5, "rss": 471859200}], "mem": […]},
 "agents": [{"termId": "01J…", "lineId": "…", "agent": "claude", "title": "…",
             "cwd": "/home/me/src/overseer", "pid": 4242, "procs": 3,
             "cpu": 3.2, "rss": 471859200}]}
```

- Percentages are 0–100 per machine for `cpu.total`/`cores`, per core for processes (a process
  using two cores shows 200). `cpu.total` is `null` on the first sample after a pause.
- Rates (`net`, `disks`) are bytes per second; sizes are bytes. `top` lists 10 processes by CPU
  and 10 by RSS. `agents` is one entry per live terminal: its process and all descendants summed.
- `pressure`, `load`, `uptimeSec` are `null` when the platform has no source.

### Point (history)

```json
{"t": 1793308481579, "cpu": 23.4, "memUsed": 10522669056, "memTotal": 17179869184,
 "swapUsed": 160563200, "rx": 18837, "tx": 119644, "read": 2041, "write": 28572}
```

### Routes

| Method | Path | Result |
|---|---|---|
| GET | `/v1/system?since=<t>` | `{intervalMs, windowMs, running, current: Sample \| null, history: Point[]}`; `history` holds the points newer than `since` (all of them without it). Starts the sampler for 5 s and waits up to 1.5 s for a fresh sample |
| GET | `/v1/system/stream` | SSE: `system.sample` events (`data` = the event envelope of §7 with `data` = Sample) every 2 s while connected; the sampler stops when the last stream closes |

`system.sample` is never sent on `/v1/events`. Additive: protocol stays 1; an older daemon
answers 404 and the app hides the monitor.

## 7c. Usage

The daemon indexes token usage from the agents' own session files, incrementally (a byte offset
per file, so a sweep only reads appended lines) into `<state>/usage.db`, kept 31 days:

- Claude Code: `message.usage` of assistant lines in `~/.claude/projects/*/<session>.jsonl` and
  `…/<session>/subagents/**/*.jsonl` (counted in the parent session). Lines repeating a
  `message.id` count once. Plan limits are not persisted by Claude Code; a recent
  `rate_limit` API error is reported, as `limits.hit`, until a later reply succeeds. With
  `claudeLimits=1` (below) the daemon also fetches the real plan limits, see "Claude plan limits".
- Codex: `token_count` events of `~/.codex/sessions/**/rollout-*.jsonl`, as the difference of
  `info.total_token_usage`; the model comes from `turn_context`. `rate_limits.primary` /
  `secondary` (`used_percent`, `window_minutes`, `resets_at`) of the newest event become the
  agent's windows.
- Gemini CLI: per-message `tokens` and `model` of `~/.gemini/tmp/*/chats/session-*.json(l)`,
  when present (recent `.jsonl` sessions may carry none).

Nothing is indexed until a client calls `GET /v1/usage`; the first call starts a background pass
over files modified in the last 31 days (`indexing: true` until it ends, then `usage.updated`).
After that a sweep runs only when a live session's transcript grows or a client asks after 60 s.

| Method | Path | Result |
|---|---|---|
| GET | `/v1/usage?range=today\|7d\|30d&by=agent\|project\|model\|session&tzOffset=<min>&line=<id>` | `Usage` |

`tzOffset` is minutes east of UTC (default 0); days are local days in that offset (data is kept
in 15-minute buckets). `line` restricts to one line's session. Defaults: `today`, `agent`.

```json
{"range": "7d", "by": "project", "tzOffset": 60, "from": "…", "to": "…", "bucketMs": 900000,
 "indexing": false,
 "totals": Totals,
 "days": [{"day": "2026-10-29", "usage": Totals}],
 "groups": [{"key": "overseer", "project": "overseer", "usage": Totals, "lastAt": "…"}],
 "agents": [{"agent": "codex", "today": Totals,
             "limits": {"source": "codex", "observedAt": "…", "plan": "plus", "hit": null,
                        "windows": [{"key": "5h", "windowMinutes": 300, "usedPercent": 29,
                                     "resetsAt": "…", "stale": false}]}}]}
```

- `Totals`: `{"input","output","cacheRead","cacheWrite","total","cost","costPartial","messages","activeMs"}`.
  `input` excludes cached input for every agent; `cost` is an estimate in USD from the static
  per-model price table in `src/usage.rs` (`null` when no model is priced, `costPartial` when
  some are not). `messages` counts API responses; `activeMs` sums gaps of at most 5 minutes
  between them (subagents excluded).
- `groups`: sorted by `total`, at most 50 sessions or 100 other groups. Session groups carry
  `agent`, `sessionId`, `lineId` (when the session is a known line), `title` and `project`
  (the line's repository, else the basename of the session's cwd).
- `windows`: `key` is derived from `windowMinutes` (`5h`, `wk`, `30d`, …); a window whose
  `resetsAt` has passed is `stale` with `usedPercent` 0. `windows` is empty when the agent does
  not expose limits locally.

Claude plan limits (`claudeLimits=1`, sent while the app setting "Fetch Claude plan limits" is on):
the daemon reads the Claude Code login it runs next to (`$CLAUDE_CONFIG_DIR/.credentials.json`, else
the macOS Keychain item `Claude Code-credentials`) and calls `GET https://api.anthropic.com/api/oauth/usage`
(`Authorization: Bearer <accessToken>`, `anthropic-beta: oauth-2025-04-20`) through `curl` with a 5 s
timeout; the token goes through curl's stdin, never argv, and neither it nor the response is logged.
`limits[]` entries `session` and `weekly_all` become the `5h` and `wk` windows (a model-scoped weekly
limit is added as `wk <model>` once used); `plan` is the login's `subscriptionType`. A fetch happens in the
usage worker at most every 5 minutes, only while a client asked with `claudeLimits=1` in the last
15 minutes and is subscribed to events, and a Claude session wrote a reply in the last hour. The daemon
never refreshes the OAuth token: an expired one (by `expiresAt`, or a 401/403) yields
`"auth": "expired"` and the last known windows; other failures yield `"auth": "unavailable"`.
Without `claudeLimits=1` the Claude entry has no `auth` and no fetched windows.

Claude plan limits from the status line (no flag needed): Claude Code passes
`rate_limits.five_hour` / `seven_day` (`used_percentage` 0–100, `resets_at` in epoch seconds) to
its status line command. `overseer statusline-tap --state <dir> [-- <original command>]` reads
that JSON from stdin, writes the windows to `<state>/claude-statusline.json` (atomically, only when
they change or every minute), then runs the original command through `sh -c` with the same stdin
and exits with its status; its stdout is passed through untouched. When that file exists the Claude
entry carries its `5h` / `wk` windows with `"via": "statusline"` (preferred over a fetched result
unless the fetch is newer), and the OAuth fetch is skipped while the file is less than an hour old.
The usage worker emits `usage.updated` when the file changes.

| Method | Path | Result |
|---|---|---|
| GET | `/v1/usage/statusline` | `{path, installed, command}`: the Claude settings file, whether the tap wraps its `statusLine`, and the user's own command (unwrapped) or `null` |
| POST | `/v1/usage/statusline/install` | body `{dryRun?}`; wraps `statusLine.command` (or adds a tap-only status line) as `[ -x <bin> ] && exec <bin> statusline-tap --state <dir> -- '<original>' \|\| exec sh -c '<original>' #overseer-statusline`; answers the status plus `changed`, `diff`, `backup` (settings copied to `<state>/backups/<flattened path>.bak-<stamp>` before writing, never beside the user's file). 409 with `error` when the file is not a JSON object or the status line is not a command |
| POST | `/v1/usage/statusline/uninstall` | same shape; restores the original command (or removes a tap-only status line) and deletes `<state>/claude-statusline.json` |

Additive: protocol stays 1. Older daemons answer 404; the app then hides usage.

## 7d. Uninstall

`overseer uninstall [--apply]` and `POST /v1/uninstall` undo everything Overseer put on the machine. A dry run (the default) changes nothing and is the "what does Overseer leave on my system" report.

| Method | Path | Body | Answer |
|---|---|---|---|
| POST | `/v1/uninstall` | `{dryRun?}`; applies only when `dryRun === false` | `{dryRun, removed: [{kind, path}], kept: [{kind, path, reason}], errors: [{kind, path, error}]}` |

`kind` is one of `service` (systemd unit or launch agent), `linger` (only when Overseer turned it on, recorded in `<state>/linger-enabled-by-overseer`), `hook` (hook entries and plugin files; a user's own entries and keys stay), `statusline` (the original status line command restored), `backup` (`<file>.bak-<12 digits>` left beside the agents' config files by earlier versions), `dir` (empty directories Overseer created, `~/.overseer/{bin,server,u,tasks,reviews,worktrees}` and `~/.overseer` itself when nothing else is in it), `worktree` and `branch` (clean Overseer worktrees and their `sb/*`, `task/*` or `review/pr-*` branches; dirty, unpushed, locked or in-use worktrees are in `kept` with the reason, as are branches with unmerged commits), `appData` (the desktop app's data, cache, config and log folders for `io.github.basilelt.overseer`) and `stateDir` (the daemon state directory, last).

On apply the answer is sent first; the daemon then deletes its state directory, boots out its launch agent on macOS and exits with status 0 (so `Restart=on-failure` and `KeepAlive.SuccessfulExit=false` do not restart it). `overseer uninstall` posts to the running daemon and, when none answers, runs the same steps in-process; it prints one line per row and exits 1 when `errors` is not empty. Backups taken from now on (hooks, status line, subagent profiles) live in `<state>/backups/`.

## 8. Server mode (remote)

`overseer daemon --detach` prints the socket path on stdout and exits 0, whether it started
the daemon (detached with `setsid`, stdio to `<state>/daemon.log`, pid in `<state>/daemon.pid`)
or one was already running. It returns once the socket accepts connections (10 s max, else exit
1). `overseer socket-path` prints the same path without starting anything.

The app connects with **one** `ssh -T <host> 'exec sh -c …'` (ADR 0028). The bootstrap script,
in that single session:

1. prints `SB:HELLO <uname -s> <uname -m> <installed version or ->` (lines before it, e.g. from
   shell rc files, are ignored);
2. reads one control line from stdin: `RUN`, or `INSTALL <size> <sha256>` followed by exactly
   `<size>` bytes of the binary for that platform (Linux binaries are static musl, `make dist`),
   written to `~/.overseer/server/<version>/overseer` (temp file, sha256 checked, mode
   0755, `mv`). Failures print `SB:ERROR <message>` and exit non-zero;
3. runs `overseer daemon --detach` (output to stderr), then `exec overseer proxy`.

## 8a. `overseer proxy` (stdio multiplexer)

`overseer proxy` writes the line `SB:MUX 1\n`, then speaks binary frames on stdin/stdout
until stdin closes (then it closes every stream and exits 0). Frame = 9-byte header + payload:

| bytes | field |
|---|---|
| 0 | type |
| 1–4 | stream id, u32 big-endian (0 = connection-level) |
| 5–8 | payload length, u32 big-endian, ≤ 65536 (a larger length is a protocol error: close) |

| type | name | direction | payload |
|---|---|---|---|
| 1 | `OPEN` | app → proxy | UTF-8 target: `daemon` (the daemon socket, as `socket-path`) or `tcp:<port>` (connects to `127.0.0.1:<port>`, then `[::1]:<port>`) |
| 2 | `OPEN_OK` | proxy → app | empty |
| 3 | `OPEN_ERR` | proxy → app | UTF-8 message; the stream id is free again |
| 4 | `DATA` | both | bytes (≤ 32768 per frame) |
| 5 | `WINDOW` | both | u32 BE: credit the peer may additionally send on that stream |
| 6 | `EOF` | both | empty: the sender will send no more DATA (half-close) |
| 7 | `CLOSE` | both | empty: the stream is gone (reset); no reply expected, later frames for it are ignored |
| 8 | `PING` | both | ≤ 64 bytes, stream 0 |
| 9 | `PONG` | both | the PING payload echoed, stream 0 |

- Stream ids are chosen by the app (non-zero, never reused while open). Only the app opens.
- Flow control: each side starts with a send credit of 262144 bytes per stream and may send DATA
  only within its credit; the receiver returns credit with `WINDOW` as it delivers bytes to the
  socket or consumer. One stalled stream never blocks the others.
- A stream ends when both sides sent `EOF`, or on `CLOSE`. Unknown frame types are ignored.
- The app sends `PING` every 15 s and treats 2 missed `PONG`s (or the `ssh` process exiting) as a
  dead link, then reconnects.

## 9. MCP (`overseer mcp`)

Stdio MCP server (JSON-RPC 2.0, protocol `2025-06-18`, tools only) for agents. It talks to the
local daemon socket. Tools: `list_tasks {project?, status?}`, `get_task {id}`,
`create_task {title, description?, project?, urgent?, important?, dependsOn?, inheritBranch?}` (always `pending`; sends
`origin: "agent"` and `parentId` = the agent's own task), `update_task_progress
{id?, done, total, note?}`, `set_task_status {id?, status}` — an agent may only move **its own**
task (`OVERSEER_TASK_ID`, set by the daemon when it starts the agent) and only to
`in_testing`. `id` defaults to `OVERSEER_TASK_ID`.

MCP review tools (§11, ADR 0030): `list_review_comments {status?, worktree?}` returns
`structuredContent: {"threads": [Thread]}` — `status` is a status or comma list (default `sent`),
`worktree` defaults to the worktree of the agent's own task, else the git top level of its cwd;
`resolve_review_comment {id, note}` marks a thread `addressed` with the note as the agent's reply.

MCP collaboration tools: `prioritize_task {id, urgent?, important?, rank?}` (PATCH, cannot change
the status), `set_task_dependencies {taskId, dependsOn, inheritBranch?}` (PATCH `dependsOn`; a
cycle, self-dependency or unknown task comes back as a tool error with the daemon's 400), `send_message {to, body}` (needs `OVERSEER_TASK_ID`; sent `from` the agent's own
task), `read_messages {id?}` returns `{"messages": [Message]}`, `find_overlaps {id?}` returns
`{"overlaps": [...]}`; `id` defaults to the agent's own task. Messages from other agents arrive in
the agent's input prefixed `[Overseer]`.

MCP organize tools (§6c, ADR 0038), read-only except the recorder: `list_sessions {project?,
sinceDays?, limit?}` (≤ 300) returns `{"sessions": [...]}`; `get_session_brief {id}` returns the
line's `goal`, `todos`, `editedFiles` and `lastMessage`; `list_projects`; `list_worktrees
{project}` returns `path, branch, merged, dirty, state, ticketKey` per worktree; `list_layout`
returns workspaces, folders and groups; `propose_organization {plan}` records a proposal and returns
`{"proposalId","status":"pending_review"}` — only when the caller's own task has origin
`configurator`. A configurator task is refused `create_task`, `set_task_status` and
`prioritize_task`.

Automatic overlap alert: when a task that is not an organizer or session task moves to `in_testing`
(turn end in the tick, or `PATCH` / `set_task_status`), the daemon computes its overlaps on a
background thread and, for each overlapping task, posts a message to both tasks (`from` absent) whose
delivered text reads `[Overseer] Overlap: task #A "title" and task #B "title" both change: f1, f2.
Agree who changes what with send_message (to: <id>).` (first 20 files). The registry `kv` key
`overlap:<minId>:<maxId>` stores the sorted file set joined by newlines; the alert is only repeated
when it changes. No schema change.

## 10. Additions and deviations from the original spec

- `GET /v1/tasks/{id}`, the `term.updated` event, `Task.error`, `workLog[].id/startedAt` and the
  top-level `at` of `/activity` are additions.
- `/start` waits for output silence instead of the `SessionStart` hook: hooks reach the registry
  only on the next scan tick, and a term has no screen capture to check the prompt.
- `review reject` without a message on a stopped agent → `pending` (otherwise the next tick would
  move it straight back to `in_testing`).
- MCP `create_task` always creates `pending` tasks (an agent cannot enqueue `auto` work), and
  `list_tasks` returns `structuredContent: {"tasks": [...]}` because MCP structured content must be
  an object.
- Flags were checked against `--help` of claude 2.1.289, codex 0.165.0, gemini 0.66.0, agy
  1.1.0, opencode 1.19.5, amp 0.0.1793260836 and copilot 1.0.48 (2026-10-29); cursor-agent is
  unverified. Resume by id for Gemini (`--resume <uuid>`) is unverified: its help documents
  `latest` or an index.
- Workspaces (§6a): `DELETE` leaves a tombstone for every workspace, not only `auto` ones; a
  broadcast to a workspace targets its alive lines and its terms without a line, not its held
  lines; unknown ids in `lineIds`/`termIds` are per-target refusals while an unknown
  `workspaceId`/`groupId` is 404; tmux deliveries use one named buffer per target so parallel
  deliveries cannot swap texts.

## 11. Live review (ADR 0030)

A GitHub-style review of a worktree's changes with inline comment threads that are sent to the
agent working there. Local to the station, stored in the registry (`review_threads`,
`review_comments`, `review_deliveries`, `review_viewed`; created with `CREATE TABLE IF NOT
EXISTS`). All additive: protocol stays 1.

**Target.** Every route takes exactly one of `worktree=<abs path>`, `line=<line id>`,
`task=<task id>` (query) or `worktree` / `lineId` / `taskId` (body). It resolves to the git top
level of the worktree (a task's `worktreePath`; for a line, its `worktreePath` or, when that
folder is gone, its live `cwd`, after a rescan if the cached line is stale). The innermost
repository wins: a line whose `cwd` is inside a submodule of its `worktreePath` reviews the
submodule, and a worktree of a submodule is reviewed as that submodule's repository. 400: none or not a
git worktree; 404: unknown line or task; **410**: the worktree folder no longer exists (moved,
renamed or removed), `{"error":"the worktree folder <path> no longer exists"}`.

**Base.** `?base=<ref>` if given (`baseSource: explicit`); else the ref the branch was created
from (`created`): `git config branch.<branch>.overseer-base` (written when Overseer creates
a worktree on a new branch), else the oldest entry of the branch's reflog
(`branch: Created from <ref>`, skipped when it is `HEAD`); else, for a task's worktree, the
branch checked out in the task's project (`task`); else, on the default branch (`origin/HEAD`,
else `main`/`master`), its upstream (`@{upstream}`, else `origin/<branch>`, `upstream`), so the
review shows the unpushed commits; else the local branch `HEAD` was first checked out from (the
worktree's `HEAD` reflog, `checkout: moving from <x> to <branch>`) when it is at least as close to
the branch tip as the default (`forkpoint`); else the default branch (`origin/<default>` when the
local one is behind it); else the first of `main`, `master`, `develop`, `trunk` that exists and is
not the current branch (`default`); else none.
The diff is the working tree (committed on the branch, staged, unstaged) against
`git merge-base HEAD <base>` (or `HEAD` without a base), plus untracked files
(`git ls-files --others --exclude-standard`) as added files.

**Submodules.** Diffs, `git status` and the watcher run with `--ignore-submodules=dirty`, so a
submodule with only uncommitted changes is not listed and is never scanned. A submodule whose
recorded commit changed is one file with `status: "submodule"`, no hunks, and
`"submodule": {"from": "<sha>"|null, "to": "<sha>"|null}` (null when it was added or removed);
nothing inside it is read. The app reviews it on its own with `worktree=<worktree>/<path>` and
`base=<from>`.

**Branch review.** `&branch=<local branch>` with any target reviews that branch's commits
against its base (`git merge-base <branch> <base>..<branch>`, no working tree, no untracked
files), read-only: `readOnly: true`, no `viewed` state, threads stay with worktrees. 404: not a
local branch.

### `GET /v1/review?worktree=|line=|task=&base=`

```json
{"worktree":"/w/x","branch":"task/7-fix","base":"main","baseSource":"created",
 "superproject":null,"mergeBase":"9f1c…","head":"4b2e…",
 "revision":"3fa94c1d2b6e8a70","truncated":false,"readOnly":false,
 "commits":[{"sha":"4b2e…","subject":"fix: x","author":"Alice","at":"2026-10-29T10:00:00Z"}],
 "recent":[],
 "files":[{"path":"src/a.rs","oldPath":"src/a.rs","status":"modified","binary":false,
   "additions":3,"deletions":1,"hash":"8d1f…","viewed":false,"truncated":false,
   "hunks":[{"header":"@@ -10,6 +10,8 @@ fn x()","oldStart":10,"oldLines":6,"newStart":10,"newLines":8,
     "lines":[{"kind":"ctx","old":10,"new":10,"text":"fn x() {"},
              {"kind":"del","old":11,"text":"    a();"},
              {"kind":"add","new":11,"text":"    b();"}]}]}]}
```

- `status` ∈ `added | modified | deleted | renamed | copied | untracked | submodule`; files sorted by path.
  `copied` only when the repository sets `diff.renames=copies` (the diff then runs with `-C`).
- `baseSource` ∈ `explicit | created | task | upstream | forkpoint | default`, null without a base
  (a branch review reports `default`, or `explicit` with `base`).
- `superproject`: the superproject's working tree when the reviewed repository is a submodule
  (`git rev-parse --show-superproject-working-tree`), else null.
- `hash` changes whenever the file's diff changes; `viewed` is true when the file was marked
  viewed at that hash (GitHub semantics: a viewed file that changes again is unviewed).
- A file with more than 3000 diff lines, or a binary one, has `hunks: []` and `truncated: true`
  (resp. `binary: true`); the whole answer stops adding hunks after 4 MiB (`truncated: true`).
- `revision` changes whenever any file does.
- `commits`: `mergeBase..HEAD`, newest first, 50 at most (the unpushed commits on the default
  branch). `recent`: the last 10 commits of `HEAD`, only when `files` and `commits` are both
  empty, so a clean branch shows its history instead of a blank review.

**Triage** (additive, computed on request, per-file part cached by `hash`, 512 files at most).
Every changed line gets one kind, first match wins: `rename` (a renamed or copied file without
changes; counted once, no hunk tag), `move` (a block deleted in one place and added in another,
whitespace ignored, at least 3 lines or 20 letters/digits; lock and generated files are not
sources), `format` (whitespace only, rewraps, blank lines), `imports` (a hunk that only reorders
the same import lines), `ident` (a line pair differing by one identifier, the same old → new pair
in 2 hunks or more), `generated` (lock files, snapshots, `@generated` in the first 5 lines),
`test` (test paths), `comment` (comment lines, docs files), else `logic`; truncated and binary
files are `unknown`. A moved block that was also edited keeps its edited lines as `logic`.

- `hunks[].tags`: runs of changed lines `{kind, from, to, ref?, sig?}`; `from`/`to` index the
  hunk's `lines` (`to` exclusive), runs never cross context lines. `ref: {path, hunk}` on `move`
  points to the other end. `sig: true` on a `logic` run with a definition line (fn, class, struct…).
- `files[].residual`: changed lines left to read (`logic`, `test`, `unknown`).
  `files[].risk` 0-100: `residual × (1 + 0.5 × signature lines) × weight` (tests 0.2, generated
  0), scaled so the riskiest file of the review is 100.
- `triage`: `{"changed": 412, "byKind": {"move": 120, "logic": 150, …}, "lowRiskPct": 62}`;
  low risk = `move`, `format`, `imports`, `ident`, `rename`, `generated`, `comment`.

### `GET /v1/review/graph?worktree=|line=|task=&branch=&base=`

The symbols the review touches, computed on request (the app asks only when its graph view is
open) and cached per `revision`:

```json
{"revision":"3fa94c1d2b6e8a70","truncated":false,
 "nodes":[{"id":"src/a.rs::Store::get","kind":"fn","name":"get","path":"src/a.rs","line":10,
   "endLine":42,"hunks":[{"path":"src/a.rs","hunk":0}],"add":5,"del":2,"tag":"logic","risk":80}],
 "edges":[{"from":"src/b.rs::run","to":"src/a.rs::Store::get","kind":"uses"},
          {"from":"src/b.rs","to":"src/a.rs::Store::get","kind":"imports"}],
 "order":["src/a.rs::Store::get","src/b.rs::run","src/b.rs"]}
```

- Definitions are found per language without a parser: Rust, TypeScript/JavaScript, Go (brace
  depth, skipping strings and comments), Python (indentation), Svelte (one `component` node per
  file). A changed line belongs to the innermost definition around it on the new side; deleted
  definitions come from the base side (`git cat-file --batch`, one process). Changes outside any
  definition make a `file` node (`id` = path).
- `uses`: a node's body names another changed symbol followed by `(`, `::`, `.` or `<` (same
  file first; short and common names ignored). `imports`: an import line of a changed file that
  resolves to another changed file (relative TS paths, `$lib`, Rust `mod`/`use`, Python modules).
- `order`: definitions before their callers (topological order on the edges), riskier and bigger
  first on ties; cycles are broken at the lowest-risk edge. Deterministic.
- At most 400 nodes, the lowest-risk ones dropped first (`truncated: true`).

### `GET /v1/review/file?worktree=…&path=src/a.rs&rev=`

`{"path":"src/a.rs","lines":["…"]}`: the file as it is in the working tree (or at commit `rev`,
4-64 hex digits, for a branch review), to expand the unchanged lines between hunks. 400 a path
outside the worktree or a bad `rev`; 404 missing; 409 binary or over 2 MiB.

### `GET /v1/review/worktrees?worktree=|line=|task=`

The worktrees of the target's repository (`git worktree list`) that have changes against their
base, plus the target itself, newest activity first (index, `HEAD` and last commit times):

```json
{"current":"/w/repo","worktrees":[{"path":"/w/repo/.claude/worktrees/fix-x","branch":"fix/x",
  "head":"4b2e…","base":"main","files":3,"dirty":2,"ahead":1,"agents":1,"current":false,
  "main":false,"at":"2026-10-29T10:00:00Z"}]}
```

`files` is the number of files `GET /v1/review` lists for that worktree, `dirty` the
`git status` entries, `ahead` the commits since the merge base, `agents` the live lines there.
Bare and prunable worktrees are skipped. Computed on request only; it does not add the listed
worktrees to the watcher.

### `GET /v1/review/branches?worktree=|line=|task=`

`{"base":"main","branches":[{"name":"fix/y","head":"…","at":"…","worktree":null,"subject":"…"}]}`:
local branches not merged into the default branch (`git for-each-ref --no-merged`), newest
first, 60 at most; `worktree` is where the branch is checked out, if anywhere.

### `GET /v1/review/bases?worktree=|line=|task=`

The refs the base picker offers, newest commit first, 200 at most:

```json
{"base":"origin/main","baseSource":"created","default":"main",
 "branches":[{"name":"main","remote":false,"head":"…","at":"2026-10-29T10:00:00Z","subject":"…"},
             {"name":"origin/release","remote":true,"head":"…","at":"…","subject":"…"}]}
```

`base`/`baseSource`: what `GET /v1/review` picks without `base`. `branches`: local
(`refs/heads`) and remote-tracking (`refs/remotes`) branches, without the current branch and the
remotes' `HEAD`. The app remembers the chosen base per worktree and sends it as `base`.

### `PUT /v1/review/viewed`

`{worktree|lineId|taskId, path, hash, viewed}` → 200 `{"path","hash","viewed"}`.

### Thread

```json
{"id":"rt-01JB…","worktree":"/w/x","path":"src/a.rs","side":"new","line":11,"endLine":12,
 "originalLine":11,"originalEndLine":12,"code":"    b();\n    c();","status":"sent",
 "outdated":false,"changed":false,"lineId":"vm-01J…","taskId":"01JB…",
 "delivery":{"id":"rd-01JB…","status":"queued","kind":"request_changes","target":"term:01JB…",
             "reason":"the agent is busy","at":"…"},
 "createdAt":"…","updatedAt":"…",
 "comments":[{"id":"rc-01JB…","author":"user","body":"Use b2.\n```suggestion\n    b2();\n```",
              "pending":false,"createdAt":"…","updatedAt":"…"}]}
```

- `side`: `new` (a line of the working tree; added or context) or `old` (a deleted line, numbered
  in the base). `line`..`endLine` (1-based, inclusive) is the current position; `original*` the
  position when the comment was written. `code` is the commented lines as they were then.
- `status`: `pending` (draft, never submitted) → `sent` (submitted; delivered or queued) →
  `addressed` (the agent replied through MCP) → `resolved` (the user confirmed, or applied a
  suggestion). The user may resolve or reopen at any time; reopening gives `sent` when the thread
  was ever submitted, else `pending`.
- **Anchors survive edits.** Each time the worktree changes, threads on the `new` side are
  re-anchored: the commented lines where they were → unchanged; the same lines elsewhere → moved
  (nearest match); the lines around them (FNV-1a hashes of the 3 lines before and after, trimmed)
  still in place with other content in between → moved there and `changed: true`; otherwise
  `outdated: true` (the thread keeps its last position).
- `changed: true` also means "changed since comment": the commented lines no longer read `code`
  after the thread was sent. The app shows it so the user can verify the agent's edit.
- `delivery` is the last submit that carried the thread (absent while `pending`).
- `comments[].author` ∈ `user | agent`; `pending` comments are drafts sent with the next submit.

### Comment routes

| Route | Body | Result |
|---|---|---|
| `GET /v1/review/comments?worktree=\|line=\|task=&status=` | | 200 `[Thread]` sorted by path then line; without a target, every worktree. `status` may be a comma list. |
| `POST /v1/review/comments` | new thread: `{worktree\|lineId\|taskId, path, side?, line, endLine?, body, base?}` (`base`: the base the review shows, for `side: old`) · reply: `{threadId, body}` | **201** Thread (the new comment is `pending`) · 400 empty body, line outside the file · 404 |
| `PATCH /v1/review/comments/{id}` | thread id: `{status: resolved\|open}` or `{status: addressed, note}` · comment id: `{body}` (pending comments only) | 200 Thread · 400 · 404 · 409 `addressed` on a pending thread, editing a sent comment |
| `DELETE /v1/review/comments/{id}` | | 200 `{"deleted": id}` · 404 · 409 not pending. Deleting the only comment of a thread deletes the thread. |
| `POST /v1/review/comments/{id}/apply` | | applies the thread's last ` ```suggestion ` block to the working tree in place of `line..endLine` → 200 Thread (`resolved`) · 409 no suggestion, outdated, or the lines no longer read `code` |
| `POST /v1/review/submit` | `{worktree\|lineId\|taskId, kind: comment\|request_changes\|approve, body?}` | 200 `{"delivery": Delivery, "threads": [Thread], "text": "<instruction>"}` · 400 · 404 |

### Submit and delivery

`submit` takes every pending comment of the worktree (new threads and replies), marks them sent,
formats **one** instruction and delivers it:

```
Code review: changes requested on 2 files (3 comments). Address each one. When done with one, call the overseer MCP tool resolve_review_comment with its id and a one-line note, if you have it.

1. src/a.rs:11-12 (id rt-01JB…)
   > b();
   > c();
   Use b2.
   Suggested change:
   --- a/src/a.rs
   +++ b/src/a.rs
   @@ -11,2 +11,1 @@
   -    b();
   -    c();
   +    b2();

Overall: please keep the public API.
```

`Delivery` = `{"id","status","kind","target","reason"?,"at","threads":[ids]}`, `status` ∈
`delivered | queued | none`. The target is the agent working in the worktree: the line given
(`lineId`), else the worktree's task line, else the most recent alive line whose
`worktreePath` is the worktree; its daemon term (bracketed paste + Enter, as for tasks) or its
tmux pane (`load-buffer`, `paste-buffer -p`, `send-keys Enter`). It is typed only when the agent
is idle at its prompt: the line is not busy and not waiting for input (a permission prompt is
never typed into); for lines without busy tracking, the term printed nothing for 1.5 s (a tmux pane: its screen did not change between two ticks). Otherwise
the delivery is `queued` (`reason`) and retried on every daemon tick until delivered; the
`review.delivery` event reports the change. `none`: nothing to send (`approve` without comments
or body). `request_changes` also moves the worktree's `in_testing` task back to `in_progress`
(as `review reject`). Every agent of §2a gets the typed instruction; MCP-capable agents can also
fetch threads and answer them (§9).

### Events

| type | data |
|---|---|
| `review.changed` | `{"worktree","files":["src/a.rs"]}` — a watched worktree changed (file system events: inotify on Linux, ReadDirectoryChangesW on Windows, FSEvents on macOS; debounced 500 ms, at most 2 s late; edits and commits inside submodules included; polled every 500 ms where events are not available, `OVERSEER_REVIEW_POLL=1` forces polling). Watched: worktrees with open threads and worktrees read through `/v1/review` or `/v1/review/comments` in the last 30 minutes (32 at most). |
| `review.thread` | `{"thread": Thread}` — created, replied, status, re-anchored, outdated or changed |
| `review.thread_deleted` | `{"id","worktree"}` |
| `review.delivery` | `{"delivery": Delivery, "worktree"}` |

## 12. Git graph

Projects and worktrees are addressed by their absolute path, percent-encoded as one path
segment (`encodeURIComponent("/home/u/src/x")`). `{project}` may be any directory inside a
repository; the daemon resolves its main worktree root. 400 when it is not in a git repository.

`GET /v1/projects/{project}/graph?limit=&before=&refs=` → 200

```json
{"project":"/home/u/src/x","repo":"github.com/Acme/x","base":"main",
 "head":{"sha":"8af2740…","branch":"main"},
 "refs":[{"name":"main","kind":"branch","sha":"8af2740…","upstream":"origin/main","ahead":0,"behind":2,
          "worktree":"/home/u/src/x"},
         {"name":"origin/main","kind":"remote","sha":"…"},
         {"name":"v1.0","kind":"tag","sha":"<peeled commit>"}],
 "overlays":[{"branch":"sb/claude/fix-login","sha":"…","worktree":"/home/u/.overseer/worktrees/x-1a2b3c4d/sb-claude-fix-login",
              "lineId":"vm-01J…","agent":"claude","state":"active","busy":true,"needsInput":false,
              "taskId":"01JB…","taskNumber":7,"taskStatus":"in_progress","ticketKey":"AS-1234",
              "pr":{"number":12,"state":"OPEN","isDraft":false,"url":"https://github.com/Acme/x/pull/12"}}],
 "commits":[{"sha":"…","parents":["…","…"],"author":"Ada","email":"ada@example.com",
             "at":"2026-10-29T09:00:00Z","subject":"Fix login"}],
 "next":"300"}
```

- `limit` 1..=2000 (default 300). `before` is the `next` cursor of the previous page (opaque);
  `next` is absent on the last page. `head`, `refs` and `overlays` are complete on every page.
- `refs=` selects the history: `all` (default: local and remote branches, tags, `HEAD`),
  `current` (`HEAD`), `agents` (`HEAD`, the base branch and the branches of the project's
  lines, tasks and managed worktrees) or a comma list of ref names.
- Commits come in `git log --date-order` order (every child before its parents). The lane
  layout is computed by the client (pure and incremental over pages); the daemon only sends
  parents.
- `kind` ∈ `branch | remote | tag`; `upstream`, `ahead`, `behind` and `worktree` (where the
  branch is checked out) are omitted when unknown. `head.branch` is `null` when detached.
- `base` is the project's base branch (§13). `overlays`: one entry per branch that has a
  Overseer line, task or managed worktree (`lineId`, `agent`, `state`, `busy`, `needsInput`
  from its newest line; `task*` from its task); `ticketKey` comes from the line or task, else
  the branch name; `pr` comes from the daemon's `gh` cache only (60 s): a branch not in the cache
  is looked up in the background and a `git.changed` follows; absent fields are omitted.

`GET /v1/projects/{project}/commits/{sha}` → 200

```json
{"sha":"…","parents":["…"],"author":"Ada","email":"ada@example.com","at":"…","committer":"Ada",
 "committedAt":"…","subject":"Fix login","body":"Longer text","refs":["main","origin/main"],
 "files":[{"path":"src/b.rs","oldPath":"src/a.rs","status":"R","added":3,"removed":1,"binary":false}],
 "diff":"diff --git a/src/a.rs b/src/b.rs\n…","truncated":false}
```

The diff is against the first parent (the empty tree for a root commit) with renames detected,
cut at 1 MiB (`truncated`). `status` ∈ `A M D R C T`. 400: `sha` is not 4–64 hex digits; 404:
unknown commit.

`POST /v1/projects/{project}/branches` `{name, sha}` → **201** `{"name","sha"}` · 400 invalid
name or commit · 409 the branch exists.

**Live refresh.** Each second the daemon fingerprints (mtime, size) `HEAD`, `packed-refs`,
`refs/**` and `worktrees/*/HEAD` of every project whose graph or worktrees were requested in
the last 10 minutes, and publishes `git.changed {project}` when that changes (at most once per
project per second).

## 13. Worktrees

Every agent session or task started from the app runs in its own worktree by default (the app
offers "run in current checkout" per start).

- **Store**: `<worktreeDir>/<repo-name>-<hash>/<dir>`; `worktreeDir` (`config.json`) defaults
  to `~/.overseer/worktrees` (outside every repository); `<hash>` = 8 hex digits of the
  FNV-1a 64 hash of the main worktree root path; `<dir>` = the branch with `/` turned into `-`.
- **Branches**: `sb/<agent>/<slug>` for agent sessions (`<slug>` defaults to `MMDD-HHMM` of the
  start, UTC), `task/<number>-<slug>` for tasks, `sb/<slug>` for a worktree created without an
  agent (`<slug>` defaults to the short sha of the base). A taken name gets `-2`, `-3`, …
- **Base**: the request's `base`, else the project's default branch: the target of
  `origin/HEAD`, else `main`, else `master`, else the main checkout's current branch. When
  `worktreeFetch` (`config.json`, default `true`) is on, `git fetch origin <default>` runs first
  (10 s timeout; on failure, e.g. offline, it is skipped for that repository for 10 minutes). The worktree starts from the local branch
  when it contains `origin/<default>` (or there is no remote-tracking branch), else from
  `origin/<default>`. New branches have no upstream.

### Worktree

```json
{"id":"/home/u/.overseer/worktrees/x-1a2b3c4d/sb-claude-fix-login","path":"(same as id)",
 "project":"/home/u/src/x","branch":"sb/claude/fix-login","head":"8af2740…","kind":"managed",
 "base":"main","state":"in_use","dirty":true,"changes":3,"ahead":2,"behind":5,"merged":false,
 "pushed":false,"unpushed":2,
 "lines":[{"id":"vm-01J…","agent":"claude","state":"active","busy":true,"needsInput":false}],
 "termIds":["01JB…"],"task":{"id":"01JB…","number":7,"title":"Fix login","status":"in_progress"},
 "ticketKey":"AS-1234","pr":{"number":12,"state":"OPEN","isDraft":false,"url":"…"},
 "sizeBytes":48123904,"lastActivityAt":"2026-10-29T09:00:00Z","locked":false}
```

- `kind`: `main` (the main checkout), `managed` (under `worktreeDir`, or the worktree of a
  task), `orphan` (any other worktree of the repository, created outside Overseer).
- `state`: `missing` (the directory is gone), `in_use` (an alive line or a running daemon term
  has its cwd in it, or its task is `in_progress`), `stale` (not in use, no uncommitted change,
  and merged or without activity for `worktreeStaleDays`, default 14), else `idle`.
- `dirty`/`changes`: uncommitted changes, untracked files included. `ahead`/`behind`: commits
  versus the base. `merged`: the branch tip is reachable from the base and the branch moved
  since it was created (its reflog), or its PR is `MERGED`. `pushed`: the tip is on a
  remote-tracking branch. `unpushed`: commits reachable from neither the base nor a
  remote-tracking branch.
- `sizeBytes` is computed in the background (cached 10 min, `null` until known; a
  `worktree.updated` follows). `pr` as in §12. `lastActivityAt`: the newest of the worktree's
  index, `HEAD` log and lines. `branch` is `null` when detached; other optional fields are
  omitted when unknown.

### Routes

| Route | Body | Result |
|---|---|---|
| `GET /v1/projects/{project}/worktrees` | | 200 `{"project","store","base","worktrees":[Worktree],"orphans":[Worktree]}`: `worktrees` = the main checkout, then the managed ones by newest activity |
| `POST /v1/worktrees` | `{project, branch?, base?, slug?, agent?}` | **201** Worktree. An existing local `branch` is checked out, `origin/<branch>` is checked out tracking it, otherwise `branch` is created from the base (`base` may be any commit, e.g. to check out a graph commit). 400 not a repository, invalid branch or base · 409 the branch is checked out elsewhere, git failed |
| `POST /v1/worktrees/{id}/cleanup` | `{dryRun?, project?}` (`dryRun` defaults to `true`) | 200 plan (below) · 404 not a worktree of a repository · 409 refused (real run) |
| `POST /v1/worktrees/{id}/merge-base` | `{}` | 200 Worktree after `git merge --no-edit <base>` · 409 uncommitted changes, or conflicts (the merge is aborted; the error names the files) |
| `POST /v1/worktrees/{id}/cherry-pick` | `{sha}` | 200 `{"commit":"<new sha>","worktree":Worktree}` after `git cherry-pick -x` (`-m 1` for a merge) · 400 bad sha · 409 uncommitted changes or conflicts (aborted) |

**Cleanup plan**:

```json
{"dryRun":true,"id":"/…/sb-claude-fix-login","refused":null,
 "remove":[{"kind":"worktree","path":"/…/sb-claude-fix-login"},{"kind":"branch","name":"sb/claude/fix-login"}],
 "keep":[{"kind":"branch","name":"task/7-x","reason":"not merged into main"}]}
```

Cleanup never loses work. It is refused (`refused` = the reason and `remove` empty; 409 on a
real run) for the main checkout, an `orphan`, a locked worktree, one `in_use`, one with
uncommitted changes (untracked files included; ignored files do not count), or one whose branch
has `unpushed` commits. Otherwise it removes the directory (`git worktree remove`, never
forced) and deletes the local branch only when `merged` or `ahead` is 0 — with no unpushed
commit, every commit of it is then in the base or on a remote; any other branch is listed in
`keep`. A `missing` worktree is cleaned by removing its administrative entry (what `git worktree
prune` does, for that one worktree only; `{"kind":"prune"}`). A real run publishes
`worktree.removed`. Orphans are listed, never touched. `project` (optional) names the
repository, which the daemon cannot find from a `missing` directory otherwise.

## Lifecycle (start at boot, restore after a reboot)

- `GET /v1/lifecycle` → `{"afterReboot": "agents"|"shells"|"nothing", "autostart": {"platform":
  "systemd"|"launchd"|"none", "supported", "installed", "enabled", "current", "supervised",
  "path", "linger": true|false|null, "lingerCommand"}}`.
- `PATCH /v1/lifecycle` with any of `{"afterReboot", "autostart": true|false, "linger": true}`
  answers the same object, plus `error` when a step failed (the other steps still apply). 400 on
  an unknown `afterReboot`. `autostart: true` writes and enables the unit (or LaunchAgent) without
  starting it; `false` disables and removes it; `linger: true` runs `loginctl enable-linger
  --no-ask-password` and never escalates.
- After a start whose boot id differs from the stored one (never with `--takeover`), the daemon
  recreates the saved terms with their ids before it serves requests, then publishes the usual
  `term.created` events.

