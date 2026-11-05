#!/bin/sh
set -u
cd "$(dirname "$0")/.."
store=.github/workflows/store.yml
paths=".github scripts Makefile"
existing=
for p in $paths; do
  [ -e "$p" ] && existing="$existing $p"
done
fail=0
hits=$(grep -rnwE --exclude=store-guard.sh \
  'deliver|upload_to_app_store|submit_for_review|upload_to_play_store|supply|app_store|play_store' \
  $existing 2>/dev/null | grep -v "^$store:")
if [ -n "$hits" ]; then
  echo "$hits" | while IFS= read -r line; do
    echo "::error title=store guard::${line}"
  done
  echo "store guard: store submission outside $store; App Store and Play Store releases are manual only (docs/mobile.md)"
  fail=1
fi
if [ -f "$store" ]; then
  triggers=$(awk '/^on:/{on=1; next} on && /^[^ ]/{exit} on && /^  [a-z_]+:/{sub(/:.*/, ""); sub(/^ +/, ""); print}' "$store")
  if [ "$triggers" != "workflow_dispatch" ]; then
    echo "::error title=store guard::$store must only run on workflow_dispatch, has: $(echo $triggers)"
    fail=1
  fi
fi
[ "$fail" -eq 0 ] && echo "store guard: store submissions only in $store (manual)"
exit "$fail"
