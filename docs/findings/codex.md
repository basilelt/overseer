# Findings: Codex

Checked 2026-10-28 on the VM. **Codex is not installed on the VM**: `codex` resolves to the
cmux-tui shim `~/.local/share/cmux-tui/shims/codex`, which prints
`cmux: the agent executable was not found`. `~/.codex/` exists but was copied from the Mac
(`config.toml` `[projects."/Users/al/…"]`, AppleDouble `._*` files). Everything below is
**file-only**; runtime behaviour (resume, hooks firing) is **unverified, to confirm on the Mac**.

## Commands run

```
codex --version                                  (fails: not installed)
ls ~/.codex ; jq -c . ~/.codex/version.json
rg '^\s*(notify|\[|model\s*=|hooks)' ~/.codex/config.toml   (secrets redacted)
jq on ~/.codex/hooks.json                        (event names, command prefix)
fd -e jsonl . ~/.codex/sessions
jq on one rollout (type/payload keys; session_meta id, cwd, originator, cli_version)
python3 sqlite3 read-only (mode=ro) on ~/.codex/state_5.sqlite: table schemas only
```

## Version

`version.json` → `latest_version 0.142.5`; last rollout `cli_version 0.142.5` (2026-07-07).

## Storage

- Rollouts: `~/.codex/sessions/YYYY/MM/DD/rollout-<ISO time>-<uuid>.jsonl` (confirmed, 2 files).
  Line kinds: `session_meta {id, session_id, timestamp, cwd, git, cli_version, originator, source,
  model_provider}`, `turn_context {cwd, model, approval_policy, sandbox_policy, turn_id, …}`,
  `event_msg` (turn start `{started_at, turn_id}`, turn complete `{completed_at, duration_ms, turn_id}`),
  `response_item`.
- **`~/.codex/state_5.sqlite`** table `threads(id, rollout_path, created_at, updated_at, source,
  model_provider, cwd, title, sandbox_policy, approval…)`: an index of every session. Prefer it to
  scanning rollouts (open read-only, `mode=ro`, WAL present).
- `history.jsonl`: prompt history (not needed).

## Resume

Believed `codex resume <id>` and `codex resume` (picker). Not verifiable here. The rollout's
`session_meta.id` is the id to pass.

## Hooks

Codex now has **Claude-style hooks** in `~/.codex/hooks.json`: `SessionStart, UserPromptSubmit,
PreToolUse, PostToolUse, PermissionRequest, PreCompact, PostCompact, SubagentStart, SubagentStop,
Stop, SessionEnd`. Each is trusted by hash in `config.toml [hooks.state]` (editing a hook needs
re-trust). All are cmux-tui journal hooks today. The legacy `notify = [...]` program still exists
(points to a Mac-only tokenbar script; `complete` event only).

Consequence: turn start/stop is observable for Codex too (`UserPromptSubmit` / `Stop`); the
mtime + process fallback stays for older versions.

## Clean exit, children

Not observable (no running Codex). To test on the Mac.
