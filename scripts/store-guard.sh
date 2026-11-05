#!/bin/sh
set -u
cd "${STORE_GUARD_ROOT:-$(dirname "$0")/..}" || exit 2
store=.github/workflows/store.yml
paths=".github scripts Makefile"
existing=
for p in $paths; do
  [ -e "$p" ] && existing="$existing $p"
done
fail=0
hits=$(grep -rnwE --exclude='store-guard*.sh' \
  'deliver|upload_to_app_store|submit_for_review|submit_beta_review|distribute_external|upload_to_play_store|supply|app_store|play_store|upload-google-play' \
  $existing 2>/dev/null | grep -v "^$store:")
if [ -n "$hits" ]; then
  echo "$hits" | while IFS= read -r line; do
    echo "::error title=store guard::${line}"
  done
  echo "store guard: App Store / Play Store submission outside $store; store releases are manual only (docs/mobile.md)"
  fail=1
fi
if [ -f "$store" ]; then
  triggers=$(awk '/^on:/{on=1; next} on && /^[^ ]/{exit} on && /^  [a-z_]+:/{sub(/^  /,""); sub(/:.*/,""); print}' "$store")
  if [ "$triggers" != "workflow_dispatch" ]; then
    echo "::error title=store guard::$store must be triggered by workflow_dispatch only, found: $(echo "$triggers" | tr '\n' ' ')"
    fail=1
  fi
fi
for f in app/src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset/*.png; do
  [ -e "$f" ] || continue
  case $(od -An -tu1 -j25 -N1 "$f" | tr -d ' ') in
    4|6)
      echo "::error title=store guard::$f has an alpha channel; App Store Connect rejects it (90717)"
      fail=1
      ;;
  esac
done
[ "$fail" -eq 0 ] && echo "store guard: store submissions only in $store (manual), opaque iOS icons"
exit "$fail"
