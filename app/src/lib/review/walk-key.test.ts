import { afterEach, describe, expect, it, vi } from 'vitest';
import { TEXT_FIELD, holdFocus, walkDirection } from './walk-key';

const key = (k: string, mods: Partial<{ ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {}) => ({ key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods });
const inside = (...selectors: string[]) => ({ closest: (sel: string) => (selectors.some((s) => sel.includes(s)) ? {} : null) });

describe('walkDirection', () => {
  it('walks forward on J and back on K', () => {
    expect(walkDirection(key('j'), inside())).toBe(1);
    expect(walkDirection(key('k'), inside())).toBe(-1);
  });

  it('ignores other keys and shortcuts with a modifier', () => {
    expect(walkDirection(key('x'), inside())).toBeNull();
    expect(walkDirection(key('j', { ctrlKey: true }), inside())).toBeNull();
    expect(walkDirection(key('k', { metaKey: true }), inside())).toBeNull();
    expect(walkDirection(key('j', { altKey: true }), inside())).toBeNull();
  });

  it('ignores a key typed into a text field', () => {
    expect(walkDirection(key('j'), inside('textarea'))).toBeNull();
    expect(walkDirection(key('j'), { closest: (sel) => (sel === TEXT_FIELD ? {} : null) })).toBeNull();
  });

  it('still walks after a Viewed checkbox was clicked', () => {
    const checkbox = { closest: (sel: string) => (sel.includes('input:not([type=checkbox])') ? null : {}) };
    expect(walkDirection(key('j'), checkbox)).toBe(1);
  });

  it('needs a target', () => {
    expect(walkDirection(key('j'), null)).toBeNull();
  });
});

describe('holdFocus', () => {
  type Box = { isConnected: boolean; focus: (o?: unknown) => void; contains: (n: unknown) => boolean };
  const active: { el: unknown } = { el: null };
  const review = (connected = true): Box => ({ isConnected: connected, focus() { active.el = this; }, contains(n) { return n === this; } });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('takes the focus back when the focused diff row was replaced, so the next J still walks', () => {
    const r = review();
    active.el = 'body';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(holdFocus(r as unknown as HTMLElement)).toBe(true);
    expect(active.el).toBe(r);
    expect(r.contains(active.el)).toBe(true);
  });

  it('leaves the focus alone when the review already has it', () => {
    const r = review();
    const focus = vi.spyOn(r, 'focus');
    active.el = r;
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(holdFocus(r as unknown as HTMLElement)).toBe(true);
    expect(focus).not.toHaveBeenCalled();
  });

  it('does nothing without a connected review', () => {
    active.el = 'body';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(holdFocus(undefined)).toBe(false);
    expect(holdFocus(review(false) as unknown as HTMLElement)).toBe(false);
    expect(active.el).toBe('body');
  });
});
