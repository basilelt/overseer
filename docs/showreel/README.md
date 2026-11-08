# Overseer showreel

The showreel and its page, https://basilelt.github.io/overseer/ (the repository's website). `.github/workflows/pages.yml` deploys this directory to GitHub Pages on every push to `next` that changes it. The reel comes in two cuts: 30 s, the default on the page, and a faster 15 s.

- `reel.html` is the reel itself, drawn on a canvas from code. Every frame is a pure function of time on a 128 BPM grid (15 s = 8 bars). `reel.html?d=30` plays the 30 s cut, where every event happens at twice its time. Open it in a browser to play it live: space pauses, the arrow keys step one frame.
- `audio.mjs` synthesizes the soundtrack from code, with no samples.
- `render.py` renders `reel.html` frame by frame in headless Chromium (Playwright) with 16-sample motion blur, then encodes with ffmpeg.
- `index.html` is the page and `poster.jpg` its poster and link preview. `overseer-showreel-30s.mp4`/`.webm` and `overseer-showreel.mp4`/`.webm` are the rendered cuts.

Rebuild from this directory (Node, Python with Playwright, ffmpeg; about 25 minutes of rendering on 6 cores):

    node audio.mjs reel.wav
    node audio.mjs reel30.wav 30
    python3 render.py --out overseer-showreel.mp4 --webm overseer-showreel.webm --audio reel.wav --samples 16 --jobs 5 --poster poster.jpg --poster-t 14.2
    python3 render.py --page "reel.html?d=30" --out overseer-showreel-30s.mp4 --webm overseer-showreel-30s.webm --audio reel30.wav --samples 16 --jobs 5

Preview while editing: `python3 render.py --sheet sheet.jpg --every 0.25` for a contact sheet, or `--stills 1.5,7.25 --dir stills`; add `--page "reel.html?d=30"` for the 30 s cut.

Publish: merge the change into `next`, and the Pages workflow deploys this directory as committed. Actions > Pages > Run workflow redeploys it by hand.

When to remake it: [REGENERATE.md](REGENERATE.md). Agents ask the user first, and it is never redone without a yes.

The screenshots in `shots/` are copies of `app/docs/screenshots/`. Fonts: Inter and JetBrains Mono, under the SIL Open Font License (`fonts/`).
