# 0037 Agent profiles: models and reasoning per CLI, applied at launch

Status: accepted

## Context
Users want to control token spend: which model and reasoning effort the main agent uses, which
model its subagents (and their workers) use, and the background model, per agent CLI, with a
global default, a per-workspace override and a per-task/session choice. The agent table (ADR
0026) already maps one model and one effort per launch. Each CLI exposes the rest differently,
and the user's own config files (`~/.claude/settings.json`, `~/.codex/config.toml`, subagent
definitions) must not be rewritten behind their back.

## Research (2026-10-29, installed CLIs, read-only)
[L] = verified locally (`--help`, or a grep of the installed binary/bundle); [D] = from docs, not
verified. No session was started and no model was called.

| CLI (version) | Main model | Effort | Default subagent model/effort | Per named subagent | Background model |
|---|---|---|---|---|---|
| Claude Code 2.1.289 | `--model` alias or id [L]; `ANTHROPIC_MODEL`, settings `model` | `--effort low\|medium\|high\|xhigh\|max` [L]; `CLAUDE_CODE_EFFORT_LEVEL`, settings `effortLevel`, `MAX_THINKING_TOKENS` [L] | env `CLAUDE_CODE_SUBAGENT_MODEL` [L]: used only when neither the Agent tool call nor the frontmatter sets a model; `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` makes it win over both (except `inherit`) [L]. No default subagent effort | frontmatter `model` (alias, id, `inherit`) and `effort` (`low…max` or integer) [L]; `--agents '<json>'` defines session agents with the same schema [L], outranking project/user ones [D] | `ANTHROPIC_DEFAULT_HAIKU_MODEL` [L] (remaps the haiku alias), `ANTHROPIC_SMALL_FAST_MODEL` legacy [L] |
| Codex 0.165.0 | `-m`, `-c model=` [L] | `-c model_reasoning_effort=none\|minimal\|low\|medium\|high\|xhigh` [L]; `model_reasoning_summary`, `plan_mode_reasoning_effort` [L] | `-c agents.default_subagent_model=`, `-c agents.default_subagent_reasoning_effort=` [L] | `[agents.<role>]` has only `description`, `config_file`, `nickname_candidates`; model and effort live in the role's `config_file` TOML [L] | `review_model` (review only) [L] |
| Gemini CLI 0.66.0 | `-m`, `GEMINI_MODEL` [L] | none at launch; `modelConfigs…thinkingConfig` in settings [L] | none | `.gemini/agents/*.md` frontmatter `model` (no effort) [L] | internal constants only |
| opencode 1.19.5 | `-m provider/model` [L] | `run --variant` (provider specific) [L] | none (subagents inherit) [D] | `agent.<name>.model` [L]; inline through env `OPENCODE_CONFIG_CONTENT` (merged at local scope) [L] | `small_model` [L], same env |
| Amp | no model flag; `--mode low\|medium\|high\|ultra` picks model and prompt [L] | the mode | built in, not configurable [L] | plugin agents only | — |
| Copilot CLI 1.0.48 | `--model` with a fixed list [L] | `--effort low\|medium\|high\|xhigh` [L] | none | `.github/agents/*.agent.md` `model:` [D] | — |
| Antigravity 1.1.0 | `--model` [L] | none | none | none | — |
| Cursor Agent (not installed) | `--model` [D] | none | none | `.cursor/agents/*.md` `model:` [D] | — |

The user's 11 Claude subagents all set `model: opus` in frontmatter, so the plain
`CLAUDE_CODE_SUBAGENT_MODEL` would not reach them.

## Decision
- **Profiles live in the daemon**, in `<state>/profiles.json`: `{default, profiles, scopes}`.
  Built-ins (`economy`, `balanced`, `max`) are code constants, never written to the file, and
  read-only; user profiles are validated on `PUT /v1/profiles` (known agent, field supported by
  that CLI, effort in the shared list, model strings limited to `[A-Za-z0-9._:/[]@-]` so nothing
  can inject flags, YAML or TOML). The daemon owns every launch (terms, tasks, auto runner,
  resume), and desktop and phone share it.
- **Resolution**, most specific first: the task's or session's explicit `profile` (`none` = CLI
  defaults) > the scope whose path is the longest prefix of the session cwd or task project >
  `default` (a task-type rule now sits after the explicit profile, ADR 0041). The default ships empty, so nothing changes until the user picks a profile. Explicit
  `model`/`effort` in a request still win over the profile. A task whose stored profile was deleted
  falls back to its scope instead of failing to start. Tasks gain a `profile` column. A task
  created with a profile and no explicit effort stores an empty effort, so the profile's effort applies at start.
- **Application at launch only** (`profiles::apply`), per row of a small `support()` table:
  `--model`/`--effort` through the existing agent table; Claude subagent and background models as
  env (`CLAUDE_CODE_SUBAGENT_MODEL`, optional `…_FORCE=1`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`); Codex
  subagent model and effort as `-c agents.default_subagent_*` flags; opencode background and
  per-agent models as `OPENCODE_CONFIG_CONTENT`. Flags join `Term.flags` so a resume replays them;
  env-only values are resolved again from the session's directory (and its main worktree) when a
  held line is resumed.
- **Per-named Claude subagent** overrides stay in the profile and reach the files only through
  `POST /v1/profiles/apply-files`: a dry run returns the frontmatter keys that would change per
  file (project `.claude/agents` first, then `~/.claude/agents`); a write needs `dryRun: false`
  and the list of confirmed paths, edits only top-level `model:`/`effort:` lines (CRLF kept), and
  copies each file to `<file>.bak-<stamp>` before an atomic rename.
- Terms carry a `profile` chip (`{id, name, model, effort}`), also kept across hot upgrades, shown
  on the session tab and in the activity panel. `profiles.changed` tells clients to refetch.
- The app estimates a relative cost per profile and agent from list prices (2026-10-29), blending
  main (65 %), subagent (30 %) and background (5 %) models with an effort factor, against the CLI
  defaults. It is labelled an estimate. The real cost per session and task (ADR 0040) is what
  to compare profiles with once they have run.

## Rejected
- Writing `~/.claude/settings.json`, `~/.codex/config.toml` or Codex `-p` profile files: global
  side effects outside the session, and Codex 0.165 changed `-p` to separate files.
- Claude `--agents '<json>'` for per-named overrides at launch: it needs the whole definition
  (prompt, tools, hooks, MCP servers) re-encoded from YAML without a parser, whether a same-name
  entry replaces or merges is not verified, and Claude's resume flags are read back from `/proc`,
  so a large JSON would be replayed on every resume and shown in `Term.cmd`.
- Claude `--settings '<json>'` for the env: it would persist through resume, but how its `env`
  block is applied per session is not verified; real process env is.
- Codex per-role models (temporary `config_file`s), Gemini thinking budgets and subagents
  (settings file through `GEMINI_CLI_SYSTEM_SETTINGS_PATH`), Copilot custom agents: they need
  generated files; left out until asked for.

## Consequences
- Fields a CLI does not support are hidden in the editor and rejected by the API; unknown CLIs
  get no section.
- An explicit model or effort on a task wins over its profile, so the Add Task form fills both
  from the chosen profile instead of the old settings defaults.
- A resumed session keeps its launch flags but takes env-only values from the profile in force
  at resume time.
- Built-in model ids (`gpt-5.4-mini`, `gemini-2.5-flash`…) age; they are plain data next to the
  agent table and covered by tests.
