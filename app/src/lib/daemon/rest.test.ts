import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const idle = vi.hoisted(() => ({ calls: [] as boolean[] }));
vi.mock('../presence', () => ({ setIdle: (on: boolean) => idle.calls.push(on) }));

import { eventTarget } from '../../test/fake-events';
import { IDLE_MS, startRest } from './rest';

describe('rest', () => {
  let win: ReturnType<typeof eventTarget>;

  beforeEach(() => {
    vi.useFakeTimers();
    idle.calls = [];
    win = eventTarget();
    vi.stubGlobal('window', win);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('goes idle after a minute without input and leaves no timer behind', () => {
    const stop = startRest();
    vi.advanceTimersByTime(IDLE_MS - 1);
    expect(idle.calls).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(idle.calls).toEqual([true]);
    expect(vi.getTimerCount()).toBe(0);
    stop();
  });

  it('pushes the deadline back on input without one timer per event', () => {
    const stop = startRest();
    vi.advanceTimersByTime(IDLE_MS - 1000);
    for (let i = 0; i < 50; i++) win.fire('pointermove');
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(IDLE_MS - 1000);
    expect(idle.calls).toEqual([]);
    vi.advanceTimersByTime(1000);
    expect(idle.calls).toEqual([true]);
    stop();
  });

  it('wakes on any key, click, scroll or window focus', () => {
    const stop = startRest();
    for (const t of ['keydown', 'pointerdown', 'wheel', 'focus']) {
      vi.advanceTimersByTime(IDLE_MS);
      win.fire(t);
    }
    expect(idle.calls).toEqual([true, false, true, false, true, false, true, false]);
    expect(vi.getTimerCount()).toBe(1);
    stop();
  });

  it('clears its timer and listeners and leaves the window active on stop', () => {
    const stop = startRest();
    vi.advanceTimersByTime(IDLE_MS);
    stop();
    expect(idle.calls).toEqual([true, false]);
    expect(vi.getTimerCount()).toBe(0);
    expect(win.count()).toBe(0);
  });
});
