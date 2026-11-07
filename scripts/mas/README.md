# Mac App Store feasibility (shelved)

Question: can the full desktop Overseer (UI, local daemon, agents in terminals) ship on the Mac App
Store, where the App Sandbox is mandatory?

**Answer: no.** The Mac App Store is covered by the iPhone/iPad app on Apple silicon Macs
(remote over SSH). The full app stays notarized outside the store (DMG, Homebrew), like iTerm2,
Warp, Ghostty or VS Code.

This branch keeps the experiment as a record. It is not meant to be merged.

It also carries the storage change from PR #178 (`feat/app-storage-dir`), shelved with it:
everything under the state dir instead of `~/.overseer` (worktrees, uploads, task attachments,
review clones, shim) with a one-time migration, and macOS worktrees in
`~/Library/Developer/overseer/worktrees`. Under the sandbox the state dir is the container.

## How it was tested

`.github/workflows/mas-smoke.yml` runs on a macOS 26 arm64 runner (on push to this branch or by
hand):

- builds the debug app with `OVERSEER_DISTRIBUTION=play` (no self-updater) and the overlay
  `app/src-tauri/tauri.mas.conf.json` (bundle id `io.github.basilelt.overseer.mac`);
- ad-hoc signs it with `app/src-tauri/entitlements.mas.plist` (app-sandbox, network client/server,
  user-selected files, app-scope bookmarks) and the bundled `overseer` server with
  `entitlements.mas-helper.plist` (app-sandbox + inherit);
- launches it, drives the daemon from outside (`scripts/mas/smoke.sh`, `drive.py`);
- runs `scripts/mas/probe.c`, a sandboxed probe that mimics daemon → child spawns, PTYs, exec
  locations and folder grants. NSOpenPanel cannot be clicked in CI, so grants are simulated with
  `sandbox_extension_issue_file` / `sandbox_extension_consume` (what PowerBox does);
- a second variant adds an SBPL exception (`entitlements.exceptions.plist`) only to show what the
  sandbox blocks; it is not App Store compatible;
- collects every sandbox denial from the unified log (`report.py`, job summary, `mas-smoke`
  artifact).

Runs: [42381953821](https://github.com/basilelt/overseer/actions/runs/42381953821) (app + daemon,
PTY, grants), [42383294398](https://github.com/basilelt/overseer/actions/runs/42383294398) (agent
CLI exec, pipe mode).

## What works in the sandbox

- The app, the WKWebView UI and the bundled daemon start in the container
  (`~/Library/Containers/<id>/Data`, which is also `$HOME`).
- The daemon socket binds only with a short path: the default one is 118 characters, over the
  104-character `sun_path` limit, so `OVERSEER_SOCKET` must be short.
- Localhost listen/connect and outbound HTTPS.
- git and worktrees on a repo inside the container, with the real Xcode git (`/usr/bin/git` is the
  xcrun shim, which refuses to run sandboxed: "xcrun: cannot be used within an App Sandbox").
- Folder grants propagate: once a sandbox extension is consumed, the process and all its children
  (shell, git, scripts, grandchildren, a setsid orphan started earlier) can read and write the
  folder. Other sandboxed processes cannot. A process holding a grant can re-issue a token to
  another sandboxed process (app → daemon after a relaunch).
- Executing a signed inherit helper inside the app bundle.
- `/bin/sh`, `/bin/zsh`, `/bin/bash`, `/usr/bin/security`, `sysctl KERN_PROC_ALL`.

## What blocks

1. **No terminals.** `file-ioctl` on `/dev/ttys*` is denied for the slave side: TIOCSCTTY,
   TIOCSWINSZ, TIOCSETA/TIOCSETAW, TIOCGPGRP/TIOCSPGRP. `POST /v1/terms` fails with
   `spawn /bin/sh: Operation not permitted`; zsh has no job control. A `com.apple.sandbox.pty`
   extension does not help. Only the SBPL exception variant runs PTYs, and App Review does not
   accept custom sandbox profiles.
2. **No user-installed agents.** `process-exec*` is denied for everything outside the system and the
   app bundle: `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, npm globals, node. Tested
   with Claude Code (native install and npm), Codex (npm + node), with grants on the folder, the
   symlink, the real target, the single file, in read-write, read-only and
   `com.apple.sandbox.executable` classes, and with the binary copied into the container: always
   `execv: Operation not permitted`. **Grants give file access, never exec.** So even a chat-only
   edition (agents over pipes with `--output-format stream-json`) cannot start the user's agents.
3. **Bundling agents is not an option**: Claude Code is proprietary and cannot be redistributed in
   the app; a bundled Codex would need node too, would only update with the app, and is the
   "code that adds features" App Review guidelines 2.4.5/2.5.2 forbid.
4. **Other losses**: `ps` and `lsof` are blocked (`forbidden-exec-sugid`), `proc_listpids` returns
   nothing (no ports/process view); tmux cannot run; `$HOME` is the container, so `~/.claude`,
   `~/.codex`, `~/.gitconfig`, `~/.ssh`, `~/.zshrc` are not the user's; writing the launchd agent to
   `~/Library/LaunchAgents` is denied; the self-updater, Homebrew path and server downloads are not
   allowed in a store build.

## How other apps deal with it

- Terminal.app and Xcode are Apple's own and not subject to these limits.
- iTerm2, Ghostty, Warp, WezTerm, Kitty, Alacritty, VS Code, Cursor, the Claude and ChatGPT desktop
  apps are notarized and distributed outside the Mac App Store.
- Store "terminals" are SSH only (Termius store build, Secure ShellFish, Prompt), need a separately
  installed unsandboxed helper (Prompt's local terminal), or bundle their own commands / emulate
  Linux in-process (a-Shell, iSH).

## Not verified

- A real NSOpenPanel → security-scoped bookmark → `startAccessingSecurityScopedResource` round trip
  (grants were simulated with the private extension API).
- The daemon's launchd plist, agent hook install and `shellenv` under the sandbox (not triggered).
- x86_64 and release builds; App Review's actual decision on any of this.

## If it is ever revisited

The only shape the store allows is the remote one (UI + SSH to a machine running the daemon),
which the iPad app on Mac already provides. A native sandboxed desktop build in remote-only mode
would add Intel Macs and the desktop layout, at the cost of a separate build flavour (no local
daemon, no updater, built-in russh client with in-app keys, `.pkg` + provisioning profile + a
`store.yml` job).
