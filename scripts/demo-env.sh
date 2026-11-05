#!/bin/sh
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --offline --release --quiet --manifest-path "$root/Cargo.toml"
SB="$root/target/release/overseer"
demo=$(mktemp -d)
sock="sb-demo-$$"
tmux_path=

export PATH="$root/tests/stubs:/usr/bin:/bin"
export HOME="$demo/home" SHELL=/bin/sh TERM=xterm-256color XDG_RUNTIME_DIR="$demo/run"
export CLAUDE_CONFIG_DIR="$HOME/.claude" CODEX_HOME="$HOME/.codex"
export OVERSEER_STATE_DIR="$demo/state" OVERSEER_TMUX_SOCKET="$sock"
export OVERSEER_TMUX_SNAPSHOT="$demo/snapshot.json" OVERSEER_SOCKET="$demo/d.sock"
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export SB_BIN="$SB" SB_STUB_LOG="$demo/stub.log"
unset TMUX TMUX_PANE

t() { tmux -L "$sock" -f /dev/null "$@"; }
sb() { "$SB" "$@"; }
say() { printf '\n== %s\n' "$*"; }
pause() { sleep "${1:-1}"; }

cleanup() {
  [ -n "${daemon_pid:-}" ] && kill "$daemon_pid" 2>/dev/null
  tmux_path=$(t display-message -p '#{socket_path}' 2>/dev/null)
  t kill-server 2>/dev/null
  case "$tmux_path" in */"$sock") rm -f "$tmux_path" ;; esac
  rm -rf "$demo"
}
trap cleanup EXIT INT TERM

mkdir -p "$HOME/.claude" "$demo/repo"
git -C "$demo/repo" init -q -b feature/AS-4242-demo
git -C "$demo/repo" config user.email demo@example.com
git -C "$demo/repo" remote add origin git@github.com:Acme/demo.git
t new-session -d -s main -x 160 -y 40 sh

start_agent() {
  t new-window -t main -n "$1" -c "$demo/repo" -e "SB_STUB_SESSION=$2" "claude --model opus"
  pause 1
}

line_id() { sb ls --json --all | jq -r --arg s "$1" '.[] | select(.line.agentSessionId == $s) | .line.id'; }
pane_of() { sb ls --json --all | jq -r --arg s "$1" '.[] | select(.line.agentSessionId == $s) | .local.tmuxPane'; }
show() { sb ls | cut -c1-150; }
