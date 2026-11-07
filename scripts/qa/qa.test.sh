#!/usr/bin/env bash
# Tests of the QA sandbox helpers on any host: launcher and binary resolution, stub start time, drive.py browser search.
set -uo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0

check() {
  if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: want '$2', got '$3'"; fail=1; fi
}

py=$(command -v python3)
mkdir -p "$work/nosetsid" "$work/fakesetsid"
ln -s "$py" "$work/nosetsid/python3"
printf '#!/bin/sh\nexec "$@"\n' > "$work/fakesetsid/setsid"
chmod +x "$work/fakesetsid/setsid"

check "launcher is setsid when installed" setsid \
  "$(PATH="$work/fakesetsid:/usr/bin:/bin" "$root/scripts/qa/sandbox.sh" launcher)"
check "launcher falls back to python3 without setsid" python3 \
  "$(PATH="$work/nosetsid" /bin/sh "$root/scripts/qa/sandbox.sh" launcher)"

check "binary honours CARGO_TARGET_DIR" "$work/tgt/debug/overseer" \
  "$(CARGO_TARGET_DIR="$work/tgt" "$root/scripts/qa/sandbox.sh" bin)"
check "desktop binary honours CARGO_TARGET_DIR" "$work/tgt/debug/overseer-app" \
  "$(CARGO_TARGET_DIR="$work/tgt" "$root/scripts/qa/sandbox.sh" app-bin)"
check "QA_DISPLAY accepts a bare number" 97 "$(QA_DISPLAY=97 "$root/scripts/qa/sandbox.sh" display)"
check "QA_DISPLAY accepts a leading colon" 97 "$(QA_DISPLAY=:97 "$root/scripts/qa/sandbox.sh" display)"
check "daemon PATH has the stubs first and Homebrew when present" "$root/tests/stubs" \
  "$("$root/scripts/qa/sandbox.sh" daemon-path | cut -d: -f1)"

# The stub keeps startedAt across status writes and reports task hooks.
cfg=$work/cfg
mkdir -p "$cfg" "$work/home"
printf '#!/bin/sh\necho "$@" >> "%s/hooks"\n' "$work" > "$work/overseer"
chmod +x "$work/overseer"
{ sleep 1; echo busy; sleep 1.3; echo task; echo untask; sleep 1; } |
  CLAUDE_CONFIG_DIR=$cfg SB_STUB_LOG=$work/stub.log SB_BIN=$work/overseer \
    "$root/tests/stubs/claude" > /dev/null &
stub=$!
sleep 0.5
first=$(sed 's/.*"startedAt":\([0-9]*\).*/\1/' "$cfg"/sessions/*.json)
sleep 2
second=$(sed 's/.*"startedAt":\([0-9]*\).*/\1/' "$cfg"/sessions/*.json)
check "stub startedAt stable across status writes" "$first" "$second"
wait "$stub"
check "stub task and untask emit TaskCreated and TaskCompleted" "2" \
  "$(grep -c 'hook claude Task\(Created\|Completed\)' "$work/hooks")"

# drive.py finds a browser in the macOS Playwright cache.
mkdir -p "$work/pw/playwright" "$work/cache/chromium_headless_shell-9/chrome-headless-shell-mac-arm64"
: > "$work/pw/playwright/__init__.py"
echo 'sync_playwright = None' > "$work/pw/playwright/sync_api.py"
exe=$work/cache/chromium_headless_shell-9/chrome-headless-shell-mac-arm64/chrome-headless-shell
printf '#!/bin/sh\n' > "$exe"
chmod +x "$exe"
mkdir -p "$work/mac/Library/Caches"
ln -s "$work/cache" "$work/mac/Library/Caches/ms-playwright"
got=$(HOME=$work/mac PLAYWRIGHT_BROWSERS_PATH= PYTHONPATH="$work/pw:$root/scripts/qa" \
  python3 -c 'import drive; print(drive.newest_chromium())')
check "drive.py searches ~/Library/Caches/ms-playwright" "$work/mac/Library/Caches/ms-playwright/chromium_headless_shell-9/chrome-headless-shell-mac-arm64/chrome-headless-shell" "$got"

exit $fail
