# Overseer showreel

The showreel and its page, https://basilelt.github.io/overseer/ (the repository's website). `.github/workflows/pages.yml` renders the videos and deploys this directory to GitHub Pages on every push to `next` that changes it. The reel comes in two cuts: 30 s, the default on the page, and a faster 15 s.

- `reel.html` is the reel itself, drawn on a canvas from code. Every frame is a pure function of time on a 128 BPM grid (15 s = 8 bars). `reel.html?d=30` plays the 30 s cut, where every event happens at twice its time. Open it in a browser to play it live: space pauses, the arrow keys step one frame.
- `audio.mjs` synthesizes the soundtrack from code, with no samples.
- `render.py` renders `reel.html` frame by frame in headless Chromium (Playwright) with 16-sample motion blur, then encodes with ffmpeg.
- `build.sh` builds the soundtracks and renders a cut: `build.sh 15`, `build.sh 30` or `build.sh` for both. It is what CI runs.
- `index.html` is the page and `poster.jpg` its poster and link preview. `overseer-showreel-30s.mp4`/`.webm` and `overseer-showreel.mp4`/`.webm` are the rendered cuts. CI renders all five; they are not committed.

Render locally with `./build.sh` (Node, Python with Playwright, ffmpeg; about 25 minutes on 6 cores). `JOBS` sets the number of browser workers (default: every core), `SAMPLES` the motion blur samples per frame (default 16), `PYTHON` the interpreter that has Playwright.

Preview while editing: `python3 render.py --sheet sheet.jpg --every 0.25` for a contact sheet, or `--stills 1.5,7.25 --dir stills`; add `--page "reel.html?d=30"` for the 30 s cut.

Publish: merge the change into `next`; the Pages workflow renders both cuts (cached when the reel's sources did not change) and deploys this directory with them. Actions > Pages > Run workflow redeploys it by hand. Run it on another branch to render without deploying: the cuts are in the run's `showreel-15` and `showreel-30` artifacts.

When to remake it: [REGENERATE.md](REGENERATE.md). Agents ask the user first, and it is never redone without a yes.

The reel uses no screenshots: every app view in it (quorum votes, review map, live review, hosts and subjects, the task type dialog, sticky notes, recording a bug, installing an agent) is drawn in code in `reel.html` after the app's real look, with the wording of `app/src/lib/i18n/en.ts`.

Fonts: Inter and JetBrains Mono, under the SIL Open Font License (`fonts/`).
