# QA path

The walk a QA agent takes through the app before a PR is marked ready. It uses the app the way a person does: the production frontend, a real daemon, real agent processes and terminals, every control clicked. It complements the tests; it does not replace them (see "QA like a human, with real agents" in `AGENTS.md`).

- **Feature PR**: run [Setup](#setup), the smoke steps **A1–A3**, every area the PR touches, and **M** (life events).
- **Branch that merges several others, or before a release**: run the whole path.
- **Bugs found on the way are fixed immediately**: in this PR when they belong to the change, otherwise at once in their own fix PR, even when they predate the branch. A QA run never ends with a list of "follow-ups".
- **Report**: the PR lists the step IDs you ran, what failed and was fixed (with the test that now covers it, or the PR that fixed it), and the steps you could not run, with the reason.

## Setup

The sandbox never touches the user's own daemon, state or sessions.

```sh
scripts/qa/sandbox.sh build    # daemon (cargo, -j 2) and production frontend (not VITE_MOCK) into $QA_DIR/dist
scripts/qa/sandbox.sh start    # sandboxed daemon, loopback proxy to its socket, static server for the app
scripts/qa/sandbox.sh crash    # kill -9 the daemon (life events)
scripts/qa/sandbox.sh stop     # stop everything; `clean` also deletes $QA_DIR
```

- `QA_DIR` defaults to `/tmp/sbqa`. Keep it short: a unix socket path must stay under about 100 bytes.
- The daemon runs with its own `HOME`, state dir, runtime dir, Claude and Codex config, `GIT_CONFIG_GLOBAL=/dev/null`, and `tests/stubs` first on `PATH`. Sessions therefore start real processes from the stub CLIs: they print a `❯` prompt, echo what you type (`got: …`), and log their command line to `$QA_DIR/stub.log`. Typing `done` ends a turn, so a task moves to In testing.
- A sample git repository is created at `$QA_DIR/home/proj`. Add files there as a step needs them.
- Drive the app with Playwright and `scripts/qa/drive.py`: `launch(p, mode=...)` opens a browser with `scripts/qa/shim.js`, which replaces the Tauri IPC with calls to the proxy (requests, `/v1/events` SSE, terminal websockets, ports). `open_app(pg, "?theme=light")` loads it and dismisses the welcome tour. Set `QA_CHROME` to use a specific Chromium binary.
- **Modes**: QA runs in every mode a user can be in. `mode` (or `QA_MODE`) picks one:
  - `web`: Chromium at 1440×900, the desktop layout in a browser.
  - `android`: Chromium emulating a Pixel 7 with touch (the Android app's WebView is Chromium); phone layout.
  - `android-tablet`: Chromium emulating a Galaxy Tab S9 portrait (640 px), phone layout; `android-tablet-landscape` (1024 px) is the desktop layout with touch, right at the switch point.
  - `iphone`: WebKit emulating an iPhone 15 with touch (the iOS app is WKWebView); phone layout.
  - `ipad`: WebKit, iPad Pro 11 portrait (834 px); the phone layout, since the app uses it on mobile below 1024 px.
  - `ipad-landscape`: WebKit, iPad Pro 11 landscape (1194 px); the desktop layout with touch.
  - `desktop`: the real Tauri app, below.
  The WebKit modes load the app from the proxy port (`QA_PORT`), which also serves `$QA_DIR/dist`, so the API is same-origin. WebKit has no switch to turn CORS off. The first run needs `python3 -m playwright install webkit` (and `install-deps webkit` as root).
- **Layout check in every mode**: `PYTHONPATH=scripts/qa python3 scripts/qa/overflow.py [modes…]` opens Agents and Tasks in each mode, saves `$QA_DIR/overflow-<mode>-<view>.png`, and fails when a button, input or link sits outside the screen (rows that scroll sideways are allowed). Look at the screenshots as well.
- Read the API directly when a step needs proof: `curl http://127.0.0.1:47811/v1/terms`, `/v1/tasks`, `/v1/lines?all=true`, `/v1/ports`.
- **Chat with real content**: start a stub session with `POST /v1/terms {"cwd": "<proj>", "cmd": ["claude", "--session-id", "<uuid>"]}`. Then write its transcript to `$QA_DIR/home/.claude/projects/<cwd with every non-alphanumeric char replaced by ->/<uuid>.jsonl`, one JSON object per line (`type` user or assistant, `message.content` with text, `image`, `tool_use` or `tool_result` blocks). Only append to it: the reader follows appended lines and ignores in-place edits.
- **The desktop app (WebKit)**: headless Chromium is not the engine users run. The desktop app renders with WebKitGTK on Linux and WKWebView on macOS, and they differ on CSS (`user-select` only works as `-webkit-user-select`), text selection, drag gestures and the clipboard. After `start`, run `scripts/qa/sandbox.sh desktop-build` once (debug Tauri build, `-j 2`), then `desktop`: it starts Xvfb on `:$QA_DISPLAY` (default 93, Mesa EGL so the NVIDIA driver does not crash it), the Vite dev server on port 1420 and the real app, with its own `HOME` and connected to the sandboxed daemon. Drive it as a person with `xdotool` (`DISPLAY=:93 xdotool mousemove X Y mousedown 1 …`, `type`, `key`), take screenshots with `sandbox.sh shot <name>` (written to `$QA_DIR/<name>.png`), and read the clipboard with `DISPLAY=:93 xclip -o -selection clipboard`. Coordinates are screen pixels of the 1440×900 display. `desktop-stop` stops the app; `stop` stops it too. It needs `Xvfb`, `xdotool`, `xclip`, ImageMagick and the WebKitGTK 4.1 development libraries. Steps marked **(desktop)** must be run there; the others run in each mode they apply to (`web` and `desktop` for desktop-layout steps, `android`, `android-tablet`, `iphone` and `ipad` for **L**, `android-tablet-landscape` and `ipad-landscape` for touch on the desktop layout).
- **Gotchas**: `window.__qaLog` lists the Tauri commands the shim does not handle. A feature that depends on one of them needs it added to `shim.js`. Never `pkill -f $QA_DIR`, because it matches your own shell; `sandbox.sh stop` kills by pid. Delete copied transcripts afterwards if they came from real sessions.

## The path

Each step is **Do → Expect**. Check every screenshot for overlaps, clipped text and unreadable contrast, and keep the browser console free of errors.

### A. Start and connect

- **A1** Open the app → the title bar shows **Local** with a green dot and the CPU/RAM chip. The welcome card offers **Skip** and **Take the tour**.
- **A2** **Take the tour**, step through it, close it → every step points at a visible control. Reload: the tour does not come back.
- **A3** Reload the app, then stop and start the daemon (`sandbox.sh stop`, then `start`) → the app reconnects on its own, without an error left on screen.

### B. Projects and tasks (Kanban)

- **B1** Tasks → **Create Project** → type the repo path → Enter → the project appears in the project filter.
- **B2** **Add Task**: title, description, attachments (paste or drop a file), project, agent, model, depends on, profile, effort → **Create task** → a card appears in Pending with the right project, agent and effort.
- **B3** Drag the card to **In progress** → a `task/<n>-<slug>` worktree is created and the agent starts there (`stub.log` shows `--session-id`). The card shows progress.
- **B4** Open a card → the detail modal shows the Urgent and Important toggles, Depends on and Messages. Send a message → it shows "delivered when it starts", or reaches the running agent.
- **B5** **Matrix**, search, the project filter, **Hide session tasks** → the board filters as labelled. **Organize** proposes, and never moves anything without confirmation.
- **B6** A task that depends on another stays in Auto until the other one is done.
- **B7** **(desktop)** With 30+ cards in two columns (Completed expanded), drag a card across the board and back → the ghost follows the cursor without stutter, the drop line moves card by card, and no text on the board gets highlighted. `PYTHONPATH=scripts/qa python3 scripts/qa/kanban_drag.py` repeats it under 4× CPU throttling and fails when one drag forces more than one layout per ten pointer moves (`DRAG_MOVES`, `DRAG_LAYOUT_BUDGET`). On the mock build, add `&scale=200` to the query for a heavy board.

### C. Agent sessions

- **C1** Agents → open the task's session → the terminal shows the agent prompt. Typing reaches the agent (`got: …`).
- **C2** Type `done` → within a few seconds the task moves to **In testing** and the session goes idle.
- **C3** Side panel: Hold, Pin, Close line, Last message, Work log, Todos, Changes, **Open review** → each acts and updates. A held session shows **Resume in terminal**, and resuming relaunches the agent with `--resume <same session>`.
- **C4** Start a session in a folder that is not a git repository (for example the agent started by the feedback sheet) and select it → the side panel shows **CHANGES** with "Not a git repository, so there are no changes to show" and no **Open review** button. The browser console has no errors and the network log has no 4xx. Select a session in a git repository → **CHANGES** and **Open review** are back.
- **C5** Hold a session, then click **Resume** in the side panel header (not in the terminal) → the session is active again, the button shows **Hold**, and `stub.log` shows `--resume <same session>`. The held terminal's "press Enter to resume" prompt is answered by the click.
- **C6** Hold and resume the same session three times in a row (side panel **Hold**, then **Resume**) → the state flips each time, the terminal shows "press Enter to resume" while held and the agent prompt after, and `stub.log` has one `--resume <same session>` per cycle. Repeat with `sandbox.sh crash` and `start` while held and while active: the state survives.

### D. Review

- **D1** Click an In testing card → the review dialog shows the diff, file tree, Viewed checkboxes, Unified or Split, List or Graph, and the commits since main.
- **D2** Add a comment → **Request changes** → the agent receives the comments and the task goes back to In progress.
- **D3** Change a file in the worktree → **Commit** → a commit appears and the task completes. With a clean worktree it shows "nothing to commit".
- **D4** Tick **Approve the PR** and **Merge the PR** → **Approve** → the task completes and the agent's terminal shows the instruction (approve, then merge).
- **D5** Same as D4 on a task whose agent is not running → toast "the agent is not running; start the task again", and the status is unchanged.
- **D6** **Reject** → the task returns to In progress, or to Pending when no agent runs.

### E. Terminals and tabs

- **E1** Tab bar **+** → **Shell** → a shell tab opens in the workspace folder. Type `echo hi` → output appears.
- **E2** Run `sleep 30`, press Ctrl+C, then `exit` → the command stops and the tab closes on its own.
- **E3** Ctrl+W on a live shell → dialog "End this session?". **Cancel** keeps the tab. **End session** closes it and the terminal leaves `/v1/terms`. **Hide tab (keep running)** removes the tab while the session keeps running (visible in By status).
- **E4** Ctrl+W on an exited terminal or a non-session tab → it closes without asking. The reopen-tab shortcut brings back the last closed tab.
- **E5** Hover a tab with a cut title → the full title shows, and the close button stays where it was.
- **E6** Split the pane right and down, move tabs between panes, close a pane → the layout follows, and nothing is lost on reload.
- **E7** Select text in a terminal → it is copied (desktop shell only; note it when it cannot be checked in Chromium).
- **E8** Drag-select a line that touches the first or last row of a terminal with no scrollback (a fresh shell, or the prompt of an agent at the bottom) → the whole dragged range is selected and copied, not a single cell.
- **E9** Settings → Terminal → turn **GPU rendering** on (off by default) → open terminals redraw with WebGL: `seq 100000` scrolls smoothly, the wheel and Shift+wheel scroll the scrollback, drag-select copies the text, colours and the cursor look as before in both themes. Reload: the option is kept. Turn it off → terminals return to the normal renderer with their content and scrollback intact. Where WebGL is unavailable (`--disable-gpu`), turning it on still draws terminals with the normal renderer.
- **E10** **(desktop)** Repeat E9 in the desktop app under Xvfb: with GPU rendering on, select text and check it with `xclip`, scroll a long output, resize the window and split a pane → no blank or garbled terminal; turn it off → the normal renderer is back.

### F. Chat

- **F1** Switch a session to **Chat** → messages, timestamps and assistant text render. An image the user sent shows as a preview.
- **F2** A `!command` step shows as `! cmd`, and expanding it shows the command and its output. Typing `!echo x` in the composer sends it to the agent.
- **F3** Click a file path in assistant text (relative, as agents write it; a path with spaces, such as `docs/J3 modifications.pdf`, is a link when it is written as a whole `code span`, and stays plain text in ordinary prose) → the file viewer opens: a PDF renders, an image shows, text shows. A missing file gives a clear not-found error. **Copy path** copies it.
- **F4** Expand a Read, Edit or Write tool step → click its path → the same viewer opens.
- **F5** Composer: Enter sends, Shift+Enter adds a line, `/` lists commands, attaching a file works. A slash-command picker such as `/advisor` shows as a prompt card (needs a real agent; say so when only the stub was available).

### G. Sidebar and workspaces

- **G1** Workspaces versus **By status** → the lists match the sessions. By status groups by Working, Idle and Held.
- **G2** Add a workspace for a parent folder and one for a child folder, then open a shell in the child → it counts under the child, not the parent. Task sessions stay in their project's workspace, and no stray **Other sessions** group appears.
- **G3** Groups: Cmd/Ctrl+click several sessions → Add to group → the group lists them and broadcast reaches them.
- **G4** Drag the sidebar narrow → the mode tabs stay on one line, cut with an ellipsis. Drag it back.
- **G5** Filter workspaces or sessions → the list narrows. Archive and restore a workspace.

### H. Ports

- **H1** In a terminal, start two servers in the background (`python3 -m http.server <port> &`) → the title-bar Ports chip counts them. The panel lists port, address, process, pid and project.
- **H2** Stop one with **Kill process** → only that row disappears, the chip count drops, and the terminal that launched it keeps running.
- **H3** A server started outside Overseer is not listed. `POST /v1/ports/<port>/kill` for it returns 404 and it keeps running.

### I. Settings

- **I1** Language: Deutsch, Español, Français, then English → the UI switches at once. Reload: the choice is kept.
- **I2** Theme light and dark → both readable on every screen you visited.
- **I3** Server: **Keep server running after reboot**, and **After a server reboot or restart** (Restore agent sessions, shells only, Nothing) → the choice is saved (`GET /v1/lifecycle`).
- **I4** Notifications: **Send test**, per-event choices → saved. Profiles: **Edit…** opens and saves.
- **I5** Sidebar: open Settings, then General, Terminal, Agents, Notifications, Connection, Updates, Help in turn → each shows only its own sections; categories with no section for this machine are absent. Change one setting per category, reload → each is kept.
- **I6** Search: type `notif`, then `reboot` → only the matching rows stay, highlighted, with a count per category; in Notifications, Connection (server) and Updates the other rows of the section are hidden too. Type `zzzz` → an empty state. In Français, type an accented word (`échoue`) and the same without the accent → same rows. **Esc** clears the search, a second **Esc** closes Settings.
- **I7** Language from the new layout: switch it in General → the sidebar, the search placeholder and the rows change language at once.
- **I8** Phone (`?phone=1`): Settings opens on a list of categories; a category opens its rows with a back button; searching shows results across categories; no horizontal scroll at 390 px.

### J. Feedback

- **J1** Title-bar feedback button → the sheet opens. **Bug** ticks Include diagnostics, **Idea** unticks it, and **Bug** ticks it again.
- **J2** **Preview what will be sent** → shows the payload, with secrets masked. Cancel closes without sending.
- **J3** **Send with Claude Code**, then reload the app while the agent still works. Type `ISSUE_URL=https://github.com/acme/proj/issues/12` and Enter in the agent's terminal → the "Issue created" toast appears with **View**; View opens the "Feedback sent" dialog with the link, and **Copy link**, **Open** and **Done** work (Done closes it). Reload again → no second toast. Reload before the link with noise lines `ISSUE_URL=noise-N` in the terminal → no toast. After `sandbox.sh crash` and `start`, the watch comes back for the restored Feedback terminal.

### K. Other views

- **K1** Tab bar **+** → Browser, Review changes, Git graph, Worktrees, System, Usage, PR reviews → each opens with content or a clear empty state, and no console errors.
- **K2** The command palette lists and runs commands. Keyboard shortcuts in Settings match what they do. Ctrl+P (Cmd+P on macOS) opens the palette outside a terminal and Escape closes it; in a shell terminal Ctrl+P reaches the shell (history recall) and Ctrl+Shift+P opens the palette; Cmd/Ctrl+K still works. The Settings shortcut list shows Ctrl+K, Ctrl+Shift+P and Ctrl+P for the palette.
- **K3** PR reviews while offline: put a `gh` stub first on the daemon's `PATH` whose `api graphql` fails with a connection error and whose `auth token` succeeds (`auth status` fails, as the real one does without a network), restart the daemon, open **PR reviews** → "Could not load review requests" with "GitHub could not be reached: …" and the connection error, no sign-in hint. With `auth token` failing too → "GitHub CLI not signed in" and `gh auth login`.

### L. Phone

- **L1** `?phone=1` at 390×844 → Agents, Tasks, Review and More tabs. Session chips scroll, and the composer and **Resume in terminal** are reachable.
- **L2** Tasks: the column chips switch columns, and long-press moves a card. The review dialog footer stays usable, wrapping instead of clipping.
- **L3** Run **L1** and **L2** in the `android`, `android-tablet`, `iphone` and `ipad` modes, then open Agents and Tasks in `android-tablet-landscape` and `ipad-landscape`: the desktop layout fits 1024 and 1194 px, and toolbars wrap instead of pushing buttons such as **Add Task** off the screen (`overflow.py` checks every mode).

### M. Life events

- **M1** With an agent session, a shell (in a workspace, renamed) and a task in progress: `sandbox.sh crash`, then `start` → the log says `daemon restarted; restored N agent sessions and M shells`. Terminals keep their ids, workspace and title, and the agent is relaunched with `--resume <same session>` (`stub.log`).
- **M2** Reload the app → tabs, workspaces and the selected session come back. The resumed agent answers input.
- **M3** Set **After a server reboot or restart** to Nothing, then crash and start → nothing is restored.
- **M4** Idle: leave the app untouched for 70 s with a session open → no DOM changes and no repaints (count `MutationObserver` records over 10 s).

## Keeping the path current

The path describes the app as it ships, so it changes with the app. A PR that changes what the user can do updates this file in the same PR.

- **New feature**: add **Do → Expect** steps in the area where a user meets it, or a new lettered section for a new area. Cover the happy path, the error the user can hit, and, when the feature keeps state, what happens across a reload and a daemon crash (add to **M**).
- **Changed behaviour**: rewrite the steps that pin the old behaviour. Never leave a step that describes something the app no longer does.
- **Removed feature**: delete its steps in the PR that removes it.
- **IDs**: append new numbers within a section, never renumber existing steps (PR reports refer to them), and do not reuse the number of a deleted step.
- **Harness**: if the feature calls a Tauri command the shim does not handle (`window.__qaLog`), extend `scripts/qa/shim.js` in the same PR. Keep `scripts/qa/sandbox.sh` in step with new environment variables the daemon reads.
- **Write what a person does and sees**, not how the code does it. Name the visible labels exactly as the English UI shows them, and update them when the strings change.
