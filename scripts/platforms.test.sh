#!/usr/bin/env bash
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
fail=0
. "$root/scripts/platforms.sh"

check() {
  if [ "$2" = "$3" ]; then
    echo "ok   $1"
  else
    echo "FAIL $1: want '$3', got '$2'"
    fail=1
  fi
}

rust_pairs() {
  awk -v fn="$1" '
    $0 ~ "fn " fn "\\(" { on = 1; next }
    on && /^    }/ { on = 0 }
    on && match($0, /Self::[A-Za-z0-9_]+ => "[^"]+"/) {
      split(substr($0, RSTART, RLENGTH), a, "\"")
      split(substr($0, RSTART, RLENGTH), v, " ")
      print v[1], a[2]
    }
  ' "$root/app/src-tauri/src/install.rs"
}

join_by_variant() {
  join <(rust_pairs name | sort) <(rust_pairs target_triple | sort) | awk '{ print $2 ":" $3 }' | sort
}

shell_pairs() { printf "%s\n" $LINUX_SERVERS $DARWIN_SERVERS | sort; }

check "install.rs maps the same platform dirs to the same triples" "$(join_by_variant)" "$(shell_pairs)"
check "install.rs lists every server platform" "$(rust_pairs name | wc -l | tr -d ' ')" "$(printf "%s\n" $LINUX_SERVERS $DARWIN_SERVERS | wc -l | tr -d ' ')"

tree="$work/tree"
mkdir -p "$tree/scripts" "$tree/dist"
cp "$root/scripts/app-resources.sh" "$root/scripts/platforms.sh" "$tree/scripts/"
for p in $LINUX_SERVERS $DARWIN_SERVERS; do echo "bin-${p%%:*}" >"$tree/dist/overseer-1.2.3-${p#*:}"; done
for p in $WINDOWS_SERVERS; do echo "exe-${p%%:*}" >"$tree/dist/overseer-1.2.3-${p#*:}.exe"; done

"$tree/scripts/app-resources.sh" v1.2.3 >/dev/null 2>&1
check "app-resources exits 0" "$?" 0
for p in $LINUX_SERVERS $DARWIN_SERVERS; do
  check "app-resources places ${p%%:*}" "$(cat "$tree/app/src-tauri/resources/server/${p%%:*}/overseer" 2>/dev/null)" "bin-${p%%:*}"
done
check "app-resources makes the daemon executable" "$([ -x "$tree/app/src-tauri/resources/server/linux-x86_64/overseer" ] && echo yes)" yes

for p in $WINDOWS_SERVERS; do
  "$tree/scripts/app-resources.sh" 1.2.3 "${p#*:}" >/dev/null 2>&1
  check "app-resources places ${p%%:*}" "$(cat "$tree/app/src-tauri/resources/server/${p%%:*}/overseer.exe" 2>/dev/null)" "exe-${p%%:*}"
done

"$tree/scripts/app-resources.sh" 1.2.3 x86_64-unknown-linux-musl >/dev/null 2>&1
check "app-resources rejects a non-Windows target" "$?" 2

rm -f "$tree/dist/overseer-1.2.3-$WINDOWS_ARM64.exe"
"$tree/scripts/app-resources.sh" 1.2.3 "$WINDOWS_ARM64" >/dev/null 2>&1
check "app-resources fails without the Windows daemon" "$?" 1

rm -f "$tree/dist/overseer-1.2.3-$LINUX_ARM64"
rm -rf "$tree/app"
"$tree/scripts/app-resources.sh" 1.2.3 >/dev/null 2>&1
check "app-resources skips a missing Linux daemon" "$?:$([ -e "$tree/app/src-tauri/resources/server/linux-aarch64" ] && echo present || echo absent)" "0:absent"

exit $fail
