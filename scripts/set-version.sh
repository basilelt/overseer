#!/bin/sh
set -eu
v=${1:-}
v=${v#v}
case $v in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) echo "usage: $0 X.Y.Z" >&2; exit 2 ;;
esac
here=$(dirname "$0")
. "$here/version-files.sh"
cd "$here/.."

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

for f in $VERSION_MANIFESTS $VERSION_LOCKS; do
  toml "$f"
done
for f in $VERSION_JSONS; do
  json "$f"
done
echo "version $v"
