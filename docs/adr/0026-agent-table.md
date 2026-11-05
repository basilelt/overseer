# 0026 One table for every agent CLI

Status: accepted

## Context
Overseer knew two agents, Claude Code and Codex, and their flags were spread over `tasks.rs`,
`lib.rs` (resume), `remote.rs` and the contract. Users also run Gemini CLI, Antigravity (`agy`),
opencode, Amp, Copilot CLI and Cursor Agent, and want to start any of them from the app. These
CLIs differ in every detail: model and effort flags, whether a session id can be chosen up
front, how a first prompt is given to the interactive UI, how to resume, and whether their
sessions are stored in a readable form.

## Decision
- `src/agents.rs` holds one static table, one row per agent id (`claude`, `codex`, `gemini`,
  `agy`, `opencode`, `amp`, `copilot`, `cursor`, `shell`), with plain data: binary, install
  hint, model flag, effort argv template plus renames (`max` → `xhigh` for Codex and Copilot,
  `ultra` for Amp's `--mode`), session-id template, prompt flag, resume-by-id template and
  "continue latest" argv, `verified`, `sessions`, `turns`. No trait: behaviour is three small
  functions over the row (`launch`, `flags`, `resume_argv`). Flags that do not exist for an
  agent are not in its row, so they are dropped. Rows were filled from `--help` of the installed
  CLIs (2026-10-29); `cursor` is marked unverified.
- Detection runs `<bin> --version` (stdin closed, 5 s timeout, all agents in parallel) and caches
  the result for 60 s per `PATH`. Nothing else ever runs an agent outside a term the user asked
  for.
- The prompt goes in argv when the CLI has an interactive prompt flag (`gemini -i`, `agy -i`,
  `copilot -i`, `opencode --prompt`): no timing guess. `Term.cmd`, events and logs show
  `<prompt>`. Claude and Codex keep the typed prompt: their argv is read back from `/proc` to
  build the resume command, so a prompt in argv would be replayed on every resume; Amp has no
  such flag (stdin only) and Cursor's is unverified.
- Lines: Claude transcripts, Codex rollouts and now Gemini chats (`~/.gemini/tmp/<project>/chats/
  session-*.json[l]`, cwd from `.project_root`) are discovered with activity. The other agents
  store sessions in SQLite with protobuf blobs (Antigravity), an empty or token-bearing SQLite
  (opencode, Copilot) or on a server (Amp); rather than guess, a daemon term running one of them
  is itself a line (session id = the id given at start for Copilot, else the term id) and
  resuming it uses the agent's "continue" argv in the same directory.
- Turn detection stays hook-based (Claude, Codex). Other agents move a task to `in_testing` when
  they exit or call MCP `set_task_status`; an output-silence heuristic was rejected because the
  prompt delivery itself waits for silence and long tools print nothing.
- The contract stays at protocol 1: `GET /v1/agents`, `agent` on `POST /v1/terms` and Term, new
  agent ids, `Task.rank` and `POST /v1/projects` are additions; clients must accept unknown agent
  ids.

## Consequences
- Supporting a new CLI is one row plus a stub in `tests/stubs` (all stubs are links to
  `tests/stubs/agent`, which records its argv).
- Lines of term-only agents vanish with their term (daemon restart). Their activity panel is
  empty (409).
- Gemini resume by session id (`--resume <uuid>`) is not documented by its help (which lists
  `latest` or an index), so its lines report `resumeVerified: false`.
- `opencode` and `amp` installed through npm without their postinstall step report
  `installed: false`, because their wrappers fail on `--version`.
