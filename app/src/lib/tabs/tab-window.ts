export interface TabWindow {
  from: number;
  to: number;
}

export const MIN_TAB_PX = 60;
export const TAB_PX = 132;
export const OVERSCAN = 8;
const EDGE = 3;

export function windowSpan(viewPx: number): number {
  return Math.ceil(Math.max(viewPx, MIN_TAB_PX) / MIN_TAB_PX) + 2 * OVERSCAN;
}

function keep(prev: TabWindow, from: number, to: number): TabWindow {
  return prev.from === from && prev.to === to ? prev : { from, to };
}

// The tabs to mount out of `count`: all of them up to `span`, else `span` tabs around the range
// [first, last] that must be on screen. The window only moves when that range comes within a few
// tabs of its edge, so a click on a visible tab does not shift the strip.
export function fitWindow(prev: TabWindow, count: number, span: number, first: number, last = first): TabWindow {
  if (count <= span) return keep(prev, 0, count);
  const lo = Math.max(0, Math.min(first, count - 1));
  const hi = Math.max(lo, Math.min(last, count - 1));
  const sized = prev.to - prev.from === span && prev.to <= count;
  if (sized && (prev.from === 0 || lo >= prev.from + EDGE) && (prev.to === count || hi < prev.to - EDGE)) return prev;
  const from = Math.max(0, Math.min(count - span, Math.round((lo + hi + 1) / 2 - span / 2)));
  return keep(prev, from, from + span);
}

export interface Edge {
  left: number;
  right: number;
}

// First and last mounted tab on screen, as indexes in the whole list; when the view shows only the
// spacer of unmounted tabs (a long fling), the index its scroll offset stands for.
export function visibleRange(win: TabWindow, rects: Edge[], view: Edge, scroll: number, tabPx: number, count: number): { first: number; last: number } {
  let first = -1;
  let last = -1;
  for (let j = 0; j < rects.length; j++) {
    if (rects[j]!.right <= view.left || rects[j]!.left >= view.right) continue;
    if (first < 0) first = win.from + j;
    last = win.from + j;
  }
  if (first >= 0) return { first, last };
  const at = Math.max(0, Math.min(count - 1, Math.round(scroll / Math.max(1, tabPx))));
  return { first: at, last: at };
}
