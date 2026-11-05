#!/usr/bin/env bash
set -uo pipefail

script=$(cd "$(dirname "$0")" && pwd)/android-play-track.sh
fail=0

expect() {
  local want=$1 track=$2 status=$3 out
  out=$(PLAY_TRACK=$track PLAY_RELEASE_STATUS=$status GITHUB_OUTPUT=/dev/null "$script" 2>&1)
  local code=$?
  if [ "$want" = reject ]; then
    if [ "$code" -ne 0 ]; then echo "ok   rejects track='$track' status='$status'"; else echo "FAIL accepted track='$track' status='$status'"; fail=1; fi
  else
    if [ "$code" -eq 0 ] && [ "$(printf '%s\n' "$out" | tr '\n' ' ')" = "$want " ]; then
      echo "ok   track='$track' status='$status' -> $want"
    else
      echo "FAIL track='$track' status='$status': got '$out', want '$want'"
      fail=1
    fi
  fi
}

expect "track=internal status=completed" "" ""
expect "track=internal status=draft" "" "draft"
expect "track=alpha status=completed" "alpha" ""
expect "track=beta status=completed" " beta " "completed"
expect "track=closed-testers status=completed" "closed-testers" ""
expect reject "production" ""
expect reject "Production" ""
expect reject " PRODUCTION" "draft"
expect reject "internal" "inProgress"
expect reject "internal" "halted"
expect reject "a b;rm" ""

exit "$fail"
