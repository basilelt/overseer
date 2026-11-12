import { describe, expect, it } from 'vitest';
import { isSubmitChord } from './chord';

describe('isSubmitChord', () => {
  it('accepts Ctrl+Enter and Cmd+Enter', () => {
    expect(isSubmitChord({ key: 'Enter', ctrlKey: true })).toBe(true);
    expect(isSubmitChord({ key: 'Enter', metaKey: true })).toBe(true);
  });

  it('rejects a bare Enter and other keys', () => {
    expect(isSubmitChord({ key: 'Enter' })).toBe(false);
    expect(isSubmitChord({ key: 'Enter', ctrlKey: false, metaKey: false })).toBe(false);
    expect(isSubmitChord({ key: 'a', ctrlKey: true })).toBe(false);
  });
});
