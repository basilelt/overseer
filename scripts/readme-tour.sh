#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
chrome=${CHROME:-$(ls -d "$HOME"/.cache/ms-playwright/chromium-*/chrome-linux64/chrome 2>/dev/null | tail -1)}
[ -x "$chrome" ] || { echo "set CHROME to a Chromium binary" >&2; exit 1; }
port=${PORT:-4173}
work=$(mktemp -d)
preview=
trap '[ -n "$preview" ] && kill "$preview" 2>/dev/null; rm -rf "$work"' EXIT

(cd "$work" && npm init -y >/dev/null && npm i --silent playwright-core >/dev/null)
cd "$root/app"
VITE_MOCK=1 pnpm exec vite build >/dev/null
pnpm exec vite preview --port "$port" --strictPort >/dev/null 2>&1 &
preview=$!
sleep 2

mkdir "$work/frames"
cat > "$work/tour.mjs" <<'EOF'
import { chromium } from 'playwright-core';
const [chrome, port, out] = process.argv.slice(2);
const b = await chromium.launch({ executablePath: chrome });
const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
await p.goto(`http://localhost:${port}/?theme=dark`);
await p.waitForSelector('text=Take the tour');
let i = 0;
const shot = async () => { await p.waitForTimeout(700); await p.screenshot({ path: `${out}/${String(i++).padStart(2, '0')}.png` }); };
await shot();
for (;;) {
  await p.keyboard.press('ArrowRight');
  await p.waitForTimeout(300);
  if (!(await p.locator('text=/^\\d+ of \\d+$/').count())) break;
  await shot();
}
await b.close();
EOF
node "$work/tour.mjs" "$chrome" "$port" "$work/frames"

mkdir -p "$root/docs/media"
ffmpeg -loglevel error -y -framerate 0.4 -i "$work/frames/%02d.png" \
  -vf "scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=none" \
  -loop 0 "$root/docs/media/tour.gif"
echo "docs/media/tour.gif: $(ls "$work/frames" | wc -l) frames"
