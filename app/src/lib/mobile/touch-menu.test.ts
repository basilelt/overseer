import { afterEach, describe, expect, it, vi } from 'vitest';
import { TOUCH_MENU_MS } from './gestures';
import { touchContextMenu } from './touch-menu';

class FakeMouseEvent extends Event {
  clientX: number;
  clientY: number;
  constructor(type: string, init: { bubbles?: boolean; cancelable?: boolean; clientX: number; clientY: number }) {
    super(type, init);
    this.clientX = init.clientX;
    this.clientY = init.clientY;
  }
}

const pointer = (type: string, x = 0, y = 0, pointerType = 'touch') => Object.assign(new Event(type, { cancelable: true }), { clientX: x, clientY: y, pointerType });
const trusted = (e: Event) => Object.defineProperty(e, 'isTrusted', { value: true });

function setup() {
  vi.useFakeTimers();
  vi.stubGlobal('MouseEvent', FakeMouseEvent);
  const el = new EventTarget() as HTMLElement;
  const action = touchContextMenu(el);
  const menu = vi.fn();
  const click = vi.fn();
  el.addEventListener('contextmenu', menu);
  el.addEventListener('click', click);
  return { el, menu, click, action };
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('touchContextMenu', () => {
  it('sends a contextmenu event at the pressed point after a touch hold, and swallows the click that follows', () => {
    const { el, menu, click, action } = setup();
    el.dispatchEvent(pointer('pointerdown', 12, 34));
    vi.advanceTimersByTime(TOUCH_MENU_MS);
    expect(menu).toHaveBeenCalledOnce();
    expect(menu.mock.calls[0]![0]).toMatchObject({ type: 'contextmenu', clientX: 12, clientY: 34, bubbles: true, cancelable: true });
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('click'));
    expect(click).not.toHaveBeenCalled();
    el.dispatchEvent(pointer('pointerdown'));
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('click'));
    expect(click).toHaveBeenCalledOnce();
    action.destroy();
  });

  it('does nothing on a short tap, so the click still selects', () => {
    const { el, menu, click } = setup();
    el.dispatchEvent(pointer('pointerdown'));
    vi.advanceTimersByTime(TOUCH_MENU_MS - 100);
    el.dispatchEvent(pointer('pointerup'));
    el.dispatchEvent(pointer('click'));
    vi.advanceTimersByTime(TOUCH_MENU_MS);
    expect(menu).not.toHaveBeenCalled();
    expect(click).toHaveBeenCalledOnce();
  });

  it('does nothing when the finger moves to scroll or the browser cancels the pointer', () => {
    const { el, menu } = setup();
    el.dispatchEvent(pointer('pointerdown', 0, 0));
    el.dispatchEvent(pointer('pointermove', 30, 0));
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    el.dispatchEvent(pointer('pointerdown'));
    el.dispatchEvent(pointer('pointercancel'));
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    expect(menu).not.toHaveBeenCalled();
  });

  it('tolerates a small finger jitter during the hold', () => {
    const { el, menu } = setup();
    el.dispatchEvent(pointer('pointerdown', 0, 0));
    el.dispatchEvent(pointer('pointermove', 3, 3));
    vi.advanceTimersByTime(TOUCH_MENU_MS);
    expect(menu).toHaveBeenCalledOnce();
  });

  it('ignores a mouse or pen press, which has a real right-click', () => {
    const { el, menu } = setup();
    el.dispatchEvent(pointer('pointerdown', 0, 0, 'mouse'));
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    el.dispatchEvent(pointer('pointerdown', 0, 0, 'pen'));
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    expect(menu).not.toHaveBeenCalled();
  });

  it('swallows the native contextmenu Android adds after a long press, but not a desktop right-click', () => {
    const { el, menu } = setup();
    const right = trusted(new Event('contextmenu', { cancelable: true }));
    el.dispatchEvent(right);
    expect(menu).toHaveBeenCalledOnce();
    expect(right.defaultPrevented).toBe(false);
    el.dispatchEvent(pointer('pointerdown'));
    vi.advanceTimersByTime(TOUCH_MENU_MS);
    expect(menu).toHaveBeenCalledTimes(2);
    const echo = trusted(new Event('contextmenu', { cancelable: true }));
    el.dispatchEvent(echo);
    expect(menu).toHaveBeenCalledTimes(2);
    expect(echo.defaultPrevented).toBe(true);
  });

  it('clears the pending timer and the listeners on destroy', () => {
    const { el, menu, action } = setup();
    el.dispatchEvent(pointer('pointerdown'));
    action.destroy();
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    el.dispatchEvent(pointer('pointerdown'));
    vi.advanceTimersByTime(TOUCH_MENU_MS * 2);
    expect(menu).not.toHaveBeenCalled();
  });
});
