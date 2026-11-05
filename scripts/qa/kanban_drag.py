import os
import sys
from playwright.sync_api import sync_playwright

from drive import launch, open_app

MOVES = int(os.environ.get("DRAG_MOVES", "400"))
LAYOUT_BUDGET = int(os.environ.get("DRAG_LAYOUT_BUDGET", str(MOVES // 10)))


def metrics(cdp):
    return {m["name"]: m["value"] for m in cdp.send("Performance.getMetrics")["metrics"]}


def drag(pg, src_col="pending", dst_col="completed"):
    pg.locator("text=Tasks >> visible=true").first.click()
    pg.wait_for_timeout(1000)
    rail = pg.locator(f"section.col.{dst_col}.collapsed button.rail")
    if rail.count():
        rail.click()
        pg.wait_for_timeout(300)
    pg.locator(".kanban header select").select_option("")
    pg.wait_for_timeout(300)
    for _ in range(6):
        pg.eval_on_selector_all("section.col .list", "ls => ls.forEach((l) => (l.scrollTop = l.scrollHeight))")
        pg.wait_for_timeout(200)
    pg.eval_on_selector_all("section.col .list", "ls => ls.forEach((l) => (l.scrollTop = 0))")
    pg.wait_for_timeout(200)
    src = pg.locator(f"section.col.{src_col} [data-task]").first
    tid = src.get_attribute("data-task")
    sb = src.bounding_box()
    tb = pg.locator(f"section.col.{dst_col}").bounding_box()
    cdp = pg.context.new_cdp_session(pg)
    cdp.send("Performance.enable")
    cdp.send("Emulation.setCPUThrottlingRate", {"rate": 4})
    m0 = metrics(cdp)
    pg.mouse.move(sb["x"] + 40, sb["y"] + 20)
    pg.mouse.down()
    pg.mouse.move(tb["x"] + 60, tb["y"] + 200, steps=MOVES // 2)
    for i in range(MOVES // 2):
        pg.mouse.move(tb["x"] + 60, tb["y"] + 200 + i % 300)
    pg.mouse.up()
    m1 = metrics(cdp)
    cdp.send("Emulation.setCPUThrottlingRate", {"rate": 1})
    pg.wait_for_timeout(1500)
    landed = pg.eval_on_selector(f'[data-task="{tid}"]', "(e, c) => e.closest('section.col').classList.contains(c)", dst_col)
    cards = pg.eval_on_selector_all("section.col [data-task]", "els => els.length")
    return {k: round(m1[k] - m0[k], 3) for k in ["LayoutCount", "RecalcStyleCount", "LayoutDuration", "RecalcStyleDuration", "ScriptDuration"]}, landed, cards, tid


if __name__ == "__main__":
    query = sys.argv[1] if len(sys.argv) > 1 else "?theme=light"
    src = sys.argv[2] if len(sys.argv) > 2 else "pending"
    dst = sys.argv[3] if len(sys.argv) > 3 else "completed"
    with sync_playwright() as p:
        b, pg, errors = launch(p)
        open_app(pg, query)
        work, landed, cards, tid = drag(pg, src, dst)
        b.close()
    print(f"{cards} cards rendered, dragged {tid} {src} -> {dst} over {MOVES} moves: {work}")
    ok = landed and work["LayoutCount"] <= LAYOUT_BUDGET and not errors
    if not landed:
        print(f"FAIL: card did not land in {dst}")
    if work["LayoutCount"] > LAYOUT_BUDGET:
        print(f"FAIL: {work['LayoutCount']} layouts, budget {LAYOUT_BUDGET}")
    for e in errors:
        print(f"FAIL: {e}")
    sys.exit(0 if ok else 1)
