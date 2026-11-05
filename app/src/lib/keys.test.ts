import { describe, expect, it } from 'vitest';
import { keyToInput, type KeyLike } from './keys';

const key = (k: string, mods: Partial<KeyLike> = {}): KeyLike => ({ key: k, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...mods });
const on = { shiftEnterNewline: true };

describe('keyToInput', () => {
  it('sends ESC CR for Shift+Enter', () => {
    expect(keyToInput(key('Enter', { shiftKey: true }), on)).toBe('\x1b\r');
  });

  it('leaves plain Enter and other keys to the terminal', () => {
    expect(keyToInput(key('Enter'), on)).toBeNull();
    expect(keyToInput(key('a', { shiftKey: true }), on)).toBeNull();
  });

  it('ignores Shift+Enter combined with another modifier', () => {
    const extra: Partial<KeyLike>[] = [{ ctrlKey: true }, { altKey: true }, { metaKey: true }];
    for (const mods of extra) expect(keyToInput(key('Enter', { ...mods, shiftKey: true }), on)).toBeNull();
  });

  it('does nothing while composing or when disabled', () => {
    expect(keyToInput(key('Enter', { shiftKey: true, isComposing: true }), on)).toBeNull();
    expect(keyToInput(key('Enter', { shiftKey: true }), { shiftEnterNewline: false })).toBeNull();
  });
});
