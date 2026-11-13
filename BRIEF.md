# Overseer — build brief for Claude Code

Historical. This is the original build brief, kept for history. The stack and the scope changed
since: the daemon, API and CLI are written in Rust (see `docs/adr/0008-rust.md`), and the product
has a desktop and mobile app (see `docs/adr/0025-desktop-app.md`). The current rules are in
`AGENTS.md`, the current behaviour in `README.md` and `docs/user-guide.md`.

> **This is the only document you need.** It contains the context, the target architecture,
> the contracts, the knowledge about the existing setup on our machines, the plan and the
> rules. Put it at the root of an empty repo as `CLAUDE.md` (or `BRIEF.md`) and start with:
> *"Read the brief and run Phase 0. Stop at the end of Phase 0 and report."*

---

## Contents

1. Your role and working rules
2. Why we are building this
3. Target architecture (the whole system, and what is in scope)
4. The central contract: `Line`
5. Overseer specification (daemon, api, app)
6. Neighbouring systems and their interfaces (Yard, feedback-hub, TwinFactory, Jira, GitHub)
7. Reference journey: from a user feedback to a merge
8. What already exists on our stations (must reuse, must not break)
9. Facts about cmux, Claude Code and Codex (verify before relying on them)
10. Technical choices (defaults, overridable by ADR)
11. Phased plan with acceptance tests
12. Open decisions
13. Repo conventions
14. First action

---

## 1. Your role and working rules

You are the lead engineer building **Overseer**. The user (Alice, with Bob) reviews
every phase. Work in phases; each phase ends with working code, tests, a demo script the user
can run, and a short written report (built / verified / open questions). Then stop and wait.

**Rules**

- **Verify, don't assume.** Section 9 lists what we believe about cmux, Claude Code and Codex.
  All of it moves fast. Before depending on a behaviour, check it on the installed version and
  record the result (with version numbers) in `docs/findings/`.
- **Write decisions down.** Any non-obvious choice → a short ADR in `docs/adr/NNNN-title.md`
  (context, options, decision, consequences). Decisions marked "recommended" in this brief are
  defaults; you may propose a different option in an ADR, but ask before switching.
- **Ask when it matters.** Anything that could stop a process, close a tab, delete a worktree or
  lose a conversation: ask one precise question rather than guess.
- **Language.** Code, comments, commits, docs: English. Users are French-speaking; keep
  user-facing strings in one place so they can be translated.

**Safety (non-negotiable)**

- **Never stop, kill, close or "clean up" a real session, tmux session, cmux tab/workspace,
  agent process or worktree** unless the user explicitly approves that specific item. Every
  destructive command ships with `--dry-run` as the default during development and logs what it
  would have done.
- **Tests run on isolated infrastructure only**: a dedicated tmux server (`tmux -L sb-test`),
  stub binaries instead of real `claude`/`codex`, temporary `HOME`/state dirs, fixture session
  files. Never point a test at the default tmux server, the real cmux socket, real Jira or real
  GitHub.
- **Read-only first.** Phases 0–1 must not modify anything outside the repo and
  `~/.local/state/overseer/` (or the platform equivalent). Installing hooks happens only when
  the user says so, with backups.
- **Coexist with the existing station toolkit (section 8).** Do not modify those scripts. If a
  change is needed, write the patch in your report and let the user apply it.
- Before editing any user config (`~/.claude/settings.json`, `~/.codex/config.toml`,
  `~/.tmux.conf`, `~/.zshrc`, `cmux.json`): back it up (`cp f f.bak-$(date +%Y%m%d%H%M)`),
  **merge** instead of overwriting, validate (`jq -e .` for JSON) before replacing.
- No secrets (station tokens, socket passwords, API tokens) in logs, UI, fixtures or git.

---

## 2. Why we are building this

From a meeting on 28 Oct 2026 (Bob, Alice) and the architecture work
that followed.

**The problem.** Developers now run many AI coding sessions (Claude Code, Codex) in parallel,
each in its own terminal, often in its own git worktree, on several machines (their Mac, a
Linux dev VM on Proxmox reached over SSH, and soon shared runners). It does not scale:

- Bob reached **35 terminals/sessions saturating the 128 GB of RAM of their Mac**.
- On Alice's dev VM the **Linux OOM killer** killed Claude sessions: dozens of Claude processes
  kept running in terminal sessions whose tab had been closed, unseen.
- There is **no inventory**: nobody can answer "what is running, where, for which ticket, and
  how do I get back to it?" After a reboot or a saturated machine, work is hard to find and resume.

**The vision.** A unified developer workstation for the AI era: every agent session is
**saved and findable**, tied to its **ticket** and **worktree**, can be **parked to free RAM**
and **resumed** from anywhere, and is shown on a **board** — first as a CLI, then as a **cmux
plugin** (cmux is the terminal the team uses; see section 9).

**Explicit boundary (decided in the meeting).** The team also builds **TwinFactory**, a visual
inventory of the whole software factory. TwinFactory **shows, never controls**, and it is **not
the developers' workstation**. Overseer must not grow into TwinFactory, and vice versa.

---

## 3. Target architecture

The system is made of independent engines, each owning one concept.

```
EXTERNAL            Jira (ticket truth)                GitHub (branches, PRs, labels)

SHARED HOSTED       TwinFactory      feedback-hub      overseer-api      Yard
(Kubernetes)        read-only view   intake, proofs,   sessions & where     one env per PR,
                    of everything    dedup, triage     each worktree runs   TTL, create/destroy

EXECUTION           Dev's Mac         Dev VM (Proxmox)   Runner (pod)        Ephemeral envs
(where code and     local station     SSH station        shared station      kube namespaces:
agents run)         claude/codex,     same capabilities, sessions of         webplatform + backend
                    worktrees,        remote             feedback-hub's      + copied database
                    overseer-app                      agents
                    + overseer-daemon  + overseer-daemon  + overseer-daemon
```

Flows:

- Each **overseer-daemon** opens an **outbound WebSocket** to **overseer-api** over the
  **NetBird** mesh VPN (stations are never dialled into by the API).
- **overseer-app** (dev's Mac) talks to its local daemon; it reaches remote sessions via the
  API and attaches to them over **SSH**.
- **TwinFactory** reads Jira and GitHub directly, plus overseer-api (lines, states) and Yard
  (env state). It never writes.
- **feedback-hub** asks overseer-api to open a line for its agent account on a runner; its
  agent writes to Jira through the **Jira MCP** (create, comment, update).
- **Yard** watches GitHub: a PR with the `preview` label gets an environment; Yard publishes
  `env.ready` with the URL; it destroys the env when the PR is merged or closed.

### One concept, one owner

| Concept | Owner | What the others do |
|---|---|---|
| Ticket | **Jira** | store the key `AS-…`, never a copy; TwinFactory reads it live; the hub's agent writes via MCP |
| Branch, PR | **GitHub** | store the PR URL; TwinFactory reads PRs/CI/branches live |
| Agent session (id, resume) and its worktree (station, path, branch) | **Overseer** | go through its API, receive its events |
| Test environment (namespace, URL, TTL) | **Yard** | request it via a PR label, read its state |
| Feedback, masked proofs, triage | **feedback-hub** | only see the resulting ticket |
| Overall view | **TwinFactory** | nothing to do: it reads, never writes |

Overseer **does not create environments** (that is Yard) — it only stores a reference
(`envRef`). An env lives as long as a PR; a line lives as long as one person's work. Two
lifecycles, two engines.

### Scope of this repo

**In scope: Overseer** — `overseer-daemon`, `overseer-api`, `overseer-app`
(CLI first, cmux plugin second), and the shared contract.

**Out of scope**: building Yard, feedback-hub or TwinFactory. You define and honour the
interfaces Overseer exposes to them (events, read API, command relay, `feedback-bot` lines),
and you test them with **fakes** in this repo.

---

## 4. The central contract: `Line`

A **line** is one agent session working in one worktree on one station for one owner. This is
the contract shared by every client. Treat it as **frozen**: changes must be additive and
optional, and need an ADR.

```ts
// the central contract, shared by all clients
interface Line {
  id: string;
  owner: string;            // a person, or an agent account such as "feedback-bot"
  ticketKey?: string;       // "AS-1234"
  stationId: string;        // "mac-bob", "vm-node2-dev1", "runner-3"
  agent: 'claude' | 'codex';
  agentSessionId: string;   // used by `claude --resume <id>` / `codex resume <id>`
  repo: string;
  branch: string;
  worktreePath: string;
  prUrl?: string;
  envRef?: string;          // id on Yard's side, never a copy of its state
  origin: 'dev' | 'feedback';
  state: 'active' | 'idle' | 'held' | 'hung-up';
  lastActivityAt: string;   // ISO 8601
}
```

State meanings (pin them down precisely in Phase 0, in `docs/line-states.md`):

- `active` — the agent process is alive and has had activity recently (working, or just answered).
- `idle` — the agent process is alive but nothing happened for a configurable delay.
- `held` — the agent process was stopped on purpose to free resources; the session id is kept
  and the line can be resumed exactly.
- `hung-up` — the work is over (closed by the user, or its PR merged/closed); kept for history.

Ownership & views:

- Every line has an **owner**: a human or an agent account (e.g. `feedback-bot`).
- A dev's **Board** shows **their** lines across all stations (Mac, VM, runner).
- **TwinFactory** sees every line of everyone, read-only.
- A **ticket** can carry several lines over time (the hub's agent, then the dev, then a
  reviewer). The ticket view groups its lines, PRs and envs.
- A line **without** a ticket is allowed (exploration) and can be linked later.

Useful local-only data the daemon may keep **outside** the contract (not synced unless an ADR
says so): process ids, memory use, tmux/cmux terminal references, original launch flags, a
"needs input" flag. Store it in the daemon's registry, not in `Line`.

---

## 5. Overseer specification

The core has **two missions**; everything else (command relay, envs, advanced views) comes after.

1. **Save every Claude and Codex session**: agent, session id to resume it, ticket, start and
   last-activity dates. A session survives a reboot or a saturated machine.
2. **Remember where every worktree runs**: station (Mac, SSH VM, pod), path, repo, branch.
   Finding a session means knowing where to go to resume it.

### 5.1 overseer-daemon — one per station

Mission: on each machine, save Claude and Codex sessions and know where each worktree runs.

Must:
- **Discover existing sessions** from Claude Code's and Codex's own files: session id, working
  directory, start and last-activity dates (see section 9 for where these live — verify).
- **Discover git worktrees** of the tracked repos: path, repo, branch (`git worktree list
  --porcelain`). Which repos are tracked is configuration (roots to scan + explicit list).
- **Link a session to a ticket**: inferred from the branch name (an uppercase `PROJECT-1234` key anywhere in it, lowercase only for keys in `OVERSEER_TICKET_KEYS`,
  e.g. `feature/AS-1234-foo`, `feedback/AS-1234`), or set by hand (`link`).
- **Track state** (`active`, `idle`) from Claude Code hooks (and Codex's equivalent if any),
  with process inspection as a fallback.
- Keep a **local registry that works offline**, then **sync** it to overseer-api over an
  **outbound** connection; resynchronise cleanly after being offline.
- **Execute commands**: `resume`, `hold` (stop the agent process, keep the session id), `close`
  (→ `hung-up`). Commands may come from the local app or relayed by the API.

Must not: create environments, write to Jira or GitHub.

**Delivered when:** after a reboot of the Mac, every recent session is listed with its worktree,
its ticket and its resume command — and resuming works.

### 5.2 overseer-api — hosted (Kubernetes)

Mission: the shared registry of all stations, queried by the other tools.

Must:
- **Enrol a station** with a token unique to that station (revocable).
- **Receive inventories and heartbeats** from daemons; mark stations offline when heartbeats stop.
- **Answer reads**: lines by owner, by ticket, by station.
- **Publish line state changes** to TwinFactory and feedback-hub (events).
- **Relay a command** to the right station, and keep an audit trail of who asked for it.

Must not: run code or launch an agent; store conversation content — **metadata only**.

**Delivered when:** lines from two different stations (the Mac and a VM) appear in one read.

### 5.3 overseer-app — developer's Mac

Mission: the developer's interface to find, resume and tidy their work.

Must:
- **V1, CLI**: `overseer ls`, `resume`, `hold`, `close`, `link <ticket>`.
  `ls` shows, per line: ticket, worktree, station, state, last activity, and the resume command.
- **Resume a remote session over SSH** without the developer needing to know where it runs.
- **Show how many sessions are active** (per station, with memory where available) to avoid
  saturating RAM.
- **V2, cmux plugin**: a **Board** per ticket; access to the worktree's diff, commit and stash.

Must not: keep its own state — everything comes from the local daemon.

**Delivered when:** `overseer ls` then `overseer resume` reopen yesterday's session,
locally as well as on a VM.

### 5.4 Behaviour details to design (Phase 0 output)

- **Hold, safely.** Only hold when: the agent is at its prompt (not mid-turn); its input box is
  empty (never discard half-typed text — check the terminal content); no tool subprocess or
  background task is running (long-lived MCP server children are fine to stop; Bash/background
  tasks/subagents are not); the line is not pinned. Each rule is a separately tested predicate.
- **Where a held line resumes.** On tmux stations, prefer leaving a small stub in the same
  terminal ("held — AS-1234 — press Enter to resume") that relaunches the agent with
  `--resume`, so the tab stays meaningful. Decide via ADR.
- **Resume command fidelity.** Resume in the original `worktreePath`, with the original launch
  flags (minus any previous `--resume/--continue/--session-id`), on the original station.
- **Memory pressure (later, opt-in).** A policy that suggests or performs `hold` on idle lines
  (least recently used first) when a station exceeds a budget — never violating the hold rules,
  dry-run by default, every action logged and notified.
- **Orphans.** Detect sessions whose terminal tab is gone but whose process still runs (the
  cause of the OOM incident) and surface them prominently. Never auto-kill.

---

## 6. Neighbouring systems and their interfaces

Build **fakes** of these in `tests/fakes/` to exercise Overseer's interfaces.

**Yard** (hosted, Kubernetes) — one ephemeral environment per PR, nobody asks by hand.
Triggered by the `preview` label on a PR; creates a namespace, deploys the PR's services and a
copied database; publishes env state and URL (`env.ready`); applies a TTL and a cap; deletes
everything when the PR is merged/closed; refuses Docker images under the agreed layer-efficiency
threshold. Knows nothing about sessions, people or tickets.
→ Overseer only stores `envRef` and may display Yard's URL/state by reading Yard.

**feedback-hub** (hosted) — turns a user feedback into a documented ticket, a draft fix and a
preview to validate. Keeps: the in-app widget, element picker, masking, screenshots and video
replay, the backend calls of the last 30 s, dedup and search for existing fixes, triage, Slack,
thread page, qualification rules. Writes to Jira through its agent and the Jira MCP.
Delegates previews to Yard (PR label) and **worktrees and agent sessions to Overseer** (a
`feedback-bot` line on a runner). It migrates through two ports, `Workspace` and `Previews`,
each with an `embedded` adapter (current local code) and a `remote` adapter.
→ Overseer must offer: "open a line for owner X on a runner, repo R, branch B, origin
`feedback`", "relaunch this same line (same agent session) with a new message", and line events.
Design this API in Phase 0 even though feedback-hub will consume it later.

**TwinFactory** (hosted) — read-only visual inventory. Reads Jira, GitHub, overseer-api and
Yard live. Ticket view (lines, PRs, CI, env); stations view (each agent, where, state). Offers a
"resume" link that opens the session in overseer-app.
→ Overseer must offer: read endpoints, a state-change event stream, and a **deep link** /
URL scheme that overseer-app handles (e.g. `overseer://resume/<lineId>`).

**Jira** — project `AS`, keys `AS-NNNN`. Source of truth for tickets. Overseer stores keys only.
**GitHub** — source of truth for branches/PRs/labels. Overseer stores PR URLs only; the app
may use the `gh` CLI (already authenticated on stations) for read-only PR/CI info on the Board.

---

## 7. Reference journey: from a feedback to a merge

1. **feedback-hub** — a user reports a problem in the app; the hub captures proofs and looks for
   an existing fix.
2. **feedback-hub → Overseer** — the hub requests a line for `feedback-bot` on a runner;
   Overseer starts it. The agent creates ticket `AS-…` via the Jira MCP, then fixes on branch
   `feedback/AS-…`.
3. **Runner** — the agent pushes and opens a **draft PR** with the `preview` label.
4. **Yard** — sees the PR, creates the namespace, deploys, publishes `env.ready` with the URL.
   The hub and Overseer display it.
5. **feedback-hub** — the reporter tests on the URL and answers ✅ or ❌. An answer **relaunches
   the same line**, i.e. the same agent session.
6. **Dev's Mac** — the dev takes over: `overseer patch AS-…`. Either they **attach over SSH**
   to the runner's line (same session), or they open **their own local line** on the same branch,
   seeded with a summary of the agent's conversation.
7. **GitHub** — merge. Yard destroys the env, Overseer hangs up the lines, TwinFactory updates.

Key constraint: a Claude/Codex session lives in the files of the station where it ran. Resuming
it *as is* requires being on that station — hence the SSH attach. Otherwise it is replaced by a
new line that inherits the context.

---

## 8. What already exists on our stations (reuse it, don't break it)

Alice's Linux dev VM is driven from **cmux on macOS over SSH**, with a battle-tested toolkit.
Overseer runs alongside it. You will not have the toolkit's source in this repo; here is what
you need to know. Inspect the real files **read-only** on the VM in Phase 0 (`~/.local/bin/`,
`~/.claude/hooks/`, `~/.zshrc`, `~/.tmux.conf`) to confirm details.

### 8.1 How a remote cmux tab works

```
Mac (cmux) ──ssh──▶ cmux-tui (headless server, one per user on the VM; cmux ≥ 0.65)
                      └─ zsh (~/.zshrc) ─exec─▶ tmux-guard ─▶ tmux attach/new -s cmux-<TABID8>
                                                               └─ shell / claude / …
Phone ──ssh/mosh──▶ zsh ─▶ `t` (touch picker) ─▶ tmux attach -t <any session>
Boot ──▶ systemd --user: tmux-restore ◀── tmux-snapshot (timer every minute)
Mac launchd ──▶ cmux-keeper (rebuilds workspaces after a remote reboot; kills the tmux
                session of a tab closed on purpose)
           └──▶ cmux-tab-titles-sync (every minute: tab names, reconnects, socket password)
```

- **Every cmux tab = its own persistent tmux session** named `cmux-<first 8 hex of the tab id,
  uppercase>`. Sessions keep running when the Mac disconnects, sleeps, quits cmux or is off.
- `tmux-guard` is a Python PTY proxy between cmux and each tmux client (filters replayed
  terminal query responses, re-asserts mouse/paste/focus modes). **Every tmux client in a cmux tab
  runs under it.** Never attach a tmux client to a cmux-tab session without going through
  `~/.local/bin/tmux-guard` (otherwise the mouse wheel types garbage into Claude).

### 8.2 tmux options used as metadata (read them, don't repurpose them)

| Option | Scope | Meaning |
|---|---|---|
| `@claude_session` | pane | Claude conversation id, set by a SessionStart hook; follows `/clear` and compaction |
| `@cmux_slot` | session | the cmux workspace the session belongs to (workspace **name** on cmux ≥ 0.65) |
| `@cmux_created` | session | original creation time of the tab's session (survives reboots) |
| `@cmux_attached` | session | last time a cmux tab attached to it |
| `@cmux_title` / `@cmux_title_tab` | session | the tab name given by the user (synced from the Mac) |
| `@restored_from` | session | set by `tmux-restore` on sessions resurrected after a reboot, cleared on adoption |

The existing SessionStart hook (`~/.claude/hooks/tmux-claude-id.sh`):

```bash
#!/usr/bin/env bash
[ -n "$TMUX_PANE" ] || exit 0
sid=$(jq -r '.session_id // empty' 2>/dev/null)
[ -n "$sid" ] && tmux set -p -t "$TMUX_PANE" @claude_session "$sid" 2>/dev/null
exit 0
```

There are also `Notification` and `Stop` hooks (`~/.claude/hooks/cmux-notify.sh`) relaying to the
cmux sidebar. **Add Overseer hooks next to these; never replace them.** Hooks are declared in
`~/.claude/settings.json` under `hooks.<Event>[].hooks[]` with absolute paths.

### 8.3 Snapshots and resurrection

- `tmux-snapshot` (every minute + after the nightly auto-update) writes
  `~/.local/state/tmux-snapshot/<UTC timestamp>/`, repoints `latest`, keeps the 20 newest, never
  replaces a good snapshot by an empty one. Per pane it records: session, window, layout, pane,
  cwd, scrollback, current command, `@claude_session`, the `@cmux_*` options, and for Claude panes
  the **original claude argv** (found by walking the pane's process tree; native binary path
  contains `/claude/versions/`), with `--resume/-r/--session-id/--continue/-c` stripped.
  Fallback when a pane has no `@claude_session`: newest `*.jsonl` in
  `~/.claude/projects/<cwd with every non-alphanumeric char replaced by '-'>/`.
- `tmux-restore` (systemd --user at boot, with linger) recreates every session and relaunches each
  Claude with `claude --resume <id> <original flags>`. Log: `~/.local/state/tmux-snapshot/restore.log`.
- **Implication for Overseer:** this is already a working "resume exactly" mechanism on tmux
  stations. Reuse its approach (argv capture, flag stripping, id fallback). A **held** line must
  stay held across a reboot (tmux-restore must not relaunch it) — design how (e.g. a pane option
  such as `@sb_held` that the user adds to tmux-restore via a small patch you propose).

### 8.4 Mac side

- `ws <repo>` opens a new cmux workspace on the VM, `cd` into the repo:
  it runs `cmux ssh <host> --name "🖥 <repo>" --command "cd <dir> && exec $SHELL -l"`.
  `ws -s` lists tmux sessions (● attached / ○ not), `ws -a <session>` reattaches an orphan
  session into a new tab, `ws -k <session>` kills one. **Remote resume in overseer-app can be
  built on the same `cmux ssh … --command "cd <worktree> && claude --resume <id> …"` pattern** —
  the VM's `.zshrc` replays cmux's initial command inside the tab's new tmux session.
- `cmux-keeper` snapshots cmux workspaces to `~/.local/state/cmux-keeper/snapshot.json`, rebuilds
  them after a remote reboot (writing reservations to `~/.cache/cmux-ws/claims.json` on the VM),
  and kills the tmux session of a tab closed on purpose (event `surface.closed` with origin
  `tab_close`, after 10 s grace, never in bursts, never if another client is attached).
- On the phone: the Claude app's Remote Control (`remoteControlAtStartup: true`) and a mosh
  terminal opening the `t` picker.

### 8.5 Known pitfalls that affect Overseer

- cmux 0.65 **left Claude processes running in sessions of closed tabs** → OOM. Orphan detection
  matters. Orphan = tmux session with no client attached and not shown by any cmux tab.
- **Do not call cmux in bursts.** A burst of calls crashed cmux's socket (`Broken pipe`); close
  tabs ~1/s at most; run `cmux ssh` workspace creations **one at a time** (in parallel they time out).
- `cmux close-surface` on a tab whose process runs **waits for a UI confirmation and blocks**.
- cmux may rewrite `cmux.json` **without `automation.socketPassword`** → socket calls fail with
  `auth_required`; `cmux-tab-titles-sync` restores it. Handle that error gracefully.
- cmux 0.65.2 **leaks file descriptors** (~10 k after two days → it can no longer spawn processes).
  If cmux calls start failing with `Code=9`/`Bad file descriptor`, report "restart cmux", don't retry in a loop.
- Two tabs on the **same** `@claude_session` interfere; Overseer should flag duplicate session ids.
- The VM's group-writable state dirs broke `cmux-tui` once (umask 002): create Overseer's state
  dirs with `0700`.
- macOS purges `$TMPDIR` after a few days: don't keep anything durable there.

---

## 9. Facts to verify (cmux, Claude Code, Codex)

**cmux** (manaflow-ai/cmux) — native macOS terminal (Swift/AppKit, Ghostty-based) for running
many coding agents. As far as we know it has **no formal plugin API**; it is extended through:
- the `cmux` CLI (`/Applications/cmux.app/Contents/Resources/bin/cmux`), self-documented:
  `cmux --help`, `cmux <cmd> --help`, `cmux docs api|browser|settings|agents`;
- a **JSON-RPC socket** (default `/tmp/cmux.sock`, overridable with `CMUX_SOCKET_PATH`, password
  auth depending on mode) with method families `system.*` (incl. `system.capabilities`),
  `window.*`, `workspace.*`, `pane.*`, `surface.*`, `notification.*`, `browser.*`;
- an in-app **browser surface** (WKWebView) scriptable from the CLI — a web Board can live there;
- a markdown viewer, sidebar status/notifications, `cmux events` (e.g. `--name surface.closed`);
- settings in `~/.config/cmux/cmux.json` (project overrides `.cmux/cmux.json`), `cmux reload-config`;
- env vars in cmux terminals: `CMUX_WORKSPACE_ID`, `CMUX_SURFACE_ID`, `CMUX_SOCKET_PATH`;
  remote tabs on ≥ 0.65 get `CMUX_TUI_TERMINAL_ID`.
"cmux plugin" therefore means: **a companion that drives cmux through these public surfaces**,
with the Board as a web page in a browser surface. Phase 0 confirms this and checks whether a
real plugin mechanism now exists.

**Claude Code**
- Transcripts: `~/.claude/projects/<sanitised cwd>/<session-id>.jsonl`.
- Resume: `claude --resume <session-id>` (in the original cwd).
- Hooks (in `~/.claude/settings.json`): at least `SessionStart`, `UserPromptSubmit`, `Stop`,
  `Notification`, `SessionEnd`; hook input is JSON on stdin including `session_id`, `cwd`,
  `transcript_path`. Hooks must be fast and must never block or fail the agent.
- Clean exit: to determine (e.g. `/exit` typed at an empty prompt vs. a signal) — test on a stub
  and on one real session designated by the user.

**Codex** (OpenAI Codex CLI)
- Sessions believed to be stored as JSONL "rollouts" under `~/.codex/sessions/YYYY/MM/DD/`.
- Resume believed to be `codex resume <session-id>` (and a picker without id).
- Hook-like integration: a `notify` program in `~/.codex/config.toml` (verify which events exist).
  If Codex has no equivalent of a turn-start event, derive state from file mtimes + process inspection.

Record each verified fact (command, output excerpt, version) in `docs/findings/`.

---

## 10. Technical choices (defaults — override by ADR, with user approval)

- **Language: Go** for `overseer-daemon`, `overseer-api` and the `overseer` CLI —
  single static binaries for macOS (arm64) and Linux (amd64/arm64), easy to drop on a Mac, a VM
  or a pod, good WebSocket/HTTP support. (Alternative to discuss: TypeScript everywhere, since the
  contract is written in TS and the Board is web.)
- **Contract as the source of truth:** define `Line`, events and API in **JSON Schema + OpenAPI**
  under `contract/`; generate Go types and TS types (for the Board) from it; contract tests ensure
  both sides agree.
- **Daemon storage:** SQLite (pure-Go driver, no cgo) in `~/.local/state/overseer/` (Linux) /
  `~/Library/Application Support/overseer/` (macOS), dirs `0700`. Offline-first; an outbox of
  changes to sync.
- **Daemon ↔ API:** outbound **WebSocket** over NetBird, authenticated by the station token;
  messages = inventory snapshot, incremental changes, heartbeat (e.g. every 15 s), command
  delivery + command results. Idempotent, sequence-numbered, resumable after disconnection.
- **API storage:** PostgreSQL. Deployed on Kubernetes (Helm chart or plain manifests in `deploy/`).
  Events published for TwinFactory/feedback-hub — start with an SSE or WebSocket stream plus
  webhooks; decide in an ADR (do not introduce a message broker without need).
- **App ↔ daemon:** local Unix socket with a small JSON API. The CLI is a thin client.
- **Hooks → daemon:** tiny hook programs that append an event to the daemon's socket or a spool
  file; < 50 ms; silently no-op if the daemon is down.
- **Remote resume:** SSH (respect `~/.ssh/config` aliases); on cmux, open the session in a cmux
  workspace (`cmux ssh … --command …`) so it lands in a persistent tmux session on tmux stations.
- **Board (V2):** static web app served by the local daemon on `127.0.0.1`, opened in a cmux
  browser surface; no build step if possible, otherwise a minimal one.
- **Service management:** launchd agent on macOS, systemd --user unit on Linux, a plain process
  in pods. All bound to localhost except the API.

---

## 11. Phased plan with acceptance tests

### Phase 0 — Discovery & design (no product code)
Read-only investigation, then design. Deliver:
- `docs/findings/cmux.md` — version, which CLI commands/socket methods/browser surface/sidebar/
  events actually work, whether a plugin mechanism exists, exact commands tried.
- `docs/findings/claude-code.md` and `docs/findings/codex.md` — session storage format and
  fields used, resume semantics (cwd, flags), available hooks/notify events and their payloads,
  clean-exit method, how to tell MCP children from tool subprocesses.
- `docs/line-states.md` — exact state machine for `active/idle/held/hung-up`, transitions and
  their triggers; how `hold` safety is checked.
- `contract/` — first draft of JSON Schema for `Line`, event types, OpenAPI for the API (reads,
  enrolment, command relay, line creation for feedback-hub), local daemon API.
- ADRs: language/stack, storage, sync protocol, hook transport, held-line representation on tmux
  stations, deep-link scheme, decisions from section 12 (as recommendations for the user).
- A refined plan and risk list. **Stop and report.**

### Phase 1 — Daemon, local, read-only (+ `ls`)
- Discovery of Claude and Codex sessions from their files, worktree discovery, ticket inference
  from branch names, local SQLite registry, `overseer ls` (table + `--json`), filters by
  ticket/repo/state.
- Tests: fixtures of session files for both agents; a fake git repo with several worktrees and
  branch names (`AS-1234` variants, no ticket); clock control for activity dates.
- **Acceptance:** on the real Mac and VM, read-only, `ls` lists recent sessions with worktree,
  ticket and a correct resume command; nothing written outside the state dir.

### Phase 2 — Hooks, live state, `resume` (local), `link`
- Hook programs + installer (dry-run shows the merged settings; real install only on approval,
  with backup). Live `active/idle`. `overseer resume <line>` locally (in the right cwd, with
  the original flags), `link <ticket>`.
- **Acceptance (= daemon "delivered when"):** after a reboot of the Mac, every recent session is
  listed with worktree, ticket and resume command, and `resume` works.

### Phase 3 — `hold` / `close`, active-session counter, orphans
- `hold` with every safety rule as a tested predicate (isolated tmux server + stub agent),
  held-line representation on tmux stations, interop with `tmux-snapshot`/`tmux-restore`
  (held stays held after a simulated reboot), `close` → `hung-up`, per-station count of active
  sessions (+ memory where available), orphan and duplicate-session detection.
- **Acceptance:** refusal cases (typed input, running tool, mid-turn, pinned) each covered by a
  test; one supervised trial on a real session chosen by the user.

### Phase 4 — overseer-api + sync
- Station enrolment with per-station tokens, WebSocket sync (snapshot + deltas + heartbeat,
  resumable), reads by owner/ticket/station, state-change event stream, command relay with audit.
  Local dev via docker compose; Kubernetes manifests.
- **Acceptance (= api "delivered when"):** lines of the Mac and of a VM appear in one read; a
  daemon offline for 10 minutes resyncs without duplicates; a relayed `hold` is audited.

### Phase 5 — App: remote resume
- `overseer resume` / `ls` across stations through the API; remote resume over SSH (through
  cmux when available), without the user knowing where the line runs; deep links
  (`overseer://resume/<lineId>`).
- **Acceptance (= app "delivered when"):** `overseer ls` then `overseer resume` reopen
  yesterday's session, locally as well as on a VM.

### Phase 6 — cmux plugin & Board (V2)
- Board per ticket in a cmux browser surface: lines grouped by ticket, states, station, PR link
  and CI status (read via `gh`), env link (`envRef` → Yard, via a fake until Yard exists);
  actions: resume, hold, close, link; worktree diff / commit / stash. Sidebar status in cmux with
  the number of active sessions. Live updates.
- **Acceptance:** with 50 fake lines over 3 stations the Board stays responsive; every action
  works end-to-end on the test setup.

### Phase 7 — Integrations
- feedback-hub endpoints (`feedback-bot` lines on a runner, relaunch same line), runner station
  packaging (container image with daemon), `overseer patch AS-…` (attach over SSH or new
  local line seeded with a conversation summary — per decision 2), TwinFactory read/event
  compatibility tests, optional memory-pressure policy.

---

## 12. Open decisions (bring a recommendation in Phase 0)

1. **How an environment is requested** — a label on the PR read by Yard (**recommended**: lowest
   coupling, nobody calls Yard) / Yard's API called by the hub or Overseer (useful for an env
   without a PR) / both. *Overseer impact: only `envRef` either way.*
2. **When a dev takes over an agent's line** — SSH attach by default, new local line on demand
   (**recommended**) / always a new local line with a summary / always SSH attach.
3. **What Yard is built on** — the Kubernetes tool Alice is installing, plus a published status
   (**recommended**) / a home-made operator with a `PreviewEnv` resource / GitHub Actions
   deploying on label. *Not Overseer's call; just keep `envRef` opaque.*
4. Overseer-specific: language (Go vs TS), event delivery to consumers (stream vs webhooks),
   held-line representation on tmux stations, what the Board shows for lines of other stations
   that are offline.

---

## 13. Repo conventions

- Layout: `contract/` (schemas, OpenAPI, generators), `daemon/`, `api/`, `cli/`, `board/`,
  `hooks/`, `deploy/` (k8s, launchd, systemd), `tests/` (`fixtures/`, `fakes/` for Yard,
  feedback-hub, TwinFactory, GitHub, Jira), `docs/` (`adr/`, `findings/`, user guide).
- `make test` runs everything offline in under a few minutes; CI runs it on Linux and macOS.
- Installers are idempotent, show a diff in dry-run, back up before changing anything, and have
  an uninstall.
- `overseer doctor` checks: daemon running, hooks installed, cmux version/socket reachable,
  tmux version, API reachable, station enrolled, coexistence with the toolkit of section 8.
- Branches/commits: follow the team's convention (ticket key `PROJECT-NNNN` in the branch name and
  commit messages) — ask the user for the ticket number of this project.
- Keep `CHANGELOG.md` and `docs/user-guide.md` updated at the end of each phase.

---

## 14. First action

Start **Phase 0** now:
1. List what you will inspect and the exact read-only commands you plan to run on the Mac and
   (via `ssh <alias>`) on the VM. Ask the user for the SSH alias and the repo roots if you don't
   find them.
2. Run them, write `docs/findings/*`, the state machine, the contract draft and the ADRs.
3. Do not install anything, do not modify any config, do not touch any running session.
4. End with your report and the questions you need answered before Phase 1.
