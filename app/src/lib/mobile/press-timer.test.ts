import { afterEach, describe, expect, it, vi } from 'vitest';
import { LONG_PRESS_MS, pressTimer } from './gestures';

afterEach(() => vi.useRealTimers());

describe('pressTimer', () => {
  it('long-press takes 500 ms for chips, tab menus, kanban cards and terminals', () => {
    expect(LONG_PRESS_MS).toBe(500);
  });

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

describe('a long press that opens a sheet', () => {
  const ev = (type: string, detail = 1) => Object.assign(new Event(type, { cancelable: true }), { detail });

  function sheet() {
    vi.useFakeTimers();
    const root = new EventTarget();
    const row = vi.fn();
    const press = pressTimer(LONG_PRESS_MS, { swallow: true, target: root });
    return { root, row, press, open: (ms: number) => (press.start(0, 0, () => root.addEventListener('click', row)), vi.advanceTimersByTime(ms)) };
  }

  it('ignores the click of the finger lifting afterwards, however long the hold', () => {
    const { root, row, open } = sheet();
    open(LONG_PRESS_MS + 5000);
    root.dispatchEvent(ev('pointerup'));
    root.dispatchEvent(ev('click'));
    expect(row).not.toHaveBeenCalled();
  });

  it('accepts a tap that starts with a new pointerdown', () => {
    const { root, row, open } = sheet();
    open(LONG_PRESS_MS);
    root.dispatchEvent(ev('pointerup'));
    root.dispatchEvent(ev('click'));
    root.dispatchEvent(ev('pointerdown'));
    root.dispatchEvent(ev('pointerup'));
    root.dispatchEvent(ev('click'));
    expect(row).toHaveBeenCalledOnce();
  });

  it('accepts a tap even when the lift produced no click at all', () => {
    const { root, row, open } = sheet();
    open(LONG_PRESS_MS);
    root.dispatchEvent(ev('pointerup'));
    root.dispatchEvent(ev('pointerdown'));
    root.dispatchEvent(ev('click'));
    expect(row).toHaveBeenCalledOnce();
  });

  it('lets a keyboard activation through', () => {
    const { root, row, open } = sheet();
    open(LONG_PRESS_MS);
    root.dispatchEvent(ev('click', 0));
    expect(row).toHaveBeenCalledOnce();
  });

  it('swallows nothing when the hold is cancelled or swallowing is not asked for', () => {
    const { root, row, press } = sheet();
    root.addEventListener('click', row);
    press.start(0, 0, () => {});
    vi.advanceTimersByTime(LONG_PRESS_MS - 1);
    press.cancel();
    root.dispatchEvent(ev('click'));
    pressTimer(LONG_PRESS_MS).start(0, 0, () => {});
    vi.advanceTimersByTime(LONG_PRESS_MS);
    root.dispatchEvent(ev('click'));
    expect(row).toHaveBeenCalledTimes(2);
  });
});
