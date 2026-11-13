import { describe, expect, it, vi } from 'vitest';
import { preload, whenIdle } from './idle';

const host = () => {
  const idle: (() => void)[] = [];
  return {
    idle,
    requestIdleCallback: vi.fn((cb: () => void) => idle.push(cb)),
    cancelIdleCallback: vi.fn(),
    setTimeout: vi.fn(),
    clearTimeout: vi.fn(),
  };
};

describe('idle preload', () => {
  it('waits for an idle period and can be cancelled', () => {
    const h = host();
    const fn = vi.fn();
    const cancel = whenIdle(fn, 3000, h);
    expect(h.requestIdleCallback).toHaveBeenCalledWith(fn, { timeout: 3000 });
    expect(fn).not.toHaveBeenCalled();
    cancel();
    expect(h.cancelIdleCallback).toHaveBeenCalled();
  });

  it('falls back to a timer without requestIdleCallback, no later than the given timeout, and cancels it', () => {
    vi.useFakeTimers();
    const h = { setTimeout: (fn: () => void, ms: number) => setTimeout(fn, ms), clearTimeout: (id: ReturnType<typeof setTimeout>) => clearTimeout(id) };
    const fn = vi.fn();
    const cancelled = vi.fn();
    whenIdle(fn, 5000, h);
    whenIdle(cancelled, 5000, h)();
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(5000);
    expect(fn).toHaveBeenCalledOnce();
    expect(cancelled).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it('loads once, on idle or on first use, and retries after a failure', async () => {
    const h = host();
    const load = vi.fn().mockRejectedValueOnce(new Error('offline')).mockResolvedValue('chunk');
    const get = preload(load, 4000, h);
    expect(load).not.toHaveBeenCalled();
    h.idle[0]();
    expect(load).toHaveBeenCalledTimes(1);
    await Promise.resolve();
    await expect(get()).resolves.toBe('chunk');
    await expect(get()).resolves.toBe('chunk');
    expect(load).toHaveBeenCalledTimes(2);
  });
});
