import collections, os, sys
from playwright.sync_api import sync_playwright

url = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:5173/bench/term.html?lines=20000"
with sync_playwright() as p:
    b = p.chromium.launch(executable_path=os.environ.get("QA_CHROME") or None)
    pg = b.new_page(viewport={"width": 1440, "height": 900})
    cdp = pg.context.new_cdp_session(pg)
    cdp.send("Profiler.enable")
    cdp.send("Profiler.setSamplingInterval", {"interval": 200})
    pg.goto(url)
    pg.wait_for_function("window.stats && window.stats.paints > 0", timeout=30000)
    cdp.send("Profiler.start")
    pg.wait_for_function("window.stats.done", timeout=600000)
    prof = cdp.send("Profiler.stop")["profile"]
    stats = pg.evaluate("window.stats")
    gpu = pg.evaluate("(() => { const c = document.createElement('canvas').getContext('webgl2'); const e = c && c.getExtension('WEBGL_debug_renderer_info'); return c ? (e ? c.getParameter(e.UNMASKED_RENDERER_WEBGL) : 'webgl2') : 'none'; })()")
    canvases = pg.evaluate("document.querySelectorAll('#t canvas').length")
    b.close()

nodes = {n["id"]: n for n in prof["nodes"]}
self_us = collections.Counter()
for sid, d in zip(prof["samples"], prof["timeDeltas"]):
    self_us[sid] += d
canvas2d = {"fillText", "fillRect", "stroke", "measureText", "beginPath", "moveTo", "lineTo", "drawImage"}
buckets, fns = collections.Counter(), collections.Counter()
for nid, us in self_us.items():
    cf = nodes[nid]["callFrame"]
    name, src = cf["functionName"] or "(anon)", cf["url"]
    ms = us / 1000
    fns[f"{name} {src.rsplit('/', 1)[-1].split('?')[0]}:{cf['lineNumber']}"] += ms
    if name.startswith("("):
        buckets[name] += ms
    elif src.startswith("wasm") or "wasm" in src:
        buckets["wasm (VT parser)"] += ms
    elif name in canvas2d:
        buckets["canvas 2D native"] += ms
    elif "ghostty-web" in src:
        buckets["ghostty-web JS"] += ms
    else:
        buckets["app JS"] += ms
total = sum(buckets.values())
print(f"renderer: {'webgl overlay' if canvases > 1 else 'canvas 2D'} | GL device: {gpu}")
print(f"wall {stats['wall']:.0f} ms | render JS {stats['render']:.0f} ms | write {stats['write']:.0f} ms | {stats['paints']} paints | {stats['bytes'] / 1e6:.1f} MB")
for k, v in buckets.most_common():
    print(f"  {k:20s} {v:8.0f} ms {v / total * 100:5.1f}%")
print("top self time:")
for k, v in fns.most_common(12):
    print(f"  {v:8.0f} ms  {k}")
