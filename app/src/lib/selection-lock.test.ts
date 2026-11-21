import { afterEach, describe, expect, it, vi } from 'vitest';
import { lockSelection } from './selection-lock';

type Listener = (e: { preventDefault: () => void }) => void;

function fakeDoc() {
  const listeners = new Map<string, Set<Listener>>();
  const ranges = { removeAllRanges: vi.fn() };
  const doc = {
    addEventListener: (t: string, l: Listener) => void (listeners.get(t) ?? listeners.set(t, new Set()).get(t)!).add(l),
    removeEventListener: (t: string, l: Listener) => void listeners.get(t)?.delete(l),
    getSelection: () => ranges,
  };
  const fire = (t: string) => {
    const e = { preventDefault: vi.fn() };
    listeners.get(t)?.forEach((l) => l(e));
    return e;
  };
  return { doc, fire, ranges, count: (t: string) => listeners.get(t)?.size ?? 0 };
}

afterEach(() => vi.unstubAllGlobals());

describe('lockSelection', () => {
  it('cancels text selection starts and clears an existing selection while held', () => {
    const f = fakeDoc();
    vi.stubGlobal('document', f.doc);
    lockSelection();
    expect(f.ranges.removeAllRanges).toHaveBeenCalledTimes(1);
    expect(f.fire('selectstart').preventDefault).toHaveBeenCalled();
  });

  it('stops cancelling once released, and releasing twice is harmless', () => {
    const f = fakeDoc();
    vi.stubGlobal('document', f.doc);
    const release = lockSelection();
    release();
    release();
    expect(f.count('selectstart')).toBe(0);
    expect(f.fire('selectstart').preventDefault).not.toHaveBeenCalled();
  });

  it('works without a document', () => {
    vi.stubGlobal('document', undefined);
    expect(() => lockSelection()()).not.toThrow();
  });
});
