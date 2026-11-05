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

  it('falls back to a short timer without requestIdleCallback', () => {
    const h = { setTimeout: vi.fn(() => 1), clearTimeout: vi.fn() };
    whenIdle(() => {}, 5000, h);
    expect(h.setTimeout).toHaveBeenCalledWith(expect.any(Function), 1500);
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
