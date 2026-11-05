#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cargo build --offline --release --quiet --manifest-path "$root/Cargo.toml"
bin="$root/target/release/overseer"
state=$(mktemp -d)
trap 'rm -rf "$state"' EXIT

run() {
  if command -v bwrap >/dev/null 2>&1; then
    bwrap --ro-bind / / --bind "$state" "$state" --dev /dev --proc /proc \
      env OVERSEER_STATE_DIR="$state" "$bin" "$@"
  else
    OVERSEER_STATE_DIR="$state" "$bin" "$@"
  fi
}

echo "== state dir: $state (everything else read-only when bwrap is available)"
echo "== overseer ls (first 15 rows)"
run ls | head -n 16
echo
echo "== lines per agent and state"
run ls --json | jq -r '.[] | "\(.line.agent) \(.line.state) \(.local.heldReason // "")"' | sort | uniq -c
echo
echo "== lines with a ticket"
run ls --json | jq -r '.[] | select(.line.ticketKey) | "\(.line.ticketKey)  \(.line.branch)  \(.line.worktreePath)"' | sort -u
echo
echo "== active lines only"
run ls --state active
echo
echo "== files written"
find "$state" -exec ls -ld {} +
