# 0016 Relayed command execution

Status: accepted; `relaunch` and `start` superseded by ADR 0018

## Context
ADR 0009 spools each relayed command to `<state>/commands/inbox/<id>.json` and reports it
`delivered`; nothing executed it. Relayed `hold` and `close` stop agents on a station from
somewhere else, so they must go through the same predicates as a local `hold --yes`.

## Decision
- **Opt-in per station**: `acceptRemoteCommands` in `<state>/config.json`, default `false`. When
  it is off every command is consumed and reported `refused` with reason
  `disabled: acceptRemoteCommands is false on this station`. The flag is read on every pass, so
  flipping it needs no restart.
- **Executor**: `overseer::inbox::process(ctx)`, called at the start of every `overseer sync`
  pass (before the scan, so the outcome is reported in the same pump) and on every
  `overseer daemon` tick. `sync` returns from its wait as soon as a command frame arrives, so a
  command runs within about a second instead of a heartbeat.
- **Claim**: a command is claimed by `rename(inbox/<id>.json, running/<pid>-<id>.json)`; whoever
  renames first owns it, so `sync` and the daemon can run together. A `running/` file whose pid is
  gone is reported `failed` (`interrupted before completion`) and never retried: a half-done hold
  is not repeated blindly.
- **Line**: `lineId` must equal the id of a line of this station (no prefix matching); otherwise
  `refused: line <id> is not on this station`.
- **Kinds** (`args.dryRun` defaults to true, as in ADR 0009):

  | kind | runs | succeeded reason | refused / failed |
  |---|---|---|---|
  | hold | `actions::hold` (all predicates) | `held` / `would-hold` | refused: `<predicate>: <detail>; …`; failed: execution error (reverted) |
  | close | `actions::close` (holds a live line first) | `closed` / `would-close` | as hold |
  | resume | `actions::resume`, `Launch::Detached` | `resumed in tmux-pane\|tmux-session` | refused: running, missing cwd, tmux error |
  | link | `actions::link` | `linked to AS-…` | refused: bad key |
  | relaunch, start | nothing | – | refused: `not supported by this station yet` |

  `relaunch` would type `args.message` into an agent; that is the feedback-hub path of Phase 7 and
  is not executed before its own design.
- **Detached resume**: a relayed resume (and the local daemon's `POST /v1/lines/{id}/resume`) never
  execs and never opens a window in someone's client: the held pane if there is one, else a new
  detached tmux session `sb-<last 6 of the id>` on the station's default tmux server.
- **Audit**: every outcome is appended to `actions.log` as
  `{"action":"remote-command","commandId","kind","lineId","requestedBy","status","reason"}`, next
  to the `hold`/`close`/`resume` record of the action itself; the API keeps its own audit
  (`GET /v1/lines/{id}/commands`).

## Consequences
- A station that never sets the flag answers every command; the requester sees why.
- No queue survives a crash mid-command; the requester re-sends with a new id.
- Execution runs with the station user's rights; the API token is the only gate before the flag
  and the predicates.
