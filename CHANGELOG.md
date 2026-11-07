# Changelog

## Unreleased

### Everything under the state directory

- **No more `~/.overseer`**: worktrees (`worktrees/`), files sent from the app (`u/`), task
  attachments (`tasks/`), PR review clones (`reviews/`) and the stable `bin/overseer` shim now live
  in the state directory (`~/Library/Application Support/overseer` on macOS,
  `~/.local/state/overseer` on Linux). The first start moves `u/` and `tasks/` over and leaves a
  symlink at the old path; existing worktrees and review clones stay where they are and keep working;
  the old shim becomes a link to the new one and agent hooks are rewritten to it. Server binaries on
  remote hosts stay in `~/.overseer/server`, so any app version can still upgrade any server.

### Sessions survive a server crash

- **A crashed or restarted server brings its sessions back**: the server restored terminals and
  resumed agent sessions only after a machine reboot. When it crashed or was restarted on the same
  boot, every session was lost, and the empty list it then saved also emptied the next reboot's
  restore. It now restores on every start except a hot upgrade, following Settings → Server →
  **After a server reboot or restart**.
- **One process out of memory no longer ends every session**: the server's systemd unit stopped as
  a whole when the kernel killed any one of its processes for memory, such as a build an agent ran.
  The unit now sets `OOMPolicy=continue`, so only that process dies. An installed unit is updated
  when the server starts.

### Spanish and German, and easier translations

- **Two new languages**: Settings → Language now lists Español and Deutsch next to English and
  Français. The list comes from the locale files, so a new language appears without code changes.
- **System language by default**: a fresh install starts in the system language when Overseer
  has it, English otherwise. A saved choice is kept; a saved language that no longer exists falls
  back to the default.
- **English only when coding**: every string now lives in `app/src/lib/i18n/en.ts`. Other languages
  may lag behind and show English for missing keys; `pnpm i18n` fills them with `claude -p`.
### Less energy when you step away

- **An untouched window rests like an unfocused one**: after a minute with no key, click, mouse
  move or scroll, Overseer treats a focused window as inactive. It stops the cursor blink,
  pauses looping animations, caps terminal repaints at 15 per second and stops the chat clock,
  exactly as when another app is in front. The next input brings everything back at once.
- **Smaller desktop startup**: the Android update checker now loads only on phones.
### Attachments on new tasks

- **Attach files when you create a task**: the New task dialog takes images, videos and other
  files from a **browse** link, drag and drop or a pasted screenshot (50 MB each, 20 per task).
  They are kept in `~/.overseer/tasks/<id>/`, so the 7-day upload cleanup does not touch
  them. Their paths are added to the end of the description, where the agent reads them.
  Deleting the task deletes its files.

### Closed sessions stay closed

- **No more closed tabs reopening on connect**: an event that arrived while the app was still
  loading its first list of sessions made the workspace treat that partial list as complete. It
  forgot every closed tab missing from it, then reopened them all when the full list landed. Tabs
  now wait for a complete list, so closed sessions stay closed after a launch, reconnect or wake.
- **A closed session resumed later stays closed**: when a closed agent session came back under
  the same line (a `claude --resume` in another terminal, or a restore after a reboot), the
  workspace had already forgotten it was closed and opened it as a new tab. It now remembers the
  last 200 sessions per workspace. The session still shows in the sidebar.
- **Closing a tab ends its session**: `×`, Cmd/Ctrl+W and Close used to only hide the tab while
  the agent kept running, so it came back after a reboot and on the next launch. Closing now
  stops the agent (or the shell); its conversation can still be resumed from the sidebar. To keep
  it running in the background, right-click the tab and choose **Hide tab (keep running)**.

### Faster chat rendering

- **Long agent messages render in linear time**: an unclosed `**`, `__` or `~~` no longer makes
  the chat rescan the rest of the line from every marker, which slowed long messages on phones.

### macOS app for Apple Silicon only

- **No more Intel Mac app**: the macOS `.dmg` and self-update bundle are built for Apple Silicon
  only (`Overseer_<version>_aarch64.dmg`), which halves the macOS build. Intel Macs get no new
  app version and the Homebrew cask requires arm64. The daemon still ships for Intel Macs, so
  remote mode on an Intel host keeps working.

### Tabs you can name, move and read

- **Rename a tab**: double-click a session or terminal tab (or use Rename… in its menu) to give it
  your own title; every client shows it, it survives daemon restarts and upgrades, and clearing it
  brings the automatic title back.
- **Drag a tab onto a workspace**: dropping a session or terminal tab on a sidebar workspace (or
  Move to workspace… in its menu) moves it there, overriding where it would land on its own.
- **Long tab titles on hover**: hovering a cut tab title shows it in full over the strip, without
  widening the tab (#34).
- **Long chats switch instantly**: coming back to a chat tab reuses what was loaded, older messages
  load as you scroll up, code is highlighted only near the screen, and the daemon parses a cold
  transcript once even when several clients ask for it, without blocking other chats (#36).

### Sessions found and resumed correctly

- **Sessions started outside Overseer**: a live session running in another terminal shows
  "runs outside Overseer" in its terminal pane instead of offering a resume that would start a
  second writer on the same transcript.
- **Codex discovery**: the Codex desktop app and its server or tooling processes are no longer
  listed as sessions, and a Codex session started through npm counts once.
- **Resume with spaced flags**: sessions launched with an argument containing spaces (an MCP config
  under "Application Support", for example) resume with that argument whole, including sessions
  recorded before the fix.

### Copy from the chat, pick the browser for links

- **Chat text can be selected again**: messages, code blocks, tool output and sub-agent
  transcripts select with the mouse or a long-press on the phone, and Cmd/Ctrl+C copies them.
  Each message gets a copy button next to its time.
- **Choose where links open**: Settings > Links sets chat, terminal, port, PR and notification
  links to open in Overseer, the default browser or a specific installed browser (detected
  once per launch on macOS, Windows and Linux), or to ask every time. Cmd/Ctrl+click opens in the
  default browser, Shift+click shows the picker, and right-clicking a link offers every option
  plus Copy link. The phone offers Overseer or the system browser.

### Snappier, lighter app

- **Faster start**: the app parses about a third less JavaScript before the first screen (code
  highlighting, the French dictionary, rarely used dialogs and the phone shell now load when
  needed), so the window is usable ~20% sooner on slow machines.
- **Busy daemons cost half the CPU**: a burst of agent activity, terminal titles and task updates
  no longer recomputes the whole sidebar and tab layout for every event.
- **Closing tabs frees memory again**: every closed terminal used to stay in memory until the
  window was reloaded (about 0.7 MB each); it is now released.
- **Tasks and back is instant**: leaving the Tasks board no longer rebuilds the sidebar and
  reattaches every terminal.
- **A failing view no longer blanks the window**: the sidebar, workspace, side panel, Tasks
  board, dialogs and each tab (terminal, chat, review, graph, worktrees, system, usage, PRs,
  browser) show an error with **Retry** instead, and your sessions keep running.
- **Tasks board with hundreds of tasks opens ~5× faster**: cards draw their icons from one shared
  sprite and long columns render the first cards, adding more as you scroll.
- **Smoother workspace switches**: no more forced layouts from tab scrolling and terminal focus,
  and a terminal is only re-rendered when its font metrics really change.
- **Lighter event bursts**: notifications only look at what changed in each daemon event.
- **Ctrl+K, update and APK sheets load on demand** (the palette is fetched in the first idle
  moment), trimming the startup JavaScript again.
- **Nothing you type is lost**: chat messages, the new-task form and review comments survive a
  reload or a failed request, and Send, Create task, Start, Apply plan, Add comment and Submit
  review can no longer fire twice on a double click or double Enter. The review error state has a
  **Retry** button.

### Organize my sessions

- **Organize my sessions…** (palette, Settings > Help, phone More) proposes workspaces, folders,
  groups, holds for long-idle agents and completed tasks for merged branches, and changes nothing
  until you tick and apply the items. **Quick** works offline from what the daemon already knows;
  **With your agent** lets a read-only configurator agent write the plan with a reason per item.
- **Pick up where you left off** lists recent sessions with edited files or open todos, with
  Resume and Open in workspace.
- The sidebar suggests it when more than 15 recent sessions have no workspace; the tour shows it
  under What's new.
- New MCP tools for agents: `list_sessions`, `get_session_brief`, `list_projects`,
  `list_worktrees`, `list_layout` and `propose_organization`.

### One Dock icon after an update (macOS)

- **Relaunch** after an update (in-app or Homebrew) no longer leaves a second Overseer icon in
  the Dock: the app now quits first and reopens the installed bundle once, instead of starting a new
  instance next to the running one.

### Native notifications with a notification board

- Every notification (agent needs input, finished, failed, all idle, ports, app and server updates,
  uploads, errors) now goes through one set of rules and shows as a native notification on macOS,
  Windows, Linux, Android and iOS. Settings > Notifications (or the gear in the notification
  center) sets, per kind, System / In-app only / Off, and adds sound, previews (hide session details
  from the lock screen), "only when Overseer is not focused", "don't notify for the session I'm
  looking at", a minimum gap per session (0 to 120 s), quiet hours with days, and per workspace,
  group and session mutes (also from the context menus and the center entries). The board shows
  the permission state with Request permission, Open system settings and Send test. The phone asks
  for the permission the first time a notification would show, never at launch. Android gets the
  channels Agents, Ports, Updates and uploads, Errors and Silent. A session keeps one notification
  on Android, iOS and Linux, and clicking one opens its session; on Windows and Linux without click
  support, bringing the window forward within 30 s opens it.

### Groups are easier to find

- The sidebar always shows the **Groups** section, with a **+** to create one (from the selected
  sessions, or empty) and a short explanation while you have none.

### Task dependencies

- A Kanban task can depend on other tasks: pick them in the new task dialog or the task details.
  The auto queue starts a task only once all its dependencies are completed, cards show "Blocked"
  and "Waiting on #12, #14", a link chip highlights the chain, and starting a blocked task by hand
  asks first. "Chain" creates a follow-up task, and with one dependency the task starts from that
  dependency's branch (toggle "Start from #N's branch"). Cycles and self-dependencies are refused;
  deleting a task drops it from its dependents. Agents can build chains too: MCP `create_task` takes
  `dependsOn` (ids or task numbers) and `set_task_dependencies` replaces a task's list.

### Web pages belong to their session

- A URL opened from a session (Cmd/Ctrl+click in its terminal, a link in its chat, "Open in
  browser" on a port its process listens on) opens beside that session inside its tab, with its
  own page strip, navigation, "Move to its own tab" and a resizable edge. Switching sessions hides
  that session's pages and shows the new one's, without reloading them; the globe button in the
  tab strip toggles them. Chat links used to leave for the system browser.
- At most 4 page webviews stay loaded (least recently shown are closed and reload from their last
  address); hidden ones are paused. Pages are saved per host (8 per session, 32 sessions) and
  dropped with their session. On the phone a page opens full screen over the session.

### Message history in the chat box

- In Chat mode, **Up** on the first line of the message box recalls your previous messages to that
  agent (taken from the conversation, so messages sent from the terminal or another device count too)
  and **Down** walks forward again, back to what you were typing.

### File drops always land somewhere

- Dropping files on the window goes through one handler: the chat, terminal or feedback sheet under
  the pointer gets them, and when the pointer position can't be matched (it is reported in different
  units on some systems) the chat or terminal of the focused pane does.

### Review beside the agent, submodules and base branch

- The review opens beside an agent: the review button in an agent's tab bar (chat or terminal),
  the side panel and the palette show it in a pane on the right of the agent (reusing an existing
  split) or as its own tab, and close it. The side review follows the selected agent unless
  pinned. The phone keeps the separate review view.
- Submodules: an agent working in a submodule (or a worktree of one) reviews that repository, not
  the superproject. A superproject's review shows each submodule whose commit changed as one
  collapsed entry with its commit range and a "Review this submodule" action; nothing inside a
  submodule is scanned, and dirty submodules no longer flood the diff (`--ignore-submodules=dirty`
  on diff, status and the watcher).
- Base branch: the review compares against the branch the worktree was created from (recorded
  for Overseer worktrees, else git's branch reflog, else the closer of the branch it was
  checked out from and the default branch) instead of always the default branch. "Comparing
  `<branch>` ← `<base>`" in the header opens a searchable picker of local and remote branches;
  the choice is remembered per worktree. New `GET /v1/review/bases`; `GET /v1/review` adds
  `baseSource`, `superproject` and `status: "submodule"` files.

### PR reviews from GitHub

- A new PR reviews tab (`+` menu, command palette, More on the phone) lists the open pull requests
  waiting for your review on GitHub, grouped by repository, with author, age, size, draft and
  review state. It uses the gh login already on the host and says exactly what to run when gh is
  missing or signed out.
- Review with agent checks the PR out in its own worktree (your clone, or a partial clone under
  `~/.overseer/reviews`), starts your default agent on it and opens it next to the live review.
  The agent writes its findings and only posts the review or inline comments after you confirm.

### Review triage and graph

- The review sorts every changed line into moved code, formatting, reordered imports, renamed
  identifiers, generated or lock files, tests, comments and logic, without a parser or new
  dependency. Hunks with only low-risk changes fold into one row ("Moved from src/x.rs",
  "Formatting only"), the gutter is tinted by kind, the header shows the low-risk share, and the
  file tree gets a risk score with a sort by risk.
- List | Graph: the changed symbols as a graph (uses and imports edges) with a suggested review
  order, definitions before callers, and Next to walk it. Fetched only when the graph is open.
- Copied files are detected when the repository sets `diff.renames=copies`.

### Closed tabs stay closed

- A terminal or chat tab you closed no longer comes back after a relaunch, an update or a window
  reload. Each window remembers, per host and workspace, the agents and terminals it has already
  shown; only ones started since then open by themselves (from the CLI, another device or a task).
  The Other view and groups keep their tabs across relaunches too, and a tab change made right
  before quitting is saved when the window hides or closes.

### Notification center

- A bell in the title bar (Notifications in More on the phone) lists background events per host:
  agent needs input / finished / failed, task in testing, all idle, ports forwarded or closed, app
  and server updates, uploads and errors, with time, source, unread state and click-to-act
  (open the line, the port, the update). Mark all read, Clear; the last 200 entries per host are
  kept across reloads, and replayed agent events never duplicate.
- Port forwarding is discreet: no toast or OS notification anymore, the ports chip flashes once
  and the forward goes to the center. A restarting dev server keeps one entry per port and host.
- Toasts are only for errors and direct answers to an action; Settings > Notifications > "Show
  toasts for background events" (off by default) brings them back for background events.

### Instant workspace switches

- Coming back to a workspace shows its tabs at once instead of waiting a round trip to the server
  (about 160-200 ms per switch over SSH, plus the "Loading the conversation…" state). Chat keeps the
  last page of the 12 most recent conversations in memory (bounded in size) with their scroll
  position, shows it immediately and only fetches what is new in the background. The side panel
  shows the last diff and worktrees of a session while it refreshes them.
- The 4 most recently used terminals of each pane (2 on phones) stay attached while hidden: no
  re-attach, scrollback replay or terminal setup on return. Hidden terminals do not paint and get
  their output in batches; their stream stays open. A kept terminal costs about 10 MB of renderer
  memory (measured at 1x scale).
- Chat now stays pinned to the bottom when the composer or the agents strip changes its height.
- The mock daemon takes `?latency=<ms>` to mimic an SSH round trip on every request.

### Sub-agents in Chat

- Chat shows Claude sub-agents (Agent/Task tool, foreground or background) as cards with their
  type, description, live state, elapsed time, tokens, tool calls and latest step. Open a card to
  read the sub-agent's own conversation and final report, nested and kept live while it runs. An
  **Agents** strip above the composer lists the running ones, like the CLI footer; click to jump.
- Daemon: Agent steps carry an `agent` summary built from the launch result, task notifications,
  hand-backs and the sub-agent transcript (read incrementally, only while it runs), and
  `GET /v1/lines/<id>/chat/agents/<agentId>` pages a sub-agent's transcript. A running sub-agent's
  progress fires `line.chat` even when the main transcript is quiet.

### Resume keeps arguments with spaces (macOS)

- Overseer reads an agent's exact command line from the kernel instead of splitting `ps`
  output on spaces, so an argument with a space (a `--mcp-config` JSON pointing into
  `Application Support`, a quoted prompt) is recorded whole and **Resume** relaunches the agent
  with the same arguments instead of a broken command line that exits at once. `ps` stays the
  fallback for processes whose arguments the kernel does not expose.

### Clean terminal environment after starting the daemon from an agent

- A daemon first started from a Claude Code or Codex session (and kept across upgrades) no longer
  hands that session's markers to its terminals and agents: `claude --resume` in a Overseer
  terminal saves its transcript again, and the session's messaging token stays out of children.
  Your own settings (`CLAUDE_CODE_USE_BEDROCK`, `ANTHROPIC_API_KEY`, `CODEX_HOME`...) are kept.

### Dictation

- A mic button in the agent composer starts the OS dictation: on macOS it is the same as Edit >
  Start Dictation (macOS asks for the microphone, the app needs no permission), the words land in
  the composer to review and send. On Windows it reminds you of Win+H. Hidden on Linux and phones
  (use the keyboard's mic). OS dictation also types straight into an agent's terminal.

### Task status from its tab

- Right-click an agent's tab (or its sidebar row) to move its task: **Move to In testing**, **Move to
  Completed**, and **Move to In progress** for session tasks. The current status is checked.

### Many tabs

- The tab bar scrolls sideways with the mouse wheel, keeps the active tab in view and shows a thin
  scrollbar on hover. **Settings → Keyboard → Wrap tabs on several rows** lays them out on several
  rows instead, like VS Code.

### Answer Claude Code questions from the chat view

- The prompt card handles Claude Code question forms like the terminal does: type a free answer
  in place of **Type something.** (Shift+Enter for a new line), pick **Chat about this** to
  leave the question and write a normal message, tick multi-select options then **Submit**, and
  confirm or cancel the final review of a multi-question form.
- The card shows the question tabs (answered ones checked) and option descriptions, moves from
  wherever the cursor is instead of assuming the top, and reads the screen again after each
  answer so the next question appears without polling.

### Custom colors

- **Settings → Appearance** lets you pick your own accent, background, panel, border, text and
  secondary text colors, separately for the light and the dark theme (the controls edit the theme in
  use; **Reset** goes back to the defaults). Hover, focus and selection tints follow the accent, and
  terminals and the git graph follow the custom background and text. Works on desktop and phone.

### Review only holds work to review

- Session tasks found already finished (sessions from before Overseer, or ended while it was
  not running) are created in Completed instead of In testing.
- A session Overseer saw working moves to In testing when it ends only if it edited files or
  works in its own worktree; read-only sessions (questions, searches, reports) go to Completed.
- A completed session task no longer bounces back to In testing while its session stays ended.
- A task whose branch has landed in the default branch moves from In testing to Completed, for
  tasks Overseer started as well as session tasks whose session is idle. Merge commits,
  fast-forwards, rebase merges and squash merges are recognised against the up-to-date
  `origin/<default>` (fetched in the background at most every 5 minutes while such tasks exist),
  including branches deleted after the merge; a branch merged only into another branch stays in
  In testing.

### Several windows, one host each

- **New Window** (File menu on macOS, palette, Cmd+Option+N / Ctrl+Alt+N) opens another desktop
  window on the host picker, so local and remote workspaces can be used side by side. Each window
  has its own connection, terminals, port forwards and embedded browser, and its title names the
  host; closing a window drops only its connection.
- The first window still reopens the last host at launch; other windows never change it. The
  selected workspace is remembered per host, and settings saved in one window no longer undo the
  changes made in another.

### Preview files in the app

- Click a file path in a terminal, a chat message or a tool step to open it in the app: images,
  video and audio play inline, PDFs render (not on Android, whose WebView has no PDF viewer),
  Markdown is rendered and text, logs, JSON, YAML and CSV are shown as text. Relative paths resolve
  against the terminal's current directory. Works on desktop and phone, over SSH too.
- New daemon route `GET /v1/file?path=…&term=…` serves the raw bytes of a regular file the user can
  read, up to 50 MiB.

### Resizable sidebar and side panel

- Drag the edge of the sidebar or the side panel on the desktop to resize it (arrow keys when the
  edge is focused, Shift for bigger steps, double-click to reset). Widths are remembered and capped
  so the terminals keep room on small windows.
- Drag a side panel section header (Last message, Work log, Todos, Edited files, Changes) to put
  the sections in the order you want; the order is remembered.
- **Cmd+R** (macOS), **Ctrl+R** outside a terminal or **Ctrl+Shift+R** in a terminal reconnects to
  the host without reloading the window; Ctrl+R in a shell still searches history.

### Phone

- Long-press a tab in the session strip to **Pin/Unpin** its agent or **Close** the tab.
- **More → Restart server to use vX** appears while a newer server is installed but not running,
  even after the banner was dismissed.
- Pasting or attaching files works on Android (the app now accepts the byte format Android's IPC
  sends instead of failing with "expected raw bytes").
- The keyboard no longer makes the screen flicker: the layout shrinks once to the space above the
  keyboard, the key row stays in place, and the terminal refits once when the keyboard settles.
- The **Keep running** switch in Server settings has the same size and centred thumb as the others.

### Notifications and Review

- Notifications no longer come back after a reconnect, resume or restart: each one is shown once
  (remembered across restarts), bursts are coalesced per session, a session that flips busy/idle
  notifies at most every 20 s, and a new notification replaces the previous one for that session.
- The Review list shows only worktrees with a running agent and review tabs you have open, instead
  of every worktree and unmerged branch; the daemon no longer runs git on worktrees without a
  running agent for this list.

### Android: system theme, back gesture, image paste, switches

- The **System** theme follows the phone's dark mode at launch (Android and iOS) and live when it changes (the
  WebView no longer gets a hard-coded light theme from the window, and the activity tells the page
  when the night mode flips without restarting). The status and navigation bar icons follow the
  theme in use, including a forced Light or Dark.
- The Android back gesture/button acts inside the app: it closes the open menu, sheet, dialog or
  palette, then the drawer or activity sheet, leaves the open review file, returns to the previous
  tab, and only at the root sends the app to the background (the SSH session stays up) instead of
  closing it.
- The terminal key-row **Paste** button pastes an image from the Android clipboard like on the
  desktop: it is uploaded to the host and its short path is typed. Pasting an image in the compose
  sheet inserts the uploaded path too.
- Settings and Feedback switches have a proper size on a phone (thumb centred, whole row tappable)
  and the same look in both themes.

### Less energy while agents work

- Terminal output for tabs that are not on screen (or while the window is hidden) is delivered to
  the webview in batches every 200 ms instead of once per chunk, so a busy agent in a background
  tab no longer wakes the webview dozens of times a second; switching to the tab flushes it at
  once. Chunks that are already waiting are also merged for visible terminals, without delay.
- While the window is visible but not focused, terminals repaint at most 15 times a second (two
  streaming panes: 86 → 27 paints/s). The focused window is unchanged.
- The sidebar refreshes a working session's activity summary at most every 2.5 s (was up to every
  400 ms per session, one server request each); the session shown in the side panel still refreshes
  after 400 ms.
- A new idle-budget test drives the system chip, both usage feeds and the terminal scheduler with
  fake timers and fails if anything runs (timer, interval, frame or request) while the window is
  unfocused or hidden, or if a focused idle window does more than the cursor blink and the 15 s
  system chip poll.

### No Writing Tools or smart punctuation in text fields (macOS)

- The macOS webview is created with Apple Intelligence Writing Tools turned off
  (`writingToolsBehavior = none`, macOS 15.1 and later), so the "Writing Tools" / "Write with Siri"
  popup no longer appears over terminals, search boxes and settings fields.
- Smart quotes, smart dashes, text replacement and automatic spelling correction are off by
  default in the app (Edit → Substitutions still turns them back on), so code, paths and commands
  typed into Overseer stay as typed.
- Single-line fields and the terminal get `writingsuggestions="false"`, `autocorrect="off"`,
  `autocapitalize="off"` and `spellcheck="false"`; the multi-line prose boxes (chat composer, review
  comments and summary, task description, broadcast, feedback, phone compose) keep spellcheck.

### Claude plan limits from the status line

- Claude Code hands its 5-hour and weekly plan usage to the status line command. Settings → Usage →
  **Plan limits from the status line** (off by default, per host) wraps your status line command
  with `overseer statusline-tap`, which records those percentages and runs your command with the
  same input, so the status line looks the same. The usage chip and panel then show Claude's `5h`
  and `wk` bars with reset times ("from the status line"), updated on every Claude reply, without
  any network call. Turning it off restores your command; `~/.claude/settings.json` is backed up
  before each change. CLI: `overseer statusline-tap install|uninstall [--apply]`.
- While those numbers are fresh (under an hour old), the daemon skips the Anthropic usage request.

### Server lifecycle: reconnect, no surprise agents, survive reboots

- **Reconnect on launch**: the desktop app reconnects to the last host (the phone to its last
  host) unless you pressed Disconnect, showing the connecting state; the connect dialog opens
  only when that fails. Settings keep `lastHost` and `explicitlyDisconnected`.
- **Never start an agent by itself**: restoring tabs, clicking a session task or **Attach** only
  attaches to a running process. The daemon refuses `POST /v1/lines/<id>/resume` with
  `"attachOnly": true` (409) when the agent is not running, so a stale "running" view can no longer
  turn an attach into `claude --resume`; only the explicit **Resume** actions start it. The auto
  runner and **Run** skip session tasks.
- **Keep the server running after reboot** (Settings → Server, also offered once per remote host):
  a `systemd --user` unit `overseer-daemon.service` (`ExecStart` = the
  `~/.overseer/bin/overseer` shim, `Restart=on-failure`) on Linux, a LaunchAgent on macOS.
  Lingering off is reported with its command and **Enable lingering** runs
  `loginctl enable-linger --no-ask-password` (never sudo). The shim keeps the unit valid across
  hot upgrades; `daemon --detach` starts the unit when installed; a supervised daemon that finds
  another one running exits 0. API: `GET`/`PATCH /v1/lifecycle`.
- **After a server reboot** (Settings → Server): the daemon keeps its terminal list in the registry
  and, only when the boot id changed, reopens shells in their folders and resumes the agents that
  were running with their own flags, under the same term ids. Restore agent sessions (default),
  restore shells only, or nothing. A daemon restart, hot upgrade or app relaunch never does.
- **Folders come back after a failed load**: when the daemon answers the workspaces, folders or
  groups load with an error (e.g. a locked database right after a hot upgrade), the app retries
  after 1, 2, 5 and 10 s, then stops (a reconnect tries again), so the sidebar gets its folders
  back without a restart. Disconnecting cancels the retry; the selection is kept.
- **Chat and terminal in sync**: a running line is one Claude process. Its chat and activity
  follow the session that process reports in `~/.claude/sessions/<pid>.json`, so a terminal
  started with `--session-id N` that resumed another session shows that session's messages. Two
  processes on one session stay two lines (marked duplicate) instead of one line reading the other
  process's view.
- **Short upload paths**: uploads and pasted files are stored as `~/.overseer/u/<id>.<ext>`
  (6-character id, still deleted after 7 days) and typed with a bracketed paste: images by absolute
  path so Claude Code shows `[Image #N]`, other files as `~/.overseer/u/a1b2c3.pdf`.
  `POST /v1/uploads` also returns `short`.

### Welcome screen and guided tour

- First launch shows a welcome card (after the first successful connection, immediately in local
  mode) with **Take the tour** / **Skip**. The tour spotlights the real controls (host, workspaces,
  tabs and new session, Terminal/Chat, side panel, Tasks, Review, usage, system and ports chips,
  notifications and hooks, Agents & models, command palette, Feedback) with Back / Next / Skip, a step counter and
  ←/→/Esc. Steps whose control is not on screen are skipped; on a phone the card is a sheet and
  points at the bottom navigation and session strip. English and French.
- The seen version is stored in settings (`tourSeen`); a future version that adds steps shows only
  those under **What's new**. Replay from Settings > Help, the palette (**Start the tour**) or More
  on a phone.
- The overlay is mounted only while it is shown and repositions on resize and scroll through
  ResizeObserver and events, with no animation loop; transitions honour `prefers-reduced-motion`.

### Claude plan limits in the usage chip and panel

- The usage chip and panel show Claude's 5-hour and weekly plan percentages ("Claude 5h 83% · wk
  47%") with reset times. The daemon reads them from Anthropic with the local Claude Code login,
  the same call Claude Code makes: at most every 5 minutes, only while the app shows usage and a
  Claude session replied in the last hour. The login is never refreshed or logged.
- Settings → Usage → **Fetch Claude plan limits** turns it on (off by default); while off, Claude
  shows tokens and cost only. An expired login shows "Limits unavailable — open Claude Code to refresh
  login".

### Keyboard shortcuts for tabs, panes and the terminal

- **Cmd+W / Ctrl+W** close the active tab, never the app (the agent keeps running; the last tab
  leaves the empty workspace). On macOS the File menu now has Close Tab (Cmd+W) and Close Window
  (Cmd+Shift+W). On Linux and Windows, Settings > Keyboard > **Ctrl+W closes the tab** (on by
  default) gives Ctrl+W back to the shell when turned off.
- **Cmd/Ctrl+Shift+T** reopens the last closed tab (the last 10 per workspace); an agent that ended
  reopens on its Resume button. **Cmd/Ctrl+T** opens the `+` menu with the default agent selected
  (Enter starts it), or a shell directly with Settings > Keyboard > **New tab opens a shell**.
- Ctrl+Tab / Ctrl+Shift+Tab cycle tabs; split right and down (Cmd+D / Cmd+Shift+D on macOS,
  Ctrl+Shift+E / Ctrl+Shift+D elsewhere so Ctrl+D stays EOF); Cmd/Ctrl+Alt+arrows move between
  panes; find in terminal (Cmd+F, Ctrl+Shift+F); Ctrl+Shift+C copies the terminal selection;
  Cmd/Ctrl+= / - / 0 terminal font size; Cmd/Ctrl+B sidebar, Cmd/Ctrl+J activity panel (Ctrl+Shift
  in a terminal on Linux and Windows); Cmd/Ctrl+Shift+K Kanban; Cmd/Ctrl+Shift+N new workspace.
- A **Keyboard shortcuts** sheet (palette, `?` in Settings > Keyboard) lists them for the current
  platform, and the palette shows each action's shortcut. On macOS the palette is Cmd+K only, so
  Ctrl+K reaches the terminal. Not on the phone app.

### Send feedback as a GitHub issue

- A **Send feedback** sheet (title bar button, palette, Settings > Feedback, phone More, and
  **Report this crash** on the crash dialog): Bug / Idea / Other, optional title, description,
  attachments uploaded to the active host, optional redacted diagnostics (versions, OS, connection
  mode, last 200 app and 100 server log lines) with a preview of the exact text, and an optional
  read-only investigation by the agent.
- **Send with <agent>** starts the default agent CLI in a new tab (in a clone of the repo when
  known), which looks for duplicates and files one issue with the user's own `gh` login; the app
  watches the tab for `ISSUE_URL=` and shows the link. **Send directly** runs `gh issue create` on
  the host; **Open on GitHub** opens a prefilled new-issue page.
- Daemon: `POST /v1/feedback/start` and `POST /v1/feedback/direct` (contract § 3b). Target repo in
  Settings (default `basilelt/overseer`).

### Near-zero CPU when idle

- Terminals no longer repaint 60 times a second. ghostty-web 0.4.0 (now pinned) ran a
  `requestAnimationFrame` loop per terminal forever and, with a blinking cursor, redrew the cursor
  row on every frame, so macOS listed Overseer under "Using Significant Energy" even with
  nothing on screen changing. Overseer replaces that loop with one shared, on-demand frame
  scheduler: a terminal paints only after output, scrolling, selection, link hover, resize or a
  font change, at most once per frame, and not at all while the window is hidden or the terminal
  is off screen. Unfocused split panes are capped at 30 fps.
- The cursor blinks every 600 ms, only in the focused terminal and only while the window is
  focused; typing shows it solid again.
- Spinners and pulses pause while the window is unfocused, and every animation pauses while it
  is hidden. The "working" workspace dot,
  running work-log and todo steps and queued review comments use static indicators; the
  reconnecting dot stops pulsing after a few seconds.
- The status list's relative times tick only while the window is visible.
- The freeze watchdog (ADR 0035) parks while the window is hidden or unfocused instead of pinging
  the webview every 2 s.
- Events that change nothing no longer re-render the sidebar, and the server no longer emits
  `line.updated` when only an agent's memory use changed (every 5 s scan per live agent).
- The server polls agent transcripts every 250 ms (was 100 ms) and sends `line.activity` at most
  once a second per line (was twice).
- CI gains a `perf` job: a release build against the mock backend runs under Xvfb with 5 terminals
  and fails if the app's process tree uses 3 % or more of one core over 30 s idle (it also reports
  CPU while one terminal streams 200 lines/s).

### Chat view for agents

- Agent tabs (Claude Code, Codex, Gemini) get a Chat view next to the Terminal: the
  conversation rebuilt live from the session file, with Markdown replies, collapsed tool steps
  (diffs, commands and output), todo checklists, subagent tasks, timestamps and the model. Phones
  open agents in Chat by default; desktops keep the terminal, with a Chat | Terminal toggle.
- A composer sends to the agent when it is idle, or lets you Queue / Send now when it is busy;
  images are uploaded and their path inserted; `/` completes built-in and custom slash
  commands. Interrupt sends Esc. When the agent waits for input, a card shows the question with
  quick answer buttons and an "Open terminal" fallback.
- Only desktops resize an agent's terminal: a phone no longer squeezes it to phone width.
- Hold and Close work for agents running in Overseer terminals (the held stub stays in the
  same terminal; Enter or Resume restarts the agent there).
- Daemon: `GET /v1/lines/{id}/chat` (incremental, cursor-based), SSE `line.chat`,
  `GET /v1/lines/{id}/screen`, `POST /v1/lines/{id}/keys`, `GET /v1/lines/{id}/commands`
  (ADR 0036).

### Agent profiles: models and reasoning per CLI

- Profiles set, per agent CLI, the main model and reasoning effort, the default subagent model
  (and effort for Codex), the background model, and per-subagent overrides. **Economy**,
  **Balanced** and **Max** are built in; duplicate one to make your own. **Settings → Agents &
  models** edits them and shows an estimated cost relative to the CLI defaults.
- The profile in force comes from the task or the next session (Add Task, **+** menu), else the
  workspace (**Agent profile…** in its menu), else the default profile. The default is **CLI
  defaults**, so nothing changes until you pick one.
- Values are passed when the daemon starts the agent: `--model` and `--effort`,
  `CLAUDE_CODE_SUBAGENT_MODEL` (optionally forced over subagent files),
  `ANTHROPIC_DEFAULT_HAIKU_MODEL`, Codex `-c agents.default_subagent_*` and
  `OPENCODE_CONFIG_CONTENT`. Global config files are never edited.
- To change one Claude subagent's model or effort in its `.md` file, use **Apply to agent
  files…**. It previews each frontmatter change, writes only the files you tick, and keeps a
  `.bak-<stamp>` copy.
- Session tabs and the activity panel show the profile, model and effort a session started with.
- Claude's model list now includes `haiku`. Copilot CLI lists its supported models.

### Review that shows the work

- On the default branch the review compares against its upstream (`origin/main`), so unpushed
  commits, staged, unstaged and untracked changes all show; before, `main` was its own base and
  the review was always empty when `origin/HEAD` was not set. The commits under review are listed
  (collapsible), and a clean branch shows its recent commits, each with "Review since this
  commit", instead of a blank "No changes".
- The review follows the session's live folder: a line whose recorded worktree was renamed or
  removed resolves to its live working directory, and a folder that is really gone shows a clear
  "This worktree folder no longer exists" state (HTTP 410) instead of an error string.
- A selector at the top of the review lists every worktree of the repository with changes
  (agent worktrees under `.claude/worktrees` included), newest first, with files, commits and
  running agents, plus the local branches not merged into the default branch; branches without
  a worktree open as a read-only review. The side panel shows "Changes in N worktrees" with the
  same counts as the review tab. New routes: `GET /v1/review/worktrees`, `GET
  /v1/review/branches`, `branch=` on `GET /v1/review`. Refreshed from the existing watcher
  events, debounced; no polling.

### Agent notifications from every CLI

- Hooks now call `~/.overseer/bin/overseer`, a symlink the daemon repoints at its own binary
  on every start and hot upgrade. The daemon rewrites existing Overseer hook entries that still
  name a versioned server path (same merge, backup and validation), so a server upgrade no longer
  breaks them and nobody has to rerun the installer.
- New installers: Gemini CLI (`~/.gemini/settings.json`), Copilot CLI
  (`~/.copilot/hooks/overseer.json`), Cursor Agent (`~/.cursor/hooks.json`), and plugins for
  opencode (`~/.config/opencode/plugins/overseer.js`) and Amp
  (`~/.config/amp/plugins/overseer.js`). Their events map onto busy, finished and needs input.
  `overseer hooks install` takes agent names.
- Agents without hooks (Antigravity, or any agent whose hooks did not fire) end their turn from
  terminal activity: busy while output continues after Enter, finished after 4 s of quiet.
- `GET /v1/hooks/status`, `POST /v1/hooks/install` and `POST /v1/hooks/uninstall`
  (`{agents, dryRun}`, one result per file with its diff and backup).
- The app offers to install the hooks when it connects (Install / Not now / Never) and has a switch
  per agent in Settings > Notifications. Codex needs the hooks trusted again in codex.
- OS notifications "<agent> finished in <workspace>" and "<agent> needs your input"; clicking one
  focuses that tab. Line events now carry `local.busy` and `local.needsInput`.

### System monitor

- A `CPU 23% · RAM 61%` chip in the title bar for the connected host (remote or local), amber at
  80 %, red at 95 %. It opens a System panel (also a workspace tab, and a full-screen sheet on the
  phone): 10-minute charts for CPU, memory, network and disk I/O, per-core bars, filesystems,
  the top processes by CPU or memory, and each Overseer session's own usage ("Claude in
  overseer: 3 % CPU, 450 MB").
- Daemon: `GET /v1/system` and the `GET /v1/system/stream` SSE stream (`system.sample`, contract
  §7b). The sampler runs only while a client watches and stops 5 s after the last one; the app
  stops polling when its window is hidden or unfocused.

### Agent usage

- The daemon indexes token usage from Claude Code transcripts (subagents included), Codex
  rollouts and Gemini CLI sessions incrementally into `usage.db` (31 days), with Codex plan
  windows from `rate_limits`; `GET /v1/usage?range=&by=&tzOffset=&line=` and a debounced
  `usage.updated` event (contract §7c). Costs are estimates from one static price table.
- The title bar chip refetches at most once every 10 s while an agent works (trailing, only while
  the window is visible); an open Usage panel at most every 2 s.
- Title bar usage chip ("Codex 5h 29% · wk 38%", or tokens today when an agent exposes no
  limits), amber at 80 %, red at 95 % or after a refused request, reset times in the tooltip.
- Usage panel (desktop modal or tab, full-screen sheet on phones): limit gauges with reset
  countdowns, today / 7 / 30 day totals and cost, tokens per day, breakdown by project, session,
  model or agent; the side panel shows the session's usage.
- Claude Code's 5-hour and weekly percentages are not stored locally, so Claude shows tokens and
  cost only.

### Session tasks on the Kanban

- Every agent session in a git repository (Claude, Codex, Gemini, opencode, …) gets its own task
  on the board, in its project, created by the daemon and kept to one per session
  (`origin: "session"`). Only sessions alive or active in the last `sessionTasksDays` days
  (default 7, `0` turns it off) get one.
- Title from the session's AI title, else the first prompt (about 60 characters), else
  "<agent> session in <branch>"; description, branch, worktree, ticket, agent and model from the
  session; progress bar from the agent's todo list.
- The status follows the session live: working → In progress, waiting for input → In progress
  with a "needs input" badge, turn finished or process ended → In testing, held → "paused",
  closed or merged → Completed. A card moved by hand stays put until the session works again
  (`manualStatus`).
- Cards carry a session glyph, open the session's tab (In testing: the review), and dropping one
  on Completed offers to hold the session. **Hide session tasks** filters them out.
- Contract: `Task.origin`, `Task.manualStatus`, `Task.badges`; session tasks accept
  `PATCH status: in_progress`, refuse `pending`/`auto`, start and delete.

### Terminal: scrolling back through history works

- Scrolling up in a terminal stays where you put it while output keeps coming. ghostty-web moved
  the view back to the bottom on every write, and Claude Code redraws its spinner all the time,
  so the history could never stay in view. A "New output" pill jumps back to the bottom, and
  typing or pasting also jumps back.
- Terminals keep 10,000 lines of history by default (Settings > Terminal > Scrollback lines). The
  old `scrollback: 5000` was read by ghostty as 5,000 bytes, which left only a few hundred lines.
- The wheel still goes to the app only when it turns mouse tracking on (Claude Code's fullscreen
  mode, vim, htop) and becomes arrow keys only on the alternate screen. Shift+wheel always scrolls
  the terminal's own history.

### Logs, crash and freeze reports (ADR 0035)

- The desktop and mobile apps write a rotating log (5 × 5 MB) to the platform log folder; on
  macOS `~/Library/Logs/io.github.basilelt.overseer/`. It covers the start (version, OS, build, code
  signature), the connection lifecycle (ssh -G resolution, relay, bootstrap, reconnects with their
  cause, hot upgrade), daemon errors and requests over 2 s, commands over 1 s, terminals, updates
  and frontend errors with their stack. Secrets are masked; terminal contents are never logged.
- A crash (Rust panic) writes a crash report with the backtrace and the last 200 log lines. A
  watchdog logs "UI unresponsive for N s" with the view and the commands in flight when the
  window stops answering.
- When the previous run did not exit cleanly (crash, freeze, kill or force quit), the next launch
  opens "Previous session ended unexpectedly" with the crash report, the end of the app log and of
  the server log: **Copy all** gives one plain-text block to paste into a bug report; also Save as
  file, Open log folder and Dismiss. Palette > "Show logs" and Settings > Diagnostics open a live
  view with a level filter and a search.
- The daemon timestamps its log lines, rotates `daemon.log` (5 × 5 MB) and serves its redacted
  tail on `GET /v1/logs?tail=N` (protocol 1, additive).

### Other sessions: right workspace, and actionable

- A live agent now joins the workspace of its process's working directory (`/proc/<pid>/cwd`,
  `lsof` on macOS, or its Overseer terminal) before the cwd recorded in its transcript. A
  Claude session resumed after its folder was renamed keeps writing under the old
  `~/.claude/projects/<old name>` folder with the old path; it is now matched to its real folder
  through its live session file instead of landing in "Other sessions".
- "Other sessions" has a context menu: broadcast, hold all, resume all and "Close all…", which
  previews the close checks (the same as hold) before you confirm.
- Every line's context menu (tabs, status list) now has Open, Attach in terminal, Resume, Hold
  and Close…. An agent running in a leftover tmux pane opens in a new terminal attached to its
  tmux session, and Close ends it through the hold checks, then marks it hung up.

### Android in-app updates (ADR 0032)

- The Android app updates itself from the GitHub releases: it checks at start, every 6 hours, when
  it comes back to the foreground, and from Settings > Updates, More > Software update or the
  palette. An update shows a pill in the top bar and a bottom sheet with the release notes;
  "Download & install" downloads the APK through the GitHub API, checks it against the release's
  `.sha256`, and opens the Android installer.
- The repository is private, so the first launch asks once for a GitHub token that can read
  `basilelt/overseer` (fine-grained, Contents: read-only, or classic `repo`), with a
  button to GitHub's token page. The token is checked against GitHub before it is saved, kept in
  the Android Keystore-backed store, never shown again, and can be removed in Settings > Updates.
- Android asks once to allow installs from Overseer; the sheet explains it and opens that
  setting. An APK signed with another key (a pull request debug build installed) is refused with
  a clear message instead of failing in the system installer. Older downloads are deleted.
- iOS keeps TestFlight; the in-app updater is hidden there. The desktop update entries are no
  longer shown on phones.
- Phones install the server: when a host has no server or an older one, the phone downloads its
  own version from the GitHub release (asset picked from `uname -sm`, checked against
  `SHA256SUMS`, cached per target), uploads it over the SSH session and starts it; a running
  older daemon is hot-upgraded as from the desktop. Steps: "Downloading server for
  linux/x86_64", "Installing server", "Starting". Uses the same GitHub token (asked on iOS for
  this only); without it, an older server keeps running and a host without one offers "Add GitHub
  token" or the desktop app.

### Stable macOS signature and server upgrades (ADR 0028, ADR 0034)

- macOS release builds are signed with a persistent `Overseer Self-Signed` certificate
  (Developer ID takes precedence once configured; ad-hoc only when neither secret is set). The
  designated requirement now pins the certificate instead of the cdhash, so the Local Network
  grant survives updates; CI checks it with `codesign -d -r-` and a `macos-signing` job proves two
  builds share one requirement. Local Network panel: the signature shows `selfSigned`, and ad-hoc
  builds get the Terminal workaround (`/Applications/Overseer.app/Contents/MacOS/overseer-app`).
- The app keeps the server in step with itself: after installing the matching server it hot
  upgrades a running older daemon in place (terminals stay open); a daemon without hot upgrade is
  replaced at once when it has no live terminal, otherwise a banner offers "Restart server" with
  the number of terminals that will close. Desktop and mobile share the logic.
  `overseer daemon --detach --replace` stops a running daemon of another version first.

### Mobile app (ADR 0033)

- Android and iOS: the same app with a phone layout (drawer sidebar, tab chips, bottom navigation
  Agents / Tasks / Review / More, one Kanban column per screen with swipe and "Move to…", single
  column review with tap-to-comment, full-screen palette, bottom sheets, 44 px targets, safe areas).
- Remote only, through an in-process SSH client (`russh`, ring) that runs the installed server and
  the same `overseer proxy` mux as the desktop: saved hosts with optional ProxyJump, an Ed25519
  device key in the Android Keystore / iOS Keychain with a copyable `authorized_keys` one-liner,
  password authentication, app-data `known_hosts` with an explicit trust prompt, keepalive and
  reconnect with backoff, retried at once when the app comes back.
- Terminal on a phone: accessory key row (Esc, Tab, sticky Ctrl, Alt, arrows, `| ~ / -`, newline
  via the Shift+Enter mapping), paste, photo/file upload through `POST /v1/uploads`, pinch to zoom, long press to copy, and a Compose sheet delivered like a broadcast.
- Shared terminals: resizes name their client (`"client": "phone" | "desktop"`), terms report
  `sizedBy`; the most recently focused client sets the size and the other shows a "Resized for
  phone · Fit" hint.
- `?phone=1` in the mock shows the phone layout.

### Terminal input, uploads, repository discovery, agent PATH

- Shift+Enter in a terminal sends `ESC CR` (Meta+Enter): a new line in Claude Code, Codex and
  most TUIs instead of submitting. Setting "Shift+Enter inserts a newline" (on by default); zsh
  needs `bindkey '^[^M' self-insert-unmeta` (user guide 14.1).
- Daemon terms no longer inherit the SSH session the daemon was started from: `SSH_CONNECTION`,
  `SSH_CLIENT`, `SSH_TTY`, `TMUX`, `TMUX_PANE`, `STY` and a forwarded `SSH_AUTH_SOCK`
  (`/tmp/ssh-*/agent.*`) are removed (also before the login-shell probe), so an rc file that
  opens tmux for SSH logins no longer wraps every Overseer terminal in tmux. Terms get
  `TERM_PROGRAM=overseer`, `TERM_PROGRAM_VERSION` and `OVERSEER_TERM=1`.
- Terminal font: Settings → Terminal → Font family (any installed font, with suggestions), font
  size and line height. A bundled "Symbols Nerd Font Mono" (Nerd Fonts 3.5.1, MIT, woff2
  1.2 MB) always ends the font chain, so Nerd Font / powerline prompt icons render. The
  terminal waits for the fonts and re-measures its cells when they load or change.
- Mouse wheel in terminals follows the program's DEC modes (1000/1002/1003, 1006/1015/1005,
  1049/1047/47, 1007): wheel reports when mouse tracking is on (tmux with mouse mode, so a
  Claude Code session inside tmux scrolls again), arrows on the alternate screen, local
  scrollback otherwise; trackpad deltas are accumulated and capped. Shift+wheel scrolls locally.
- Paste an image or drop files on a terminal: on a remote host the app uploads them through the
  daemon and types their paths (quoted, followed by a space, never Enter); locally it types the
  dropped path or saves the pasted image to a temporary file. Progress toast for large files.
  New `POST /v1/uploads` (raw body streamed to `<state>/uploads/<yyyymmdd>/`, mode `0600`,
  50 MB cap, sanitised names, deleted after 7 days).
- Repository discovery: the daemon scans `discoverRoots` (default `~/Documents/Github`, `~/src`,
  `~/code`, `~/projects`, `~/dev`) three levels deep for git repositories at start, every
  10 minutes and on `POST /v1/projects/discover` ("Rescan folders" in the workspace `+` menu and
  the palette), so repositories without an agent line, or renamed, get a workspace. Projects and
  workspaces carry `missing`; automatic workspaces whose folder is gone are hidden, ones you
  added show "Folder missing".
- Agents installed under `~/.local/bin`, nvm, bun… were shown as not installed when the app
  started the daemon over SSH (minimal `PATH`). The daemon now adopts the login shell's
  environment at start (`$SHELL -l -i -c`, 5 s timeout, fallback to the common user dirs;
  `OVERSEER_LOGIN_ENV=0` disables) for detection and for every term it spawns. `GET /v1/env`
  and `overseer doctor` show the `PATH` and its source; "Rescan" in the agent picker
  (`POST /v1/agents/rescan`) re-runs detection. Applies from the next daemon start.

### Mobile releases (ADR 0032)

- Android and iOS Tauri projects generated in CI (`mobile-init.yml`) and committed under
  `app/src-tauri/gen/`.
- Releases attach a signed universal Android APK, `Overseer_<version>_android.apk` with its
  `.sha256`, installable and updatable with Obtainium (`docs/mobile.md`).
- Releases upload an iOS build to TestFlight (internal group) with fastlane match; never
  submitted for App Store review. Both mobile jobs are skipped while their secrets are missing.
- CI builds a debug APK on pull requests, and `scripts/store-guard.sh` fails on any store
  submission command.

### Removed

- Removed cmux integration (board --cmux, status --cmux-sidebar, deploy/cmux, doctor cmux checks).

### macOS Local Network (ADR 0028)

- Fix: "No route to host" to LAN machines on macOS 15+ while `ssh` works in Terminal, even with
  Overseer allowed under Local Network. When the host has no `ProxyCommand`/`ProxyJump`, the
  app opens the TCP connection itself and relays it to the system `ssh` over loopback
  (`--ssh-relay`, `OVERSEER_SSH_RELAY=0` to disable).
- `local_network_status` asks macOS for the permission once at startup (mDNS send, in-app connect
  to private addresses) and classifies it as granted, denied or unreachable.
- A "No route to host" opens an "Allow local network access" panel in the Connect dialog: open
  Privacy & Security, quit and reopen, try again, a note on ad-hoc signed builds, and the
  resolved addresses, interface, signature and raw error under Details.

### Worktree-first sessions and the git graph (ADR 0031)

- `src/worktrees.rs`: one store outside the repositories,
  `~/.overseer/worktrees/<repo>-<hash>/<branch>` (`worktreeDir`, was `~/worktrees`); branches
  `sb/<agent>/<slug>` and `task/<n>-<slug>` from the default branch, fetched first
  (`worktreeFetch`). `GET /v1/projects/{p}/worktrees` (main, managed and orphan worktrees with
  state, ahead/behind, dirty, unpushed, PR, size, last activity), `POST /v1/worktrees`,
  `POST /v1/worktrees/{id}/cleanup` (dry run by default; refuses the main checkout, orphans,
  locked, in-use, dirty and unpushed worktrees; deletes a branch only when its commits are in
  the base), `merge-base` and `cherry-pick` (aborted on conflicts). Events `worktree.created`,
  `worktree.updated`, `worktree.removed`; `worktreeStaleDays`.
- Task start goes through the module (base = default branch instead of `HEAD`) and takes
  `inPlace`; `POST /v1/terms` takes `worktree: true | {branch, base, slug}`.
- `src/gitgraph.rs`: `GET /v1/projects/{p}/graph` (paged commits with parents, refs, HEAD,
  overlays of lines, tasks, worktrees, tickets and PRs from the gh cache), `GET
  /v1/projects/{p}/commits/{sha}` (files and diff), `POST /v1/projects/{p}/branches`, and a ref
  watcher that publishes `git.changed`.

### App updates and Homebrew tap

- The app updates itself (`tauri-plugin-updater`, `tauri-plugin-process`): check at startup, every
  6 h, from the palette and Settings > Updates; an "Update 0.x.y" title-bar pill opens the notes and
  Download & restart with progress. Self-updates for the macOS `.app`, the Windows NSIS install and
  the Linux AppImage; Homebrew installs get `brew upgrade --cask overseer`, `.deb`/`.rpm` the
  release page.
- Private releases: the token comes from Settings > Updates (file in the app config folder, 0600),
  `OVERSEER_GITHUB_TOKEN`, `HOMEBREW_GITHUB_API_TOKEN`, `GITHUB_TOKEN`, then `gh auth token`
  (3 s); without one the app only shows "Sign in to GitHub to get updates". The manifest URL is
  resolved from `releases/latest` at each check.
- Homebrew installs get an **Update** button: the app finds `brew` (`/opt/homebrew/bin`,
  `/usr/local/bin`, `PATH`), runs `brew update`, `brew upgrade --cask overseer` and, when the
  formula is installed, `brew upgrade --formula overseer`, with `HOMEBREW_GITHUB_API_TOKEN` from
  the app's token (scrubbed from the output). The dialog streams the step, the last line and the
  full output; **Restart to finish** opens the new `/Applications/Overseer.app` and quits.
  Untrusted tap (**Trust and retry** runs `brew trust`), refused token, missing Homebrew and a stale
  tap get their own message; the copyable command stays as a fallback.
- Release notes in the update dialog are rendered from Markdown (headings, bullets, links opened in
  the browser, inline code); compare links are dropped and commit links become sha chips.
- Release workflow: the app jobs sign updater bundles when `TAURI_SIGNING_PRIVATE_KEY` is set
  (`tauri.updater.conf.json`), `publish` uploads `latest.json` built by `scripts/latest-json.sh`,
  and a `tap` job updates `basilelt/homebrew-tap` with `scripts/update-tap.sh` through a GitHub App
  token.

### Live review (ADR 0030)

- `src/review/`: a GitHub-style review of a worktree. `GET /v1/review?worktree=|line=|task=`
  returns files and numbered hunks of the branch against its merge base (task project branch,
  `origin/HEAD`, `main`/`master`) plus uncommitted and untracked files; `GET /v1/review/file` for
  expanding context; `PUT /v1/review/viewed` (reset when the file changes).
- Comment threads in the registry (`review_*` tables, `CREATE TABLE IF NOT EXISTS`):
  `GET/POST/PATCH/DELETE /v1/review/comments`, replies, drafts, resolve/reopen, ```` ```suggestion ````
  blocks applied with `POST /v1/review/comments/{id}/apply`. Anchors follow edits (FNV-1a context
  hashes): moved, "changed since comment", or outdated.
- `POST /v1/review/submit {kind: comment|request_changes|approve}` sends all pending comments as one
  instruction (path:lines, quoted code, comment, suggestion as a unified diff, thread ids) to the
  worktree's agent — daemon term or tmux pane — only when it is idle and not waiting for input;
  otherwise queued and delivered on a later tick. `request_changes` moves an In testing task back
  to In progress. Status pending → sent → addressed → resolved.
- Worktrees are watched (`notify`, debounced, polling fallback) for live updates: SSE `review.changed`,
  `review.thread`, `review.thread_deleted`, `review.delivery`.
- `overseer mcp`: `list_review_comments {status?, worktree?}` and
  `resolve_review_comment {id, note}`.
- App: review view as a workspace tab (file tree, unified/split diff, lazy syntax highlighting,
  folded unchanged lines, viewed files, inline threads, comment box with suggestions, submit bar
  with delivery state), opened from the activity panel, the Kanban In testing review and the
  palette; tab badges count unresolved threads. Protocol stays 1.

### Remote: system ssh, one tunnel, port forwarding (ADR 0028)

- Fix: connecting to a host that `ssh` reaches but the app did not (`No route to host` on a
  `HostName` the in-app config reader got wrong: `Match` blocks, `/etc/ssh/ssh_config.d`,
  `ProxyCommand` and more were ignored). The app now runs the system `ssh`, so every OpenSSH
  option, agent and proxy applies; `ssh -G` gives the "alias → host:port via …" line shown in
  the Connect dialog and in errors. macOS bundles declare `NSLocalNetworkUsageDescription`, and a
  "No route to host" there points to the Local Network privacy setting. `russh` is removed.
- One SSH connection per host: the bootstrap (platform, version, upload, `daemon --detach`) runs
  in the same session, which then becomes `overseer proxy`, a flow-controlled stream
  multiplexer (contract §8, §8a) carrying API requests, SSE, every terminal and forwarded ports.
- Host key, passphrase and password prompts in the app via `SSH_ASKPASS` (the app binary); a new
  key reaches `known_hosts` only after "Trust and connect".
- Reconnect on ssh exit or missed keepalives with backoff (0.5 s → 30 s), "Reconnecting" with
  the cause, permanent errors stop retrying; terminals re-attach and replay their scrollback.
- Ports: the daemon detects TCP listeners in its terms' process trees (`GET /v1/ports`,
  `port.opened`/`port.closed`; protocol stays 1); the app auto-forwards them to the same local
  port (next free when taken) with an "Open in browser" notification, and a Ports panel manages
  forwards (open, copy, stop, add) and settings (auto-forward, ignore list, localhost only).
- Mosh evaluated and not adopted (ADR 0028).
- Tests: mux, `ssh -G` parsing, ProxyCommand tokens, ProxyJump chains, reconnect policy, askpass,
  port tables (`/proc/net/tcp*`, `lsof -F`), forward allocation; `remote_it` drives the whole
  remote path through a fake `ssh`; `make test-remote` runs it against Docker sshd containers
  (`Match`, `ProxyJump`, `ProxyCommand`, host key prompt, one TCP connection, reconnect).

### Workspaces, groups and broadcast (ADR 0029)

- Registry tables `workspaces`, `workspace_tombstones`, `folders`, `groups` (created in place, no
  migration). Routes `/v1/workspaces*`, `/v1/folders*`, `/v1/groups*`: a workspace is any
  directory with name, folder, pin, archive, rank, stored tabs and idle-notification flag; folders
  nest one level; groups hold `line:<id>`/`term:<id>` keys. Protocol stays 1.
- Every project gets an `auto` workspace on `GET /v1/workspaces` (served from the last scan);
  deleting a workspace leaves a tombstone so it is not re-created.
- `POST /v1/broadcast` types one text into lines/terms, a workspace's agents or a group: through
  the daemon term, else the tmux pane (bracketed paste); idle targets only by default (needs input
  and busy are not idle), with `onlyIdle: false` to force or `queue: true` to deliver when the
  agent becomes idle (in memory, 1 h). `actions.log` records the results and the length, never
  the text.
- Events: `workspace.*`, `folder.*`, `group.*`, `broadcast.delivered`.
- App: the sidebar is a tree of folders and workspaces (pinned on top, drag to reorder or move,
  rename, pin, archive), each row with the aggregated state of its agents, host and connection,
  cwd and last status line; "By status" keeps the former grouping. Each workspace has its own tab
  strip (agents, shells, browser, review) with a `+` menu, drag reorder, splits and Cmd/Ctrl+1–9,
  Cmd/Ctrl+Shift+[ ], Cmd/Ctrl+T, saved in the daemon. Groups from a multi-selection, broadcast
  dialog (idle agents ticked, queue for busy ones), hold all / resume all with a dry-run preview,
  OS notification when a workspace or group is all idle, Kanban filtered on the current workspace.
- Tests: `read_until` in the integration tests scans only new bytes (the 3 MB replay test took
  ~15 s under load).

## 0.2.0 — 2026-10-29

### More agents (ADR 0026)

- `src/agents.rs`: one table for Claude Code, Codex, Gemini CLI, Antigravity (`agy`), opencode,
  Amp, Copilot CLI, Cursor Agent and the shell: binary, detection, model/effort flags, session
  id up front, prompt flag, resume and "continue" argv. `GET /v1/agents` reports it with
  `installed`/`version` (cached 60 s).
- `POST /v1/terms` takes `{agent, model?, effort?}`; Term has `agent`. Tasks accept any agent but
  the shell; the prompt goes by flag where the CLI has one (shown as `<prompt>` in `Term.cmd`),
  typed otherwise. Lines from Gemini chats (`~/.gemini/tmp/*/chats`) with activity; terms of the
  other agents are lines while they exist. `local.resumeVerified` follows the table.
- Tasks: `rank` (column order, additive column). `POST /v1/projects` persists a project in the
  registry. Protocol stays 1.

The daemon side of the desktop app (`contract/app-daemon.md`, protocol 1; ADRs 0022–0024).

- Terminals: the daemon owns PTYs (`portable-pty`, ConPTY on Windows) that survive the app; REST
  routes `/v1/terms*`, WebSocket `/v1/terms/{id}/io` with a 2 MiB scrollback replay, several
  clients, resize and kill. Agent lines link to their term by pid (`local.termId`);
  `resume {"target":"term"}` resumes a line in a new term.
- Tasks: local Kanban in the registry (`/v1/tasks*`, `/v1/projects`), per-project numbers, start
  in a `task/<n>-<slug>` worktree with Claude or Codex in a term (model, effort, prompt typed into
  the PTY, never written to disk or logs), auto-runner bounded by `autoConcurrency`, `in_testing`
  when the agent ends its turn or exits, review approve/reject/commit.
- Activity: `/v1/lines/{id}/activity` parses Claude transcripts and Codex rollouts incrementally
  (work log, todos, edited files, last assistant text); `line.activity` SSE events.
- Events: `term.*`, `task.*` and `line.activity` on `/v1/events`.
- MCP: `overseer mcp`, a stdio MCP server with task tools for agents.
- Server mode: `daemon --detach`, `version [--json]`, `socket-path`, `GET /v1/version`;
  `make dist` builds static musl binaries.
- Windows: the crate builds for Windows; the daemon uses loopback TCP with a bearer token there.
- CI and releases on self-hosted Linux runners (ADR 0027, `docs/ci.md`): every merge
  to `main` tags `vX.Y.Z` from Conventional Commits and publishes a GitHub Release with the
  daemon for Linux musl x86_64/aarch64, macOS x86_64/aarch64 and Windows x86_64 (cross-built with
  `cargo-zigbuild` and `cargo-xwin`), `overseer-api`, and the app as `.deb`, `.AppImage`,
  `.rpm`, NSIS `.exe` and `.dmg`; `make dist-cross`, `make app-resources`,
  `scripts/set-version.sh`.

### Desktop app (`app/`, ADR 0025)
- Tauri 2 + Svelte 5 client for the daemon (protocol 1, `contract/app-daemon.md`): sidebar of
  sessions grouped by what they need (input, testing, working, pushed, idle, held), live
  terminals with ghostty-web over the daemon PTYs (tabs, two-pane split), activity panel (work
  log, todos, edited files, live diff, hold/pin/close/open PR), task board (drag and drop and
  keyboard moves, auto queue, review with approve/reject/commit), command palette, built-in
  browser (child webview), OS notifications and badge, settings, English and French.
- Neutral light and dark themes following the system, one accent color.
- Remote mode over SSH (russh): hosts from `~/.ssh/config` with ProxyJump, server install and
  version check under `~/.overseer/server/<version>`, everything through
  `direct-streamlocal` channels of one connection, reconnect with backoff.
- Local mode: Unix socket (loopback + token on Windows), daemon auto-start.
- Mock daemon for `pnpm dev`; vitest and Rust unit tests on in-memory streams;
  `make app-dev`, `make app-build`, `make app-test`.

## 0.1.0 — 2026-10-28

First complete version. `overseer` (daemon + CLI) records every Claude Code and Codex session
of a station with its worktree, ticket and resume command, tracks live state from hooks, holds
and closes lines safely, and syncs to `overseer-api`, the shared registry (enrolment, reads,
events, command relay, feedback-hub line creation). Lines resume locally or over SSH from any
station, through deep links, from the Board (per ticket, live, PR/CI, env links, diff/commit/
stash) or with `overseer patch`. Runner stations start and relaunch feedback-bot lines.
Everything is tested offline against an isolated tmux server, stub agents and fakes.

### Phase 7: integrations
- Board and remote relay: the API answers each sync heartbeat with a `stations` frame; `sync`
  writes `<state>/stations/<id>.json` (this owner's lines); hold, close, resume and link on lines
  of other online stations go through the API command relay with the user token (dry run,
  predicates and `acceptRemoteCommands` applied on the target). ADR 0019.
- Runner stations execute relayed `start` (new worktree on the ticket branch, tmux session,
  `claude --session-id`, prompt pasted from a `0600` temp file, never logged) and `relaunch`
  (new turn on the same line, refused unless at an empty prompt; resumes a held line first).
  `POST /v1/lines` accepts `dryRun` (default true). New config: `repos`, `worktreeDir`,
  `startArgs`. ADR 0018.
- `overseer patch AS-N`: SSH attach by default, `--local` opens a fresh worktree of the branch
  seeded with a summary (titles, branch, PR; never conversation content); `overseer titles`.
  ADR 0020.
- `link <line> --env <ref>` sets a local `envRef`; started lines keep their owner and origin.
- Opt-in `memoryPolicy` reports idle lines that would be held above a memory threshold in
  `status` and `doctor`; never holds. `doctor` adds API reachability, cmux socket and runner
  readiness. ADR 0021.
- `GET /v1/lines?ticket=` alias for TwinFactory.
- Packaging (generated, never installed): `deploy/runner/Dockerfile` (daemon, sync, tmux, Claude
  Code from npm), systemd user units and launchd plists for the daemon and sync.
- Tests: feedback-hub start/relaunch on a runner, Board relay across stations, patch, memory
  policy and doctor; TwinFactory consumer reading `/v1/lines?ticket=`, SSE and signed webhooks
  checked against `contract/schemas`; fakes in `tests/fakes/`. `scripts/demo-phase7.sh`.
- README, complete user guide.

### Phase 6: Board and cmux
- Board page (`board/`, vanilla JS, light/dark, English and French strings in one object) served
  by the daemon at `GET /board` on its socket and, with `daemon --http 127.0.0.1:<port>`, on
  loopback behind a per-start token; `overseer board [--cmux]` prints the URL.
- Lines grouped by ticket with state, station, worktree, branch, PR + CI (via `gh`, cached),
  env link (`envRef` → Yard); lines of offline stations greyed with actions disabled.
- Actions from the Board: resume, hold, close, link, pin (existing routes, dry run then confirm,
  refusals with their predicates), worktree diff (read-only), commit and stash (confirm).
- `GET /v1/events` SSE stream (line and station changes from each scan, `Last-Event-ID` replay);
  the daemon serves each connection on its own thread.
- Other stations read from `<state>/stations/<id>.json` until the sync client fills it.
- `overseer status --cmux-sidebar`; cmux sidebar plugin files in `deploy/cmux/` (unverified,
  not installed). `board --cmux` prints the cmux command instead of running it.
- Tests: Board end to end, 50 lines over 3 stations with request and SSE fanout timings, fake
  `gh`, fake Yard, headless browser check (skipped without a browser). `scripts/demo-phase6.sh`.
- ADR 0012.
### Phase 5: remote resume
- `overseer login --api URL` (user token on stdin, checked, `<state>/user.json` `0600`).
- `ls` merges the API's lines of other stations (STATION column, `OFFLINE` flag, `--owner`,
  `--local`); an unreachable API falls back to this station with a warning.
- `resume` of a line on another station: `ssh -t <target> -- overseer resume <id> --attach`,
  target from config `ssh` (default the station id); offline stations refused up front;
  `--print` shows the ssh command. cmux detected but not driven yet.
- `resume --attach`: held pane or detached session, then `tmux attach` (or attach to the running
  pane).
- `overseer open overseer://resume/<lineId>` with confirmation; handler files for Linux
  (`.desktop`) and macOS (AppleScript applet + `CFBundleURLTypes`) in `deploy/url-handler/`.
- ADR 0017; ADR 0006 accepted; `scripts/demo-phase5.sh`.

### Relayed command execution
- `sync` and `daemon` execute relayed `hold`, `close`, `resume`, `link` from
  `<state>/commands/inbox/` through the local actions with every hold predicate and report
  `succeeded` / `refused` / `failed` with a reason; atomic claim, crash recovery, audit in
  `actions.log`. Off unless `acceptRemoteCommands` is true (`refused: disabled`); `relaunch` and
  `start` refused. `sync` reacts to a command at once instead of at the next heartbeat.
- The daemon's local resume route uses the held pane or a detached tmux session.
- ADR 0016. Hook, hold and daemon ADRs renumbered 0013–0015 after the merge with Phase 4.
- End-to-end test: in-process API, `overseer sync`, stub agent on an isolated tmux server.

### Phase 4: overseer-api + sync
- `api/` crate `overseer-api` (axum, SQLite): station enrolment with hashed, revocable tokens;
  user tokens; WebSocket sync (snapshot, deltas, heartbeat, resumable and idempotent by seq);
  reads by owner, ticket, station; SSE events with `Last-Event-ID` replay; HMAC-signed webhooks;
  command relay with audit trail; `POST /v1/lines` for feedback-hub. Message and prompt texts are
  never stored.
- `overseer enroll` and `overseer sync [--once]`: outbox-based sync client; relayed
  commands are spooled to `<state>/commands/inbox/` for the local executor, never run by sync.
- `deploy/`: Dockerfile, docker compose for local dev, Kubernetes manifests (Deployment, Service,
  PVC, Secret reference).
- `make test`/`build`/`lint` cover both crates; `scripts/demo-phase4.sh`.
- ADR 0009; additive contract changes (`/users`, `DELETE /webhooks/{id}`, `/healthz`,
  `Station.revoked`, command kind `start`).

### Phase 3: `hold` / `close`, counters, orphans
- `overseer hold` with the predicates alive, inTmux, atPrompt, inputEmpty, noToolRunning,
  notPinned, notNeedsInput, each a tested function; dry run by default, `--yes` to act, every
  run logged to `actions.log`.
- Held line on tmux: `@sb_held` pane option and `overseer stub` ("press Enter to resume");
  held stays held across a simulated reboot; toolkit patch proposed in `docs/toolkit-patch.md`.
- `overseer close` (→ `hung-up`, holds a live line first), `pin`.
- Station counters (active / idle / held, agents RSS, memory), orphan and duplicate detection,
  `overseer status`, `overseer doctor`.
- ADR 0014.

### Phase 2: hooks, live state, `resume`, `link`
- `overseer hook` (spool per agent pid, allow-listed fields) and `overseer hooks
  install|uninstall` (dry-run diff, `--apply` with backup, validation, idempotent).
- Live `active`/`idle`/needs-input from hook events, with `sessions/<pid>.json` polling as fallback.
- `overseer daemon`: rescan loop + local HTTP API on a Unix socket (`daemon.openapi.yaml`,
  extended additively).
- `overseer resume` (stub pane, tmux window, or exec; original cwd and flags), `link`.
- `ls` shows the last 7 days by default (`--all`), owner from the worktree's git email, launch
  flags of dead sessions from the tmux-snapshot state file, macOS pid-reuse guard.
- Registry upgraded in place (WAL, new columns and tables). `serde_json` `preserve_order`.
- ADRs 0013, 0015; 0004 and 0005 accepted.

### Phase 1: daemon, local, read-only (+ `ls`)
- `overseer ls` (table and `--json`), filters `--ticket`, `--repo`, `--state`.
- Discovery of Claude Code transcripts and live sessions, Codex threads (state DB copy or
  rollouts), git worktrees, ticket keys from branch names, resume commands.
- Local SQLite registry (`lines`, `worktrees`) with stable line ids and remembered launch flags.
- ADR 0008: Rust, superseding 0001; amendments to 0002.

### Phase 0: discovery
- Findings, line states, contract drafts, ADRs 0001–0007, refined plan.
