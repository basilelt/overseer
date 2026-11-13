import { describe, expect, it, vi } from 'vitest';
import { PASTE_TARGETS, blockStrayMiddlePaste, pastesOnMiddleClick } from './middle-paste';

const el = (inPasteTarget: boolean) => ({ closest: vi.fn((sel: string) => (sel === PASTE_TARGETS && inPasteTarget ? {} : null)) });

function mouseup(button: number, target: unknown) {
  const e = { button, target, preventDefault: vi.fn() };
  blockStrayMiddlePaste(e as unknown as MouseEvent);
  return e.preventDefault.mock.calls.length > 0;
}

describe('middle-click paste guard', () => {
  it('stops a middle-click outside any field from pasting into the focused terminal', () => {
    expect(mouseup(1, el(false))).toBe(true);
  });

  it('keeps the middle-click paste inside a field or a terminal', () => {
    expect(mouseup(1, el(true))).toBe(false);
  });

  it('leaves the other buttons alone', () => {
    expect(mouseup(0, el(false))).toBe(false);
    expect(mouseup(2, el(false))).toBe(false);
  });

  it('blocks a middle-click on a target that is not an element', () => {
    expect(pastesOnMiddleClick(null)).toBe(false);
    expect(pastesOnMiddleClick({} as EventTarget)).toBe(false);
    expect(mouseup(1, null)).toBe(true);
  });

  it('counts text fields, editable content and terminals as paste targets', () => {
    for (const sel of ['input', 'textarea', '[contenteditable]:not([contenteditable="false"])', '[data-terminal]']) {
      expect(PASTE_TARGETS.split(', ')).toContain(sel);
    }
  });
});
