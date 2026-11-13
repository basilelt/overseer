import { describe, expect, it } from 'vitest';
import { PROJECT_SWATCHES, fallbackProjectColor } from './project-colors';

describe('project colours', () => {
  it('offers the eight picker swatches, blue second', () => {
    expect(PROJECT_SWATCHES).toHaveLength(8);
    expect(PROJECT_SWATCHES[1]).toBe('#3b82f6');
  });

  it('gives a project the same fallback colour every time', () => {
    expect(fallbackProjectColor('overseer')).toBe(fallbackProjectColor('overseer'));
  });

  it('hashes the name with the 31 multiplier into the six fallback colours', () => {
    expect(fallbackProjectColor('')).toBe('#64748b');
    expect(fallbackProjectColor('a')).toBe('#0891b2');
    expect(fallbackProjectColor('ab')).toBe('#db2777');
  });
});
