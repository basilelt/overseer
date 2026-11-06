#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

elf=$(command -v ls)
host=$(objdump -T "$elf" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
mkdir -p "$work/tree/usr/bin"
cp "$elf" "$work/tree/usr/bin/app"
printf 'not an elf\n' >"$work/tree/AppRun"

expect() {
  local want=$1 label=$2
  shift 2
  if "$root/scripts/ci-glibc.sh" "$@" >"$work/out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$want" ]; then
    echo "ok   $label"
  else
    echo "FAIL $label: $want expected"
    cat "$work/out"
    fail=1
  fi
}

expect pass "a binary at the floor passes" "$host" "$work/tree"
expect pass "a newer floor passes" 99.0 "$work/tree"
expect pass "files that are not ELF are skipped" 2.0 "$work/tree/AppRun"
expect fail "a binary newer than the floor fails" 2.0 "$work/tree"
if grep -q "usr/bin/app needs GLIBC_${host}, newer than 2.0" "$work/out"; then
  echo "ok   the error names the file and version"
else
  echo "FAIL the error names the file and version"
  fail=1
fi

exit "$fail"
