# 0009 overseer-api and daemon sync (Phase 4)

Status: accepted (amends 0002, refines 0003, amends 0008)

## Context
Phase 4 builds the hosted registry and the daemon's outbound sync. The user asked for a whole
version without stopping, so the defaults below were chosen without a review round. A second
agent is building Phases 2–3 in `src/` at the same time, so daemon changes stay in `src/sync.rs`.

## Decisions

### Crate layout and stack
- `api/` is a standalone crate (`overseer-api`, own `Cargo.lock`), not a workspace member:
  the async stack stays out of daemon builds and out of the root lock. `make test`, `make build`
  and `make lint` run both crates and share `target/`.
- API: `axum` 0.8 (`http1`, `json`, `query`, `ws`), `tokio`, `rusqlite` (bundled), `sha2`, `hmac`,
  `futures-util`. No HTTP client crate: webhooks use a minimal HTTP/1.1 POST (see below).
- Daemon: `tungstenite` 0.29 with `default-features = false, features = ["handshake"]` (blocking,
  no TLS). This amends the crate list of 0008; it adds about 20 small crates (`http`, `sha1`, `rand`…).

### Storage: SQLite, not PostgreSQL (amends 0002)
- One SQLite file (WAL) on a `ReadWriteOnce` PVC, one connection behind a mutex. Tables
  `stations`, `users`, `lines` (contract JSON + indexed `owner`, `ticket_key`, `station_id`,
  `state`), `commands` (audit), `events` (7-day retention), `webhooks`.
- Consequence: exactly one replica, `strategy: Recreate`. Event ids are the SQLite rowid, so they
  are monotonic. PostgreSQL is deferred until more than one replica is needed; the SQL is plain
  and would port with a connection pool and a `BIGSERIAL` for events.

### Authentication
- Three kinds of bearer tokens, 256 random bits, shown once, stored as SHA-256 hex only:
  admin (`OVERSEER_ADMIN_TOKEN`, from a Kubernetes Secret), user (`POST /v1/users`, additive
  to the contract) and station (`POST /v1/stations`).
- Admin: enrol/revoke stations and users, register webhooks. Users (and admin): reads, events,
  commands, `POST /lines`. Stations: only their own `/sync` socket.
- Revocation clears the hash and closes the live socket; the next handshake gets 401.
  Enrolling a revoked station again issues a new token (rotation = revoke + enrol).
- The audit's `requestedBy` is the user name (`admin` for the admin token).

### Sync protocol details (refines 0003)
- `hello{lastAckedSeq}` → `ack{seq: last applied, resync}`; `resync` when the API has nothing for
  the station (`seq 0`) or the daemon claims more than the API applied.
- `snapshot{seq}` is always applied: upserts every line, sets `last = seq`. Lines absent from a
  snapshot are kept as they are (history).
- `delta{seq}`: `seq <= last` → ignored, `ack{last}` (idempotent replay); `seq == last+1` →
  applied, `ack{seq}`; otherwise `ack{last, resync: true}`.
- The daemon replays from the API's `ack.seq`, not its own; if its outbox no longer starts at
  `ack.seq + 1` it sends a snapshot (new seq) and drops the outbox.
- A station can only write lines whose `stationId` is its own and whose id is not owned by
  another station; invalid lines are skipped.
- Any frame marks the station online (`station.online` when it was offline). A sweep every 5 s
  marks it offline after 45 s without a frame (`station.offline`). Tests drive the sweep with an
  injected clock.

### Events
- Emission: `line.created` on first sight; `line.state_changed` when `state` differs (plus
  `line.hung_up` when it becomes `hung-up`); `line.updated` when another field than
  `lastActivityAt` changed; nothing for activity-only changes. `station.online/offline`,
  `command.requested`, `command.completed` (terminal status only).
- SSE `GET /v1/events`: with `Last-Event-ID` the stored events after that id are replayed, then
  live ones, deduplicated by id; without it, live only. If a slow client lags behind the
  in-memory buffer (1024 events) the stream ends and the client resumes with `Last-Event-ID`.
  `?types=a,b` filters. Browsers' `EventSource` cannot send the bearer header; the Board goes
  through the local daemon (Phase 6), so no query-string token is accepted.

### Webhooks
- `POST /v1/webhooks {url, types}` (admin) returns the signing secret once. The secret is stored
  in clear because it is needed to sign.
- Delivery: `POST` of the event envelope, headers `X-Overseer-Event`, `X-Overseer-Delivery`
  (event id) and `X-Overseer-Signature: t=<unix seconds>,v1=<hex HMAC-SHA256(secret, "<t>.<body>")>`.
  Receivers recompute the HMAC, compare in constant time and reject old `t` (replay).
- 3 attempts (immediately, +1 s, +4 s), 5 s timeout each, unordered across events; consumers order
  by event id and can fill gaps from SSE replay.
- `http://` only: the consumers (TwinFactory, feedback-hub) are in the same cluster. HTTPS needs
  a TLS client crate and is deferred.

### Commands
- `POST /v1/lines/{id}/commands` is refused with 409 when the station is offline or not
  connected (0007: not queued). Same `id` again → the same audit row, no new event.
- `pending` → `delivered` when the daemon confirms receipt → `succeeded|refused|failed` with
  `reason`. Commands not yet delivered are re-sent when the station reconnects (kept in memory).
- **Metadata only**: `args.message` (relaunch) and `prompt` (start) travel in the WebSocket frame
  only. They are stripped from the audit row, from `command.*` events and from logs. Tests assert
  the strings never reach the database files.

### `POST /v1/lines` (feedback-hub)
- The API picks the station (`stationId`, else the most recently seen online runner), mints the
  line id `<stationId>-<ULID>` (the one exception to daemon-generated ids; the daemon adopts it),
  stores a provisional line (`state: active`, `agentSessionId` and `worktreePath` = `pending`)
  and relays a `start` command (new kind, additive). It waits up to 10 s for `delivered`
  (202), else 504; refused → 409; no runner → 503. `idempotencyKey` is the command id.

### Daemon client (`src/sync.rs`)
- `overseer enroll --api URL` reads the station token on stdin and writes
  `<state>/sync.json` (`0600`). `overseer sync [--once]` scans, stages, connects, heartbeats
  every 15 s, reconnects with exponential backoff (1 s to 60 s).
- Own database `<state>/sync.db` (`sync_meta`, `sync_outbox`, `sync_pushed`, `sync_commands`),
  so the registry schema of Phases 1–3 is untouched. A delta is staged only when a line's
  contract JSON changed; the outbox survives restarts and outages.
- The daemon never sends `remove`: a line that drops out of a scan (pruned transcript, deleted
  rollout) stays upstream as history and in later snapshots, per mission 1 ("save every
  session"). The API still honours `remove` for protocol completeness. A failed scan pushes
  nothing and skips that heartbeat.
- `ws://` only. Stations reach the API over NetBird (WireGuard), which encrypts the transport.
- The sync transport never executes a command itself. Each new command is written to
  `<state>/commands/inbox/<id>.json` (`0600`, full frame, ids restricted to `[A-Za-z0-9._@-]`) and
  acknowledged as `delivered`. The executor (Phase 3 code, wired in ADR 0016) runs it, honouring
  `args.dryRun` (default true), and calls `sync::complete(state_dir, id, status, reason, now)`, which writes
  `<state>/commands/done/<id>.json`; the client reports it once and deletes the file.

## Consequences / deferred
- Real deployment: image registry, namespace, how the API is exposed on NetBird (Service type,
  ingress or NetBird routing peer), backups of the PVC, the admin Secret.
- TLS in the daemon and HTTPS webhooks (needs `rustls`), PostgreSQL and more than one replica,
  Helm chart, rate limiting, real enrolment of the Mac and the VM. The executor wiring of the
  inbox is done in ADR 0016.
