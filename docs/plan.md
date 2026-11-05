# Refined plan (after Phase 0)

Historical (Phase 0). Overseer no longer integrates with cmux; the cmux items below were dropped.

Phases 1–7 of the brief stand. Changes driven by the findings:

## Phase 1 — daemon read-only + `ls`
- Discovery sources, in priority order:
  - Claude: `~/.claude/sessions/<pid>.json` (live: pid, sessionId, cwd, status, tmux pane), then
    transcripts `~/.claude/projects/*/*.jsonl` (sessionId, cwd, gitBranch, timestamps, ai-title; read
    first and last lines only).
  - Codex: `~/.codex/state_5.sqlite` `threads` (read-only), fallback rollouts `sessions/YYYY/MM/DD/`.
  - tmux: `list-panes -a` with `@claude_session`, `@cmux_*`, `pane_pid`, `pane_current_path`.
  - cmux-tui (if present): `agent list --json`, `terminal list` for orphan detection only.
- Repo identity = normalized `origin` URL (several clones of one repo share it). Worktrees from
  `git worktree list --porcelain` under configured roots (VM: `~/Documents/Github`),
  including `.claude/worktrees/*`; flag worktrees under `/tmp`.
- Ticket inference case-insensitive (`proj-10844` → `AS-10844`), from branch, then cmux/tmux title.
- Join process ↔ pane by pid / pane id, never by the session name recorded by Claude.
- Station id and canonical `owner` in config (Mac user is `al`, VM user is `alice`: `$USER` cannot be the owner).
- Fixtures: copy of the observed shapes (keys only, synthetic values).

## Phase 2 — hooks, live state, resume, link
- `overseer-hook` (ADR 0004) + installer (dry-run diff, backup, `jq -e`, uninstall). Codex: print
  the re-trust step. Polling of `sessions/*.json` as fallback from day one.
- Verify the full hook payloads with a stub agent; record in findings.

## Phase 3 — hold / close / orphans
- Predicates per line-states.md; clean-exit method chosen after a stub test and one supervised real test.
- `@sb_held` + stub; toolkit patch delivered as a diff.

## Phases 4–7 — unchanged, plus
- Phase 6: inspect the cmux-tui sidebar/agent plugin format first; Board can be a sidebar plugin on
  cmux-tui and a browser tab on cmux.app.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| cmux-tui 0.1.0 and Claude Code 2.1.x change fast (new registry files, hook events) | discovery breaks | version-gated parsers, `doctor` prints versions, fixtures per version, polling fallback |
| `cmux-tui agent list` keeps dead entries | false active lines | never use it for liveness; pid check |
| Session renames after restore/adoption | wrong pane for hold/resume | key on pane id / pid |
| Hook count per event already 3–4 | latency, ordering | append-only spool, < 5 ms, exit 0 always |
| Codex hooks need re-trust by hash | silent no-op | installer checks `[hooks.state]`, doctor warns |
| Codex absent on VM | Codex path untested on real data until Mac access | fixtures from the Mac; test on the Mac in Phase 1 acceptance |
| Mac unreachable from here | half of Phase 1 acceptance unverified | user provides SSH alias or runs `overseer doctor` + `ls` on the Mac |
| Input-box detection by screen scraping | false "empty" → lost typed text | conservative regex, refuse on doubt, re-check right before stopping |
| Toolkit patch not applied | held stub lost after reboot | degrades safely to plain shell; doctor reports |
| `/tmp` worktrees (agent scratchpads) | resume target vanishes | flag as volatile in `ls` |
| cmux socket bursts / fd leak | cmux crash | ≤ 1 call/s, serialised, report "restart cmux" on `Bad file descriptor` |
| Secrets near the data (`sessions/*.key`, `auth.json`, socket password) | leak | allow-list of files/fields read; never open `*.key`, `auth.json`, `.credentials.json` |
