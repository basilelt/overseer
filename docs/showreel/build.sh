#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
cut="${1:-all}"
jobs="${JOBS:-$(nproc)}"
samples="${SAMPLES:-16}"
py="${PYTHON:-python3}"

case "$cut" in
  15|45|all) ;;
  *) echo "usage: build.sh [15|45|all]" >&2; exit 1 ;;
esac

if [[ "$cut" != 45 ]]; then
  node audio.mjs reel.wav
  "$py" render.py --out overseer-showreel.mp4 --webm overseer-showreel.webm --audio reel.wav \
    --samples "$samples" --jobs "$jobs" --poster poster.jpg --poster-t 14.65
fi
if [[ "$cut" != 15 ]]; then
  node audio.mjs reel45.wav 45
  "$py" render.py --page "reel.html?d=45" --out overseer-showreel-45s.mp4 --webm overseer-showreel-45s.webm \
    --audio reel45.wav --samples "$samples" --jobs "$jobs"
fi
