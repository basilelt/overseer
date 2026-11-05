#!/bin/sh
set -u
cd "$(dirname "$0")/.."
paths=".github app/fastlane scripts Makefile"
existing=
for p in $paths; do
  [ -e "$p" ] && existing="$existing $p"
done
fail=0
hits=$(grep -rnwE --exclude=store-guard.sh \
  'deliver|upload_to_app_store|submit_for_review|submit_beta_review|distribute_external|upload_to_play_store|supply' \
  $existing 2>/dev/null)
if [ -n "$hits" ]; then
  echo "$hits" | while IFS= read -r line; do
    echo "::error title=store guard::${line}"
  done
  echo "store guard: App Store / Play Store submission found; Overseer ships to TestFlight and GitHub Releases only (docs/mobile.md)"
  fail=1
fi
for f in $(grep -rlwE 'upload_to_testflight|pilot' $existing 2>/dev/null | grep -v 'store-guard.sh$'); do
  if ! grep -q 'skip_submission: true' "$f"; then
    echo "::error title=store guard::$f uploads to TestFlight without skip_submission: true"
    fail=1
  fi
done
[ "$fail" -eq 0 ] && echo "store guard: TestFlight only, no store submission"
exit "$fail"
