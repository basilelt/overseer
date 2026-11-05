# 0028 Remote: system ssh, one multiplexed tunnel, port forwarding

Status: accepted (supersedes the transport part of ADR 0025)

## Context
On a Mac, "Connect to host" for `alice-workstation` failed with `ssh: workstation:22: No route
to host (os error 65)` while `ssh alice-workstation` worked. The app had its own SSH client
(`russh`) and its own `~/.ssh/config` reader. That reader only knew `Host`, `HostName`, `User`,
`Port`, `IdentityFile` and `ProxyJump`: the options of `Match` blocks were attached to a block
that never matched, `/etc/ssh/ssh_config` and its `ssh_config.d` (where tools such as NetBird and
Tailscale drop their `Match exec … ProxyCommand …`) were never read, and `ProxyCommand`,
`CanonicalizeHostname`, `IdentityAgent` (1Password, Secretive), certificates, FIDO keys,
`Match exec`, keyboard-interactive and passphrase prompts were unsupported. Whatever OpenSSH does
differently from the copy, the copy dials the wrong place. The fixture test
`sshg::real_ssh_g_honors_match_blocks_the_old_parser_ignored` shows one such case (a `Match`
block that the old parser resolved to `workstation`, OpenSSH to another name via a ProxyCommand).

A second macOS cause gives the same `EHOSTUNREACH`: since macOS 15, an app (and the processes it
spawns) needs the Local Network permission to reach LAN addresses; Terminal has it, a new app may
not. The bundle now declares `NSLocalNetworkUsageDescription` so macOS asks, and a "No route to
host" error on macOS says where to allow it.

The remote mode must also carry, over one connection, the API, the event stream, every terminal
and forwarded ports, survive sleep and network changes, and work from macOS, Linux and Windows.

## Options
1. **Keep russh, feed it `ssh -G`.** `ssh -G <host>` prints the fully resolved options; the app
   would honor `hostname`, `port`, `user`, `identityfile`, `identityagent`, `proxycommand` (run it,
   speak SSH over its stdio), `proxyjump` (chain, each hop resolved with `ssh -G`),
   `userknownhostsfile`, `stricthostkeychecking`. Still a second SSH implementation: no GSSAPI,
   PKCS#11, FIDO, certificates, Windows agent pipes, `ControlMaster`; each new OpenSSH option is
   another divergence, which is how the bug happened.
2. **System `ssh` as the transport.** Run `ssh -T <host> <command>` once and multiplex everything
   over its stdin/stdout to a small `overseer proxy` on the remote side. OpenSSH does all of
   the resolution, authentication, agents, host keys and proxies exactly like in the terminal.
   `ControlMaster` and Unix socket forwarding are not needed (OpenSSH for Windows has neither);
   stdio works everywhere.

## Decision
Option 2, the way VS Code Remote-SSH uses the system client.

- **Resolution for display only**: `ssh -G` (same `-F`, `-l`, `-p` as the real connection) gives
  "alias → hostname:port as user via ProxyJump x (x.example:22) / via ProxyCommand `…`" for the
  Connect dialog and every error, and tells whether the user set `ServerAliveInterval` and
  `ConnectTimeout` (otherwise the app adds `ServerAliveInterval=15`, `ServerAliveCountMax=3`,
  `ConnectTimeout=20`). Each ProxyJump hop is resolved with `ssh -G` too.
- **Command**: `ssh -T -o ControlMaster=no -o ControlPath=none -o ClearAllForwardings=yes
  -o ForwardX11=no -o RequestTTY=no [keepalive] <host> "exec sh -c '<bootstrap>'"`. No
  `StrictHostKeyChecking` is passed: the user's own policy applies (`accept-new` included).
  `OVERSEER_SSH` and `OVERSEER_SSH_CONFIG` override the binary and add `-F` (tests use them;
  Windows looks for `%SystemRoot%\System32\OpenSSH\ssh.exe` first).
- **Prompts**: `SSH_ASKPASS` is the app's own executable with `SSH_ASKPASS_REQUIRE=force`; started
  that way it connects back to the running app (a 0700 directory with a Unix socket; loopback TCP
  on Windows) with a per-connection token, and the app shows the prompt: host key ("Trust host
  key SHA256:…?", answered `yes` only on an explicit click, after which OpenSSH itself appends
  the key to `known_hosts`), passphrase or password, confirmation. Cancel = refusal. A changed
  host key is never a prompt: OpenSSH refuses and the error is shown.
- **Bootstrap in the same session** (contract §8): print `SB:HELLO <os> <arch> <installed
  version>`, read `RUN` or `INSTALL <size> <sha256>` + the binary, check, install, `daemon
  --detach`, then `exec overseer proxy`. One authentication, one TCP connection, no second
  passphrase prompt; lines printed by shell rc files are skipped.
- **Mux** (contract §8a): 9-byte frames, streams opened by the app (`daemon` = the daemon socket,
  `tcp:<port>` = a remote localhost port), per-stream credit flow control (256 KiB) so a terminal
  flood never blocks the API, PING every 15 s. `DaemonConn::Tunnel` opens a mux stream where the
  local mode opens a Unix socket, so HTTP, SSE and WebSocket code is unchanged. `russh` is gone.
- **Reconnect**: the link is dead when the `ssh` process exits, the pipe closes or two PINGs go
  unanswered. The app then shows "Reconnecting (attempt n)" with the cause, retries with backoff
  (0.5 s doubling to 30 s), and stops only on permanent errors (host key verification failed or
  changed, unsupported platform, missing server binary). Prompts during a reconnect appear in the
  banner. Terminal panes wait up to 60 s for the new connection and re-attach; the daemon replays
  the scrollback (2 MiB ring), so nothing is lost. The event stream resumes with `Last-Event-ID`.
  Forward listeners stay bound across reconnects and use the new tunnel.
- **Ports**: the daemon watches the process trees of its terms for TCP listeners (Linux `/proc`,
  macOS `lsof -F`), publishes `port.opened`/`port.closed` and serves `GET /v1/ports` (contract
  §7a). On a remote host the app forwards each one through the tunnel to the same local port when
  free, else the next free one (up to +20, then any), bound to 127.0.0.1 unless "localhost only"
  is off, and notifies "Port 5173 forwarded — Open in browser". Settings: auto-forward on/off,
  ignore list (default 22), localhost only. A port stopped by hand is not auto-forwarded again on
  that connection. Local mode lists the same ports without forwarding.
- **macOS Local Network** (macOS 15+): even with Overseer switched on under Privacy & Security >
  Local Network, the `ssh` it spawns got `EHOSTUNREACH` to LAN hosts. The children are attributed
  to the app, and the build is ad-hoc signed, so its code identity changes on every update and
  macOS can keep the grant on an older build. Other ad-hoc apps that open the socket themselves
  work. So on macOS, when `ssh -G` shows no `ProxyCommand` or `ProxyJump`, the app resolves
  `HostName` and connects to it in-process (honoring `AddressFamily` and `ConnectTimeout`), binds a
  one-shot loopback listener and runs `ssh -o ProxyCommand='<app> --ssh-relay 127.0.0.1:<port>'`,
  token in `OVERSEER_RELAY_TOKEN`; that mode of the app's executable pipes stdin/stdout to the
  listener, which copies to the in-process socket. Loopback is not subject to Local Network
  privacy; `ssh` still does authentication, `known_hosts` and agents. `OVERSEER_SSH_RELAY=0`
  turns it off, `1` turns it on elsewhere. `local_network_status` triggers the macOS prompt once
  at startup (a UDP mDNS query to 224.0.0.251:5353, then an in-app connect to the host when its
  address is private: 10/8, 172.16/12, 192.168/16, 169.254/16, 100.64/10, fd00::/8, fe80::/10) and
  classifies the result: reached or refused = granted, `EHOSTUNREACH`/`ENETUNREACH`/`EPERM` with
  the mDNS send also blocked = denied, host blocked but mDNS allowed = unreachable. A "No route
  to host" then opens an "Allow local network access" panel (Privacy & Security, quit and reopen,
  try again, a note on ad-hoc builds) with the resolved addresses, interface, signature and raw
  error under Details. `NSLocalNetworkUsageDescription` is in `src-tauri/Info.plist`, which Tauri
  merges into the bundle; `NSBonjourServices` is not declared because it only gates Bonjour
  browsing, not a raw multicast send. Developer ID signing (or any stable identity) removes the
  stale-grant case.

## Mosh
Mosh replaces the SSH session with its own UDP protocol and its own terminal emulator: it carries
one interactive screen, not byte streams, so it cannot carry the daemon API, the event stream,
several terminals or forwarded ports, and its predictive echo works on its own screen model,
which ghostty-web does not expose. What users want from mosh here, sessions that survive roaming
and sleep, already comes from the daemon owning the PTYs plus the automatic reconnect and the
scrollback replay. Mosh is not added. A predictive local echo setting for very high latency links
is future work: it needs ghostty-web cursor and cell access to be done without corrupting the
screen, and a 15 s keepalive with a reconnect already covers the common failure (a dead link after
sleep).

## Consequences
- Requires an OpenSSH client on the machine running the app (always on macOS and Linux; built
  into Windows 10/11 as an optional feature). The error says how to install it.
- Win32-OpenSSH's `SSH_ASKPASS` support is not verified here; without it, prompts fail and the
  dialog shows ssh's error (keys in the agent and known hosts keep working).
- Every connection is a separate `ssh` process: two hosts = two processes; the app never uses a
  user's `ControlMaster` socket.
- `make test-remote` (Docker, `OVERSEER_DOCKER_TESTS=1`) runs the transport against real sshd
  containers: a `Match` block, `ProxyJump` through a bastion to a name only the bastion resolves,
  `ProxyCommand nc %h %p`, a host key prompt answered from the app, three terminals + API + ports
  over exactly one TCP connection, a killed session followed by reconnect and scrollback replay,
  and `python3 -m http.server` forwarded to a local port.
- Port detection is not implemented on Windows daemons.

## Addendum: stable macOS code identity

The in-process relay was not enough. On macOS 15+ a user with an ad-hoc build kept getting
"No route to host" with no Local Network prompt, and toggling Overseer under Local Network,
quitting and reopening changed nothing, while `ssh` in Terminal and another ad-hoc app worked.
TCC stores the grant against the app's designated requirement; for an ad-hoc signature that is
the cdhash of one build, so every update (Homebrew or the updater) is a new client, the listed
toggle belongs to a build that no longer exists, and macOS does not prompt again for the same
bundle id.

Decision: every macOS release build is signed with a stable identity: Developer ID when its
secrets exist, otherwise the persistent `Overseer Self-Signed` certificate (see
[docs/ci.md](../ci.md#macos-signing)). The DR becomes `identifier "io.github.basilelt.overseer" and
certificate root = H"<sha1>"`, identical across builds, which CI asserts with `codesign -d -r-`.
Hardened runtime stays off for self-signed builds (no notarization, no entitlement needed for a
network client); `NSLocalNetworkUsageDescription` is unchanged.

Consequences: after the first stable-signed install the user answers the prompt or toggles
Overseer once under Local Network (off and on when an old entry is listed), quits and reopens;
the grant then survives updates. The Details of the "Allow local network access" panel report the
signature (`selfSigned`, `developerId`, `adhoc`) and, for ad-hoc builds, the immediate
workaround: macOS attributes network access of a process started from Terminal to Terminal, so
running `/Applications/Overseer.app/Contents/MacOS/overseer-app` from Terminal connects
until a stable-signed build is installed. Rotating the certificate costs one more toggle.
Apple's TN3179 notes that Local Network privacy also uses the main executable's Mach-O UUID and
only guarantees reliable tracking for Apple-issued identities, so a Developer ID remains the
complete fix; the self-signed identity removes the per-build cdhash, the documented cause.

