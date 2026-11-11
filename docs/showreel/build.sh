#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
cut="${1:-all}"
jobs="${JOBS:-$(nproc)}"
samples="${SAMPLES:-16}"
py="${PYTHON:-python3}"

case "$cut" in
  15|30|all) ;;
  *) echo "usage: build.sh [15|30|all]" >&2; exit 1 ;;
esac

if [[ "$cut" != 30 ]]; then
  node audio.mjs reel.wav
  "$py" render.py --out overseer-showreel.mp4 --webm overseer-showreel.webm --audio reel.wav \
    --samples "$samples" --jobs "$jobs" --poster poster.jpg --poster-t 14.2
fi
if [[ "$cut" != 15 ]]; then
  node audio.mjs reel30.wav 30
  "$py" render.py --page "reel.html?d=30" --out overseer-showreel-30s.mp4 --webm overseer-showreel-30s.webm \
    --audio reel30.wav --samples "$samples" --jobs "$jobs"
fi
