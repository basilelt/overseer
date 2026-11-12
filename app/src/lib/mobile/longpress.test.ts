import { afterEach, describe, expect, it, vi } from 'vitest';
import { CHIP_PRESS_MS as LONG_PRESS_MS } from './gestures';
import { longPress } from './longpress';

function pointer(type: string, x = 0, y = 0): PointerEvent {
  return Object.assign(new Event(type, { cancelable: true }), { clientX: x, clientY: y }) as unknown as PointerEvent;
}

afterEach(() => vi.useRealTimers());

describe('longPress', () => {
  it('fires after the hold and swallows the click that follows', () => {
    vi.useFakeTimers();
    const el = new EventTarget() as HTMLElement;
    const onLong = vi.fn();
    const onClick = vi.fn();
    const a = longPress(el, onLong);
    el.addEventListener('click', onClick);
    el.dispatchEvent(pointer('pointerdown'));
    vi.advanceTimersByTime(LONG_PRESS_MS);
    expect(onLong).toHaveBeenCalledOnce();
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('click'));
    expect(onClick).not.toHaveBeenCalled();
    el.dispatchEvent(pointer('pointerdown'));
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('click'));
    expect(onClick).toHaveBeenCalledOnce();
    a.destroy();
  });

  it('does not fire on a short tap or when the finger scrolls', () => {
    vi.useFakeTimers();
    const el = new EventTarget() as HTMLElement;
    const onLong = vi.fn();
    longPress(el, onLong);
    el.dispatchEvent(pointer('pointerdown'));
    vi.advanceTimersByTime(LONG_PRESS_MS - 100);
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('pointerdown', 0, 0));
    el.dispatchEvent(pointer('pointermove', 30, 0));
    vi.advanceTimersByTime(LONG_PRESS_MS * 2);
    expect(onLong).not.toHaveBeenCalled();
  });
});
