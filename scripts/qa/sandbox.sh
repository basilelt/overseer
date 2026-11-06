#!/bin/sh
set -eu
cmd=${1:-start}
R=$(cd "${0%/*}/../.." && pwd)
D=${QA_DIR:-/tmp/sbqa}
PORT=${QA_PORT:-47811}
WEB=${QA_WEB:-4918}
DISPLAY_NUM=${QA_DISPLAY:-93}
MESA_EGL=/usr/share/glvnd/egl_vendor.d/50_mesa.json
BIN=${CARGO_TARGET_DIR:-$R/target}/debug/overseer

# Daemon PATH: the stubs first, then Homebrew when present (tmux, git live there on macOS).
daemon_path=$R/tests/stubs
for d in /opt/homebrew/bin /usr/local/bin; do
  [ -d "$d" ] && daemon_path=$daemon_path:$d
done
daemon_path=$daemon_path:/usr/bin:/bin

# macOS has no setsid: run a new session through python3 instead.
if command -v setsid >/dev/null 2>&1; then
  SETSID=$(command -v setsid)
  SETSID_ARG=
else
  SETSID=$(command -v python3 || true)
  SETSID_ARG=$R/scripts/qa/setsid.py
fi

linux_only() {
  [ "$(uname -s)" = Linux ] || { echo "$1 needs Xvfb and WebKitGTK: Linux only, use the browser modes on macOS" >&2; exit 1; }
}

stop_pid() {
  if [ -f "$1" ]; then kill "$(cat "$1")" 2>/dev/null || true; fi
  rm -f "$1"
}

stop_group() {
  if [ -f "$1" ]; then kill -- "-$(cat "$1")" 2>/dev/null || kill "$(cat "$1")" 2>/dev/null || true; fi
  rm -f "$1"
}

case $cmd in
launcher)
  echo "${SETSID##*/}"
  ;;
bin)
  echo "$BIN"
  ;;
daemon-path)
  echo "$daemon_path"
  ;;
build)
  (cd "$R" && nice -n 10 cargo build -j 2 --offline)
  (cd "$R/app" && nice -n 10 pnpm exec vite build --outDir "$D/dist" --emptyOutDir)
  ;;
start)
  "$0" stop
  mkdir -p "$D/state" "$D/run" "$D/home/proj"
  chmod 700 "$D/run"
  if [ ! -d "$D/home/proj/.git" ]; then
    (cd "$D/home/proj" && git init -q -b main && echo hello > a.txt \
      && GIT_CONFIG_GLOBAL=/dev/null git -c user.name=qa -c user.email=qa@example.com add -A \
      && GIT_CONFIG_GLOBAL=/dev/null git -c user.name=qa -c user.email=qa@example.com commit -qm init)
  fi
  sed "s/__QA_PORT__/$PORT/" "$R/scripts/qa/shim.js" > "$D/shim.js"
  env -i PATH="$daemon_path" HOME="$D/home" SHELL=/bin/sh TERM=xterm-256color LANG=C.UTF-8 \
    XDG_RUNTIME_DIR="$D/run" CLAUDE_CONFIG_DIR="$D/home/.claude" CODEX_HOME="$D/home/.codex" \
    OVERSEER_STATE_DIR="$D/state" OVERSEER_TMUX_SOCKET="$D/tmux.sock" OVERSEER_LOGIN_ENV=0 \
    GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_AUTHOR_NAME=qa GIT_AUTHOR_EMAIL=qa@example.com \
    GIT_COMMITTER_NAME=qa GIT_COMMITTER_EMAIL=qa@example.com \
    SB_BIN="$BIN" SB_STUB_LOG="$D/stub.log" \
    "$SETSID" ${SETSID_ARG:+"$SETSID_ARG"} "$BIN" daemon --interval 1 >> "$D/daemon.log" 2>&1 &
  i=0
  until python3 -c 'import socket,sys; socket.socket(socket.AF_UNIX).connect(sys.argv[1])' "$D/state/daemon.sock" 2>/dev/null; do
    i=$((i+1))
    [ $i -lt 100 ] || { echo "daemon did not start, see $D/daemon.log" >&2; exit 1; }
    sleep 0.2
  done
  cp "$D/state/daemon.pid" "$D/daemon.pid"
  "$SETSID" ${SETSID_ARG:+"$SETSID_ARG"} python3 "$R/scripts/qa/bridge.py" "$D/state/daemon.sock" "$PORT" "$D/dist" >> "$D/bridge.log" 2>&1 &
  echo $! > "$D/bridge.pid"
  "$SETSID" ${SETSID_ARG:+"$SETSID_ARG"} python3 -m http.server "$WEB" -d "$D/dist" -b 127.0.0.1 >> "$D/web.log" 2>&1 &
  echo $! > "$D/web.pid"
  for port in "$PORT" "$WEB"; do
    i=0
    until python3 -c 'import socket,sys; socket.create_connection(("127.0.0.1", int(sys.argv[1])))' "$port" 2>/dev/null; do
      i=$((i+1))
      [ $i -lt 50 ] || { echo "port $port did not open, see $D/*.log" >&2; exit 1; }
      sleep 0.2
    done
  done
  echo "daemon $D/state/daemon.sock  api http://127.0.0.1:$PORT  app http://127.0.0.1:$WEB/"
  ;;
desktop-build)
  linux_only desktop-build
  (cd "$R/app/src-tauri" && nice -n 10 cargo build -j 2 --offline)
  ;;
desktop)
  linux_only desktop
  [ -f "$D/daemon.pid" ] || { echo "run $0 start first" >&2; exit 1; }
  "$0" desktop-stop
  mkdir -p "$D/apphome"
  [ -f "$MESA_EGL" ] || MESA_EGL=
  setsid env ${MESA_EGL:+__EGL_VENDOR_LIBRARY_FILENAMES="$MESA_EGL"} Xvfb ":$DISPLAY_NUM" -screen 0 1440x900x24 -nolisten tcp >> "$D/xvfb.log" 2>&1 &
  echo $! > "$D/xvfb.pid"
  # The app's devUrl is fixed at localhost:1420, which another checkout's dev server may hold; then
  # Vite and the app share a private network namespace (the daemon and X are on Unix sockets).
  ns=
  if python3 -c 'import socket; socket.create_connection(("localhost", 1420))' 2>/dev/null; then
    unshare -rn true 2>/dev/null || { echo "port 1420 is taken by another dev server" >&2; exit 1; }
    ns="unshare -rn"
  fi
  export R D DISPLAY_NUM MESA_EGL
  setsid $ns sh -c '
    ip link set lo up 2>/dev/null || true
    cd "$R/app" && nice -n 10 pnpm exec vite --port 1420 --strictPort >> "$D/vite.log" 2>&1 &
    i=0
    until python3 -c "import socket; socket.create_connection((\"localhost\", 1420))" 2>/dev/null; do
      i=$((i+1))
      [ $i -lt 100 ] || { echo "vite did not start, see $D/vite.log" >&2; exit 1; }
      sleep 0.2
    done
    exec env -i PATH=/usr/bin:/bin DISPLAY=":$DISPLAY_NUM" HOME="$D/apphome" XDG_CONFIG_HOME="$D/apphome/.config" \
      XDG_DATA_HOME="$D/apphome/.local/share" XDG_CACHE_HOME="$D/apphome/.cache" XDG_RUNTIME_DIR="$D/run" \
      OVERSEER_STATE_DIR="$D/state" LANG=C.UTF-8 WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 NO_AT_BRIDGE=1 \
      ${MESA_EGL:+__EGL_VENDOR_LIBRARY_FILENAMES="$MESA_EGL"} \
      "$R/app/src-tauri/target/debug/overseer-app"
  ' >> "$D/app.log" 2>&1 &
  echo $! > "$D/app.pid"
  echo "desktop app on DISPLAY=:$DISPLAY_NUM, log $D/app.log; screenshot with: $0 shot <name>"
  ;;
shot)
  linux_only shot
  DISPLAY=":$DISPLAY_NUM" import -window root "$D/${2:-shot}.png"
  echo "$D/${2:-shot}.png"
  ;;
desktop-stop)
  stop_group "$D/app.pid"
  stop_group "$D/vite.pid"
  stop_group "$D/xvfb.pid"
  ;;
crash)
  kill -9 "$(cat "$D/daemon.pid")"
  ;;
stop)
  "$0" desktop-stop
  stop_pid "$D/daemon.pid"
  stop_pid "$D/state/daemon.pid"
  stop_pid "$D/bridge.pid"
  stop_pid "$D/web.pid"
  ;;
clean)
  "$0" stop || true
  rm -rf "$D"
  ;;
*)
  echo "usage: $0 build|start|desktop-build|desktop|shot|desktop-stop|crash|stop|clean" >&2
  exit 2
  ;;
esac
