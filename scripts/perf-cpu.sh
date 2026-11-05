#!/usr/bin/env bash
set -euo pipefail

bin=${1:?usage: perf-cpu.sh <app binary>}
idle_budget=${IDLE_BUDGET:-3}
stream_budget=${STREAM_BUDGET:-80}
enforce=${PERF_ENFORCE:-1}
warmup=${PERF_WARMUP:-12}
window=${PERF_WINDOW:-30}
stream_at=${PERF_STREAM_AT:-55}
hz=$(getconf CLK_TCK)
page=$(getconf PAGESIZE)

sample() {
  cat /proc/[0-9]*/stat 2>/dev/null | awk -v root="$1" -v page="$page" '
    {
      pid = $1
      line = $0
      sub(/^.*\) /, "", line)
      split(line, f, " ")
      ppid[pid] = f[2]
      ticks[pid] = f[12] + f[13]
      rss[pid] = f[22]
    }
    END {
      keep[root] = 1
      grew = 1
      while (grew) {
        grew = 0
        for (p in ppid) if (!(p in keep) && (ppid[p] in keep)) { keep[p] = 1; grew = 1 }
      }
      t = 0; r = 0; n = 0
      for (p in keep) if (p in ticks) { t += ticks[p]; r += rss[p]; n++ }
      printf "%d %d %d\n", t, r * page / 1048576, n
    }'
}

now() { date +%s.%N; }

measure() {
  local label=$1 a b t0 t1
  read -r a _ _ < <(sample "$pid")
  t0=$(now)
  sleep "$window"
  read -r b mib procs < <(sample "$pid")
  t1=$(now)
  awk -v a="$a" -v b="$b" -v t0="$t0" -v t1="$t1" -v hz="$hz" 'BEGIN { printf "%.2f\n", (b - a) / hz / (t1 - t0) * 100 }' >"$out/$label"
  echo "$label: $(cat "$out/$label") % of one core over ${window}s (tree: $procs processes, ${mib} MiB RSS)"
}

out=$(mktemp -d)
export WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 NO_AT_BRIDGE=1
"$bin" >"$out/app.log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true' EXIT

sleep "$warmup"
kill -0 "$pid" 2>/dev/null || { cat "$out/app.log"; echo "::error::app exited during warmup"; exit 1; }
measure idle
sleep $((stream_at - warmup - window > 0 ? stream_at - warmup - window : 0))
measure streaming
kill -0 "$pid" 2>/dev/null || { cat "$out/app.log"; echo "::error::app exited during the run"; exit 1; }

idle=$(cat "$out/idle")
stream=$(cat "$out/streaming")
{
  echo "### App CPU (Xvfb, mock backend, $(nproc) cores)"
  echo
  echo "| phase | CPU % of one core | budget |"
  echo "| --- | --- | --- |"
  echo "| idle, 5 terminals | $idle | < $idle_budget |"
  echo "| streaming, 1 of 5 terminals | $stream | < $stream_budget |"
} >>"${GITHUB_STEP_SUMMARY:-/dev/null}"

over=$(awk -v i="$idle" -v ib="$idle_budget" -v s="$stream" -v sb="$stream_budget" 'BEGIN { print (i >= ib || s >= sb) ? 1 : 0 }')
if [ "$over" = 1 ]; then
  echo "::error title=CPU budget::idle ${idle}% (budget ${idle_budget}%), streaming ${stream}% (budget ${stream_budget}%)"
  [ "$enforce" = 1 ] && exit 1
fi
exit 0
