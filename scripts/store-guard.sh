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
for f in app/src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/*.png; do
  [ -e "$f" ] || continue
  case $(od -An -tu1 -j25 -N1 "$f" | tr -d ' ') in
    4|6)
      echo "::error title=store guard::$f has an alpha channel; App Store Connect rejects it (90717)"
      fail=1
      ;;
  esac
done
[ "$fail" -eq 0 ] && echo "store guard: TestFlight only, no store submission, opaque iOS icons"
exit "$fail"
