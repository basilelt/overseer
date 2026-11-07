#!/usr/bin/env python3
import argparse
import base64
import functools
import http.server
import multiprocessing
import os
import queue
import subprocess
import sys
import threading
import time

from playwright.sync_api import sync_playwright

HERE = os.path.dirname(os.path.abspath(__file__))
GRAB = "([i, s, ty, q]) => { const a = performance.now(); const d = reel.frame(i, s, ty, q); return [d, performance.now() - a]; }"
SHEET_INIT = """([cols, w, h, label, n]) => {
  const c = document.createElement('canvas');
  c.width = cols * w; c.height = Math.ceil(n / cols) * (h + label);
  const x = c.getContext('2d');
  x.fillStyle = '#000'; x.fillRect(0, 0, c.width, c.height);
  window.__sheet = { c, x, cols, w, h, label };
}"""
SHEET_ADD = """async ([k, i, s, text]) => {
  const { x, cols, w, h, label } = window.__sheet;
  const a = performance.now();
  const im = new Image();
  im.src = reel.frame(i, s, 'image/jpeg', 0.92);
  await im.decode();
  const ms = performance.now() - a;
  const px = (k % cols) * w, py = Math.floor(k / cols) * (h + label);
  x.drawImage(im, px, py, w, h);
  x.fillStyle = '#e8a33a'; x.font = '500 18px "JetBrains Mono", monospace'; x.textBaseline = 'middle';
  x.fillText(text, px + 8, py + h + label / 2);
  return ms;
}"""


class Quiet(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def serve(root):
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Quiet, directory=root))
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd


def launch(pw):
    try:
        return pw.chromium.launch()
    except Exception as e:
        if "Executable doesn't exist" not in str(e):
            raise
        subprocess.run([sys.executable, "-m", "playwright", "install", "chromium"], check=True)
        return pw.chromium.launch()


def open_page(browser, url):
    page = browser.new_page(viewport={"width": 1920, "height": 1080}, device_scale_factor=1)
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("console", lambda m: print(f"console.{m.type}: {m.text}", file=sys.stderr) if m.type in ("error", "warning") else None)
    page.goto(url)
    try:
        page.wait_for_function("() => !!(window.reel && window.reel.ready)", timeout=30000)
        page.evaluate("() => window.reel.ready.then(() => document.fonts.ready).then(() => true)")
    except Exception as e:
        sys.exit(f"reel not ready: {str(e).splitlines()[0]}")
    if errors:
        sys.exit("page errors: " + "; ".join(errors))
    info = page.evaluate("() => ({fps: reel.fps, duration: reel.duration, width: reel.width, height: reel.height})")
    if (info["width"], info["height"]) != (1920, 1080):
        sys.exit(f"unexpected canvas size {info['width']}x{info['height']}")
    page.errors = errors
    return page, info


def grab(page, i, samples, ty="image/png", q=None):
    d, ms = page.evaluate(GRAB, [i, samples, ty, q])
    if page.errors:
        sys.exit("page errors: " + "; ".join(page.errors))
    return base64.b64decode(d[d.index(",") + 1:]), ms


def frame_range(info, a, b):
    fps, total = info["fps"], round(info["duration"] * info["fps"])
    lo = max(0, round(a * fps))
    hi = min(total, round((info["duration"] if b is None else b) * fps))
    if hi <= lo:
        sys.exit(f"empty range {a}..{b}")
    return lo, hi


def encoder(args, fps, lo, hi):
    t0, dur = lo / fps, (hi - lo) / fps
    cmd = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "image2pipe", "-framerate", str(fps), "-c:v", "png", "-i", "-"]
    if args.audio:
        cmd += ["-ss", f"{t0:.6f}", "-t", f"{dur:.6f}", "-i", args.audio]
    maps = ["-map", "0:v:0"] + (["-map", "1:a:0"] if args.audio else [])
    vf = ["-vf", "scale=out_color_matrix=bt709:out_range=tv:flags=accurate_rnd+full_chroma_int,format=yuv420p,setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv"]
    tags = ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]
    if args.out:
        cmd += maps + vf + tags + ["-c:v", "libx264", "-preset", "slow", "-crf", "18", "-profile:v", "high", "-pix_fmt", "yuv420p"]
        if args.audio:
            cmd += ["-c:a", "aac", "-b:a", "192k", "-ar", "48000"]
        cmd += ["-t", f"{dur:.6f}", "-movflags", "+faststart", args.out]
    if args.webm:
        cmd += maps + vf + tags + ["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "32", "-row-mt", "1", "-deadline", "good", "-cpu-used", "2", "-pix_fmt", "yuv420p"]
        if args.audio:
            cmd += ["-c:a", "libopus", "-b:a", "128k", "-ar", "48000"]
        cmd += ["-t", f"{dur:.6f}", args.webm]
    return subprocess.Popen(cmd, stdin=subprocess.PIPE)


def summary(label, times, wall):
    n = len(times)
    avg = sum(times) / n
    print(f"{label}: {n} frames in {wall:.1f}s wall = {wall * 1000 / n:.0f} ms/frame (in-page render+encode avg {avg:.0f} ms, min {min(times):.0f}, max {max(times):.0f})")


def worker(url, frames, samples, q):
    try:
        with sync_playwright() as pw:
            browser = launch(pw)
            page, _ = open_page(browser, url)
            for i in frames:
                q.put((i,) + grab(page, i, samples))
            browser.close()
    except BaseException as e:
        q.put((None, f"{type(e).__name__}: {e}", 0))


def take(q, proc, i):
    while True:
        try:
            k, data, ms = q.get(timeout=10)
            break
        except queue.Empty:
            if not proc.is_alive():
                raise SystemExit(f"worker for frame {i} died with code {proc.exitcode}")
    if k is None:
        raise SystemExit(f"worker failed: {data}")
    if k != i:
        raise SystemExit(f"worker sent frame {k}, expected {i}")
    return data, ms


def render_video(page, info, args, url):
    fps = info["fps"]
    lo, hi = frame_range(info, args.start, args.end)
    samples = args.samples or 6
    jobs = max(1, min(args.jobs, hi - lo))
    workers = []
    if jobs > 1:
        ctx = multiprocessing.get_context("spawn")
        queues = [ctx.Queue(maxsize=4) for _ in range(jobs)]
        workers = [ctx.Process(target=worker, args=(url, list(range(lo + k, hi, jobs)), samples, queues[k]), daemon=True) for k in range(jobs)]
        for w in workers:
            w.start()
        source = lambda i: take(queues[(i - lo) % jobs], workers[(i - lo) % jobs], i)
    else:
        source = lambda i: grab(page, i, samples)
    proc = encoder(args, fps, lo, hi)
    times, start = [], time.time()
    try:
        for i in range(lo, hi):
            png, ms = source(i)
            times.append(ms)
            try:
                proc.stdin.write(png)
            except BrokenPipeError:
                raise SystemExit(f"ffmpeg exited early with code {proc.wait()}")
            if (i - lo) % 60 == 59:
                print(f"  frame {i + 1 - lo}/{hi - lo}  {(time.time() - start) * 1000 / (i + 1 - lo):.0f} ms/frame", flush=True)
        proc.stdin.close()
        if proc.wait():
            raise SystemExit(f"ffmpeg failed with code {proc.returncode}")
    except BaseException:
        proc.kill()
        for w in workers:
            w.kill()
        raise
    for w in workers:
        w.join()
    summary(f"video samples={samples} jobs={jobs}", times, time.time() - start)


def render_poster(page, info, args):
    i = min(round(args.poster_t * info["fps"]), round(info["duration"] * info["fps"]) - 1)
    data, _ = grab(page, i, args.samples or 6, "image/jpeg", 0.9)
    with open(args.poster, "wb") as f:
        f.write(data)
    print(f"poster: {args.poster} (t={i / info['fps']:.3f})")


def render_stills(page, info, args):
    os.makedirs(args.dir, exist_ok=True)
    total = round(info["duration"] * info["fps"])
    times, start = [], time.time()
    for t in (float(x) for x in args.stills.split(",") if x.strip()):
        i = min(max(0, round(t * info["fps"])), total - 1)
        data, ms = grab(page, i, args.samples or 1)
        times.append(ms)
        path = os.path.join(args.dir, f"t{i / info['fps']:06.3f}.png")
        with open(path, "wb") as f:
            f.write(data)
        print(path)
    summary(f"stills samples={args.samples or 1}", times, time.time() - start)


def render_sheet(page, info, args):
    lo, hi = frame_range(info, args.start, args.end)
    step = max(1, round(args.every * info["fps"]))
    idx = list(range(lo, hi, step))
    page.evaluate(SHEET_INIT, [5, 384, 216, 30, len(idx)])
    times, start = [], time.time()
    for k, i in enumerate(idx):
        times.append(page.evaluate(SHEET_ADD, [k, i, args.samples or 1, f"{i / info['fps']:6.3f}s  f{i}"]))
    if page.errors:
        sys.exit("page errors: " + "; ".join(page.errors))
    d = page.evaluate("() => window.__sheet.c.toDataURL('image/jpeg', 0.9)")
    with open(args.sheet, "wb") as f:
        f.write(base64.b64decode(d[d.index(",") + 1:]))
    summary(f"sheet samples={args.samples or 1}", times, time.time() - start)
    print(f"sheet: {args.sheet} ({len(idx)} thumbs)")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--page", default="reel.html")
    p.add_argument("--out")
    p.add_argument("--webm")
    p.add_argument("--audio")
    p.add_argument("--samples", type=int)
    p.add_argument("--from", dest="start", type=float, default=0.0)
    p.add_argument("--to", dest="end", type=float)
    p.add_argument("--poster")
    p.add_argument("--poster-t", type=float, default=14.0)
    p.add_argument("--stills")
    p.add_argument("--dir", default="stills")
    p.add_argument("--sheet")
    p.add_argument("--every", type=float, default=0.25)
    p.add_argument("--jobs", type=int, default=1)
    args = p.parse_args()
    if not (args.out or args.webm or args.poster or args.stills or args.sheet):
        p.error("nothing to do: pass --out, --webm, --poster, --stills or --sheet")
    if args.audio and not os.path.exists(args.audio):
        p.error(f"audio not found: {args.audio}")
    httpd = serve(HERE)
    try:
        with sync_playwright() as pw:
            browser = launch(pw)
            url = f"http://127.0.0.1:{httpd.server_address[1]}/{args.page}?render"
            page, info = open_page(browser, url)
            if args.out or args.webm:
                render_video(page, info, args, url)
            if args.poster:
                render_poster(page, info, args)
            if args.stills:
                render_stills(page, info, args)
            if args.sheet:
                render_sheet(page, info, args)
            browser.close()
    finally:
        httpd.shutdown()


if __name__ == "__main__":
    main()
