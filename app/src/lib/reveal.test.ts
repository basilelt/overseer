import { afterEach, describe, expect, it, vi } from 'vitest';
import { reveal, REVEAL_MARGIN } from './reveal';

class FakeIO {
  static last: FakeIO | null = null;
  observed: Element[] = [];
  disconnected = false;
  constructor(
    public cb: (entries: { isIntersecting: boolean }[]) => void,
    public opts: { root: Element | null; rootMargin: string },
  ) {
    FakeIO.last = this;
  }
  observe(el: Element) {
    this.observed.push(el);
  }
  disconnect() {
    this.disconnected = true;
  }
}

afterEach(() => vi.unstubAllGlobals());

describe('reveal', () => {
  it('watches the sentinel inside its scrolling list and calls the latest handler', () => {
    vi.stubGlobal('IntersectionObserver', FakeIO);
    const list = {} as Element;
    const node = { parentElement: list } as unknown as Element;
    const first = vi.fn();
    const second = vi.fn();
    const r = reveal(node, first);
    const io = FakeIO.last!;
    expect(io.opts).toEqual({ root: list, rootMargin: REVEAL_MARGIN });
    expect(io.observed).toEqual([node]);
    io.cb([{ isIntersecting: false }]);
    expect(first).not.toHaveBeenCalled();
    io.cb([{ isIntersecting: true }]);
    expect(first).toHaveBeenCalledOnce();
    r.update(second);
    io.cb([{ isIntersecting: true }]);
    expect(second).toHaveBeenCalledOnce();
    r.destroy();
    expect(io.disconnected).toBe(true);
  });

  it('shows everything when IntersectionObserver is missing', async () => {
    vi.stubGlobal('IntersectionObserver', undefined);
    const cb = vi.fn();
    reveal({ parentElement: null } as unknown as Element, cb);
    await Promise.resolve();
    expect(cb).toHaveBeenCalledOnce();
  });
});
