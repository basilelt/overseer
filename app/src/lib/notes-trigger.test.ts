import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FakeEl } from '../test/fake-dom';
import { isNotesTrigger } from './notes-trigger';

beforeEach(() => vi.stubGlobal('CSS', { escape: (s: string) => s }));
afterEach(() => vi.unstubAllGlobals());

const as = (el: FakeEl | null) => el as unknown as Element | null;

describe('isNotesTrigger', () => {
  it('recognises the notes button of the open session and anything inside it', () => {
    const button = new FakeEl('button', { 'data-notes': 'ln_1' });
    const icon = new FakeEl('svg').in(button);
    expect(isNotesTrigger(as(button), 'ln_1')).toBe(true);
    expect(isNotesTrigger(as(icon), 'ln_1')).toBe(true);
  });

  it('does not treat the notes button of another session as the trigger', () => {
    expect(isNotesTrigger(as(new FakeEl('button', { 'data-notes': 'ln_2' })), 'ln_1')).toBe(false);
  });

  it('does not treat other elements or no target as the trigger', () => {
    expect(isNotesTrigger(as(new FakeEl('div')), 'ln_1')).toBe(false);
    expect(isNotesTrigger(null, 'ln_1')).toBe(false);
  });
});
