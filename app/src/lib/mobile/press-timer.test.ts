import { afterEach, describe, expect, it, vi } from 'vitest';
import { LONG_PRESS_MS, pressTimer } from './gestures';

afterEach(() => vi.useRealTimers());

describe('pressTimer', () => {
  it('fires once after the hold', () => {
    vi.useFakeTimers();
    const press = pressTimer(LONG_PRESS_MS);
    const fire = vi.fn();
    press.start(10, 10, fire);
    vi.advanceTimersByTime(LONG_PRESS_MS - 1);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledOnce();
    vi.advanceTimersByTime(LONG_PRESS_MS * 2);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('waits the duration it was given', () => {
    vi.useFakeTimers();
    const fire = vi.fn();
    pressTimer(900).start(0, 0, fire);
    vi.advanceTimersByTime(899);
    expect(fire).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('is cancelled by a move past the tolerance, not by a small one', () => {
    vi.useFakeTimers();
    const press = pressTimer(LONG_PRESS_MS);
    const fire = vi.fn();
    press.start(0, 0, fire);
    press.move(6, 6);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(fire).toHaveBeenCalledOnce();
    press.start(0, 0, fire);
    press.move(20, 0);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(fire).toHaveBeenCalledOnce();
  });

  it('is cancelled explicitly and a new start replaces the pending one', () => {
    vi.useFakeTimers();
    const press = pressTimer(LONG_PRESS_MS);
    const first = vi.fn();
    const second = vi.fn();
    press.start(0, 0, first);
    press.cancel();
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(first).not.toHaveBeenCalled();
    press.start(0, 0, first);
    press.start(0, 0, second);
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledOnce();
  });

  it('ignores moves when nothing is pending', () => {
    const press = pressTimer(LONG_PRESS_MS);
    expect(() => press.move(100, 100)).not.toThrow();
  });
});
