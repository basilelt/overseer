import glob
import os
import re
from playwright.sync_api import sync_playwright

D = os.environ.get("QA_DIR", "/tmp/sbqa")
WEB = f"http://127.0.0.1:{os.environ.get('QA_WEB', '4918')}/"
SAME_ORIGIN = f"http://127.0.0.1:{os.environ.get('QA_PORT', '47811')}/"
APP = WEB
DESKTOP = {"width": 1440, "height": 900}
PHONE = {"width": 390, "height": 844}
MODES = {
    "web": ("chromium", None),
    "android": ("chromium", "Pixel 7"),
    "android-tablet": ("chromium", "Galaxy Tab S9"),
    "android-tablet-landscape": ("chromium", "Galaxy Tab S9 landscape"),
    "iphone": ("webkit", "iPhone 15"),
    "ipad": ("webkit", "iPad Pro 11"),
    "ipad-landscape": ("webkit", "iPad Pro 11 landscape"),
}


def newest_chromium():
    root = os.environ.get("PLAYWRIGHT_BROWSERS_PATH") or os.path.expanduser("~/.cache/ms-playwright")
    found = []
    for pattern in ("chromium_headless_shell-*/*/chrome-headless-shell", "chromium-*/*/chrome"):
        for path in glob.glob(os.path.join(root, pattern)):
            m = re.search(r"-(\d+)/", path)
            if m and os.access(path, os.X_OK):
                found.append((int(m.group(1)), path.endswith("chrome-headless-shell"), path))
    return max(found)[2] if found else None


def launch_chromium(p):
    args = ["--disable-web-security"]
    exe = os.environ.get("QA_CHROME")
    if exe:
        return p.chromium.launch(executable_path=exe, args=args)
    try:
        return p.chromium.launch(args=args)
    except Exception:
        fallback = newest_chromium()
        if not fallback:
            raise
        return p.chromium.launch(executable_path=fallback, args=args)


def launch(p, viewport=DESKTOP, mode=None):
    global APP
    engine, device = MODES[mode or os.environ.get("QA_MODE", "web")]
    if engine == "webkit":
        b = p.webkit.launch()
        APP = SAME_ORIGIN
    else:
        b = launch_chromium(p)
        APP = WEB
    ctx = b.new_context(**p.devices[device]) if device else b.new_context(viewport=viewport)
    ctx.add_init_script(path=f"{D}/shim.js")
    pg = ctx.new_page()
    errors = []
    pg.on("console", lambda m: errors.append(f"{m.type}: {m.text}") if m.type == "error" else None)
    pg.on("pageerror", lambda e: errors.append(f"pageerror: {e}"))
    return b, pg, errors


def open_app(pg, query="?theme=dark", hash=""):
    pg.goto(APP + query + hash)
    pg.wait_for_timeout(3500)
    for label in ["Skip", "Not now"]:
        s = pg.locator(f"text={label} >> visible=true")
        if s.count():
            s.first.click()
            pg.wait_for_timeout(300)


def shot(pg, name):
    pg.screenshot(path=f"{D}/{name}.png")
