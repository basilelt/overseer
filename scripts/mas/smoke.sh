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

sign_app_env() {
  local extra=
  [ "$P" = exc: ] && extra=/opt/homebrew/bin:
  printf 'PATH=%s%s:/usr/bin:/bin:/usr/sbin:/sbin' "$extra" "$(dirname "$GITBIN")"
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
  PREFIX=$P python3 "$R/scripts/mas/drive.py" "$sock" "$base" "$RES" "$GRANT" 2>&1 | tee -a "$OUT/drive$P.log" >/dev/null
  find "$CA" -maxdepth 6 2>/dev/null | head -400 >"$OUT/container-tree$P.txt"
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
app) app ;;
denials) denials ;;
report) report ;;
*)
  echo "usage: $0 sign|probes|app|denials|report" >&2
  exit 2
  ;;
esac
