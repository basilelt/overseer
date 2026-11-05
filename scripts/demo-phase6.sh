#!/bin/sh
set -eu
. "$(dirname -- "$0")/demo-env.sh"

SID=abababab-0000-4000-8000-000000000001
echo "isolated tmux server: -L $sock; HOME and state dir under $demo; stub agents, fake gh and Yard"

git -C "$demo/repo" config user.name Demo
echo hello > "$demo/repo/README"
git -C "$demo/repo" add -A
git -C "$demo/repo" -c commit.gpgsign=false commit -q -m init
start_agent agent "$SID"

say "two other stations: vm-b online, mac-c offline for an hour (greyed on the Board)"
mkdir -p "$OVERSEER_STATE_DIR/stations"
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
old=$(date -u -d '-1 hour' +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -v-1H +%Y-%m-%dT%H:%M:%SZ)
station() {
  jq -n --arg id "$1" --arg seen "$2" --arg at "$now" --arg st "$3" '{stationId: $id, lastSeenAt: $seen,
    lines: [range(0; 4) as $i | {line: {id: "\($id)-\($i)", owner: "demo@example.com", stationId: $id,
      agent: "claude", agentSessionId: "s-\($id)-\($i)", ticketKey: "AS-\(4242 + $i % 2)",
      repo: "github.com/Acme/demo", branch: "feature/AS-\(4242 + $i % 2)-demo",
      worktreePath: "/w/\($id)/\($i)", origin: "dev", state: $st, lastActivityAt: $at, envRef: "pr-101"}}]}' \
    > "$OVERSEER_STATE_DIR/stations/$1.json"
}
station vm-b "$now" idle
station mac-c "$old" held

say "fake gh: a draft PR with failing CI on the local branch"
export SB_GH_DIR="$demo/gh"
mkdir -p "$SB_GH_DIR"
echo '[{"number":7,"url":"https://github.com/Acme/demo/pull/7","state":"OPEN","isDraft":true,"title":"demo","statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"}]}]' \
  > "$SB_GH_DIR/github_com_Acme_demo_feature_AS_4242_demo.json"

if command -v python3 >/dev/null; then
  yport=$((20000 + $$ % 20000))
  python3 -m http.server --bind 127.0.0.1 "$yport" -d "$root/tests/fakes/yard" >/dev/null 2>&1 &
  yard_pid=$!
  trap 'kill "$yard_pid" 2>/dev/null; cleanup' EXIT INT TERM
  jq -n --arg u "http://127.0.0.1:$yport" '{yardUrl: $u}' > "$OVERSEER_STATE_DIR/config.json"
  say "fake Yard on http://127.0.0.1:$yport (env links)"
fi

say "daemon with the Board on a loopback port"
"$SB" daemon --interval 2 --http 127.0.0.1:0 2>"$demo/daemon.log" &
daemon_pid=$!
until [ -f "$OVERSEER_STATE_DIR/board.json" ]; do pause 0.2; done
sb board
say "board summary over the socket"
curl -s --unix-socket "$OVERSEER_SOCKET" http://unix/v1/board |
  jq -c '.lines[] | {station: .line.stationId, ticket: .line.ticketKey, state: .line.state, actionable, reason}'

wait_for=${DEMO_WAIT:-}
if [ -z "$wait_for" ] && [ -t 0 ]; then
  say "open the URL above; press Enter to stop and clean up"
  read -r _
else
  pause "${wait_for:-1}"
fi
