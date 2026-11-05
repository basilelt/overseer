# 0023 Windows: loopback TCP with a token, cfg-gated Unix code

Status: accepted

## Context
The desktop app runs on Windows; the daemon must build and serve there. Unix sockets exist on
recent Windows but std does not expose them, and named pipes need `windows-sys` plus a hand-written
overlapped server.

## Decision
- **Transport**: on Windows the daemon listens on `127.0.0.1:0` and writes
  `<state>/daemon.json` = `{"addr","token","pid"}` (`<state>` = `%LOCALAPPDATA%\overseer`,
  inheriting the user-only ACL of that directory). Every request needs
  `Authorization: Bearer <token>` or `?token=` (`board::authorized`, which also checks the Host is
  loopback). `overseer socket-path` prints the `daemon.json` path; `daemon::request` reads it.
  Same code path as `daemon --http` (ADR 0012), no new dependency.
- **Portability**: Unix-only calls go through `src/sys.rs` (`OpenOptionsExt`, `DirBuilderExt`,
  `PermissionsExt`, `MetadataExt`, `CommandExt::exec`, randomness, socket probe). On Windows the
  mode setters are no-ops, `exec` runs the child and exits with its code, randomness comes from
  std's OS-seeded `RandomState`. `/proc`, `ps`, `tmux` and `kill` stay as they are: they simply
  find nothing on Windows. PTYs use ConPTY through `portable-pty`; kill is `TerminateProcess`.
  `daemon --detach` uses `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`.
- **Verification**: `cargo clippy --target x86_64-pc-windows-gnu -- -D warnings` is clean for the
  library and the binary. Locally the bundled SQLite C build was replaced by a no-op compiler
  (`CC_x86_64_pc_windows_gnu`), since `check` does not link; CI builds for real on
  `windows-latest` (MSVC). Integration tests and the `overseer-api` dev-dependency stay
  Unix-only.

## Consequences
- Any local process of the same user that can read `daemon.json` can drive the daemon, the same
  trust model as the `0600` socket.
- tmux detection, hold and the held stub do nothing on Windows; terms replace them there.
