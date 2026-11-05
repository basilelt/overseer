# 0034 Daemon hot upgrade: exec in place, terminals survive

Status: accepted

## Context
The desktop app installs the server matching its own version under
`~/.overseer/server/<version>/` on every host it connects to (ADR 0028), but `daemon --detach`
only starts a daemon when none runs. After an app update the host therefore kept running the old
daemon: the new binary sat next to it unused, and the only way to switch was to stop the daemon,
which kills every terminal it owns (terms are PTYs held by the daemon process, ADR 0022). Asking
users to choose between a stale server and losing their running agents is not acceptable.

## Decision
The daemon upgrades itself in place, keeping its pid, its terms and its socket.

**Trigger.** `POST /v1/upgrade {"path"?, "version"?, "force"?}` on the running daemon. `version`
resolves to `$HOME/.overseer/server/<version>/overseer`. The app calls it right after
connecting when the daemon reports an older version than the one it just installed.

**Pre-flight, while the old daemon keeps serving.**
1. `<new> daemon --takeover-check` must print `{"version","protocol","takeover"}` within 10 s.
   The protocol must equal the running one, the takeover format (currently 1) must match, and the
   version must be strictly newer (dotted numeric compare) unless `force` is set, so an older app
   can never downgrade a host.
2. The daemon writes its state to `<state>/takeover-<pid>.json` (mode 0600: the scrollback can
   hold secrets) and runs `<new> daemon --takeover-check <file>`, proving the new binary parses it.
3. Only then it answers `202 {"from","to","epoch"}`. Any failure answers 400/409 and changes
   nothing.

**Handover.** After the 202 is flushed, the request thread snapshots the state again (the very
last step before the exec, so at most the output read in that instant is lost), clears
`FD_CLOEXEC` on every live PTY master, the listening Unix socket and the `daemon.lock` fd, writes
the file and `execve`s `<new> daemon <same options> --takeover <file>`. Exec keeps the pid, so
`daemon.pid` stays right, the terms' processes stay children of the daemon (`waitpid` keeps
working and their exit codes are still collected), the flock on `daemon.lock` is never released,
and connections arriving during the exec wait in the socket backlog. If `execve` returns, the
flags are restored, the file is removed, the failure is logged in `daemon.log` and the old daemon
keeps serving.

**State file.** Format version, previous version, epoch, the lock and listener fds, and per term:
id, pid, master fd, cwd, cmd, title, size, creation time, exit status, line/task/agent/session
links, flags, `sizedBy`, the output counters (`total`, last output time, which idle detection
relies on) and the 2 MiB scrollback ring (hex). It also carries the in-memory broadcast queue.
Everything else is either on disk (tasks, reviews and their delivery queue in SQLite, workspaces)
or rebuilt by the first scan (lines, ports, git watchers, caches).

**Takeover.** The new daemon reads the file, sets `FD_CLOEXEC` again on every inherited fd before
anything can spawn, adopts the lock fd instead of locking, rebuilds the terms (reader and writer
are dups of the master fd, resize is `TIOCSWINSZ`, exit via `waitpid`, kill via the process-group
signal), restores the broadcast queue, deletes the file, adopts the listening socket and starts
serving. Adoption is per term: a bad entry loses that term, not the daemon. A normal start deletes
any leftover `takeover-*.json`.

**Client signal.** `GET /v1/version` gains `epoch` (0 on a fresh start, +1 per takeover), `pid` and
`upgradedFrom`. Clients see their WebSockets and SSE stream drop during the exec, reconnect as
usual and get the scrollback replayed; they detect the upgrade through `epoch`. The new daemon
also emits `daemon.upgraded {from,to,epoch}`, but that is best effort: nobody is connected at that
moment, and the event ids restart from 1 in the new process.

**Fallback.** When the hot upgrade is unavailable (a daemon that predates `/v1/upgrade` answers
404, Windows answers 501) or fails its pre-flight, the app uses `daemon --detach --replace`: the
new binary stops the running daemon (pid from `daemon.pid`, checked to be a `overseer daemon`
command line; SIGTERM, then SIGKILL after 5 s) and starts itself. The app does that on its own
when the old daemon has no live terminal, and otherwise shows a banner saying how many terminals a
restart closes, with a Restart button. Every daemon now writes its own pid to `daemon.pid`.

## Consequences
- App updates reach the hosts without closing anything; the banner only remains for daemons from
  before this change, Windows, and a failed pre-flight.
- Unix only: Windows has no exec-in-place and its terms use ConPTY handles.
- The `overseer proxy` process that carries an SSH session (ADR 0028) is not replaced; it is a
  separate process and keeps relaying to the new daemon through the same socket until the next
  reconnect.
- `--http` boards are re-bound by the new daemon rather than handed over; with `--http
  127.0.0.1:0` the port, and so the board URL, changes on each hot upgrade.
- The state file format is versioned (`takeover` in `--takeover-check`); a change to it bumps the
  format, and an old daemon then falls back to the restart path instead of handing over state the
  new binary cannot read.
