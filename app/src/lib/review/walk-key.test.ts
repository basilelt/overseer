import { describe, expect, it } from 'vitest';
import { TEXT_FIELD, walkDirection } from './walk-key';

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
