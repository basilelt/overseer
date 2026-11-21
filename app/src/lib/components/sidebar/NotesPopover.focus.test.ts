import { describe, expect, it } from 'vitest';
import source from './NotesPopover.svelte?raw';

const body = (name: string) => {
  const from = source.indexOf(`async function ${name}(`);
  return source.slice(from, source.indexOf('\n  }\n', from));
};

describe('source guard: new-note focus', () => {
  it('moves focus back to the new-note box when Add note is clicked, before the button disables itself', () => {
    const add = body('add');
    expect(add).toMatch(/focusNew\(\)/);
    expect(add.indexOf('focusNew()')).toBeLessThan(add.indexOf('await addNote'));
  });

  it('uses the same focus helper when the popover opens or switches session', () => {
    expect(source).toMatch(/shown = id;\s+focusNew\(\);/);
  });
});
