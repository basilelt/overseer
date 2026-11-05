#!/usr/bin/env bash
set -uo pipefail

script=$(cd "$(dirname "$0")" && pwd)/android-16k.sh
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

bundle() {
  local name=$1 page=$2
  mkdir -p "$work/$name/base/lib/x86_64"
  echo 'int overseer(void) { return 1; }' >"$work/lib.c"
  cc -shared -fPIC -Wl,-z,max-page-size="$page" -o "$work/$name/base/lib/x86_64/libapp.so" "$work/lib.c"
  (cd "$work/$name" && zip -qr "../$name.aab" base)
  echo "$work/$name.aab"
}

expect() {
  local want=$1 label=$2 file=$3
  if "$script" "$file" >"$work/out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$want" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: $want expected"
    cat "$work/out"
    fail=1
  fi
}

expect pass "accepts 16 KB aligned libraries" "$(bundle aligned 16384)"
expect fail "rejects 4 KB aligned libraries" "$(bundle legacy 4096)"
mkdir -p "$work/empty/base/manifest" && echo x >"$work/empty/base/manifest/m" && (cd "$work/empty" && zip -qr ../empty.aab base)
expect fail "rejects a bundle without a 64-bit library" "$work/empty.aab"
expect fail "rejects a missing file" "$work/missing.aab"

exit "$fail"
