#!/bin/sh
set -eu
[ $# -ge 3 ] || { echo "usage: $0 X Y FILE... (drags FILE from a native GTK source onto X,Y of the sandbox desktop app)" >&2; exit 2; }
R=$(cd "${0%/*}/../.." && pwd)
D=${QA_DIR:-/tmp/sbqa}
x=$1 y=$2
shift 2
export DISPLAY=":$("$R/scripts/qa/sandbox.sh" display)"
src=$D/dragsrc
if [ ! -x "$src" ] || [ "$R/scripts/qa/dragsrc.c" -nt "$src" ]; then
  gcc -o "$src" "$R/scripts/qa/dragsrc.c" $(pkg-config --cflags --libs gtk+-3.0)
fi
"$src" "$@" &
pid=$!
sleep 1.5
xdotool mousemove 40 40 mousedown 1
i=0
while [ $i -le 20 ]; do
  xdotool mousemove $((40 + (x - 40) * i / 20)) $((40 + (y - 40) * i / 20))
  i=$((i + 1))
  sleep 0.05
done
sleep 0.5
"$R/scripts/qa/sandbox.sh" shot drop-hover >/dev/null
xdotool mouseup 1
sleep 1
kill "$pid" 2>/dev/null || true
