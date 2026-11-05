#!/bin/sh
set -eu
R=$(cd "$(dirname "$0")/.." && pwd)
D=${OVERSEER_DEV_DIR:-$HOME/.overseer-dev}
BIN=${OVERSEER_DEV_BIN:-$R/target/debug/overseer}
TMUX_NAME=${OVERSEER_DEV_TMUX:-overseer-dev}
SOCK=$D/state/daemon.sock

usage() {
  cat <<EOF
usage: scripts/dev.sh <command>

  up      build the daemon from this checkout and start it, isolated in $D
  app     run the desktop app (tauri dev, id io.github.basilelt.overseer.dev) on that daemon
  status  show whether the dev daemon is running
  down    stop the dev daemon (its tmux sessions keep running, as with the real one)
  reset   stop everything dev and delete $D

The installed app and its daemon are never touched: the dev daemon gets its own HOME,
state directory and tmux server.
EOF
}

denv() {
  env -u XDG_CONFIG_HOME -u XDG_DATA_HOME -u XDG_STATE_HOME -u XDG_CACHE_HOME \
    -u CLAUDE_CONFIG_DIR -u CODEX_HOME -u COPILOT_HOME -u OVERSEER_SOCKET -u TMUX -u TMUX_PANE \
    HOME="$D/home" OVERSEER_STATE_DIR="$D/state" OVERSEER_TMUX_SOCKET="$TMUX_NAME" "$@"
}

pid_alive() {
  [ -f "$D/state/daemon.pid" ] && kill -0 "$(cat "$D/state/daemon.pid")" 2>/dev/null
}

stop() {
  if pid_alive; then
    pid=$(cat "$D/state/daemon.pid")
    kill "$pid" 2>/dev/null || true
    i=0
    while kill -0 "$pid" 2>/dev/null; do
      i=$((i + 1))
      [ $i -lt 50 ] || { kill -9 "$pid" 2>/dev/null || true; break; }
      sleep 0.1
    done
  fi
  rm -f "$D/state/daemon.pid"
}

case ${1:-} in
up)
  [ -n "${OVERSEER_DEV_BIN:-}" ] || (cd "$R" && nice -n 10 cargo build -j 2 --bin overseer)
  mkdir -p "$D/home"
  if [ ! -f "$D/home/.gitconfig" ] && [ -f "$HOME/.gitconfig" ]; then
    cp "$HOME/.gitconfig" "$D/home/.gitconfig"
  fi
  denv "$BIN" daemon --detach >/dev/null
  echo "dev daemon up: $SOCK (log $D/state/daemon.log)"
  ;;
app)
  shift
  pid_alive || { echo "dev daemon is not running: scripts/dev.sh up" >&2; exit 1; }
  cd "$R/app"
  exec env OVERSEER_STATE_DIR="$D/state" OVERSEER_SOCKET="$SOCK" \
    pnpm tauri dev -c src-tauri/tauri.dev.conf.json "$@"
  ;;
status)
  if pid_alive; then
    echo "running pid $(cat "$D/state/daemon.pid") on $SOCK"
  else
    echo "stopped"
    exit 1
  fi
  ;;
down)
  stop
  echo "dev daemon down"
  ;;
reset)
  stop
  tmux -L "$TMUX_NAME" kill-server 2>/dev/null || true
  rm -rf "$D"
  echo "removed $D"
  ;;
*)
  usage
  if [ -n "${1:-}" ]; then exit 2; fi
  ;;
esac
