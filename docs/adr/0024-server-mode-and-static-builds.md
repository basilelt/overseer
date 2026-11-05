# 0024 Server mode: detach, version, socket path and static builds

Status: accepted

## Context
In remote mode the app SSHes to a server, installs `overseer` under
`~/.overseer/server/<version>/`, starts the daemon and talks to its Unix socket over SSH
`direct-streamlocal` channels (the Mac is only a screen and keyboard).

## Decision
- `overseer daemon --detach [--interval N] [--http …]`: when `<state>/daemon.lock` is held,
  prints the socket path and exits 0. Otherwise re-executes itself as `daemon` (no fork in a
  threaded process) under `setsid`, stdin from `/dev/null`, stdout/stderr appended to
  `<state>/daemon.log`, pid in `<state>/daemon.pid`, then waits (10 s max) until
  `GET /v1/version` answers before printing the path, so `ssh host … --detach` returns promptly
  and never holds the SSH channel open.
- `overseer version [--json]` and `GET /v1/version`: `{version, commit, os, arch, protocol}`;
  `commit` comes from `build.rs` (`OVERSEER_COMMIT` or `git rev-parse --short=7 HEAD`).
  `protocol` (1) changes on any breaking change of `contract/app-daemon.md`.
- `overseer socket-path`: the fixed, discoverable path (`<state>/daemon.sock`).
- `make dist` (`scripts/dist.sh`) builds static `*-unknown-linux-musl` binaries into `dist/` for
  every installed target with a usable C compiler: `<arch>-linux-musl-gcc`, else `musl-gcc`,
  else, for the host architecture only, the host `gcc` with `-DSQLITE_DISABLE_LFS` (glibc headers
  would otherwise reference `open64`-style symbols musl does not export; off_t is 64-bit on
  64-bit Linux anyway). aarch64 needs `aarch64-linux-musl-gcc` or a native arm64 runner (CI uses
  `ubuntu-24.04-arm` with `musl-tools`). macOS binaries (`aarch64-apple-darwin`,
  `x86_64-apple-darwin`) are built on the macOS CI runner.

## Consequences
- A Unix socket path must stay under ~104–108 bytes: deep `OVERSEER_STATE_DIR`s fail to bind
  (the error says so); the default path is short.
- The detached daemon is not supervised; systemd/launchd units in `deploy/` remain the way to run
  it permanently.
