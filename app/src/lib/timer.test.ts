import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { OneShot } from './timer';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('OneShot', () => {
  it('runs the callback once after the delay and then goes inactive', () => {
    const t = new OneShot();
    const fn = vi.fn();
    t.set(fn, 100);
    expect(t.active).toBe(true);
    vi.advanceTimersByTime(99);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fn).toHaveBeenCalledTimes(1);
    expect(t.active).toBe(false);
  });

  it('replaces a pending callback when set again', () => {
    const t = new OneShot();
    const a = vi.fn();
    const b = vi.fn();
    t.set(a, 100);
    vi.advanceTimersByTime(50);
    t.set(b, 100);
    vi.advanceTimersByTime(100);
    expect(a).not.toHaveBeenCalled();
    expect(b).toHaveBeenCalledTimes(1);
  });

  it('cancels with clear and tolerates clearing when idle', () => {
    const t = new OneShot();
    const fn = vi.fn();
    t.clear();
    t.set(fn, 10);
    t.clear();
    expect(t.active).toBe(false);
    vi.advanceTimersByTime(50);
    expect(fn).not.toHaveBeenCalled();
  });

  it('can be re-armed from inside its own callback', () => {
    const t = new OneShot();
    const fn = vi.fn();
    t.set(() => t.set(fn, 10), 10);
    vi.advanceTimersByTime(20);
    expect(fn).toHaveBeenCalledTimes(1);
  });
});
