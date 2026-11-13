import { afterEach, describe, expect, it, vi } from 'vitest';
import { setPresence } from './presence';
import { visibleClock } from './visible-clock';

afterEach(() => {
  vi.useRealTimers();
  setPresence({ visible: true });
});

describe('visibleClock', () => {
  it('ticks every period while visible and stops on cleanup', () => {
    vi.useFakeTimers();
    const tick = vi.fn();
    const stop = visibleClock(tick, 30_000);
    expect(tick).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(60_000);
    expect(tick).toHaveBeenCalledTimes(3);
    stop();
    vi.advanceTimersByTime(60_000);
    expect(tick).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('pauses while hidden and refreshes when shown again', () => {
    vi.useFakeTimers();
    const tick = vi.fn();
    const stop = visibleClock(tick, 30_000);
    setPresence({ visible: false });
    expect(vi.getTimerCount()).toBe(0);
    vi.advanceTimersByTime(120_000);
    expect(tick).toHaveBeenCalledTimes(1);
    setPresence({ visible: true });
    expect(tick).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(1);
    stop();
  });
});
