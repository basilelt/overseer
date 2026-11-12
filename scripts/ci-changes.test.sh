#!/usr/bin/env bash
set -euo pipefail

script=$(cd "$(dirname "$0")" && pwd)/ci-changes.sh
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
git -C "$work" init -q
g() { git -C "$work" -c commit.gpgsign=false -c user.name=ci -c user.email=ci@localhost "$@"; }
g commit -q --allow-empty -m base

fail=0
expect() {
  local name=$1 want=$2 file=$3 got
  shift 3
  mkdir -p "$work/$(dirname "$file")"
  echo "$RANDOM" >>"$work/$file"
  g add -A
  g commit -qm "$name"
  got=$(cd "$work" && env "$@" "$script" 2>/dev/null | awk -F= '/^[a-z_]+=true$/ { printf "%s ", $1 }')
  if [ "$got" = "$want" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: want '$want', got '$got'"
    fail=1
  fi
}

expect "docs only runs nothing" "" docs/ci.md
expect "root markdown runs nothing" "" README.md
expect "issue template runs nothing" "" .github/ISSUE_TEMPLATE/bug.yml
expect "nested markdown runs nothing" "" sub/notes.md
expect "daemon source" "rust app windows " src/main.rs
expect "wire vocabulary shared by the daemon and the relay" "rust app windows " contract/wire.rs
expect "app source" "app " app/src/main.ts
expect "tauri source" "app android " app/src-tauri/src/lib.rs
expect "setup action is wide" "rust app windows android actionlint store_guard tap ci_scripts " .github/actions/setup/action.yml
expect "apt script is wide" "rust app windows android actionlint store_guard tap ci_scripts " scripts/ci-apt.sh
expect "ci workflow is wide" "rust app windows android actionlint store_guard tap ci_scripts " .github/workflows/ci.yml
expect "release workflow" "macos_signing actionlint store_guard " .github/workflows/release.yml
expect "tap script" "store_guard tap " scripts/update-tap.sh
expect "version stamp script" "store_guard ci_scripts " scripts/set-version.sh
expect "version check script" "store_guard ci_scripts " scripts/check-version.sh
expect "version file list" "store_guard ci_scripts " scripts/version-files.sh
expect "docs path helper" "store_guard ci_scripts " scripts/docs-paths.sh
expect "push to next filters like a pull request" "app " app/src/y.ts EVENT=push REF=refs/heads/next
expect "release push runs everything" "rust app windows android macos_signing macos_procs actionlint store_guard tap ci_scripts " docs/x.md EVENT=push REF=refs/heads/main
expect "manual run runs everything" "rust app windows android macos_signing macos_procs actionlint store_guard tap ci_scripts " docs/y.md EVENT=workflow_dispatch REF=refs/heads/next
expect "release gate skip runs nothing" "" src/x.rs EVENT=push SKIP=true

gate=$(cd "$(dirname "$0")" && pwd)/release-gate.sh
agree() {
  local want=$1 file=$2 got
  mkdir -p "$work/$(dirname "$file")"
  echo "$RANDOM" >>"$work/$file"
  g add -A
  g commit -qm "gate $file"
  got=$(cd "$work" && "$gate" 2>/dev/null | sed -n 's/^release=//p')
  if [ "$got" = "$want" ]; then
    echo "ok   release gate releases=$want for $file"
  else
    echo "FAIL release gate $file: want release=$want, got '$got'"
    fail=1
  fi
}

agree false docs/guide.md
agree false README.md
agree false .github/ISSUE_TEMPLATE/bug.yml
agree false sub/notes.md
agree true src/main.rs
agree true Makefile
agree true .github/workflows/ci.yml

exit $fail
