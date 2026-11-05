import { describe, expect, it } from 'vitest';
import { clampLineHeight, fontChain, fontFamilies, lineMetrics, quoteFamily, sameMetrics } from './fonts';

describe('terminal font chain', () => {
  it('defaults to the bundled font, then the symbols fallback', () => {
    expect(fontChain('')).toBe('"JetBrains Mono", "Symbols Nerd Font Mono", monospace');
    expect(fontChain('  ')).toBe(fontChain(''));
  });

  it('puts the chosen fonts first and quotes them', () => {
    expect(fontChain('MesloLGS NF')).toBe('"MesloLGS NF", "JetBrains Mono", "Symbols Nerd Font Mono", monospace');
    expect(fontChain(`'Hack Nerd Font', "SF Mono"`)).toBe('"Hack Nerd Font", "SF Mono", "JetBrains Mono", "Symbols Nerd Font Mono", monospace');
  });

  it('dedupes, drops generics and keeps the symbols font last', () => {
    expect(fontFamilies('jetbrains mono, monospace, Symbols Nerd Font Mono, Menlo, menlo')).toEqual(['jetbrains mono', 'Menlo', 'Symbols Nerd Font Mono']);
  });

  it('strips characters that would break the CSS font shorthand', () => {
    expect(quoteFamily('Evil" , x')).toBe('"Evil , x"');
    expect(quoteFamily('ui-monospace')).toBe('ui-monospace');
  });

  it('clamps line height and centres the glyphs in taller cells', () => {
    expect(clampLineHeight(0.5)).toBe(1);
    expect(clampLineHeight(3)).toBe(2);
    expect(clampLineHeight(Number.NaN)).toBe(1);
    expect(clampLineHeight(1.23)).toBe(1.25);
    expect(lineMetrics({ width: 8, height: 18, baseline: 14 }, 1)).toEqual({ width: 8, height: 18, baseline: 14 });
    expect(lineMetrics({ width: 8, height: 18, baseline: 14 }, 1.5)).toEqual({ width: 8, height: 27, baseline: 18 });
  });
});

describe('sameMetrics', () => {
  it('compares cell geometry so unchanged fonts skip a full re-render', () => {
    const m = { width: 8, height: 18, baseline: 14 };
    expect(sameMetrics(m, { ...m })).toBe(true);
    expect(sameMetrics(m, lineMetrics(m, 1))).toBe(true);
    expect(sameMetrics(m, lineMetrics(m, 1.2))).toBe(false);
    expect(sameMetrics(undefined, m)).toBe(false);
  });
});
