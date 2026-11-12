# 0035 App logs, crash and freeze reports

Status: accepted

## Context
Users report freezes and crashes of the desktop app (macOS, connected over SSH to a Linux host)
with nothing to show: the app wrote no log, the daemon only appended unrotated, untimestamped
`eprintln!` output to `daemon.log`, and a killed or frozen app left no trace. Reports need the
app's version and build, what the connection was doing, what the UI was doing when it stopped
answering, and the server's side, gathered in one block a user can paste.

## Decision
**App log.** A small appender in `app/src-tauri/src/diag.rs`, no new crate: one line per event,
`<UTC RFC 3339 ms> <LEVEL> <target>: <message>`, continuation lines indented. It writes to
`app_log_dir()` (macOS `~/Library/Logs/io.github.basilelt.overseer/`, Linux
`~/.local/share/io.github.basilelt.overseer/logs/`, Windows `%LOCALAPPDATA%\io.github.basilelt.overseer\logs\`,
app data on mobile), rotates by rename at 5 MB keeping `app.log` and `.1`..`.4`, and keeps the
last 200 lines in memory. Lines logged before the folder is known are buffered. Every message
goes through `redact()` (bearer values, `token`/`password`/`secret`/`api_key`/`authorization`/
`cookie` values, GitHub and `sk-` keys, URL passwords); terminal data and prompt text are never
passed to the log. The relay and askpass helper processes do not log.

What is logged: start (app and server versions, OS, arch, build, pid, macOS code signature from
`codesign -dvv`), every connection state change at the `Reporter` choke point (target, phase,
attempt, detail, error), the `ssh -G` description (which says when the in-app relay is used),
connection loss with its cause, hot upgrade and its fallback, daemon request failures, 5xx and
latencies over 2 s (`transport.rs`), commands over 1 s, terminal attach/close, updater events,
and frontend errors forwarded by `diag_log` (`window.onerror`, `unhandledrejection`,
`console.error`, `console.warn` matching svelte/ghostty/wasm/webgl/tauri/ipc/terminal), rate
limited both in the webview (20 per 10 s, 3 per identical message) and in Rust (30 per 10 s).
Svelte 5 has no global error hook; uncaught component errors reach `window.onerror`, and a
`<svelte:boundary>` would replace the whole UI on error, so none is used.

**Abnormal exit detection.** `diag::init` writes a `session` sentinel (pid, start, version,
state, crash file, frozen seconds, view) and `RunEvent::Exit` removes it; so do the update
relaunch paths (`diag_session_end` before `relaunch()`, before `app.exit` after Homebrew, and
before the Windows installer takes over). If the next launch finds it, the run ended abnormally:
`crash` when the panic hook recorded a report, `freeze` when the watchdog had marked the UI
unresponsive, otherwise `killed`. On phones the sentinel switches to `background` when the page is
hidden, because the OS kills backgrounded apps routinely; such a kill is not reported. The
outcome, the crash report and the last 500 lines of the previous log are saved in
`unexpected-exit.json` until the user dismisses the dialog, so a second crash does not lose it.

**Crash report.** A panic hook (it runs before `panic = "abort"`) writes `crash-<ms>.txt`
synchronously: time, version, thread, location, message, `Backtrace::force_capture()`, the last
200 lines. It only `try_lock`s, so a panic while logging cannot deadlock. Release builds keep
`strip = true`: the backtrace is addresses only, location and message carry the signal; we did
not trade binary size for symbols.

**Freeze detection.** A Rust thread emits `diag:ping` every 2 s; the webview answers with
`diag_pong {view}`. After 5 s without a pong it logs "UI unresponsive for N s" with the view, the
commands in flight (tracked by a guard in `daemon_request`, `term_attach`, `conn_connect`,
`ports_forward`, …) and the last completed one, repeats every 10 s, records the seconds in the
sentinel and logs the recovery. It is armed only after the first pong. The thread parks (no ping,
no IPC) while the page is hidden or, on desktop, the main window is unfocused (native
`WindowEvent::Focused`, so a frozen webview cannot keep it asleep); it is unparked when the window
is visible and focused again, and the time asleep does not count as a freeze.

**Daemon log.** `dlog::line` prefixes the daemon's lifecycle messages with a timestamp and level.
`daemon.log` is rotated by copy-truncate (its fd is the daemon's inherited stderr, kept across hot
upgrades) at 5 MB, at `--detach` and every 60 s, keeping 5 files. `GET /v1/logs?tail=N` (default
300, max 5000) returns `{path, lines}`, redacted, across `daemon.log` and `daemon.log.1`; older
daemons answer 404 and the app says the server is too old.

**UI.** An overlay rather than a second window (no new capability or route): "Previous session
ended unexpectedly" opens at launch after an abnormal exit; the same dialog in live mode opens from
the palette, Settings > Diagnostics and More on phones, and is a bottom sheet on phones. Copy all
builds one plain-text bundle (`formatBundle` in `lib/diag/diag-bundle.ts`): header (versions, OS, log folder,
host), previous session summary, crash report, app log tail, server log tail.

## Consequences
- A freeze of the Rust main thread is reported the same way as a webview freeze; a freeze that
  ends in a force quit shows as `freeze`, one that recovers only leaves log lines.
- Copy-truncate can lose the lines written between the copy and the truncate of a rotation.
- Redaction is pattern based and can over-mask; it is not a guarantee for arbitrary secrets in
  error strings, which is why terminal data and prompts are kept out of the log entirely.

## Superseded in part

The desktop updater was later reworked to be in-app only and never run Homebrew, so the
"after Homebrew" relaunch path above no longer exists; the other relaunch paths are unchanged.
