import sys
from playwright.sync_api import sync_playwright

from drive import MODES, launch, open_app, shot

CHECK = """() => {
  const scrolls = (e) => { for (let p = e.parentElement; p; p = p.parentElement) if (/auto|scroll/.test(getComputedStyle(p).overflowX)) return true; return false; };
  return [...document.querySelectorAll('button, input, select, a')]
  .filter((e) => e.checkVisibility() && !scrolls(e))
  .map((e) => [e, e.getBoundingClientRect()])
  .filter(([, r]) => r.width && (r.right > innerWidth + 1 || r.left < -1))
  .map(([e, r]) => `${e.tagName.toLowerCase()} "${(e.textContent || e.getAttribute('aria-label') || '').trim().slice(0, 30)}" at ${Math.round(r.left)}..${Math.round(r.right)} of ${innerWidth}`);
}"""

SPILL = """() => [...document.querySelectorAll('*')]
  .filter((e) => {
    const cs = getComputedStyle(e);
    if (!/flex/.test(cs.display) || cs.overflowX !== 'visible' || !e.checkVisibility()) return false;
    const r = e.getBoundingClientRect();
    return r.width && [...e.children].some((c) => {
      if (/absolute|fixed/.test(getComputedStyle(c).position)) return false;
      const b = c.getBoundingClientRect();
      return b.width && (b.right > r.right + 1 || b.left < r.left - 1);
    });
  })
  .map((e) => `children spill out of ${e.tagName.toLowerCase()}.${String(e.className).split(' ')[0]} (${Math.round(e.getBoundingClientRect().width)} px wide)`)"""

VIEWS = {"agents": "Agents", "tasks": "Tasks"}

if __name__ == "__main__":
    modes = sys.argv[1:] or list(MODES)
    bad = 0
    with sync_playwright() as p:
        for mode in modes:
            b, pg, errors = launch(p, mode=mode)
            open_app(pg, "?theme=light")
            for view, label in VIEWS.items():
                tab = pg.locator(f"text={label} >> visible=true")
                if tab.count():
                    tab.first.click()
                    pg.wait_for_timeout(800)
                off = pg.evaluate(CHECK) + sorted(set(pg.evaluate(SPILL)))
                shot(pg, f"overflow-{mode}-{view}")
                for o in off:
                    print(f"FAIL {mode} {view}: {o}")
                bad += len(off)
            for e in errors:
                print(f"FAIL {mode}: {e}")
            bad += len(errors)
            b.close()
            print(f"{mode}: checked {', '.join(VIEWS)}")
    sys.exit(1 if bad else 0)
