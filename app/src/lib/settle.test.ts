import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { settle } from './settle';

describe('settle', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('runs once after a burst of sizes settles', () => {
    const fn = vi.fn();
    const s = settle(fn, 100);
    for (let h = 844; h >= 544; h -= 20) {
      s.push(`390x${h}`);
      vi.advanceTimersByTime(16);
    }
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('skips a size that ends where it was last applied', () => {
    const fn = vi.fn();
    const s = settle(fn, 100);
    s.push('390x844');
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
    s.push('390x790');
    s.push('390x844');
    vi.advanceTimersByTime(100);
    s.push('390x844');
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
    s.push('390x544');
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('cancel drops the pending run', () => {
    const fn = vi.fn();
    const s = settle(fn, 100);
    s.push('a');
    s.cancel();
    vi.advanceTimersByTime(200);
    expect(fn).not.toHaveBeenCalled();
  });
});
