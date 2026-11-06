#!/bin/sh
# Fails when a file stamped by scripts/set-version.sh does not carry X.Y.Z.
set -u
v=${1:-}
v=${v#v}
case $v in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "usage: $0 X.Y.Z" >&2; exit 2 ;;
esac
cd "${CHECK_VERSION_ROOT:-$(dirname "$0")/..}" || exit 2

bad=0
report() {
  if [ "$2" != "$v" ]; then
    echo "$1: ${2:-missing}, want $v" >&2
    bad=1
  fi
}

# [package] version of a manifest.
toml() {
  awk '/^\[/ { pkg = ($0 == "[package]") } pkg && /^version = / { gsub(/^version = "|"$/, ""); print; exit }' "$1"
}

# version of each overseer crate in a lockfile, as "name version".
lock() {
  awk '/^name = "overseer(-api|-app)?"$/ { n = $3; gsub(/"/, "", n); next_v = 1; next }
    next_v && /^version = / { gsub(/^version = "|"$/, ""); print n, $0; next_v = 0 }' "$1"
}

for f in Cargo.toml api/Cargo.toml app/src-tauri/Cargo.toml; do
  report "$f" "$(toml "$f")"
done
for f in Cargo.lock api/Cargo.lock app/src-tauri/Cargo.lock; do
  entries=$(lock "$f")
  [ -n "$entries" ] || report "$f" ""
  while read -r name ver; do
    [ -n "$name" ] && report "$f ($name)" "$ver"
  done <<EOF
$entries
EOF
done
for f in app/package.json app/src-tauri/tauri.conf.json; do
  report "$f" "$(sed -n 's/^  "version": "\(.*\)",\{0,1\}$/\1/p' "$f" | head -1)"
done

if [ "$bad" -ne 0 ]; then
  echo "run scripts/set-version.sh $v and commit the result" >&2
  exit 1
fi
echo "every version is $v"
