#!/bin/sh
set -eu
. "$(dirname -- "$0")/demo-env.sh"

SID=ffffffff-0000-4000-8000-000000000001
echo "isolated tmux server: -L $sock; HOME and state dir under $demo; stub agents only"

start_agent agent "$SID"
ID=$(line_id "$SID")
PANE=$(pane_of "$SID")
show

refusal() {
  say "$1"
  sb hold "$ID" --yes || echo "(exit $? = refused)"
}

t send-keys -t "$PANE" -l "half typed"; pause
refusal "typed input in the prompt"
t send-keys -t "$PANE" C-u
t send-keys -t "$PANE" busy Enter; pause
refusal "mid-turn"
t send-keys -t "$PANE" done Enter
t send-keys -t "$PANE" tool Enter; pause
refusal "a Bash tool subprocess is running"
t send-keys -t "$PANE" untool Enter; pause
sb pin "$ID"
refusal "pinned line"
sb pin "$ID" --off
t send-keys -t "$PANE" ask Enter; pause
refusal "agent waits for a permission"
t send-keys -t "$PANE" done Enter; pause

say "hold, dry run (default)"
sb hold "$ID"
say "hold --yes"
sb hold "$ID" --yes
pause
t capture-pane -p -t "$PANE" | grep -v '^$' | tail -n 4
echo "@sb_held=$(t show-options -p -v -t "$PANE" @sb_held)"
show

say "simulated reboot: the tmux server dies; the line stays held(user)"
t kill-server
t new-session -d -s main -x 160 -y 40 sh
show
say "a patched tmux-restore would start the stub again"
P2=$(t new-window -P -F '#{pane_id}' -t main "$SB stub $ID")
t set-option -p -t "$P2" @sb_held "$ID"
pause
t capture-pane -p -t "$P2" | grep -v '^$'

say "resume from elsewhere: relaunches in the stub's pane with the original flags"
sb resume "$ID"
pause
tail -n 1 "$SB_STUB_LOG"
show

say "a second process on the same session: duplicate; orphans are flagged (no client attached)"
start_agent dup "$SID"
sb status
sb hold "$ID" || true

say "close a line: dry run, then --yes after its duplicate is gone"
t kill-window -t main:dup
pause
sb close "$ID" || true
sb close "$ID" --yes || true
show

say "doctor"
sb doctor

say "actions.log"
jq -c '{action, dryRun, outcome: .result.action}' "$OVERSEER_STATE_DIR/actions.log"
