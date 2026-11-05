# 0001 Language and stack

Status: superseded by 0008 (Rust)

## Context
Three binaries: daemon (Mac arm64, Linux amd64/arm64, pod), API (Kubernetes), CLI (Mac, VM).
The VM has node 24 and python3 but no Go toolchain; stations must not need a toolchain to run
Overseer. The Board (V2) is web. cmux-tui exposes agent/sidebar plugins installed from git
(format not yet inspected).

## Options
1. Go for daemon, API, CLI; TS types generated from `contract/` for the Board.
2. TypeScript everywhere (Node or Bun), single language, Board shares code.
3. Go daemon/CLI, TS API.

## Decision (recommended)
Option 1. Single static binaries (`CGO_ENABLED=0`) dropped on any station, cross-compiled in CI,
no runtime dependency on nvm-managed Node. Board: plain HTML + ES modules served by the daemon,
types generated with `json-schema-to-typescript`; Go types with `oapi-codegen` / `go-jsonschema`.
Toolchains are needed only on the build machine.

## Consequences
Two languages (Go + a little TS). Contract tests validate fixtures against the schemas from both sides.
If the cmux-tui plugin format turns out to require JS, the plugin is a thin JS shim calling the daemon.
