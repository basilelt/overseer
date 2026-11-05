#!/bin/sh
set -eu
R=$(cd "$(dirname "$0")/../.." && pwd)
D=${QA_DIR:-/tmp/sbimport}
OLD_BIN=${OLD_BIN:?build switchboard-ide and set OLD_BIN to its binary}
NEW_BIN=${NEW_BIN:-$R/target/debug/overseer}
H=$D/home
OLD_STATE=$H/.local/state/switchboard
NEW_STATE=$H/.local/state/overseer
SID=11111111-2222-4333-8444-555555555555

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok: $*"; }
alive() { kill -0 "$1" 2>/dev/null; }
api() { curl -sS --unix-socket "$1/daemon.sock" -X "$2" -H 'content-type: application/json' ${4:+-d "$4"} "http://d$3"; }
wait_sock() {
  i=0
  until curl -s --unix-socket "$1/daemon.sock" http://d/v1/version >/dev/null 2>&1; do
    i=$((i+1)); [ $i -lt 100 ] || fail "daemon on $1 did not start"; sleep 0.2
  done
}
run() {
  bin=$1; shift
  env -i PATH="$R/tests/stubs:/usr/bin:/bin" HOME="$H" SHELL=/bin/sh TERM=xterm-256color LANG=C.UTF-8 \
    XDG_RUNTIME_DIR="$D/run" CLAUDE_CONFIG_DIR="$H/.claude" CODEX_HOME="$H/.codex" \
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=qa GIT_AUTHOR_EMAIL=qa@example.com \
    GIT_COMMITTER_NAME=qa GIT_COMMITTER_EMAIL=qa@example.com SWITCHBOARD_LOGIN_ENV=0 OVERSEER_LOGIN_ENV=0 \
    SB_BIN="$bin" SB_STUB_LOG="$D/stub.log" "$bin" "$@"
}

cleanup() {
  for p in "$OLD_STATE/daemon.pid" "$NEW_STATE/daemon.pid"; do
    [ -f "$p" ] && kill "$(cat "$p")" 2>/dev/null || true
  done
  pkill -f "$D/home" 2>/dev/null || true
}
trap cleanup EXIT
cleanup
rm -rf "$D"
mkdir -p "$H/proj" "$H/.claude" "$D/run"
chmod 700 "$D/run"
(cd "$H/proj" && git init -q -b main && echo hi > a.txt \
  && GIT_CONFIG_GLOBAL=/dev/null git -c user.name=qa -c user.email=qa@example.com add -A \
  && GIT_CONFIG_GLOBAL=/dev/null git -c user.name=qa -c user.email=qa@example.com commit -qm init)
echo '{"theme":"dark","hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo mine"}]}]}}' > "$H/.claude/settings.json"

run "$OLD_BIN" hooks install claude --apply >/dev/null
grep -q switchboard "$H/.claude/settings.json" || fail "old switchboard did not install its hooks"
run "$OLD_BIN" daemon --interval 1 >> "$D/old.log" 2>&1 &
wait_sock "$OLD_STATE"
ok "old switchboard daemon up ($(cat "$OLD_STATE/daemon.pid"))"

api "$OLD_STATE" POST /v1/workspaces "{\"path\":\"$H/proj\",\"name\":\"legacy-ws\"}" >/dev/null
api "$OLD_STATE" POST /v1/tasks "{\"title\":\"legacy task\",\"project\":\"$H/proj\"}" >/dev/null
api "$OLD_STATE" POST /v1/terms "{\"cmd\":[\"claude\",\"--session-id\",\"$SID\"],\"cwd\":\"$H/proj\"}" >/dev/null
i=0
until api "$OLD_STATE" GET /v1/lines | jq -e --arg s "$SID" '.. | objects | select(.agentSessionId? == $s)' >/dev/null 2>&1; do
  i=$((i+1)); [ $i -lt 100 ] || fail "old daemon never saw the claude session"; sleep 0.2
done
CLAUDE_PID=$(awk 'NR==1{print $1}' "$D/stub.log")
alive "$CLAUDE_PID" || fail "stub claude not running"
ok "claude session $SID running in switchboard (pid $CLAUDE_PID)"
OLD_PID=$(cat "$OLD_STATE/daemon.pid")

run "$NEW_BIN" daemon --interval 1 >> "$D/new.log" 2>&1 &
wait_sock "$NEW_STATE"
ok "overseer daemon up"

S=$(api "$NEW_STATE" GET /v1/import/switchboard)
echo "$S" | jq -e '.found and .running and .hooks and (.imported|not) and .sessions >= 1 and .tasks >= 1 and .workspaces >= 1' >/dev/null \
  || fail "detection: $S"
ok "detected: $(echo "$S" | jq -c '{dir,sessions,tasks,workspaces,running,hooks}')"

P=$(api "$NEW_STATE" POST /v1/import/switchboard '{}')
echo "$P" | jq -e '.imported and .stopped' >/dev/null || fail "import: $P"
ok "imported: $(echo "$P" | jq -c .)"

i=0
while alive "$OLD_PID" || alive "$CLAUDE_PID"; do
  i=$((i+1)); [ $i -lt 50 ] || fail "old daemon or its claude survived the import"; sleep 0.2
done
ok "old daemon and its claude process stopped"

! grep -q '#switchboard' "$H/.claude/settings.json" || fail "old hooks left in settings.json"
grep -q 'overseer' "$H/.claude/settings.json" || fail "overseer hooks not installed"
grep -q 'echo mine' "$H/.claude/settings.json" || fail "user hook lost"
jq -e '.theme == "dark"' "$H/.claude/settings.json" >/dev/null || fail "user settings lost"
ok "hooks handed over, user hooks kept"

api "$NEW_STATE" GET /v1/tasks | grep -q 'legacy task' || fail "task missing"
api "$NEW_STATE" GET /v1/workspaces | grep -q 'legacy-ws' || fail "workspace missing"
ROWS=$(api "$NEW_STATE" GET /v1/lines | jq -c --arg s "$SID" '[.. | objects | select(.agentSessionId? == $s)]')
[ "$(echo "$ROWS" | jq 'length')" = 1 ] || fail "expected one line for the session: $ROWS"
L=$(echo "$ROWS" | jq -c '.[0]')
echo "$L" | jq -e '.duplicate != true' >/dev/null || fail "session marked duplicate: $L"
echo "$L" | jq -e '(.local.pid // null) == null' >/dev/null || fail "session still has a live pid: $L"
ok "task, workspace and session imported; session not running elsewhere"

LID=$(echo "$L" | jq -r .id)
R2=$(api "$NEW_STATE" POST "/v1/lines/$LID/resume?dryRun=1" '{}')
echo "$R2" | grep -q 'already running' && fail "resume refused: $R2"
ok "resume plan: $R2"

api "$NEW_STATE" GET /v1/import/switchboard | jq -e .imported >/dev/null || fail "imported flag not kept"
ok "popup will not come back"
echo PASS
