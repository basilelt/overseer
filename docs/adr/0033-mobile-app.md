# 0033 Mobile app: Android and iOS, remote only

Status: accepted

Numbered 0033 because ADR 0032 (mobile builds and releases) owns the generated projects, signing
and the release jobs; this one covers what the app does on a phone.

## Context
Users want to watch and drive their agents from a phone while the desktop app stays open, both
showing the same sessions at the same time. Everything already lives in the daemon (ADR 0025):
terms accept any number of clients, events are broadcast over SSE, tasks, reviews and workspaces
are daemon state. A phone is therefore one more client of a daemon on a server. Two things do not
carry over from the desktop: there is no system `ssh` on a phone (ADR 0028 relies on it), and the
desktop layout does not fit a 390 px screen operated with thumbs.

## Decision
- **Same app, same code.** The Tauri 2 + Svelte 5 app is built for Android and iOS (ADR 0032).
  Mobile-only Rust is chosen with `cfg(mobile)` (emitted by `tauri-build`) or
  `target_os = "android" | "ios"`; mobile-only UI with a runtime layout (`lib/mobile/layout.ts`).
  Desktop builds keep the system `ssh` transport, the updater, the built-in browser and the local
  daemon; on mobile those are left out (`updater`, `browser` and `tauri-plugin-updater` are
  desktop-only).
- **Remote only.** On a phone the app never looks for a local daemon. It starts on the last saved
  host, or on the Connect screen.
- **Transport: in-process SSH, same tunnel.** `src-tauri/src/direct/` is a `russh` client (ring
  backend, no aws-lc, so it cross-compiles without cmake) that ends up in the same place as the
  system `ssh` of ADR 0028: one session, `exec` of a bootstrap, the `SB:HELLO` line, `RUN`, the
  `SB:MUX 1` banner, then `mux::Mux` over the channel stream. `ssh::bootstrap` is shared by both
  paths (it reads any `AsyncRead`/`AsyncWrite`), `DaemonConn::Tunnel` and everything above it
  (HTTP, SSE, terminals, port forwards, reconnect) are unchanged. `Target.kind = "direct"` with
  `host` = the id of a saved host selects it; the code builds and is tested on desktop too.
- **Server from the phone's own release** (see the addendum; this replaced "no install from the
  phone"). The phone runs `installed_command`: the server of its own version when present, else
  the most recent `~/.overseer/server/*/overseer`. When that is missing or older, it
  downloads its version from the GitHub release and uploads it with `INSTALL`.
- **Hosts.** Saved hosts (`hosts.json` in the app data folder): label, host, port, user, optional
  ProxyJump (one hop, `direct-tcpip` through the first session), key or password authentication,
  remember password. No `~/.ssh/config` on a phone.
- **Device key.** An Ed25519 key is generated on first use (`ring` randomness, OpenSSH format,
  comment `overseer-android` / `overseer-ios`) and kept in the platform secure store through
  `keyring-core`: Android Keystore-backed storage (`android-native-keyring-store`) and the iOS
  data-protection Keychain (`apple-native-keyring-store`, `protected`, after-first-unlock so a
  reconnect in the background can read it). When the store is unavailable (an unsigned iOS build
  answers -34018) the key falls back to a `0600` file in the app data folder, which is also what
  desktop builds and tests use. The Device key screen shows the public key, its SHA256
  fingerprint, and a copyable one-liner that appends it to `~/.ssh/authorized_keys` once.
- **Host keys.** `known_hosts` in the app data folder, OpenSSH line format. An unknown key raises
  the same "Trust host key SHA256:…" prompt as the desktop (`askpass::Prompt` kind `hostkey`,
  answered `yes` only on an explicit tap); a changed key is never a prompt: the connection is
  refused, permanently, until the user removes the trusted key from the host's settings.
  Passwords are asked with the same prompt (`secret`), kept in memory for reconnects, and in the
  secure store only with "remember password".
- **Phones sleep and roam.** russh keepalive every 15 s (3 missed = dead) on top of the mux PING.
  The reconnect policy of ADR 0028 applies (0.5 s doubling to 30 s, permanent errors stop it,
  plus: host key changed, key refused, wrong password, server not installed). `conn_wake`, called
  when the app becomes visible again or the network comes back, cuts the current backoff wait,
  resets it, and probes a link that looks alive with `GET /v1/version` (5 s) so a half-open TCP
  session after sleep is dropped at once instead of after two PINGs. Terminals re-attach and the
  daemon replays the scrollback (2 MiB), the event stream resumes with `Last-Event-ID`.
- **Two clients on one terminal.** The daemon keeps "last resize wins" and now records which kind
  of client sent it: the resize control carries `"client": "phone" | "desktop"` (added by the Rust
  side, the web code does not change) and the Term gets `sizedBy`. The most recently focused
  client's size wins: a client re-sends its size when its window gets focus, when it becomes
  visible and when the pane is focused; while another kind of client owns the size the pane shows
  a small "Resized for phone · Fit" (desktop) or "Sized for desktop · Fit" (phone). Nothing else
  assumes a single client: events, tasks, reviews and workspaces were already shared.
- **Phone layout.** The same components and tokens with a breakpoint (720 px, or a mobile OS under
  1024 px, or `?phone=1`): the sidebar becomes a slide-over drawer, the tab strip horizontally
  scrollable chips, and a bottom navigation switches Agents / Tasks / Review / More. Kanban shows
  one column per screen with swipe and a column picker, and a long press (or the card's menu)
  opens "Move to…" instead of drag and drop. Review is a single-column diff, the file tree is a
  picker, and tapping a line number opens the comment box in a bottom sheet. The palette is full
  screen, dialogs are bottom sheets, touch targets are at least 44 px, safe-area insets are
  honoured and nothing depends on hover.
- **Terminal on a phone.** ghostty-web with an accessory row above the keyboard (Esc, Tab, sticky
  Ctrl: tap for one key, double tap to lock, Alt, arrows, `|`, `~`, `/`, `-`, newline as
  Alt+Enter), Paste, pinch to change the font size (kept per device), long press to copy, and
  Compose: a sheet whose text is delivered with `POST /v1/broadcast` to that one session, typed
  when it is idle (queued otherwise) or at once, exactly like broadcast and review delivery.
  Clipboard goes through `tauri-plugin-clipboard-manager` (the Android page is not a secure
  context, so `navigator.clipboard` is missing).
- **Notifications.** The existing notifications (needs input, ready for testing, finished,
  failed) use `tauri-plugin-notification`, which works on both phones, while the app runs.
- **Mock.** `VITE_MOCK=1` with `?phone=1` reports a mobile platform, two saved hosts (one behind a
  ProxyJump), a device key and the direct connection steps, so the phone layout can be captured by
  CI.

## Consequences
- Push notifications while the app is closed are not possible: iOS suspends the web view a few
  seconds after the app leaves the screen and Android may stop it. Future work: a relay through the
  hosted API (`api/`) that the daemon notifies and that sends APNs/FCM pushes to registered
  devices.
- The phone installs and upgrades the server only with a GitHub token that can read the private
  repository; without one, or offline, a host that has an older server keeps running it, and a
  host with none says to add a token or to connect once from a desktop.
- Reaching the server off the LAN needs a route to its SSH port: NetBird or Tailscale on the phone
  and the server (the phone dials the overlay address like any host), or a ProxyJump bastion.
- The mobile-only code paths (keystore stores, mobile entry point, plugin set) are compiled only by
  the mobile pipeline of ADR 0032; desktop CI compiles and tests the transport, key, host key,
  saved host and reconnect code, and the phone layout logic.
- Port forwarding still works from a phone (listeners on the phone's loopback) but is hidden from
  the phone UI except in More > Ports.

## Addendum: installing the server from the phone

A phone carries no server binaries (`bundle.resources` is empty on mobile), so it used to refuse a
host without a server. It now gets the binary of its own version from the GitHub release, the
same place the desktop bundle comes from.

- **Script.** `installed_script` keeps its fallback to the newest installed server and accepts the
  desktop's `INSTALL <size> <sha256>` upload: it writes `~/.overseer/server/<version>/overseer`
  after checking the digest, then starts it (with `--replace` when asked) and execs `proxy`.
- **When.** After `SB:HELLO`, the phone downloads when nothing is installed or the installed
  server is older than its own version (`install::wants_fetch`); a newer server is used as it is.
  The rest is the desktop path: `establish` hot-upgrades a running older daemon, or offers
  "Restart server" (ADR 0034).
- **Source.** `uname -sm` maps to the release asset `overseer-<version>-<triple>`
  (`x86_64`/`aarch64` `unknown-linux-musl`, `x86_64`/`aarch64` `apple-darwin`), downloaded with the
  GitHub token of the in-app updater (same secure store; on iOS the token is asked only for this)
  from `releases/tags/v<version>`, and checked against the release's `SHA256SUMS` before it is
  cached in the app cache folder (`server/<triple>/`, only the current version kept). The upload
  is checked again by the remote script.
- **Steps.** Connecting, Checking server, Downloading server for linux/x86_64, Installing server,
  Starting. "Downloading" only appears when the phone downloads.
- **Failures.** Without a token, offline, or when GitHub refuses: a host with an older server keeps
  running it; a host with none fails with `NOT_INSTALLED` and the reason, permanent (no retry
  loop), and the Connect screen offers "Add GitHub token" or connecting once from a desktop.
