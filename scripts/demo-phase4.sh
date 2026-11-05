#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --offline --release --quiet --manifest-path "$root/Cargo.toml"
cargo build --offline --release --quiet --manifest-path "$root/api/Cargo.toml" --target-dir "$root/target"
sb="$root/target/release/overseer"
apibin="$root/target/release/overseer-api"

tmp=$(mktemp -d)
export OVERSEER_TMUX_SOCKET="sb-demo4-$$"
api_pid=""
sync_pid=""
cleanup() {
  for p in $sync_pid $api_pid; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done
  rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

admin=$(od -An -tx1 -N24 /dev/urandom | tr -d ' \n')
OVERSEER_API_ADDR=127.0.0.1:0 OVERSEER_API_DB="$tmp/api.db" OVERSEER_ADMIN_TOKEN="$admin" \
  "$apibin" >"$tmp/api.log" 2>&1 &
api_pid=$!
addr=""
for _ in $(seq 50); do
  addr=$(sed -n 's/^overseer-api listening on //p' "$tmp/api.log")
  [ -n "$addr" ] && break
  sleep 0.1
done
[ -n "$addr" ] || { cat "$tmp/api.log"; exit 1; }
url="http://$addr"
echo "== api on $url (db and state in $tmp)"
curl -fsS "$url/healthz"; echo

as_admin() { curl -fsS -H "Authorization: Bearer $admin" -H 'Content-Type: application/json' "$@"; }
user=$(as_admin -d '{"name":"demo"}' "$url/v1/users" | jq -r .token)
as_user() { curl -fsS -H "Authorization: Bearer $user" -H 'Content-Type: application/json' "$@"; }

station() {
  id=$1 kind=$2 agents=$3
  dir="$tmp/$id"
  mkdir -p "$dir/state" "$dir/claude" "$dir/codex"
  cp -R "$root/tests/fixtures/$agents/." "$dir/$agents/"
  find "$dir/$agents" -type f | while read -r f; do
    sed -e "s#{{WT_LOGIN}}#/home/demo/wt-login#g" -e "s#{{WT_LOWER}}#/home/demo/wt-lower#g" \
      -e "s#{{WT_CHORE}}#/home/demo/wt-chore#g" -e "s#{{MAIN}}#/home/demo/main#g" \
      -e "s#{{CLONE}}#/home/demo/clone#g" "$f" >"$f.tmp" && mv "$f.tmp" "$f"
  done
  printf '{"stationId":"%s","owner":"alice"}\n' "$id" >"$dir/state/config.json"
  as_admin -d "{\"stationId\":\"$id\",\"kind\":\"$kind\"}" "$url/v1/stations" | jq -r .token |
    OVERSEER_STATE_DIR="$dir/state" "$sb" enroll --api "$url"
}
run_station() {
  dir="$tmp/$1"; shift
  OVERSEER_STATE_DIR="$dir/state" CLAUDE_CONFIG_DIR="$dir/claude" CODEX_HOME="$dir/codex" "$sb" "$@"
}

echo "== enrol two fake stations (Claude fixtures on demo-mac, Codex fixtures on demo-vm)"
station demo-mac mac claude
station demo-vm vm codex
echo
echo "== one sync pass per station"
run_station demo-mac sync --once
run_station demo-vm sync --once
echo
echo "== merged read: GET /v1/lines?owner=alice"
as_user "$url/v1/lines?owner=alice&includeHungUp=true" |
  jq -r '.[] | [.line.stationId, .line.agent, .line.state, (.line.ticketKey // "-"), (if .line.branch == "" then "-" else .line.branch end), .line.id] | @tsv' |
  column -t
echo
echo "== stations"
as_user "$url/v1/stations" | jq -c '.[] | {stationId, kind, online, activeCount}'
echo
echo "== relayed hold on a demo-mac line (sync running, acceptRemoteCommands off: refused)"
OVERSEER_STATE_DIR="$tmp/demo-mac/state" CLAUDE_CONFIG_DIR="$tmp/demo-mac/claude" CODEX_HOME="$tmp/demo-mac/codex" \
  "$sb" sync >"$tmp/sync.log" 2>&1 &
sync_pid=$!
line=$(as_user "$url/v1/lines?stationId=demo-mac" | jq -r '.[0].line.id')
for _ in $(seq 50); do
  code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $user" -H 'Content-Type: application/json' \
    -d '{"id":"demo-hold-1","kind":"hold","args":{"dryRun":true}}' "$url/v1/lines/$line/commands")
  [ "$code" = 202 ] && break
  sleep 0.2
done
for _ in $(seq 50); do
  status=$(as_user "$url/v1/lines/$line/commands" | jq -r '.[0].status')
  case $status in succeeded|refused|failed) break ;; esac
  sleep 0.2
done
echo "audit:"
as_user "$url/v1/lines/$line/commands" | jq -c '.[] | {id, kind, requestedBy, status, reason, args}'
echo
echo "== event stream replay (Last-Event-ID: 0)"
curl -sN --max-time 1 -H "Authorization: Bearer $user" -H 'Last-Event-ID: 0' "$url/v1/events" |
  sed -n 's/^event: //p' | sort | uniq -c || true
echo
echo "== stopping (temp dir removed on exit)"
