import { describe, expect, it, vi } from 'vitest';
import { captureFocus, keepFocus } from './focus';

class FakeEl {
  isConnected = true;
  focus = vi.fn();
  ownerDocument = { activeElement: null as unknown };
}

function stubDom() {
  vi.stubGlobal('HTMLElement', FakeEl);
}

describe('captureFocus', () => {
  it('puts focus back on the element that had it', () => {
    stubDom();
    const el = new FakeEl();
    const restore = captureFocus({ activeElement: el, body: {} } as never);
    restore();
    expect(el.focus).toHaveBeenCalledOnce();
  });

  it('does nothing when the page body had focus', () => {
    stubDom();
    const body = new FakeEl();
    captureFocus({ activeElement: body, body } as never)();
    expect(body.focus).not.toHaveBeenCalled();
  });

  it('does nothing where there is no document, as when rendering on the server', () => {
    expect(() => captureFocus()()).not.toThrow();
  });

  it('does nothing when the element left the page', () => {
    stubDom();
    const el = new FakeEl();
    const restore = captureFocus({ activeElement: el, body: {} } as never);
    el.isConnected = false;
    restore();
    expect(el.focus).not.toHaveBeenCalled();
  });
});

describe('keepFocus', () => {
  it('refocuses an element that lost focus while the work ran, after the page settled', async () => {
    const el = new FakeEl();
    el.ownerDocument.activeElement = el;
    const order: string[] = [];
    el.focus.mockImplementation(() => order.push('focus'));
    const out = await keepFocus(
      el as never,
      async () => {
        el.ownerDocument.activeElement = null;
        return 7;
      },
      async () => void order.push('settle'),
    );
    expect(out).toBe(7);
    expect(order).toEqual(['settle', 'focus']);
  });

  it('does not take focus when the element did not have it', async () => {
    const el = new FakeEl();
    await keepFocus(el as never, async () => 1, async () => {});
    expect(el.focus).not.toHaveBeenCalled();
  });

  it('refocuses even when the work rejects', async () => {
    const el = new FakeEl();
    el.ownerDocument.activeElement = el;
    await expect(keepFocus(el as never, () => Promise.reject(new Error('x')), async () => {})).rejects.toThrow('x');
    expect(el.focus).toHaveBeenCalledOnce();
  });

  it('works without an element', async () => {
    expect(await keepFocus(undefined, async () => 2, async () => {})).toBe(2);
  });
});
