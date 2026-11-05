import { afterEach, describe, expect, it, vi } from 'vitest';
import { chunks, scrollParent, watchNear } from './window';

describe('chunks', () => {
  it('splits rows into fixed-size chunks with summed heights', () => {
    const c = chunks(120, 50, (i) => (i % 10 === 0 ? 28 : 20));
    expect(c.map((x) => [x.start, x.end])).toEqual([
      [0, 50],
      [50, 100],
      [100, 120],
    ]);
    expect(c[0].height).toBe(5 * 28 + 45 * 20);
    expect(c[2].height).toBe(2 * 28 + 18 * 20);
  });

  it('returns no chunks for no rows', () => {
    expect(chunks(0, 50, () => 20)).toEqual([]);
  });
});

type Fake = { parentElement: Fake | null; style: Record<string, string>; scrollHeight: number; clientHeight: number };
const node = (parent: Fake | null, overflowY = 'visible', scrollHeight = 0, clientHeight = 0): Fake => ({ parentElement: parent, style: { overflowY }, scrollHeight, clientHeight });

describe('scrollParent', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('skips ancestors that only scroll horizontally and finds the vertical scroller', () => {
    vi.stubGlobal('getComputedStyle', (el: Fake) => el.style);
    const scroller = node(null, 'auto', 5000, 800);
    const grid = node(node(scroller), 'auto', 400, 400);
    const chunk = node(grid);
    expect(scrollParent(chunk as unknown as Element)).toBe(scroller);
  });

  it('returns null when nothing scrolls', () => {
    vi.stubGlobal('getComputedStyle', (el: Fake) => el.style);
    expect(scrollParent(node(node(null)) as unknown as Element)).toBeNull();
  });
});

describe('watchNear', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('reports near at once without IntersectionObserver', () => {
    const cb = vi.fn();
    watchNear(node(null) as unknown as Element, cb);
    expect(cb).toHaveBeenCalledWith(true);
  });

  it('shares one observer per scroller and stops on cleanup', () => {
    vi.stubGlobal('getComputedStyle', (el: Fake) => el.style);
    vi.stubGlobal('document', { documentElement: node(null) });
    const made: { cb: (e: { target: unknown; isIntersecting: boolean }[]) => void; opts: IntersectionObserverInit; seen: Set<unknown> }[] = [];
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        seen = new Set<unknown>();
        constructor(cb: (e: { target: unknown; isIntersecting: boolean }[]) => void, opts: IntersectionObserverInit) {
          made.push({ cb, opts, seen: this.seen });
        }
        observe(el: unknown) {
          this.seen.add(el);
        }
        unobserve(el: unknown) {
          this.seen.delete(el);
        }
      },
    );
    const scroller = node(null, 'auto', 5000, 800);
    const a = node(scroller);
    const b = node(scroller);
    const ca = vi.fn();
    const cb = vi.fn();
    const stopA = watchNear(a as unknown as Element, ca);
    watchNear(b as unknown as Element, cb);
    expect(made).toHaveLength(1);
    expect(made[0].opts.root).toBe(scroller);
    made[0].cb([
      { target: a, isIntersecting: true },
      { target: b, isIntersecting: false },
    ]);
    expect(ca).toHaveBeenCalledWith(true);
    expect(cb).toHaveBeenCalledWith(false);
    stopA();
    expect(made[0].seen.has(a)).toBe(false);
    made[0].cb([{ target: a, isIntersecting: false }]);
    expect(ca).toHaveBeenCalledTimes(1);
  });
});
