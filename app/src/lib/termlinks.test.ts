import { describe, expect, it, vi } from 'vitest';
import { pathRanges, routeTermUrls, type CellLine } from './termlinks';

function line(cells: number[]): CellLine {
  return { length: cells.length, getCell: (x) => (x < cells.length ? { getCodepoint: () => cells[x] } : undefined) };
}

const ascii = (s: string) => line([...s].map((c) => c.codePointAt(0)!));

describe('terminal path links', () => {
  it('maps paths to inclusive cell ranges', () => {
    expect(pathRanges(ascii('see ./out/shot.png:12:3 and docs/a.md'))).toEqual([
      { path: './out/shot.png', x0: 4, x1: 22 },
      { path: 'docs/a.md', x0: 28, x1: 36 },
    ]);
  });

  it('keeps columns right after wide and astral characters', () => {
    const cells = [0x1f600, 0, 0x20, ...[...'a.png'].map((c) => c.codePointAt(0)!)];
    expect(pathRanges(line(cells))).toEqual([{ path: 'a.png', x0: 3, x1: 7 }]);
  });

  it('finds nothing in plain text or empty cells', () => {
    expect(pathRanges(ascii('hello world'))).toEqual([]);
    expect(pathRanges(line([0, 0, 0]))).toEqual([]);
  });
});

describe('terminal URL routing', () => {
  function fakeWindow() {
    let down: ((e: Event) => void) | null = null;
    const open = vi.fn((..._: unknown[]) => null);
    const w = {
      open,
      addEventListener: (_: string, f: (e: Event) => void) => void (down = f),
      removeEventListener: () => void (down = null),
    } as unknown as Window;
    const press = (termId: string | null, shiftKey = false) => down?.({ shiftKey, target: { closest: () => (termId ? { getAttribute: () => termId } : null) } } as unknown as Event);
    return { w, open, press };
  }

  it('sends a URL opened right after a click in a terminal to that terminal’s session', () => {
    const { w, open, press } = fakeWindow();
    let now = 1000;
    const route = vi.fn(() => true);
    const stop = routeTermUrls(w, route, () => now);
    press('t1');
    now += 200;
    w.open('https://a.dev/', '_blank', 'noopener');
    expect(route).toHaveBeenCalledWith('https://a.dev/', 't1', false);
    expect(open).not.toHaveBeenCalled();
    w.open('https://b.dev/', '_blank');
    expect(open).toHaveBeenCalledTimes(1);
    stop();
    expect(w.open).toBe(open);
  });

  it('leaves other window.open calls to the browser', () => {
    const { w, open, press } = fakeWindow();
    let now = 0;
    const route = vi.fn(() => false);
    routeTermUrls(w, route, () => now);
    press(null);
    w.open('https://a.dev/');
    press('t1');
    now += 5000;
    w.open('https://a.dev/');
    press('t1');
    w.open('/?window=w-2', '_blank');
    expect(route).not.toHaveBeenCalled();
    w.open('https://a.dev/');
    expect(route).toHaveBeenCalledTimes(1);
    expect(open).toHaveBeenCalledTimes(4);
  });

  it('passes Shift from the click so the link picker can open', () => {
    const { w, press } = fakeWindow();
    const route = vi.fn(() => true);
    routeTermUrls(w, route, () => 0);
    press('t1', true);
    w.open('https://a.dev/');
    expect(route).toHaveBeenCalledWith('https://a.dev/', 't1', true);
  });
});
