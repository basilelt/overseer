#!/bin/sh
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --offline --release --quiet --manifest-path "$here/api/Cargo.toml" --target-dir "$here/target"
. "$here/scripts/demo-env.sh"
export SB_SSH_LOG="$demo/ssh.log"
SID=ffffffff-0000-4000-8000-000000000005
echo "isolated tmux server: -L $sock; HOME, state and api db under $demo; stub agent and fake ssh only"

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
user=$(as_admin -d '{"name":"demo"}' "$url/v1/users" | jq -r .token)
as_user() { curl -fsS -H "Authorization: Bearer $user" -H 'Content-Type: application/json' "$@"; }
relay() {
  as_user -o /dev/null -d "{\"id\":\"$2\",\"kind\":\"$3\",\"args\":$4}" "$url/v1/lines/$1/commands"
  for _ in $(seq 50); do
    s=$(as_user "$url/v1/lines/$1/commands" | jq -r --arg c "$2" '.[] | select(.id == $c) | .status')
    case $s in succeeded|refused|failed) break ;; esac
    sleep 0.2
  done
  as_user "$url/v1/lines/$1/commands" | jq -c --arg c "$2" '.[] | select(.id == $c) | {id, kind, status, reason, requestedBy}'
}

say "api on $url; this station is demo-st1 (acceptRemoteCommands on), a second one is demo-vm2"
mkdir -p "$OVERSEER_STATE_DIR"
printf '{"stationId":"demo-st1","owner":"demo@example.com","acceptRemoteCommands":true,"ssh":{"demo-vm2":"dev@vm2.example"}}\n' \
  >"$OVERSEER_STATE_DIR/config.json"
as_admin -d '{"stationId":"demo-st1","kind":"vm"}' "$url/v1/stations" | jq -r .token | sb enroll --api "$url"
start_agent agent "$SID"
ID=$(line_id "$SID")
PANE=$(pane_of "$SID")
"$SB" sync >"$demo/sync.log" 2>&1 &
sync_pid=$!
for _ in $(seq 50); do
  [ "$(as_user "$url/v1/lines/$ID" 2>/dev/null | jq -r .stationOnline)" = true ] && break
  sleep 0.2
done

say "relayed hold while text is typed in the prompt: refused by the inputEmpty predicate"
t send-keys -t "$PANE" -l "half typed"; pause
relay "$ID" demo-hold-1 hold '{"dryRun":false}'
t send-keys -t "$PANE" C-u
say "relayed hold, dry run by default"
relay "$ID" demo-hold-2 hold '{}'
say "relayed hold with dryRun false: executed, audited"
relay "$ID" demo-hold-3 hold '{"dryRun":false}'
pause
echo "@sb_held=$(t show-options -p -v -t "$PANE" @sb_held)"
grep remote-command "$OVERSEER_STATE_DIR/actions.log" | jq -c '{commandId, status, reason, requestedBy}'

say "acceptRemoteCommands off: refused (disabled)"
jq '.acceptRemoteCommands = false' "$OVERSEER_STATE_DIR/config.json" >"$demo/c.json"
mv "$demo/c.json" "$OVERSEER_STATE_DIR/config.json"
relay "$ID" demo-resume-1 resume '{"dryRun":false}'

say "a second station (Claude fixtures) syncs once"
vm="$demo/vm2"
mkdir -p "$vm/state" "$vm/codex"
cp -R "$root/tests/fixtures/claude" "$vm/claude"
find "$vm/claude" -type f | while read -r f; do
  sed -e "s#{{WT_LOGIN}}#/home/demo/wt-login#g" -e "s#{{WT_LOWER}}#/home/demo/wt-lower#g" \
    -e "s#{{WT_CHORE}}#/home/demo/wt-chore#g" -e "s#{{MAIN}}#/home/demo/main#g" \
    -e "s#{{CLONE}}#/home/demo/clone#g" "$f" >"$f.tmp" && mv "$f.tmp" "$f"
done
printf '{"stationId":"demo-vm2","owner":"demo@example.com"}\n' >"$vm/state/config.json"
as_admin -d '{"stationId":"demo-vm2","kind":"vm"}' "$url/v1/stations" | jq -r .token |
  OVERSEER_STATE_DIR="$vm/state" "$SB" enroll --api "$url"
OVERSEER_STATE_DIR="$vm/state" CLAUDE_CONFIG_DIR="$vm/claude" CODEX_HOME="$vm/codex" \
  "$SB" sync --once

say "login, then ls across stations"
printf '%s\n' "$user" | sb login --api "$url"
sb ls --all | cut -c1-170
REMOTE=$(sb ls --json --all | jq -r '[.[] | select(.line.stationId == "demo-vm2")][0].line.id')

say "resume a demo-vm2 line from here (fake ssh records the call)"
sb resume "$REMOTE" --print
sb resume "$REMOTE"
say "deep link overseer://resume/$REMOTE"
sb open "overseer://resume/$REMOTE" --yes
echo "fake ssh was called with:"
sed 's/^/  ssh /' "$SB_SSH_LOG"
say "an unknown URL is refused"
sb open "overseer://hold/$REMOTE" --yes || echo "(refused)"
say "done; everything above ran on a temp dir, an isolated tmux server and a fake ssh"
