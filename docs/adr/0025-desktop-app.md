# 0025 Desktop app (Tauri 2, Svelte 5, remote over SSH)

Status: accepted (the remote transport is superseded by ADR 0028)

Numbered 0025 because the daemon work for the app (terms, tasks, activity, MCP, detach) may take
0022 to 0024 on its own branch.

## Context
The Board (ADR 0012) is a read-mostly web page. Users want one fast desktop app to watch and drive
every agent session: live terminals, a task board, activity, diffs, and the same experience on a
remote machine as on the local one (like VS Code Remote-SSH). Sessions must outlive the app.

## Decision
- **Shell**: Tauri 2 (`app/src-tauri`, Rust) + Svelte 5 + Vite + TypeScript (`app/`, pnpm). No UI
  framework beyond Svelte; icons from `@lucide/svelte` imported one by one; fonts vendored with
  `@fontsource` (Inter, JetBrains Mono, latin subsets). Terminal: `ghostty-web` (libghostty-vt in
  WASM, xterm.js-compatible API), loaded lazily in its own chunk.
- **The daemon owns everything** (lines, terms/PTYs, tasks, activity). The app is a client and keeps
  no session state; closing it changes nothing.
- **No network from the webview**: every daemon call goes through Tauri commands.
  `daemon_request(method, path, body)` for HTTP, `daemon_events` (a Tauri `Channel`) for SSE,
  `term_attach/write/resize/detach` for terminal bytes (WebSocket `GET /v1/terms/{id}/io`, server
  binary frames delivered as raw `Channel` payloads). The CSP has no `connect-src` to the daemon.
- **Transport**: one enum `DaemonConn { Local, Loopback, Ssh }`. A request opens a fresh stream
  (Unix socket; on Windows the loopback TCP address and token of `<state>/daemon.json`, token in
  the query; or an SSH `direct-streamlocal@openssh.com` channel to the remote daemon socket) and
  speaks HTTP/1.1 on it; one generic client (content-length, chunked, SSE, WebSocket via
  `tokio-tungstenite`) works over any `AsyncRead + AsyncWrite`, so tests run on in-memory duplex
  streams. `GET /v1/version` must report `protocol: 1` (`contract/app-daemon.md`); another
  protocol is refused with a clear message.
- **Remote mode**: `russh` client, hosts from `~/.ssh/config` (Host patterns, HostName, User,
  Port, IdentityFile, ProxyJump through `direct-tcpip`), auth with ssh-agent then identity files,
  host keys checked against `known_hosts` (unknown or changed key = error; the app never writes
  `known_hosts`). On connect: `uname -sm`, then `~/.overseer/server/<version>/overseer version
  --json`; when missing or different, the matching binary (Tauri resource
  `server/<os>-<arch>/overseer`, linux musl x86_64/aarch64 and darwin x86_64/aarch64; or a
  folder set in Settings) is uploaded with `cat > tmp`, checked with sha256, `chmod +x`, `mv`;
  then `overseer daemon --detach` and `overseer socket-path`. One SSH connection, many
  channels. Reconnect with exponential backoff; the UI shows the host chip and phase.
- **Local daemon**: auto-started with `overseer daemon --detach` when the socket does not
  answer; the binary is looked up next to the app executable (sidecar), in resources, in the
  Settings folder, then on `PATH`. A missing binary is an error state, never a crash.
- **Built-in browser**: a Tauri child webview (`unstable` feature) placed over a measured
  placeholder, hidden while a dialog is open (native views draw above HTML).
- **Mock daemon**: `app/src/lib/daemon/mock.ts` implements the same client interface in memory (12 lines,
  14 tasks, activity, diffs, fake terminal, connection phases). Used with `VITE_MOCK=1` or when the
  page runs outside Tauri, and by the unit tests.
- **Design**: neutral light and dark themes, `auto` by default (follows the OS), every color a CSS
  token on `:root`; one accent (blue) for focus, selection and primary buttons only; status colors
  as small dots or thin bars; no decoration. Terminal palettes follow the theme.
- **Strings**: English in `app/src/lib/i18n/en.ts`; one lazy file per other language next to it
  (`fr.ts`, ...), picked up automatically. Missing keys fall back to English and are filled by
  `pnpm i18n` (machine translation through `claude -p`).

## Consequences
- The app depends on the daemon routes of `contract/app-daemon.md` (protocol 1: terms, tasks,
  projects, activity, version, `daemon --detach`, `socket-path`); against a 0.1.0 daemon only
  lines, hold/close, diff and events work. `line.activity` events only name the line; the app
  refetches `/activity` (debounced) and the diff of the selected line.
- Destructive line actions ask for confirmation in the app and call the routes with
  `dryRun=false`; refusals show the daemon's error.
- `make app-server` copies the static Linux binaries of `make dist` into
  `src-tauri/resources/server/linux-<arch>/` before a bundle; darwin binaries are not built yet,
  so remote macOS hosts need the Settings folder. No `externalBin` sidecar: the build must not
  depend on a binary being present, so the local daemon is looked up next to the executable.
- `daemon_it` (Rust) runs the app transport against a real daemon in a temporary state dir.
