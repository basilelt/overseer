# Findings: cmux

Historical (Phase 0). Overseer no longer integrates with cmux.

Checked 2026-10-28 on the VM (`workstation`, Debian 13, kernel 7.0.14-26-pve). The Mac was not
reachable (no SSH alias for it in `~/.ssh/config`); every Mac-side item below is **unverified**.

## Versions

| Component | Where | Version | How |
|---|---|---|---|
| `cmux-tui` (headless server + CLI) | VM, `~/.local/bin/cmux-tui` | `cmux 0.1.0 (a9b104f3…; ghostty cb1ef709…)` | `cmux-tui --version` |
| `cmuxd-remote` (older relay daemon) | VM, `~/.cmux/bin/cmuxd-remote/` | `0.64.25`, `0.65.2` (current symlink → 0.65.2) | `ls` |
| cmux.app | Mac | assumed 0.65.x | unverified |

There is no `cmux` binary on the VM `PATH`. `~/.cmux/bin/cmux` is a 655-byte shell wrapper that
execs `cmuxd-remote-current` (relay to the Mac's socket, address in `~/.cmux/socket_addr`). This is
what `~/.claude/hooks/cmux-notify.sh` uses to reach the Mac sidebar.

## Two generations coexist on the VM

1. **cmuxd-remote 0.65.2** (`~/.cmux/`): the relay used by `cmux ssh` workspaces and `cmux notify`.
2. **cmux-tui 0.1.0**: a full terminal multiplexer running headless as one durable server per user.
   - Process: `/proc/self/exe --headless --session cmux` (pid 3274, RSS ≈ 2.7 GB), plus
     `remote-sidecar` and `remote-link --stdio --session cmux --agent-hooks claude,codex`.
   - Socket: `/run/user/1000/cmux-tui-1000/cmux.sock`. The CLI defaults to session `main`
     (`main.sock`, absent) so **every call needs `--session cmux`**.
   - Mac clients appear as `native-mirror` clients named "cmux cloud terminal".
   - Each cmux-tui terminal id maps to the tmux session name: `term_562f5f55…` ↔ `cmux-562F5F55`.

## Commands run (all read-only)

```
cmux-tui --version
cmux-tui --help ; cmux-tui <scope> --help   (agent sidebar notification terminal session tab workspace server machine projection client)
cmux-tui help shorthands
cmux-tui server status --session cmux
cmux-tui --session cmux agent list [--json]
cmux-tui --session cmux agent hook status --json
cmux-tui --session cmux agent plugin list --json
cmux-tui --session cmux sidebar plugin list
cmux-tui --session cmux workspace list | tab list | terminal list | notification list | client list
cmux-tui --session cmux --json terminal <own terminal> state read
cmux-tui --session cmux --json terminal <own terminal> process show
```

Calls were made one per second (burst pitfall, brief 8.5). `session list` and `machine list`
returned `request contains unknown parameters`. `agent hook status` returned
`one or more coding-agent hook operations failed` (details not recorded: the payload contains
credentials). Not run because they may mutate: `server ensure|stop`, `* create|close|rename|focus`,
`terminal write|keys`, `agent report|hook install|hook emit`, `* plugin install|use`,
`journal append|checkpoint|producer put|hook put`, `projection put`.

## What exists (confirmed from help + reads)

| Capability | Status | Notes |
|---|---|---|
| Resource CLI (`workspace`, `tab`, `pane`, `terminal`, `screen`, `notification`, `client`) | works | `--json` / `--jsonl` output |
| Agent state tracking | works | `agent list` → `{id, state, extra:{agent, agent_session_id}, terminal_id, source:"hook", updated_at_ms}`; states seen: `idle`, `working`, `blocked` |
| Agent hooks | installed | cmux-tui injected a hook on ~30 Claude events and 11 Codex events (see claude-code.md, codex.md) |
| **Plugin mechanism** | **exists** | `agent plugin install <git-url>`, `sidebar plugin install <git-url>`, `sidebar view show|attach|input`; none installed |
| Session journal (event stream) | exists | `session <sel> journal subscribe --kinds … --classes … --max-sensitivity …`, `journal producer put`, `journal hook put` |
| Terminal introspection | works | `terminal <sel> process show` → `{argv, children, cwd, executable, foreground_cwd, foreground_executable, pid}`; `screen read`, `state read` |
| Browser tabs | exists | `tab create browser --url` (not run) |
| Notifications | works | `notification list`; Claude hooks produce "Claude finished" / "Claude reported an error" |

## Caveats

- `agent list` held 28 entries (24 `idle`) while only 8 `claude` processes ran: entries are not
  removed when a process dies. **Not authoritative for liveness**; cross-check with pids.
- `terminal list` reports `cwd=/home/alice` for every tab (the tmux client's cwd, not the pane's).
  Use tmux `pane_current_path` or the agent's own registry for the worktree.
- Mac side (`cmux docs api|browser|settings|agents`, JSON-RPC `/tmp/cmux.sock`, `system.capabilities`,
  `cmux events --name surface.closed`, `cmux.json`): **unverified, to confirm on the Mac.**

## Consequence for "cmux plugin"

The brief assumed no plugin API. On the cmux-tui side a git-installable **agent plugin** and
**sidebar plugin** mechanism exists, plus a journal with producers/hooks. Overseer's V2 can be
(a) a cmux-tui sidebar plugin showing the Board, and/or (b) a browser tab on the local daemon's
Board URL. The plugin format itself is undocumented here (no `docs` scope on cmux-tui); inspect it
in Phase 6 before committing.
