#!/bin/bash
set -u
cmd=${1:-}
R=$(cd "$(dirname "$0")/../.." && pwd)
OUT=${MAS_OUT:-$R/mas-out}
RES=$OUT/results.tsv
APP=${MAS_APP:-$R/app/src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/Overseer.app}
ENT=$R/app/src-tauri/entitlements.mas.plist
ENT_HELPER=$R/app/src-tauri/entitlements.mas-helper.plist
PROBE=$OUT/Probe.app
PROBE_ID=io.github.basilelt.overseer.probe
APP_ID=io.github.basilelt.overseer.mac
CP=$HOME/Library/Containers/$PROBE_ID/Data
CA=$HOME/Library/Containers/$APP_ID/Data
GRANT=/tmp/sbx-grant
mkdir -p "$OUT"

rec() {
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" | tee -a "$RES"
}

take() {
  grep '^RESULT' | cut -f2- | tee -a "$RES"
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
  codesign --force -s - --entitlements "$ENT" "$PROBE"
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

probes() {
  : >"$OUT/probe-all.txt"
  prep_fixtures
  build_probe
  local p="$PROBE/Contents/MacOS/probe"
  {
    echo "== execs"
    "$p" execs "$HOME" 2>&1
  } | tee -a "$OUT/probe-all.txt" | take >/dev/null
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
  "$p" pty 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  "$p" misc "$CP" "$HOME" 2>&1 | tee -a "$OUT/probe-all.txt" | take >/dev/null
  ls -la "$GRANT" >"$OUT/grant-dir-after.txt" 2>&1
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

app() {
  GRANT=$(cd "$GRANT" 2>/dev/null && pwd -P || echo "$GRANT")
  local sdir="$CA/Library/Application Support/overseer"
  local dflt="$sdir/daemon.sock"
  rec app_default_socket_path_length INFO "${#dflt} chars (sun_path limit is 104): $dflt"
  open -n "$APP" --stdout "$OUT/app.out" --stderr "$OUT/app.err"
  local i=0
  while [ $i -lt 40 ] && [ ! -d "$CA/Library" ]; do sleep 1; i=$((i + 1)); done
  [ -d "$CA" ] && rec app_container_created PASS "$CA" || rec app_container_created FAIL "no container: app not sandboxed or not launched"
  pgrep -fl "Overseer.app" >"$OUT/app-procs.txt" 2>&1
  rec app_process_running "$([ -s "$OUT/app-procs.txt" ] && echo PASS || echo FAIL)" "$(head -c 200 "$OUT/app-procs.txt")"
  local sock=$dflt
  if wait_sock "$dflt" 60; then
    rec app_launches_daemon_default_socket PASS "daemon socket up at default path"
  else
    rec app_launches_daemon_default_socket FAIL "no socket at default path after 60s; daemon.log: $(tail -c 400 "$sdir/daemon.log" 2>&1 | tr '\n' ' ') app.err: $(tail -c 300 "$OUT/app.err" 2>&1 | tr '\n' ' ')"
    ls -laR "$CA/Library/Application Support" >"$OUT/support-tree-attempt1.txt" 2>&1
    stop_app
    sock="$CA/d.sock"
    rm -f "$sock"
    open -n --env "OVERSEER_SOCKET=$sock" "$APP" --stdout "$OUT/app2.out" --stderr "$OUT/app2.err"
    if wait_sock "$sock" 90; then
      rec app_launches_daemon_short_socket_override PASS "OVERSEER_SOCKET=$sock"
    else
      rec app_launches_daemon_short_socket_override FAIL "no socket at $sock after 90s; daemon.log: $(tail -c 400 "$sdir/daemon.log" 2>&1 | tr '\n' ' ')"
      local helper="$PROBE/Contents/Helpers/overseer"
      sock="$CP/d.sock"
      rm -f "$sock"
      "$PROBE/Contents/MacOS/probe" daemon "$helper" "$sock" 2>&1 | take >/dev/null
      wait_sock "$sock" 30 || sock=
    fi
  fi
  if [ -z "$sock" ] || [ ! -S "$sock" ]; then
    rec app_driver_skipped FAIL "no daemon socket to talk to"
    return 0
  fi
  local base=$CA
  case $sock in "$CP"/*) base=$CP ;; esac
  mkdir -p "$base/stubs"
  cp "$R/tests/stubs/agent" "$base/stubs/agent"
  chmod +x "$base/stubs/agent"
  rm -rf "$base/proj"
  mkdir -p "$base/proj"
  (
    cd "$base/proj" &&
      git init -q -b main &&
      echo hi >a.txt &&
      git -c user.name=qa -c user.email=qa@example.com add -A &&
      git -c user.name=qa -c user.email=qa@example.com commit -qm init
  )
  python3 "$R/scripts/mas/drive.py" "$sock" "$base" "$RES" "$GRANT" 2>&1 | tee "$OUT/drive.log" >/dev/null
  find "$CA" -maxdepth 6 2>/dev/null | head -400 >"$OUT/container-tree.txt"
  cp "$sdir/daemon.log" "$OUT/daemon.log" 2>/dev/null || true
  stop_app
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
app) app ;;
denials) denials ;;
report) report ;;
*)
  echo "usage: $0 sign|probes|app|denials|report" >&2
  exit 2
  ;;
esac
