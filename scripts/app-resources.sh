#!/bin/sh
set -eu
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <version> [windows-target-triple]" >&2
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
if [ $# -eq 2 ]; then
  case $2 in
    x86_64-pc-windows-msvc) d=windows-x86_64 ;;
    aarch64-pc-windows-msvc) d=windows-aarch64 ;;
    *) echo "$0: not a Windows target: $2" >&2; exit 2 ;;
  esac
  f=dist/overseer-$v-$2.exe
  if [ ! -f "$f" ]; then
    echo "$0: missing $f, the Windows app would ship without its daemon" >&2
    exit 1
  fi
  mkdir -p "app/src-tauri/resources/server/$d"
  cp "$f" "app/src-tauri/resources/server/$d/overseer.exe"
fi
