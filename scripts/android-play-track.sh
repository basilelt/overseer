#!/bin/sh
set -eu
track=$(printf '%s' "${PLAY_TRACK:-}" | tr -d '[:space:]')
status=$(printf '%s' "${PLAY_RELEASE_STATUS:-}" | tr -d '[:space:]')
track=${track:-internal}
status=${status:-completed}
case $(printf '%s' "$track" | tr '[:upper:]' '[:lower:]') in
  production)
    echo "::error title=google play::PLAY_TRACK=$track: Overseer never publishes to the production track from CI, only to internal or closed testing (docs/mobile.md)"
    exit 1
    ;;
esac
if ! printf '%s' "$track" | grep -qE '^[A-Za-z0-9][A-Za-z0-9:_-]*$'; then
  echo "::error title=google play::PLAY_TRACK=$track is not a track name"
  exit 1
fi
case $status in
  completed | draft) ;;
  *)
    echo "::error title=google play::PLAY_RELEASE_STATUS=$status: use completed or draft"
    exit 1
    ;;
esac
echo "track=$track" | tee -a "${GITHUB_OUTPUT:-/dev/null}"
echo "status=$status" | tee -a "${GITHUB_OUTPUT:-/dev/null}"
