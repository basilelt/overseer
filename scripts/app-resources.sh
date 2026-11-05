#!/bin/sh
set -eu
if [ $# -ne 1 ]; then
  echo "usage: $0 <version>" >&2
  exit 2
fi
v=${1#v}
cd "$(dirname "$0")/.."
for t in x86_64-unknown-linux-musl:linux-x86_64 aarch64-unknown-linux-musl:linux-aarch64 \
  x86_64-apple-darwin:darwin-x86_64 aarch64-apple-darwin:darwin-aarch64; do
  f=dist/overseer-$v-${t%%:*}
  d=app/src-tauri/resources/server/${t#*:}
  if [ -f "$f" ]; then
    mkdir -p "$d"
    cp "$f" "$d/overseer"
    chmod 755 "$d/overseer"
  fi
done
