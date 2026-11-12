#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
. "$root/scripts/version-files.sh"
files="$VERSION_MANIFESTS $VERSION_LOCKS $VERSION_JSONS"

# A copy of the stamped files, so each case edits its own tree.
tree() {
  local dir=$work/$RANDOM$RANDOM
  mkdir -p "$dir/scripts"
  cp "$root/scripts/set-version.sh" "$root/scripts/version-files.sh" "$dir/scripts/"
  for f in $files; do
    mkdir -p "$dir/$(dirname "$f")"
    cp "$root/$f" "$dir/$f"
  done
  "$dir/scripts/set-version.sh" 1.2.3 >/dev/null
  echo "$dir"
}

expect() {
  local want=$1 label=$2 dir=$3 version=$4 needle=${5:-}
  if CHECK_VERSION_ROOT=$dir "$root/scripts/check-version.sh" "$version" >"$work/out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$want" ] && { [ -z "$needle" ] || grep -q -- "$needle" "$work/out"; }; then
    echo "ok   $label"
  else
    echo "FAIL $label: $want expected${needle:+ mentioning $needle}"
    cat "$work/out"
    fail=1
  fi
}

expect pass "passes when every file has the version" "$(tree)" 1.2.3
expect pass "accepts a leading v" "$(tree)" v1.2.3
expect fail "rejects a different version and names it" "$(tree)" 1.2.4 "Cargo.toml: 1.2.3, want 1.2.4"
expect fail "rejects a missing argument" "$(tree)" "" usage
expect fail "rejects a malformed version" "$(tree)" 1.2 usage

for f in $files; do
  dir=$(tree)
  case $f in
    *.json) jq '.version = "0.0.1"' "$dir/$f" >"$dir/$f.tmp" && mv "$dir/$f.tmp" "$dir/$f" ;;
    *.toml) sed -i.bak '1,/^version = /s/^version = .*/version = "0.0.1"/' "$dir/$f" ;;
    *.lock) sed -i.bak '/^name = "overseer/{n;s/^version = .*/version = "0.0.1"/;}' "$dir/$f" ;;
  esac
  expect fail "rejects a stale $f" "$dir" 1.2.3 "$f.*: 0.0.1"
done

want=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
if [ "$("$root/scripts/version.sh")" = "$want" ] && [ -n "$want" ]; then
  echo "ok   version.sh reads the Cargo.toml version"
else
  echo "FAIL version.sh: want $want"
  fail=1
fi
for f in $files; do
  if [ -f "$root/$f" ]; then :; else echo "FAIL listed file $f is missing"; fail=1; fi
done

exit $fail
