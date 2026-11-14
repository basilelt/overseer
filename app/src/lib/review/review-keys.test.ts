import { afterEach, describe, expect, it, vi } from 'vitest';
import { TEXT_FIELD } from './walk-key';
import { keepFocus, reviewAction } from './review-keys';

const key = (k: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods });
const inside = (...selectors: string[]) => ({ closest: (sel: string) => (selectors.some((s) => sel.includes(s)) ? {} : null) });

describe('reviewAction', () => {
  const on = (...hits: string[]) => ({ closest: (sel: string) => (sel.split(',').some((part) => hits.includes(part.trim())) ? {} : null) });

  it('walks with J and K and switches views with M in both views', () => {
    for (const view of ['list', 'map'] as const) {
      expect(reviewAction(key('j'), inside(), view)).toBe('next');
      expect(reviewAction(key('k'), inside(), view)).toBe('prev');
      expect(reviewAction(key('m'), inside(), view)).toBe('toggleView');
    }
  });

  it('keeps Esc, Enter, A and the arrows for the map, so the list scrolls and Esc closes dialogs as before', () => {
    for (const k of ['Escape', 'Enter', 'a', 'ArrowLeft', 'ArrowDown']) expect(reviewAction(key(k), inside(), 'list')).toBeNull();
    expect(reviewAction(key('Escape'), inside(), 'map')).toBe('clear');
    expect(reviewAction(key('Enter'), inside(), 'map')).toBe('open');
    expect(reviewAction(key('a'), inside(), 'map')).toBe('allLinks');
    expect(reviewAction(key('ArrowLeft'), inside(), 'map')).toBe('left');
    expect(reviewAction(key('ArrowRight'), inside(), 'map')).toBe('right');
    expect(reviewAction(key('ArrowUp'), inside(), 'map')).toBe('up');
    expect(reviewAction(key('ArrowDown'), inside(), 'map')).toBe('down');
  });

  it('opens with Enter on a map cell or ranked file, but lets other buttons take their own Enter', () => {
    expect(reviewAction(key('Enter'), on('button', '[data-pick]'), 'map')).toBe('open');
    expect(reviewAction(key('Enter'), on('button'), 'map')).toBeNull();
    expect(reviewAction(key('Enter'), on('[role=radio]'), 'map')).toBeNull();
  });

  it('steps the ranked list with up and down, and leaves left and right to it', () => {
    expect(reviewAction(key('ArrowUp'), on('[data-rank]'), 'map')).toBe('prev');
    expect(reviewAction(key('ArrowDown'), on('[data-rank]'), 'map')).toBe('next');
    expect(reviewAction(key('ArrowLeft'), on('[data-rank]'), 'map')).toBeNull();
  });

  it('leaves the arrows to the indicator chips and the view switch', () => {
    expect(reviewAction(key('ArrowRight'), on('[role=radiogroup]'), 'map')).toBeNull();
  });

  it('ignores keys typed into a comment, with a modifier, or without a target', () => {
    expect(reviewAction(key('m'), { closest: (sel) => (sel === TEXT_FIELD ? {} : null) }, 'map')).toBeNull();
    expect(reviewAction(key('Escape'), { closest: (sel) => (sel === TEXT_FIELD ? {} : null) }, 'map')).toBeNull();
    expect(reviewAction(key('a', { ctrlKey: true }), inside(), 'map')).toBeNull();
    expect(reviewAction(key('m', { metaKey: true }), inside(), 'list')).toBeNull();
    expect(reviewAction(key('Escape'), null, 'map')).toBeNull();
    expect(reviewAction(key('x'), inside(), 'map')).toBeNull();
  });
});


describe('keepFocus', () => {
  afterEach(() => vi.unstubAllGlobals());
  const box = (inside: unknown[]) => {
    const r = { isConnected: true, focused: false, focus() { r.focused = true; active.el = r; }, contains: (n: unknown) => n === r || inside.includes(n) };
    return r;
  };
  const active: { el: unknown } = { el: null };

  it('takes the focus back after Enter removed the focused map cell, so M still switches views', () => {
    const r = box([]);
    active.el = 'body';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(keepFocus(r as unknown as HTMLElement)).toBe(true);
    expect(r.focused).toBe(true);
  });

  it('leaves the focus on a control still inside the review', () => {
    const cell = {};
    const r = box([cell]);
    active.el = cell;
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(keepFocus(r as unknown as HTMLElement)).toBe(true);
    expect(r.focused).toBe(false);
    expect(active.el).toBe(cell);
  });
});
