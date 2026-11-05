#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

store() {
  cat <<'YML'
on:
  workflow_dispatch:
    inputs:
      track:
        type: choice
        options: [production, internal]
jobs:
  ios:
    steps:
      - run: fastlane ios app_store
  android:
    steps:
      - uses: r0adkll/upload-google-play@e738b9dd8f2476ea806d921b64aacd24f34515a5 # v1.1.5
        with:
          track: ${{ inputs.track }}
YML
}

expect() {
  local want=$1 label=$2 file=$3 body=$4
  local dir=$work/$RANDOM$RANDOM
  mkdir -p "$dir/.github/workflows"
  printf '%s\n' "$body" >"$dir/.github/workflows/$file"
  if STORE_GUARD_ROOT=$dir "$root/scripts/store-guard.sh" >"$work/out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$want" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: $want expected"
    cat "$work/out"
    fail=1
  fi
}

expect pass "allows store submissions in the manual store workflow" store.yml "$(store)"
expect fail "rejects a push trigger on the store workflow" store.yml "$(store | sed 's/^  workflow_dispatch:/  push:\n  workflow_dispatch:/')"
expect fail "rejects a schedule on the store workflow" store.yml "$(store | sed 's/^  workflow_dispatch:/  schedule:\n    - cron: "0 0 * * 1"\n  workflow_dispatch:/')"
expect fail "rejects a Google Play upload from the release workflow" release.yml "$(store | sed -n '/android:/,$p')"
expect fail "rejects the App Store lane from the release workflow" release.yml "      - run: fastlane ios app_store"
expect fail "rejects fastlane supply outside the store workflow" release.yml "      - run: fastlane supply --track internal"
expect fail "rejects fastlane upload_to_play_store outside the store workflow" release.yml "      - run: fastlane run upload_to_play_store"
expect fail "rejects App Store submission outside the store workflow" ci.yml "      - run: fastlane deliver"
if "$root/scripts/store-guard.sh" >"$work/out" 2>&1; then echo "ok   the repository passes"; else echo "FAIL the repository"; cat "$work/out"; fail=1; fi

exit "$fail"
