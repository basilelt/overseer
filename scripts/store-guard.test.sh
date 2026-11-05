#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

upload() {
  cat <<'YML'
jobs:
  play:
    steps:
      - id: play
        run: scripts/android-play-track.sh
      - uses: r0adkll/upload-google-play@e738b9dd8f2476ea806d921b64aacd24f34515a5 # v1.1.5
        with:
          packageName: io.github.basilelt.overseer
          releaseFiles: play/Overseer.aab
          track: ${{ steps.play.outputs.track }}
          status: ${{ steps.play.outputs.status }}
YML
}

expect() {
  local want=$1 label=$2 body=$3
  local dir=$work/$RANDOM$RANDOM
  mkdir -p "$dir/.github/workflows"
  printf '%s\n' "$body" >"$dir/.github/workflows/release.yml"
  if STORE_GUARD_ROOT=$dir "$root/scripts/store-guard.sh" >"$work/out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$want" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: $want expected"
    cat "$work/out"
    fail=1
  fi
}

expect pass "allows an internal or closed testing upload from the checked track" "$(upload)"
expect fail "rejects a literal production track" "$(upload | sed 's/track: .*/track: production/')"
expect fail "rejects a hard-coded track that skips the check" "$(upload | sed 's/track: .*/track: internal/')"
expect fail "rejects a staged rollout fraction" "$(upload; echo '          userFraction: 0.5')"
expect fail "rejects an upload without the track check" "$(upload | grep -v android-play-track)"
expect fail "rejects PLAY_TRACK set to production" "$(upload; echo '    env:'; echo '      PLAY_TRACK: production')"
expect fail "rejects fastlane supply" "      - run: fastlane supply --track internal"
expect fail "rejects fastlane upload_to_play_store" "      - run: fastlane run upload_to_play_store"
expect fail "rejects App Store submission" "      - run: fastlane deliver"
expect fail "rejects TestFlight without skip_submission" "      - run: fastlane run upload_to_testflight"
if "$root/scripts/store-guard.sh" >"$work/out" 2>&1; then echo "ok   the repository passes"; else echo "FAIL the repository"; cat "$work/out"; fail=1; fi

exit "$fail"
