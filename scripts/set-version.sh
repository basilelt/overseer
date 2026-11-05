#!/bin/sh
set -eu
v=${1:-}
v=${v#v}
case $v in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "usage: $0 X.Y.Z" >&2; exit 2 ;;
esac
cd "$(dirname "$0")/.."

toml() {
  awk -v v="$v" '
    /^\[\[?package]]?$/ { pkg = ($0 == "[package]"); own = 0 }
    /^name = "overseer(-api|-app)?"$/ { own = 1 }
    (pkg || own) && /^version = / { $0 = "version = \"" v "\""; pkg = 0; own = 0 }
    { print }
  ' "$1" >"$1.tmp"
  mv "$1.tmp" "$1"
}

json() {
  jq --arg v "$v" '.version = $v' "$1" >"$1.tmp"
  mv "$1.tmp" "$1"
}

for f in Cargo.toml Cargo.lock api/Cargo.toml api/Cargo.lock app/src-tauri/Cargo.toml app/src-tauri/Cargo.lock; do
  toml "$f"
done
for f in app/package.json app/src-tauri/tauri.conf.json; do
  json "$f"
done
echo "version $v"
