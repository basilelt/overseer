#!/bin/sh
set -u

here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

check() {
  if [ "$2" = "$3" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1: want '$3', got '$2'"
    fail=1
  fi
}

. "$here/android-common.sh"

check "norm_fp drops colons and spaces and lowercases" "$(printf 'AB:CD EF:01\n' | norm_fp)" "abcdef01"

mkdir -p "$work/none" "$work/one/x" "$work/two"
touch "$work/one/x/app-release.aab" "$work/two/a-release.aab" "$work/two/b-release.aab"

out=$(find_one "android aab" "release AAB" "$work/none" "*-release.aab" 2>&1)
check "find_one fails on no artefact" "$?:$out" "1:::error title=android aab::expected one release AAB under $work/none, found 0"

out=$(find_one "android aab" "release AAB" "$work/two" "*-release.aab" 2>&1 | head -1)
check "find_one fails on two artefacts and counts them" "$out" "::error title=android aab::expected one release AAB under $work/two, found 2"

find_one "android aab" "release AAB" "$work/one" "*-release.aab"
check "find_one returns the single artefact" "$found" "$work/one/x/app-release.aab"

echo data >"$work/in.bin"
publish "$work/in.bin" "$work/out/sub/final.bin" >/dev/null
check "publish copies the artefact" "$(cat "$work/out/sub/final.bin")" data
check "publish writes a relative sha256 file" "$(cut -d' ' -f3 "$work/out/sub/final.bin.sha256")" final.bin
check "publish checksum matches" "$(cut -d' ' -f1 "$work/out/sub/final.bin.sha256")" "$(sha256sum "$work/in.bin" | cut -d' ' -f1)"

exit $fail
