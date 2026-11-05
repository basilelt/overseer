#!/bin/sh
set -eu
. "$(dirname -- "$0")/demo-env.sh"

SID=eeeeeeee-0000-4000-8000-000000000001
echo "isolated tmux server: -L $sock; HOME and state dir under $demo; stub agents only"

say "hooks install: dry run against a fake ~/.claude/settings.json"
printf '{\n  "hooks": {\n    "SessionStart": [{"hooks": [{"type": "command", "command": "~/.claude/hooks/tmux-claude-id.sh"}]}]\n  }\n}\n' > "$HOME/.claude/settings.json"
sb hooks install | head -n 30
say "hooks install --apply (backup, merge, validation), then again (idempotent)"
sb hooks install --apply
sb hooks install --apply
ls "$HOME/.claude"

say "a stub claude starts in the isolated tmux server; its hooks spool events"
start_agent agent "$SID"
ID=$(line_id "$SID")
PANE=$(pane_of "$SID")
show
say "spool content (no prompt text stored)"
cat "$OVERSEER_STATE_DIR"/spool/*.jsonl

say "user submits a prompt: UserPromptSubmit hook -> busy"
t send-keys -t "$PANE" busy Enter; pause
sb ls --json | jq -c '.[0] | {state: .line.state, busy: .local.busy}'
t send-keys -t "$PANE" done Enter; pause
sb ls --json | jq -c '.[0] | {state: .line.state, busy: .local.busy}'

say "link a ticket by hand, then back to the branch's ticket"
sb link "$ID" AS-7
sb ls --json | jq -r '.[0].line.ticketKey'
sb link "$ID" none
sb ls --json | jq -r '.[0].line.ticketKey'

say "resume refuses a running line"
sb resume "$ID" || true

say "the agent exits (Ctrl-D); the line becomes held(lost)"
t send-keys -t "$PANE" C-d; pause
show

say "resume --print, then resume (detached tmux session on the demo server)"
sb resume "$ID" --print
sb resume "$ID"
pause
tail -n 1 "$SB_STUB_LOG"
show

say "daemon: local API on a Unix socket"
"$SB" daemon --interval 1 >/dev/null 2>&1 &
daemon_pid=$!
pause 2
if command -v curl >/dev/null 2>&1; then
  curl -s --unix-socket "$OVERSEER_SOCKET" http://unix/v1/status | jq .
  curl -s --unix-socket "$OVERSEER_SOCKET" "http://unix/v1/lines?ticketKey=AS-4242" | jq -c '.[] | {id: .line.id, state: .line.state}'
fi
