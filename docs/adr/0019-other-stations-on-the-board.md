# 0019 Other stations on the Board

Status: accepted

## Context
ADR 0012 shows other stations' lines from `<state>/stations/<id>.json` and leaves them read-only
until the sync client fills those files. Station tokens cannot read the API (only user and admin
tokens can), and Board actions on another station must keep every guarantee of a relayed command.

## Decision
- **Transport**: the API answers each sync `heartbeat` with a `stations` frame
  (`{kind: "stations", stations: [{stationId, lastSeenAt, lines}]}`: every other non-revoked
  station and its non hung-up lines). Additive to the sync protocol; the station token stays
  write-only. `overseer sync` writes one file per station (`0600`, atomic rename), keeps only
  lines whose `owner` is this station's `owner` (the Board shows the developer's lines), and
  removes files of stations that disappeared. Refresh rate = heartbeat (15 s).
- **Offline**: a station whose `lastSeenAt` is older than 2 minutes is offline on the Board; when
  sync itself is down the files go stale and every other station greys out after 2 minutes.
- **Actions**: on a line of another online station the daemon relays `hold`, `close`, `resume` and
  `link` to the API (`POST /v1/lines/{id}/commands`) with the user token of `overseer login`,
  then polls the audit for the outcome (`OVERSEER_RELAY_TIMEOUT_MS`, default 15 s, then 504).
  `dryRun` follows the Board's dry-run/confirm steps (link acts at once, as locally). The target
  applies `acceptRemoteCommands`, its predicates and its dry-run default; a refusal comes back as
  409 with the target's reason. Without a user token the line shows "run `overseer login`".
- `pin`, `diff`, `commit`, `stash` stay local: they need the worktree or local marks.

## Consequences
- The Board on a Mac can hold an idle agent on a VM with the same safety as on the VM.
- Every station receives every other station's lines each heartbeat; fine for tens of stations,
  revisit with a filtered frame if it grows.
