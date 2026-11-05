# Findings: Claude Code

Checked 2026-10-28 on the VM. Version: `claude --version` → `2.1.289 (Claude Code)`.
Binary: `~/.local/bin/claude` → `~/.local/share/claude/versions/2.1.289` (2.1.287 and 2.1.288 also
kept). cmux-tui also ships a shim `~/.local/share/cmux-tui/shims/claude` that runs
`cmux-tui agent claude-wrapper "$@"`.

## Commands run

```
claude --version ; claude --help ; claude agents --help
claude agents --json                       (keys only, values truncated)
ls ~/.claude ~/.claude/projects ~/.claude/sessions
jq on ~/.claude/sessions/<pid>.json        (structure)
jq -c '[.type, (keys|join(","))]' <one transcript> | sort | uniq -c
jq on ~/.claude/settings.json .hooks       (event → command, truncated)
ps --ppid <claude pid>
```

## Storage

- **Transcripts**: `~/.claude/projects/<cwd with non-alphanumerics → '-'>/<sessionId>.jsonl`
  (confirmed; 33 project dirs, 130 transcripts). Each line has `type`; message lines carry
  `sessionId`, `cwd`, `gitBranch`, `timestamp`, `version`, `entrypoint`. Metadata lines:
  `ai-title {aiTitle}`, `last-prompt`, `permission-mode`, `mode`, `bridge-session`.
  Overseer needs only: `sessionId`, `cwd`, `gitBranch`, first/last `timestamp`, `ai-title`.
- **Live registry (new, important)**: `~/.claude/sessions/<pid>.json`, one per running process:
  `pid, sessionId, cwd, startedAt (ms), procStart, version, kind ("interactive"), entrypoint,
  tmux ("cmux-562F5F55:@19.%19"), status ("busy" | "idle"), updatedAt, statusUpdatedAt, name,
  messagingSocketPath, bridgeSessionId`. Sibling `*.key` files are secrets: never read.
  `claude agents --json` prints the same set (pid, cwd, kind, startedAt, sessionId, name, status).
- The `tmux` field records the **session name at launch**; after `tmux-restore` + adoption the
  session was renamed (`cmux-3CBE62BD` recorded vs `cmux-A1F15E44` actual). The pane id (`%5`)
  stays valid. Join on pane id or pid, never on session name.
- Background agents: `~/.claude/daemon/roster.json {proto, supervisorPid, updatedAt, workers}`.

## Resume semantics

- `claude --resume <id>` / `-r <id>`; `-c/--continue` (latest in cwd); `--session-id <uuid>`;
  `--fork-session` (new id when resuming); `-n/--name`; `-w/--worktree [name]` + `--tmux`.
- Must run in the original cwd (transcript path is derived from it). `tmux-restore` already does
  `claude <original flags> --resume <id>` and works (all 7 sessions after this morning's reboot
  run as `claude --resume <id>`).
- Flag stripping as in `tmux-snapshot.strip_resume`: drop `--resume/-r/--session-id <v>`,
  `--continue/-c`, `--resume=…`, `--session-id=…`.
- Background sessions: `claude --bg`, `claude agents`, `attach <id>`, `stop|kill <id>` (keeps the
  conversation), `respawn`, `rm`. A native "hold" for background sessions only.

## Hooks

Declared in `~/.claude/settings.json` `hooks.<Event>[].hooks[]`. Present today:

| Event | Hooks (in order) |
|---|---|
| SessionStart | `~/.claude/hooks/tmux-claude-id.sh`, cmux-tui journal hook |
| Notification | `matrix-notify.sh`, `cmux-notify.sh`, cmux-tui |
| Stop | `matrix-notify.sh`, `cmux-notify.sh`, cmux-tui |
| PreToolUse | `rtk-rewrite.sh` (matcher `Bash`), cmux-tui |
| UserPromptSubmit, SessionEnd, PostToolUse, PostToolUseFailure, PostToolBatch, PermissionRequest, PermissionDenied, SubagentStart, SubagentStop, TaskCreated, TaskCompleted, TeammateIdle, PreCompact, PostCompact, StopFailure, CwdChanged, FileChanged, ConfigChange, InstructionsLoaded, Elicitation, ElicitationResult, UserPromptExpansion, Setup | cmux-tui only |

cmux-tui hook form: `h=${CMUX_TUI_HOOK:-${TMUX:+…/cmux-tui-hook}};"${h:-:}" 'claude' '<Event>' 2>/dev/null||:;echo {}`
— a no-op outside tmux, always prints `{}`, never fails the agent. Overseer's hooks must
follow the same shape.

Payload: existing hooks read `.session_id`, `.cwd`, `.message` from stdin JSON. The full payload
per event (incl. `transcript_path`, `hook_event_name`) is **to verify in Phase 2 with a stub**.

## State signals

| Signal | Source |
|---|---|
| turn started | `UserPromptSubmit` |
| turn finished / at prompt | `Stop` (and `status: idle` in `sessions/<pid>.json`) |
| waiting on user (permission/elicitation) | `Notification`, `PermissionRequest`, `Elicitation`; cmux-tui shows `blocked` |
| subagent running | `SubagentStart` / `SubagentStop` |
| process alive | `sessions/<pid>.json` + `kill -0 pid` + `procStart` matches `/proc/<pid>/stat` field 22 |

## Children: MCP vs tool subprocesses

`ps --ppid` on the 7 idle sessions: **no children** (MCP servers on this station are remote
connectors). The busy session had one child: `/bin/zsh -c source ~/.claude/shell-snapshots/snapshot-zsh-….sh … `
— the Bash tool. Rule: a child whose argv starts with a shell sourcing `~/.claude/shell-snapshots/`
is a tool subprocess; a child that matches a configured MCP server command (`claude mcp list` /
settings `mcpServers`) is an MCP server and may be stopped. `cmux-tui terminal <id> process show`
returns the same tree (`children`) when cmux-tui is present.

### Phase 2 status (2026-10-28)

Hook payloads are still **unverified against a real agent**: hooks were not installed on the VM
(user approval needed). Overseer reads only `session_id`, `cwd`, `hook_event_name` and
`notification_type`, and works without them (polling fallback). Exercised with a stub agent only.
A dry-run merge against copies of the VM's `~/.claude/settings.json` and `~/.codex/hooks.json`
adds 12 + 7 entries and removes or reorders nothing.

Hold predicates, dry run only, on real 2.1.289 sessions: two idle lines → every predicate passes
(`would-hold`, prompt layout rule / `❯ ` / rule / status lines recognised); the busy line → refused
(`atPrompt`, `noToolRunning`: Bash tool child). Nothing was stopped.

## Clean exit

Not tested (would require stopping a session). Candidates: `/exit` typed at an empty prompt
(sends `SessionEnd`), or SIGTERM/SIGHUP. **To test in Phase 3** on a stub and on one real session
designated by the user.
