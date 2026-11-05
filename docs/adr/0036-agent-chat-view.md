# 0036 Agent chat view, desktop-owned PTY size

Status: accepted

## Context
Agents (Claude Code, Codex, Gemini) run as their real CLI in a daemon PTY. A PTY has one size.
When the phone attached to an agent's terminal it resized the PTY to ~46 columns, the CLI
re-wrapped its whole screen, and the desktop view of the same session was stuck at phone width
until someone pressed "Fit" again. Each side kept taking the size from the other. A terminal is
also a poor reading and typing surface on a phone: no reflow, tiny text, no native text input,
menus driven by arrow keys.

The user's decision: keep the terminal (full fidelity, desktop default) and add a chat view
rendered from the agent's own session file.

## Decision
**PTY size belongs to desktops.** A resize from a client that names itself `phone` is ignored
by the daemon for agent terms (`lineId` or `agent` set); shell terms keep the shared-size policy
of ADR 0033. The app on a phone never sends a resize for an agent term: it renders the terminal
at the PTY's size and scrolls horizontally, without the "Fit" action.

**Chat view.** A per-tab view mode (`terminal` | `chat`) on agent tabs; the tab key stays the
line/term key so tab ordering, closing and restore are unchanged. Default: chat on phones for
agents with a parser (Claude, Codex, Gemini), terminal on desktops; a Terminal/Chat toggle
switches. Other agents always use the terminal.

**Daemon feed.** `GET /v1/lines/{id}/chat` parses the transcript incrementally from the last
byte offset (complete lines only, one line at a time so a large transcript is never loaded
whole) into entries keyed by a stable id (`message uuid:block`, tool-call id, Gemini message
id). Each entry has an order (`seq`) and a revision (`rev`) from a daemon-wide counter seeded
with the start time × 1000, bumped when the entry is created or changes (a tool result updates
its tool step). The client's cursor is the newest `rev`: `after=<cursor>` returns new and
changed entries, so the client only upserts by id. A cursor from before a truncation, a Gemini
rewrite, an evicted cache or an earlier daemon is older than the chat's base revision and gets
`reset: true` with a fresh tail. Byte offsets were rejected as the cursor because results
mutate earlier entries, Gemini `.json` files are rewritten whole, and files can shrink.

The chat cache is separate from the activity cache, holds at most 2000 entries per chat, 8
chats, and drops a chat nobody read for 15 minutes; texts are capped (tool output 4 KiB).
`line.chat {lineId, cursor}` is published from the existing transcript watcher only for chats
that have a cache slot, so a daemon whose chats nobody opened does no extra parsing.

**Input.** The composer reuses `POST /v1/broadcast` (`deliver.rs`: bracketed paste + Enter
when idle; queue until idle; or send now). Interrupt is never automatic: "Esc" is an explicit
button that types ESC through `POST /v1/lines/{id}/keys`, which takes named keys only (no
arbitrary bytes) plus an optional `text` pasted after them (bracketed when the app enabled it,
no Enter). Prompt cards read `GET /v1/lines/{id}/screen` (the PTY ring replayed through
a small VT grid, or a tmux capture) when the line needs input; hook payloads carry no prompt
text by design (ADR 0013), and the quick buttons send the keys the CLI expects (digits for
Claude menus, `y`/`n` for Codex, arrows + Enter for cursor menus). Claude Code question forms
(AskUserQuestion) are driven with arrows counted from the `❯` cursor, because a digit typed
while the cursor sits on "Type something." lands in that text field: answers paste into the
field then press Enter, "Chat about this" is the row below the separator, and multi-select
rows toggle with Space then Submit. Custom slash commands come
from `GET /v1/lines/{id}/commands`; built-ins are a static list in the app.

**Hold and close for daemon terms.** `inTmux` also passes when the agent runs in a live daemon
term; `inputEmpty` reads the grid-rendered screen. Hold stops the agent and restarts the held
stub in the same term (same id, same attached clients), and Enter in it, or resume, restarts
the agent there. Close stops the agent and kills the term.

## Consequences
- Phone and desktop no longer fight over the PTY size; the phone reads and writes through the
  chat view, which reflows to any width.
- The chat view is only as complete as the agent's session file: interactive UI that never
  reaches the transcript (menus, permission prompts) is covered by the prompt card and the
  terminal fallback.
- Markdown is rendered by a small escaping renderer (no HTML passthrough); code highlighting
  reuses the lazily loaded review highlighter.
- No polling: the chat fetches only on `line.chat` while it is visible, and on becoming visible.
