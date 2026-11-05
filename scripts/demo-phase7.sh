#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --offline --release --quiet --manifest-path "$here/api/Cargo.toml" --target-dir "$here/target"
. "$here/scripts/demo-env.sh"
export SB_SSH_LOG="$demo/ssh.log"
echo "isolated tmux server: -L $sock; HOME, state, clone and api db under $demo; stub claude, fake ssh and gh"

api_pid=""
sync_pid=""
stop() {
  for p in $sync_pid $api_pid; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done
  cleanup
}
trap stop EXIT INT TERM

admin=$(od -An -tx1 -N24 /dev/urandom | tr -d ' \n')
OVERSEER_API_ADDR=127.0.0.1:0 OVERSEER_API_DB="$demo/api.db" OVERSEER_ADMIN_TOKEN="$admin" \
  "$root/target/release/overseer-api" >"$demo/api.log" 2>&1 &
api_pid=$!
addr=""
for _ in $(seq 50); do
  addr=$(sed -n 's/^overseer-api listening on //p' "$demo/api.log")
  [ -n "$addr" ] && break
  sleep 0.1
done
[ -n "$addr" ] || { cat "$demo/api.log"; exit 1; }
url="http://$addr"
as_admin() { curl -fsS -H "Authorization: Bearer $admin" -H 'Content-Type: application/json' "$@"; }
hub=$(as_admin -d '{"name":"feedback-hub"}' "$url/v1/users" | jq -r .token)
as_hub() { curl -fsS -H "Authorization: Bearer $hub" -H 'Content-Type: application/json' "$@"; }
outcome() {
  for _ in $(seq 150); do
    s=$(as_hub "$url/v1/lines/$1/commands" | jq -r --arg c "$2" '.[] | select(.id == $c) | .status')
    case $s in succeeded|refused|failed) break ;; esac
    sleep 0.2
  done
  as_hub "$url/v1/lines/$1/commands" | jq -c --arg c "$2" '.[] | select(.id == $c) | {id, kind, status, reason}'
}

git -C "$demo/repo" config user.name Demo
echo hello > "$demo/repo/README"
git -C "$demo/repo" add -A
git -C "$demo/repo" -c commit.gpgsign=false commit -q -m init

say "this station plays runner-1: acceptRemoteCommands on, its clone of github.com/acme/demo is $demo/repo"
mkdir -p "$OVERSEER_STATE_DIR"
jq -n --arg r "$demo/repo" --arg w "$demo/worktrees" '{stationId: "runner-1", owner: "runner@example.com",
  acceptRemoteCommands: true, repos: {"github.com/acme/demo": $r}, worktreeDir: $w}' \
  >"$OVERSEER_STATE_DIR/config.json"
as_admin -d '{"stationId":"runner-1","kind":"runner"}' "$url/v1/stations" | jq -r .token | sb enroll --api "$url"
"$SB" sync >"$demo/sync.log" 2>&1 &
sync_pid=$!
for _ in $(seq 50); do
  [ "$(as_hub "$url/v1/stations" | jq -r '.[0].online')" = true ] && break
  sleep 0.2
done

body() {
  jq -n --arg k "$1" --argjson d "$2" '{owner: "feedback-bot", repo: "github.com/acme/demo",
    branch: "feedback/AS-4242", agent: "claude", origin: "feedback", ticketKey: "AS-4242",
    prompt: "The save button does nothing", idempotencyKey: $k, dryRun: $d}'
}
say "feedback-hub: POST /v1/lines as a dry run (the default)"
ID=$(body fb-dry true | as_hub -d @- "$url/v1/lines" | jq -r .id)
outcome "$ID" fb-dry
say "feedback-hub: POST /v1/lines with dryRun false"
ID=$(body fb-1 false | as_hub -d @- "$url/v1/lines" | jq -r .id)
outcome "$ID" fb-1
pause 2
as_hub "$url/v1/lines/$ID" | jq -c '.line | {id, owner, origin, ticketKey, branch, worktreePath, agentSessionId}'
say "the runner's tmux session: the prompt was pasted from a 0600 temp file"
t capture-pane -p -t "sb-$(printf %s "$ID" | tail -c 6)" | grep -v '^$' | tail -3

say "relaunch the same line with the reporter's answer"
as_hub -o /dev/null -d '{"id":"rl-1","kind":"relaunch","args":{"message":"Still broken on Safari","dryRun":false}}' \
  "$url/v1/lines/$ID/commands"
outcome "$ID" rl-1

say "prompt and message stored anywhere on disk outside the agent's files?"
grep -rl -e 'save button' -e 'Still broken' "$OVERSEER_STATE_DIR" "$demo/api.db" "$demo/stub.log" 2>/dev/null || echo "no"

say "overseer patch AS-4242 --print (attach is the default)"
sb patch AS-4242 --print
say "memory policy at 0 % (report only) and doctor"
jq '. + {memoryPolicy: {enabled: true, thresholdPercent: 0}}' "$OVERSEER_STATE_DIR/config.json" >"$demo/c.json"
mv "$demo/c.json" "$OVERSEER_STATE_DIR/config.json"
sb status | tail -1
sb doctor | cut -c1-150
