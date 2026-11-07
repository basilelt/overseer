# Overseer showreel

Source of the 15-second showreel and of its page, https://basilelt.github.io/overseer/, which GitHub Pages serves from the `gh-pages` branch.

- `reel.html` is the reel itself, drawn on a canvas from code. Every frame is a pure function of time, cut to a 128 BPM grid (15 s = 8 bars). Open it in a browser to play it live: space pauses, the arrow keys step one frame.
- `audio.mjs` synthesizes the soundtrack from code, with no samples.
- `render.py` renders `reel.html` frame by frame in headless Chromium (Playwright) with 16-sample motion blur, then encodes with ffmpeg.
- `index.html` is the page, `poster.jpg` its poster and link preview.

The rendered videos (`overseer-showreel.mp4` and `overseer-showreel.webm`, about 13 MB together) are build outputs: they live on `gh-pages` only.

Rebuild from this directory (Node, Python with Playwright, ffmpeg):

    node audio.mjs reel.wav
    python3 render.py --out overseer-showreel.mp4 --webm overseer-showreel.webm --audio reel.wav --samples 16 --jobs 5 --poster poster.jpg --poster-t 14.2

Preview while editing: `python3 render.py --sheet sheet.jpg --every 0.25` (contact sheet) or `--stills 1.5,7.25 --dir stills`.

Publish: copy this directory and the two videos to the root of `gh-pages` (leave out `reel.wav`), commit and push. Pages redeploys on push.

The screenshots in `shots/` are copies of `app/docs/screenshots/`. Fonts: Inter and JetBrains Mono, under the SIL Open Font License (`fonts/`).
