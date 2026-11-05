import { describe, expect, it, vi } from 'vitest';
import { copyOnSelect } from './selection-copy';

function fakeTerm(text: string) {
  let fire = () => {};
  const dispose = vi.fn();
  return {
    text,
    hasSelection() {
      return this.text !== '';
    },
    getSelection() {
      return this.text;
    },
    onSelectionChange(fn: () => void) {
      fire = fn;
      return { dispose };
    },
    fire: () => fire(),
    dispose,
  };
}

describe('copyOnSelect', () => {
  it('copies the selected text when the selection changes', () => {
    const t = fakeTerm('hello');
    const copy = vi.fn(async () => {});
    copyOnSelect(t, copy);
    t.fire();
    expect(copy).toHaveBeenCalledWith('hello');
  });

  it('does not copy when the selection is cleared', () => {
    const t = fakeTerm('');
    const copy = vi.fn(async () => {});
    copyOnSelect(t, copy);
    t.fire();
    expect(copy).not.toHaveBeenCalled();
  });

  it('does not copy while skipped, so find highlights stay out of the clipboard', () => {
    const t = fakeTerm('match');
    const copy = vi.fn(async () => {});
    copyOnSelect(t, copy, () => true);
    t.fire();
    expect(copy).not.toHaveBeenCalled();
  });

  it('swallows a rejected copy', async () => {
    const t = fakeTerm('x');
    const copy = vi.fn(() => Promise.reject(new Error('denied')));
    copyOnSelect(t, copy);
    t.fire();
    await Promise.resolve();
    expect(copy).toHaveBeenCalledOnce();
  });

  it('returns the subscription so the caller can dispose it', () => {
    const t = fakeTerm('x');
    copyOnSelect(t, async () => {})?.dispose();
    expect(t.dispose).toHaveBeenCalledOnce();
  });

  it('does nothing on a terminal without selection events', () => {
    expect(copyOnSelect({ hasSelection: () => true, getSelection: () => 'x' }, async () => {})).toBeUndefined();
  });
});
