#!/bin/sh
set -eu
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <version> [windows-target-triple]" >&2
  exit 2
fi
v=${1#v}
. "$(dirname "$0")/platforms.sh"
cd "$(dirname "$0")/.."
for p in $LINUX_SERVERS $DARWIN_SERVERS; do
  f=dist/overseer-$v-${p#*:}
  d=app/src-tauri/resources/server/${p%%:*}
  if [ -f "$f" ]; then
    mkdir -p "$d"
    cp "$f" "$d/overseer"
    chmod 755 "$d/overseer"
  fi
done
if [ $# -eq 2 ]; then
  d=
  for p in $WINDOWS_SERVERS; do
    [ "${p#*:}" = "$2" ] && d=${p%%:*}
  done
  [ -n "$d" ] || { echo "$0: not a Windows target: $2" >&2; exit 2; }
  f=dist/overseer-$v-$2.exe
  if [ ! -f "$f" ]; then
    echo "$0: missing $f, the Windows app would ship without its daemon" >&2
    exit 1
  fi
  mkdir -p "app/src-tauri/resources/server/$d"
  cp "$f" "app/src-tauri/resources/server/$d/overseer.exe"
fi
