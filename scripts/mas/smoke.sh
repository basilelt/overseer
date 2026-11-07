#!/bin/bash
set -u
cmd=${1:-}
R=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${MAS_OUT:-$R/mas-out}
RES=$OUT/results.tsv
APP=${MAS_APP:-$R/app/src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/Overseer.app}
ENT=$R/app/src-tauri/entitlements.mas.plist
ENT_HELPER=$R/app/src-tauri/entitlements.mas-helper.plist
ENT_EXC=$R/scripts/mas/entitlements.exceptions.plist
GITBIN=$(xcrun -f git 2>/dev/null || echo /usr/bin/git)
PROBE=$OUT/Probe.app
PROBE_ID=io.github.basilelt.overseer.probe
APP_ID=io.github.basilelt.overseer.mac
CP=$HOME/Library/Containers/$PROBE_ID/Data
CA=$HOME/Library/Containers/$APP_ID/Data
GRANT=/tmp/sbx-grant
mkdir -p "$OUT"

P=${P:-}

rec() {
  printf '%s\t%s\t%s\n' "$P$1" "$2" "$3" | tee -a "$RES"
}

take() {
  grep '^RESULT' | cut -f2- | sed "s/^/$P/" | tee -a "$RES"
}

sign_app() {
  [ -d "$APP" ] || { echo "no app at $APP" >&2; exit 1; }
  find "$APP/Contents/Resources" -type f -name overseer | while IFS= read -r h; do
    codesign --force -s - --entitlements "$ENT_HELPER" "$h"
  done
  codesign --force -s - --entitlements "$ENT" "$APP"
  codesign --verify --strict --verbose=2 "$APP"
  codesign -d --entitlements - "$APP" 2>&1 | tee "$OUT/app-entitlements.txt"
  find "$APP/Contents/Resources" -type f -name overseer | while IFS= read -r h; do
    echo "== $h"
    codesign -d --entitlements - "$h" 2>&1
  done | tee "$OUT/helper-entitlements.txt"
}

build_probe() {
  local pent=${1:-$ENT}
  rm -rf "$PROBE"
  mkdir -p "$PROBE/Contents/MacOS" "$PROBE/Contents/Helpers"
  clang -O1 -Wall -o "$PROBE/Contents/MacOS/probe" "$R/scripts/mas/probe.c"
  clang -O1 -o "$OUT/grant" "$R/scripts/mas/probe.c"
  cp "$PROBE/Contents/MacOS/probe" "$PROBE/Contents/Helpers/probe-helper"
  cat >"$PROBE/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>probe</string>
<key>CFBundleIdentifier</key><string>$PROBE_ID</string>
<key>CFBundleName</key><string>probe</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
</dict></plist>
EOF
  codesign --force -s - --entitlements "$ENT_HELPER" "$PROBE/Contents/Helpers/probe-helper"
  local server
  server=$(find "$APP/Contents/Resources" -type f -name overseer | head -1)
  if [ -n "$server" ]; then
    cp "$server" "$PROBE/Contents/Helpers/overseer"
    codesign --force -s - --entitlements "$ENT_HELPER" "$PROBE/Contents/Helpers/overseer"
  fi
  codesign --force -s - --entitlements "$pent" "$PROBE"
  codesign --verify --strict --verbose=2 "$PROBE"
}

prep_fixtures() {
  rm -rf "$GRANT" "$HOME/sbx-grant"
  mkdir -p "$GRANT" "$HOME/sbx-grant" "$HOME/.local/bin"
  GRANT=$(cd "$GRANT" && pwd -P)
  (
    cd "$GRANT" &&
      git init -q -b main &&
      echo seed >seed.txt &&
      git -c user.name=qa -c user.email=qa@example.com add -A &&
      git -c user.name=qa -c user.email=qa@example.com commit -qm init
  )
  echo seed >"$HOME/sbx-grant/seed.txt"
  printf '#!/bin/sh\necho dummy-ok\n' >"$HOME/.local/bin/ovs-dummy"
  chmod +x "$HOME/.local/bin/ovs-dummy"
  printf '#!/bin/sh\necho dummy-ok\n' | sudo tee /usr/local/bin/ovs-dummy >/dev/null
  sudo chmod +x /usr/local/bin/ovs-dummy
  command -v tmux >/dev/null || brew install tmux >/dev/null 2>&1 || true
  ls -la /opt/homebrew/bin | head -40 >"$OUT/homebrew-bin.txt" 2>&1 || true
}

run_probes() {
  local pent=${1:-$ENT}
  build_probe "$pent"
  local p="$PROBE/Contents/MacOS/probe"
  export PROBE_GIT=$GITBIN
  "$p" execs "$HOME" "$GITBIN" "$CP" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  [ -d "$CP" ] && rec probe_container_created PASS "$CP" || rec probe_container_created FAIL "no $CP"
  printf '#!/bin/sh\necho from-stub > "$1/stub.txt"\n' >"$CP/stub-write.sh"
  "$OUT/grant" issue "$GRANT" >"$CP/tok.txt" 2>"$OUT/issue.err"
  if [ -s "$CP/tok.txt" ]; then
    rec ext_issue_token_unsandboxed PASS "token $(wc -c <"$CP/tok.txt") bytes"
  else
    rec ext_issue_token_unsandboxed FAIL "$(cat "$OUT/issue.err")"
  fi
  "$p" suite "$GRANT" "$CP/stub-write.sh" "$CP/tok.txt" "$PROBE/Contents/Helpers/probe-helper" "$HOME" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" peer "$GRANT" "$CP/tok.txt" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$OUT/grant" issue "$GRANT" >"$CP/tok.txt" 2>/dev/null
  "$p" orphan "$GRANT" "$CP/tok.txt" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" pty 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" ptyioctl 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" libproc 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" misc "$CP" "$HOME" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  ls -la "$GRANT" >"$OUT/grant-dir-after${P%:}.txt" 2>&1
  find "$GRANT" -maxdepth 1 -name "*.txt" ! -name seed.txt -delete
}

probes() {
  : >"$OUT/probe-all.txt"
  prep_fixtures
  P=
  run_probes "$ENT"
  P=exc:
  PROBE=$OUT/ProbeExc.app
  PROBE_ID=io.github.basilelt.overseer.probex
  CP=$HOME/Library/Containers/$PROBE_ID/Data
  run_probes "$ENT_EXC"
  P=
}

realpath_of() {
  python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$1"
}

CLS_RW=com.apple.app-sandbox.read-write
CLS_RO=com.apple.app-sandbox.read

install_agents() {
  mkdir -p "$OUT/agents"
  curl -fsSL https://claude.ai/install.sh -o "$OUT/agents/claude-install.sh" 2>"$OUT/agents/curl.err" && bash "$OUT/agents/claude-install.sh" >"$OUT/agents/claude-install.log" 2>&1
  if [ -x "$HOME/.local/bin/claude" ]; then
    rec install_claude_native PASS "$HOME/.local/bin/claude -> $(realpath_of "$HOME/.local/bin/claude")"
  else
    rec install_claude_native FAIL "$(tail -c 300 "$OUT/agents/claude-install.log" 2>&1)"
  fi
  npm i -g --prefix "$HOME/.npm-global" @anthropic-ai/claude-code @openai/codex >"$OUT/agents/npm.log" 2>&1
  local t
  for t in claude codex; do
    if [ -e "$HOME/.npm-global/bin/$t" ]; then
      rec "install_${t}_npm" PASS "$HOME/.npm-global/bin/$t -> $(realpath_of "$HOME/.npm-global/bin/$t")"
    else
      rec "install_${t}_npm" FAIL "$(tail -c 300 "$OUT/agents/npm.log")"
    fi
  done
}

try() {
  local name=$1 cls=$2 exe=$3
  shift 3
  : >"$CP/toks"
  local g
  for g in "$@"; do
    "$OUT/grant" issue "$g" "$cls" >>"$CP/toks" 2>/dev/null
  done
  PATH=$NODEDIR:$PATH "$PROBE/Contents/MacOS/probe" grantexec "$name" "$CP/toks" - - 30 "$exe" --version 2>&1 | take >/dev/null
}

exec_matrix() {
  local label=$1 exe=$2
  [ -e "$exe" ] || { rec "exec_${label}" FAIL "not installed"; return; }
  local real bindir realdir
  real=$(realpath_of "$exe")
  bindir=$(dirname "$exe")
  realdir=$(dirname "$real")
  try "exec_${label}_nogrant" "$CLS_RW" "$exe"
  local c cls
  for c in rw ro; do
    cls=$CLS_RW
    [ $c = ro ] && cls=$CLS_RO
    try "exec_${label}_folder_of_link_${c}" "$cls" "$exe" "$bindir"
    try "exec_${label}_folder_of_target_${c}" "$cls" "$exe" "$realdir"
    try "exec_${label}_file_link_${c}" "$cls" "$exe" "$exe"
    try "exec_${label}_file_target_${c}" "$cls" "$exe" "$real"
    try "exec_${label}_all_dirs_and_node_${c}" "$cls" "$exe" "$bindir" "$realdir" "$NODEDIR"
    try "exec_${label}_target_file_and_node_${c}" "$cls" "$exe" "$real" "$exe" "$NODEDIR/node"
  done
  local nm
  nm=$(basename "$exe")
  cp "$real" "$CP/copy-runner-$nm" 2>/dev/null && chmod +x "$CP/copy-runner-$nm"
  : >"$CP/toks"
  PATH=$NODEDIR:$PATH "$PROBE/Contents/MacOS/probe" grantexec "exec_${label}_copy_into_container_by_runner" - - - 30 "$CP/copy-runner-$nm" --version 2>&1 | take >/dev/null
  "$OUT/grant" issue "$realdir" "$CLS_RO" >"$CP/toks" 2>/dev/null
  "$PROBE/Contents/MacOS/probe" copyexec "exec_${label}_copy_into_container_by_sandbox" "$CP/toks" "$real" "$CP/copy-sandbox-$nm" 2>&1 | take >/dev/null
}

pipes() {
  local label=$1 exe=$2 kind=$3
  [ -e "$exe" ] || return 0
  local real bindir realdir
  real=$(realpath_of "$exe")
  bindir=$(dirname "$exe")
  realdir=$(dirname "$real")
  : >"$CP/toks"
  local g
  for g in "$bindir" "$realdir" "$NODEDIR" "$GRANT"; do
    "$OUT/grant" issue "$g" "$CLS_RW" >>"$CP/toks" 2>/dev/null
  done
  touch "$OUT/marker"
  sleep 1
  local p="$PROBE/Contents/MacOS/probe"
  export PATH=$NODEDIR:$PATH
  if [ "$kind" = claude ]; then
    ANTHROPIC_API_KEY=sk-ant-dummy "$p" grantexec "pipe_${label}_print_stream_json" "$CP/toks" "$GRANT" - 90 "$exe" -p "say hi" --output-format stream-json --verbose 2>&1 | take >/dev/null
    printf '%s\n%s\n' '{"type":"user","message":{"role":"user","content":"hi"}}' '{"type":"user","message":{"role":"user","content":"and again"}}' >"$CP/two-turn.jsonl"
    ANTHROPIC_API_KEY=sk-ant-dummy "$p" grantexec "pipe_${label}_two_turn_stream_json" "$CP/toks" "$GRANT" "$CP/two-turn.jsonl" 90 "$exe" -p --input-format stream-json --output-format stream-json --verbose 2>&1 | take >/dev/null
    rm -rf "$HOME/claude-real" "$HOME/claude-real2"
    mkdir -p "$HOME/claude-real" "$HOME/claude-real2"
    echo '{"model":"sonnet"}' >"$HOME/claude-real/settings.json"
    echo '{"model":"sonnet"}' >"$HOME/claude-real2/settings.json"
    "$OUT/grant" issue "$HOME/claude-real" "$CLS_RW" >>"$CP/toks" 2>/dev/null
    CLAUDE_CONFIG_DIR=$HOME/claude-real ANTHROPIC_API_KEY=sk-ant-dummy "$p" grantexec "pipe_${label}_config_dir_granted_real" "$CP/toks" "$GRANT" - 90 "$exe" -p "say hi" --output-format stream-json --verbose 2>&1 | take >/dev/null
    rec "info_${label}_real_config_dir_after" INFO "$(cd "$HOME/claude-real" && find . -maxdepth 3 | head -30 | tr '\n' ' ')"
    CLAUDE_CONFIG_DIR=$HOME/claude-real2 ANTHROPIC_API_KEY=sk-ant-dummy "$p" grantexec "pipe_${label}_config_dir_not_granted" "$CP/toks" "$GRANT" - 90 "$exe" -p "say hi" --output-format stream-json --verbose 2>&1 | take >/dev/null
    rec "info_${label}_real_config_dir2_after" INFO "$(cd "$HOME/claude-real2" && find . -maxdepth 3 | head -30 | tr '\n' ' ')"
  else
    OPENAI_API_KEY=dummy "$p" grantexec "pipe_${label}_exec_json" "$CP/toks" "$GRANT" - 90 "$exe" exec --json hi 2>&1 | take >/dev/null
  fi
  rec "info_${label}_files_touched_in_container" INFO "$(find "$CP" -newer "$OUT/marker" -maxdepth 4 2>/dev/null | sed "s|$CP||" | head -40 | tr '\n' ' ')"
}

agents() {
  P=
  GRANT=$(cd "$GRANT" && pwd -P)
  build_probe "$ENT"
  NODEDIR=$(dirname "$(realpath_of "$(command -v node)")")
  rec info_node_dir INFO "$NODEDIR"
  "$PROBE/Contents/MacOS/probe" execs "$HOME" "$GITBIN" "$CP" >/dev/null 2>&1
  exec_matrix claude_native "$HOME/.local/bin/claude"
  exec_matrix claude_npm "$HOME/.npm-global/bin/claude"
  exec_matrix codex_npm "$HOME/.npm-global/bin/codex"
  pipes claude_native "$HOME/.local/bin/claude" claude
  pipes claude_npm "$HOME/.npm-global/bin/claude" claude
  pipes codex_npm "$HOME/.npm-global/bin/codex" codex
}

wait_sock() {
  local i=0
  while [ $i -lt "$2" ]; do
    [ -S "$1" ] && return 0
    sleep 1
    i=$((i + 1))
  done
  return 1
}

stop_app() {
  pkill -f "Overseer.app/Contents/MacOS" 2>/dev/null
  pkill -f "Contents/Resources/server" 2>/dev/null
  sleep 2
  pkill -9 -f "Contents/Resources/server" 2>/dev/null
  return 0
}

app_env() {
  local extra=
  [ "$P" = exc: ] && extra=/opt/homebrew/bin:
  printf 'PATH=%s:%s/usr/bin:/bin:/usr/sbin:/sbin' "$(dirname "$GITBIN")" "$extra"
}

drive() {
  local sock=$1
  local base=$CA
  mkdir -p "$base/stubs"
  cp "$R/tests/stubs/agent" "$base/stubs/agent"
  chmod +x "$base/stubs/agent"
  rm -rf "$base/proj" "$base"/wt-term
  mkdir -p "$base/proj"
  (
    cd "$base/proj" &&
      git init -q -b main &&
      echo hi >a.txt &&
      git -c user.name=qa -c user.email=qa@example.com add -A &&
      git -c user.name=qa -c user.email=qa@example.com commit -qm init
  )
  PREFIX=$P python3 "$R/scripts/mas/drive.py" "$sock" "$base" "$RES" "$GRANT" 2>&1 | tee -a "$OUT/drive${P%:}.log" >/dev/null
  find "$CA" -maxdepth 6 2>/dev/null | head -400 >"$OUT/container-tree${P%:}.txt"
}

app() {
  GRANT=$(cd "$GRANT" 2>/dev/null && pwd -P || echo "$GRANT")
  local sdir="$CA/Library/Application Support/overseer"
  local dflt="$sdir/daemon.sock"
  P=
  rec app_default_socket_path_length INFO "${#dflt} chars (sun_path limit is 104): $dflt"
  open -n --env "$(app_env)" "$APP" --stdout "$OUT/app.out" --stderr "$OUT/app.err"
  local i=0
  while [ $i -lt 40 ] && [ ! -d "$CA/Library" ]; do sleep 1; i=$((i + 1)); done
  [ -d "$CA" ] && rec app_container_created PASS "$CA" || rec app_container_created FAIL "no container: app not sandboxed or not launched"
  pgrep -fl "Overseer.app" >"$OUT/app-procs.txt" 2>&1
  rec app_process_running "$([ -s "$OUT/app-procs.txt" ] && echo PASS || echo FAIL)" "$(head -c 200 "$OUT/app-procs.txt")"
  if wait_sock "$dflt" 60; then
    rec app_launches_daemon_default_socket PASS "daemon socket up at default path"
  else
    rec app_launches_daemon_default_socket FAIL "no socket at default path after 60s; daemon.log: $(tail -c 400 "$sdir/daemon.log" 2>&1 | tr '\n' ' ') app.err: $(tail -c 300 "$OUT/app.err" 2>&1 | tr '\n' ' ')"
    ls -laR "$CA/Library/Application Support" >"$OUT/support-tree-attempt1.txt" 2>&1
  fi
  stop_app
  local sock=$CA/d.sock
  rm -f "$sock"
  open -n --env "OVERSEER_SOCKET=$sock" --env "$(app_env)" "$APP" --stdout "$OUT/app2.out" --stderr "$OUT/app2.err"
  if wait_sock "$sock" 90; then
    rec app_launches_daemon_short_socket_override PASS "OVERSEER_SOCKET=$sock"
    drive "$sock"
  else
    rec app_launches_daemon_short_socket_override FAIL "no socket at $sock after 90s; daemon.log: $(tail -c 400 "$sdir/daemon.log" 2>&1 | tr '\n' ' ')"
  fi
  cp "$sdir/daemon.log" "$OUT/daemon.log" 2>/dev/null || true
  cp "$CA/Library/Logs/$APP_ID"/* "$OUT/" 2>/dev/null || true
  stop_app

  P=exc:
  codesign --force -s - --entitlements "$ENT_EXC" "$APP"
  rm -f "$sock"
  open -n --env "OVERSEER_SOCKET=$sock" --env "$(app_env)" "$APP" --stdout "$OUT/app3.out" --stderr "$OUT/app3.err"
  if wait_sock "$sock" 90; then
    rec app_launches_daemon_short_socket_override PASS "OVERSEER_SOCKET=$sock (sbpl exceptions)"
    drive "$sock"
  else
    rec app_launches_daemon_short_socket_override FAIL "no socket (sbpl exceptions)"
  fi
  stop_app
  P=
  return 0
}

denials() {
  sudo log show --last 60m --style compact --predicate 'eventMessage CONTAINS "deny(" OR subsystem == "com.apple.sandbox.reporting"' >"$OUT/sandbox-denials-raw.log" 2>&1
  wc -l "$OUT/sandbox-denials-raw.log"
}

report() {
  python3 "$R/scripts/mas/report.py" "$RES" "$OUT/sandbox-denials-raw.log" "${GITHUB_STEP_SUMMARY:-$OUT/summary.md}" | tee "$OUT/summary.txt" >/dev/null
  cat "${GITHUB_STEP_SUMMARY:-$OUT/summary.md}"
}

case $cmd in
sign) sign_app ;;
probes) probes ;;
install-agents) install_agents ;;
agents) agents ;;
app) app ;;
denials) denials ;;
report) report ;;
*)
  echo "usage: $0 sign|probes|app|denials|report" >&2
  exit 2
  ;;
esac
