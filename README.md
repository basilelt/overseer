<p align="center">
  <img src="app/public/logo.svg" width="80" alt="Overseer logo">
</p>

<h1 align="center">Overseer</h1>

<p align="center">
  <b>Run and supervise your AI coding agents side by side</b><br>
  on this machine or on a server over SSH, with their terminals, tasks and reviews in one place.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#features">Features</a> ·
  <a href="docs/user-guide.md">User guide</a> ·
  <a href="CHANGELOG.md">Changelog</a>
</p>

<p align="center">
  <img src="docs/media/tour.gif" width="960" alt="The guided tour of the desktop app">
</p>

Overseer is a desktop app (Linux, macOS, Windows), a phone and tablet app (Android, iOS) and a small
daemon. It works with Claude Code, Codex, Gemini CLI, Antigravity, opencode, Amp, Copilot CLI and
Cursor Agent. Each agent session is a **line**: one agent, in one worktree, on one machine, for
one owner. The daemon owns the terminals, so closing the app, losing Wi-Fi or putting the laptop
to sleep stops nothing; open the app again and every session is where you left it.

New here? Install the app, open it, and take the tour: it shows each part of the window in about
a minute. You can replay it any time from Settings → Help.

## Features

### Every agent in one window

Workspaces group sessions by project. Each session opens in a tab with its live terminal, and
the side panel shows what the agent is doing: last message, work log, todos, edited files and
changes. Switch to **By status** to see at a glance who needs your input, who is running and who
is done. Agent tabs also have a **Chat** view rebuilt live from the agent's session file.

![Workspaces](app/docs/screenshots/workspaces-light.png)

<details>
<summary>By status, and broadcast to many agents at once</summary>

![By status](app/docs/screenshots/agents-light.png)
![Broadcast](app/docs/screenshots/broadcast-dark.png)

</details>

### A task board that starts agents for you

Write a task, pick the agent, its model and reasoning effort, and start it: Overseer creates a
worktree on a new branch and types the task into the agent. Tasks in the **Auto** column start on
their own, in order, within your concurrency limit. A new task can be filled from the project's
GitHub or GitLab issues or Jira tickets, and agents can read and update tasks through the
built-in MCP server.

![Tasks](app/docs/screenshots/kanban-dark.png)

<details>
<summary>New task with the agent picker</summary>

![New task, agent picker](app/docs/screenshots/add-task-light.png)

</details>

### Live review of what agents change

A GitHub-style review of an agent's changes while it works. Comment on lines, suggest edits, and
send the comments back to the agent; threads follow the code as it changes. Approve, reject or
commit from the Kanban card when the task is in testing.

![Live review](app/docs/screenshots/review-light.png)

<details>
<summary>A review thread with a suggestion</summary>

![Review thread with a suggestion](app/docs/screenshots/review-thread-dark.png)

</details>

### Git graph and worktrees

A per-project git graph that stays fast on large histories, with agent branches, tasks and PRs
on it. Worktrees live in one store outside your repositories, show whether they are in use, idle
or stale, and are cleaned up safely.

![Git graph](app/docs/screenshots/graph-light.png)
![Worktrees](app/docs/screenshots/worktrees-dark.png)

### Remote machines over SSH

Point the app at any host that works with `ssh <host>`: your `~/.ssh/config`, `ProxyJump`,
`ProxyCommand`, agents and keys are used as is. The app installs the daemon on first connection
and carries terminals, events and forwarded ports over a single SSH connection. Nothing listens
on a TCP port on the server.

### And also

- **Phone and tablet apps** for Android and iOS (iPhone and iPad), synced with the desktop: follow
  and answer your agents from anywhere ([docs/mobile.md](docs/mobile.md)). Tablets get the phone
  layout in portrait and the full desktop layout, with touch, in landscape.

  <p>
    <img src="app/docs/screenshots/ipad-portrait-light.png" height="420" alt="Tasks on an iPad in portrait">
    <img src="app/docs/screenshots/ipad-landscape-dark.png" height="420" alt="Tasks on an iPad in landscape">
  </p>
  <p>
    <img src="app/docs/screenshots/android-tablet-portrait-dark.png" height="420" alt="An agent asking a question, on an Android tablet in portrait">
    <img src="app/docs/screenshots/android-tablet-landscape-light.png" height="420" alt="Agents on an Android tablet in landscape">
  </p>
- **Notifications** when an agent needs input or finishes, from agent hooks Overseer can
  install for you.
- **Usage and system monitor**: agent quotas, token usage, CPU and RAM in the title bar.
- **Settings with search**: categories in a sidebar, and a search box that finds any setting by name or description, in your language or in English.
- **Command palette** (history, actions, sessions) and keyboard shortcuts.
- **Organize my sessions**: a proposed workspace layout you approve before anything moves.
- **Built-in browser** and port forwarding for the dev servers your agents start; the Ports panel
  lists them and stops any one an agent left running in the background.
- **Team mode**: a shared API merges every station's lines, relays commands between stations,
  and publishes events and signed webhooks for other tools.
- **Updates** in the app, and a feedback button with optional diagnostics.

## Install

### macOS (Homebrew)

Install the desktop app, and the `overseer` CLI/daemon if you want it on this machine:

```sh
brew tap basilelt/tap https://github.com/basilelt/homebrew-tap.git
brew install --cask overseer
brew install overseer
```

Upgrade with `brew update && brew upgrade --cask overseer && brew upgrade overseer`.

The app is not signed yet. If macOS refuses to open it, run
`xattr -dr com.apple.quarantine /Applications/Overseer.app` once.

### Linux and Windows

Download from the latest release (`.AppImage`, `.deb` or `.rpm` on Linux x64 and arm64,
`-setup.exe` or `.msix` on Windows x64 and arm64). The `.msix` is self-signed: import
`overseer-msix.cer` from the release into Local Machine > Trusted People once, then open the
`.msix`; it does not update itself, use the `-setup.exe` for in-app updates. For example:

```sh
gh release download --repo basilelt/overseer --pattern '*.AppImage'
chmod +x Overseer_*.AppImage
```

The `overseer` CLI/daemon alone is a single static binary per platform
(`overseer-<version>-<target>`), e.g. `--pattern '*-x86_64-unknown-linux-musl'`. Remote hosts
need nothing: the app installs the daemon over SSH on first connection.

### Android and iOS

Android: install and update the APK from the releases with Obtainium, or download it with
`gh release download --repo basilelt/overseer --pattern '*android.apk'`. iOS: not distributed
publicly; building it needs your own Apple developer account and TestFlight. See
[docs/mobile.md](docs/mobile.md).

#### Google Play (closed testing)

Overseer is in closed testing on Google Play before its public release. To install it from Play
and get updates there:

1. With the Google account of your Android phone, join the
   [Overseer testers group](https://groups.google.com/g/overseer-testers) ("Join group").
2. Open the [testing invitation](https://play.google.com/apps/testing/io.github.basilelt.overseer)
   with the same account and tap **Become a tester**. If it says the app is not available, the
   group join has not reached Play yet: wait a few minutes and reload.
3. Install Overseer from the
   [Play Store page](https://play.google.com/store/apps/details?id=io.github.basilelt.overseer)
   (the link from the invitation page). Updates arrive through Play like any app.
4. Stay in the group and keep the app installed: Play only opens the app to everyone after
   testers have used it for 14 days. Send feedback from the app (More > Send feedback) or on the
   Play listing.

No server yet? Tap **Try the demo** on the first screen to explore the app with sample data.

Already using the APK from the releases? Both builds are signed with the same key, so Play
installs over it and keeps your servers; from then on Play updates the app instead of the in-app
updater.

### Updates

From 0.4.0 the app updates itself (AppImage and Windows installer) from the public GitHub
releases; no token or sign-in is needed. Homebrew installs get an Update button that runs
`brew upgrade` for you (the command stays copyable); `.deb`/`.rpm` installs link to the release.

## Components

- `overseer`: CLI and daemon of a station. Reads the agents' sessions, tmux and git; installs
  agent hooks on request; serves a local API on a Unix socket; syncs with the API.
- `overseer-api`: shared registry (Rust, SQLite). Station and user tokens, merged reads,
  command relay and audit, SSE events, webhooks, `POST /v1/lines` for feedback-hub.
- Board: web page served by the daemon on loopback; lines grouped by ticket, PR and CI, actions.
- Desktop app support (in the daemon): terminals owned by the daemon, a local task Kanban that
  starts agents in worktrees, an activity panel from transcripts, an MCP server for agents, a
  per-project git graph, worktrees managed in one store outside the repositories with safe
  cleanup ([ADR 0031](docs/adr/0031-git-graph-and-worktrees.md)), and a server mode reached over
  SSH ([contract](contract/app-daemon.md), user guide section 14).
- [Desktop app](app/README.md): Tauri client for the daemon, local or remote over SSH; terminals,
  activity, diffs and a task board.

## CLI quickstart

```
make build                                   # target/release/overseer, target/release/overseer-api
cp target/release/overseer ~/.local/bin/
export OVERSEER_STATE_DIR=~/.local/state/overseer

overseer ls                               # this station's lines
overseer doctor                           # what is installed and what is missing
overseer hooks install                    # dry run; add --apply to write

overseer daemon --http 127.0.0.1:0 &      # daemon with the Board
overseer board                            # prints the Board URL

printf '%s\n' "$STATION_TOKEN" | overseer enroll --api http://api.example.com:8080
overseer sync --once                      # push this station's lines to the API
printf '%s\n' "$USER_TOKEN" | overseer login --api http://api.example.com:8080
```

State lives in `~/.local/state/overseer` (Linux) or `~/Library/Application Support/overseer` (macOS); `OVERSEER_STATE_DIR` overrides it, and `bin/overseer` defaults it to `<checkout>/.state`.
Service files for the daemon and sync (systemd user units, launchd agents) and a runner image are
in `deploy/`; Overseer never installs them.

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) explains how to propose a change,
and [AGENTS.md](AGENTS.md) holds the rules every change follows (stability, energy, tests, one
draft PR per feature). [app/README.md](app/README.md) covers the app's development loop and the
mock daemon. `make test` and `make lint` run offline; tmux tests use an isolated server. Report
security problems privately, as described in [SECURITY.md](SECURITY.md).

## Documentation

- [User guide](docs/user-guide.md): setup per station, every command, Board, API, desktop app, troubleshooting.
- [Brief](BRIEF.md): context, architecture and plan.
- [Architecture decisions](docs/adr/).
- [CI and releases](docs/ci.md).
- [Changelog](CHANGELOG.md).
- [Contracts](contract/): `api.openapi.yaml`, `daemon.openapi.yaml`, JSON schemas.

## License

Overseer is free software under the [GNU Affero General Public License v3.0 or later](LICENSE).
You can use, change and share it; if you distribute a modified version, or let people use one
over a network, you must publish its source under the same license.
