import { describe, expect, it } from 'vitest';
import { fitWindow, MIN_TAB_PX, OVERSCAN, visibleRange, windowSpan, type TabWindow } from './tab-window';

const NONE: TabWindow = { from: 0, to: 0 };

describe('windowSpan', () => {
  it('covers the widest strip the screen can show, plus a margin on both sides', () => {
    expect(windowSpan(1440)).toBe(1440 / MIN_TAB_PX + 2 * OVERSCAN);
    expect(windowSpan(3840)).toBe(3840 / MIN_TAB_PX + 2 * OVERSCAN);
    expect(windowSpan(0)).toBe(1 + 2 * OVERSCAN);
  });
});

describe('fitWindow', () => {
  it('mounts every tab while they fit in the span', () => {
    expect(fitWindow(NONE, 0, 40, 0)).toEqual({ from: 0, to: 0 });
    expect(fitWindow(NONE, 12, 40, 5)).toEqual({ from: 0, to: 12 });
    expect(fitWindow(NONE, 40, 40, 39)).toEqual({ from: 0, to: 40 });
  });

  it('mounts a span of tabs around the wanted one', () => {
    expect(fitWindow(NONE, 500, 40, 250)).toEqual({ from: 231, to: 271 });
    expect(fitWindow(NONE, 500, 40, 0)).toEqual({ from: 0, to: 40 });
    expect(fitWindow(NONE, 500, 40, 499)).toEqual({ from: 460, to: 500 });
  });

  it('never mounts more than the span, for any wanted range', () => {
    for (const first of [-5, 0, 1, 37, 250, 480, 499, 900]) {
      const w = fitWindow(NONE, 500, 40, first, first + 9);
      expect(w.to - w.from).toBe(40);
      expect(w.from).toBeGreaterThanOrEqual(0);
      expect(w.to).toBeLessThanOrEqual(500);
      const at = Math.max(0, Math.min(first, 499));
      expect(w.from <= at && at < w.to).toBe(true);
    }
  });

  it('returns the same window while the wanted range stays away from its edges', () => {
    const w = fitWindow(NONE, 500, 40, 250);
    expect(fitWindow(w, 500, 40, 240)).toBe(w);
    expect(fitWindow(w, 500, 40, 236, 262)).toBe(w);
    expect(fitWindow(w, 500, 40, w.from + 3)).toBe(w);
    expect(fitWindow(w, 500, 40, w.to - 4)).toBe(w);
  });

  it('moves when the wanted range reaches an edge', () => {
    const w = fitWindow(NONE, 500, 40, 250);
    expect(fitWindow(w, 500, 40, w.from + 2)).toEqual({ from: 214, to: 254 });
    expect(fitWindow(w, 500, 40, w.to - 3)).toEqual({ from: 249, to: 289 });
    expect(fitWindow(w, 500, 40, 10)).toEqual({ from: 0, to: 40 });
  });

  it('stays at the start or the end of the list when the wanted tab is the first or the last', () => {
    const start = fitWindow(NONE, 500, 40, 0);
    expect(fitWindow(start, 500, 40, 0)).toBe(start);
    expect(fitWindow(start, 500, 40, 1)).toBe(start);
    const end = fitWindow(NONE, 500, 40, 499);
    expect(fitWindow(end, 500, 40, 499)).toBe(end);
  });

  it('refits when tabs are closed under the window or the list becomes short', () => {
    const w = fitWindow(NONE, 500, 40, 499);
    expect(fitWindow(w, 480, 40, 479)).toEqual({ from: 440, to: 480 });
    expect(fitWindow(w, 30, 40, 29)).toEqual({ from: 0, to: 30 });
    const all = fitWindow(NONE, 30, 40, 3);
    expect(fitWindow(all, 30, 40, 20)).toBe(all);
  });
});

describe('visibleRange', () => {
  const win = { from: 100, to: 140 };
  const rects = Array.from({ length: 40 }, (_, j) => ({ left: 1000 + j * 100, right: 1100 + j * 100 }));

  it('names the first and last mounted tab that show in the view', () => {
    expect(visibleRange(win, rects, { left: 1250, right: 1950 }, 0, 132, 500)).toEqual({ first: 102, last: 109 });
    expect(visibleRange(win, rects, { left: 1300, right: 1400 }, 0, 132, 500)).toEqual({ first: 103, last: 103 });
  });

  it('estimates the tab from the scroll offset when only a spacer shows', () => {
    expect(visibleRange(win, rects, { left: 0, right: 900 }, 2640, 132, 500)).toEqual({ first: 20, last: 20 });
    expect(visibleRange(win, [], { left: 0, right: 900 }, 1e9, 132, 500)).toEqual({ first: 499, last: 499 });
    expect(visibleRange(win, [], { left: 0, right: 900 }, 50, 0, 500)).toEqual({ first: 50, last: 50 });
  });
});
