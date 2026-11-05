import { describe, expect, it } from 'vitest';
import source from './ProfilesDialog.svelte?raw';

describe('agent sections of the profiles dialog', () => {
  const tag = source.match(/<details class="agent".*/)?.[0] ?? '';

  it('keeps a section the user opened open when a field in it changes', () => {
    expect(tag).toMatch(/open=\{expanded\[a\.id\] \?\?/);
    expect(tag).toMatch(/ontoggle=\{\(e\) => \(expanded\[a\.id\] = e\.currentTarget\.open\)\}/);
  });

  it('still opens the only section by default', () => {
    expect(tag).toMatch(/\?\? agents\.length === 1\}/);
  });
});
