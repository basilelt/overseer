import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { closeConfirm } from './close-confirm';

describe('closeConfirm', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('disarms by itself after 3 seconds', () => {
    const set = vi.fn();
    closeConfirm(set).arm();
    expect(set).toHaveBeenLastCalledWith(true);
    vi.advanceTimersByTime(2999);
    expect(set).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(set).toHaveBeenLastCalledWith(false);
  });

  it('restarts the countdown when armed again', () => {
    const set = vi.fn();
    const c = closeConfirm(set);
    c.arm();
    vi.advanceTimersByTime(2000);
    c.arm();
    vi.advanceTimersByTime(2000);
    expect(set).not.toHaveBeenCalledWith(false);
    vi.advanceTimersByTime(1000);
    expect(set).toHaveBeenLastCalledWith(false);
  });

  it('reset disarms at once and cancels the pending timer', () => {
    const set = vi.fn();
    const c = closeConfirm(set);
    c.arm();
    c.reset();
    expect(set).toHaveBeenLastCalledWith(false);
    set.mockClear();
    vi.advanceTimersByTime(5000);
    expect(set).not.toHaveBeenCalled();
    expect(vi.getTimerCount()).toBe(0);
  });
});
