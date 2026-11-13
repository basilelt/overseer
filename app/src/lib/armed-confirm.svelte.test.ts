import { afterEach, describe, expect, it, vi } from 'vitest';
import { armedConfirm } from './armed-confirm.svelte';
import { CONFIRM_WINDOW_MS } from './timing';

afterEach(() => vi.useRealTimers());

describe('armedConfirm', () => {
  it('arms on the first press and confirms on the second', () => {
    const c = armedConfirm();
    expect(c.press()).toBe(false);
    expect(c.armed).toBe(true);
    expect(c.press()).toBe(true);
    expect(c.armed).toBe(false);
  });

  it('disarms by itself after 3 seconds', () => {
    vi.useFakeTimers();
    const c = armedConfirm();
    c.press();
    vi.advanceTimersByTime(CONFIRM_WINDOW_MS - 1);
    expect(c.armed).toBe(true);
    vi.advanceTimersByTime(1);
    expect(c.armed).toBe(false);
    expect(c.press()).toBe(false);
  });

  it('needs a fresh first press after a reset', () => {
    vi.useFakeTimers();
    const c = armedConfirm();
    c.press();
    c.reset();
    expect(c.armed).toBe(false);
    vi.advanceTimersByTime(CONFIRM_WINDOW_MS * 2);
    expect(c.press()).toBe(false);
  });

  it('is 3000 ms', () => {
    expect(CONFIRM_WINDOW_MS).toBe(3000);
  });
});
