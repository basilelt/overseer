# 0040 Real cost per session and task

Status: accepted

## Context
The profile editor estimates a relative cost from list prices (ADR 0037). Whether a profile
(model, effort, subagent model) saves money for the same result can only be read from what the
sessions really spent. The usage index (`usage.rs`, contract §7c) already reads token counts from
the transcripts, but per 15-minute bucket for the last 31 days, without the model split of a
session, its effort, model switches or anything that outlives the transcript.

## Decision
- **Tally per session in the activity parser.** `activity::State` feeds each transcript line to
  `usage::Meter`, the same per-agent readers the usage index uses (Claude `usage` once per message
  id, Codex `token_count` deltas per `turn_context` model, Gemini message `tokens`), and keeps
  tokens per model split into uncached input, cache write (5 m and 1 h apart), cache read and
  output. It is incremental like the rest of the parser: only appended bytes are read.
- **Subagents count at their own model's price**: `activity::spend` adds the transcripts under
  `<session>/subagents/`, three levels deep so workflow agents (`subagents/workflows/<id>/`) count. Model, effort and switches come from the main
  transcript only.
- **Model switches are events**: the first main-transcript message on a new model is kept (at
  most 50), priced on its cache write and uncached input: the cache it had to build again.
- **Effort** comes from the transcript when it records one (Codex `turn_context.effort`), else
  from the launch flags (`--effort`, `model_reasoning_effort=`).
- **One price table, in the daemon** (`usage::PRICES`, dated `PRICES_AS_OF`). Models match by the
  longest id that equals the model or is followed by `-` (so `gpt-5.5` is not priced as `gpt-5`);
  an unknown model keeps its tokens with a `null` cost and marks the total
  `partial`. Agents whose logs have no token counts get no cost, never an estimate.
- **Cost is snapshot on the task** (`tasks.cost`, per `agent:session`), from `tasks::tick` when
  the line's activity moved since the last check (kept in memory): at most once a minute while
  busy, then after the turn, and at most 8 sessions per scan so the first scan after an upgrade
  does not parse every transcript at once. The row is written, and `task.updated` sent, only when
  a number changed; an idle daemon does nothing. It outlives the transcript and adds up over a
  task's sessions. An effort the session no longer reports (the process has exited) keeps its
  last value, else the task's.
- **The app groups, the daemon does not**: tasks carry their cost, so the usage view's
  **Cost per task** groups them by profile and agent, ticket or task without a new endpoint.
  There is no task type yet; profile and agent stand in for it.

## Rejected
- Querying the usage index per session: it keeps 31 days, has no per-session model split or
  switches, and indexes only while the usage view is open.
- Pricing in the app: the cache multipliers would live in two places.
- A daemon `/v1/costs` grouping endpoint: the board already sends every task.

## Consequences
- Every activity request also walks the session's subagent directory; the tallies are cached per
  file like the rest of the activity state.
- List prices age; the table is data in `usage.rs`, dated, and covered by tests.
- Subscription users see what the work would cost on the API, said as such in the hint.
