# Overseer showreel

The page at https://basilelt.github.io/overseer/ and the 15-second reel it plays.

- `reel.html` is the reel itself, drawn on a canvas from code. Every frame is a pure function of time, cut to a 128 BPM grid (15 s = 8 bars). Open it in a browser to play it live: space pauses, the arrow keys step one frame.
- `audio.mjs` synthesizes the soundtrack from code, with no samples: `node audio.mjs reel.wav`.
- `render.py` renders `reel.html` frame by frame in headless Chromium (Playwright) with 16-sample motion blur, then encodes with ffmpeg.

Rebuild from this directory:

    node audio.mjs reel.wav
    python3 render.py --out overseer-showreel.mp4 --webm overseer-showreel.webm --audio reel.wav --samples 16 --jobs 5 --poster poster.jpg --poster-t 14.2

Preview while editing: `python3 render.py --sheet sheet.jpg --every 0.25` (contact sheet) or `--stills 1.5,7.25 --dir stills`.

The screenshots in `shots/` come from the app's mock build. Fonts: Inter and JetBrains Mono, under the SIL Open Font License (`fonts/`).
