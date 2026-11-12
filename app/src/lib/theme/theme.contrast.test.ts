import { describe, expect, it } from 'vitest';
import css from '../../app.css?raw';
import { en } from '../i18n/en';
import { contrastRatio, DARK_COLORS, LIGHT_COLORS } from './theme-edit';

function lightToken(name: string): string {
  const root = css.slice(css.indexOf(':root {'), css.indexOf(":root[data-theme='dark']"));
  const m = root.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`));
  if (!m) throw new Error(`missing --${name}`);
  return m[1];
}

describe('light theme contrast', () => {
  it.each(['bg', 'bg-subtle', 'surface', 'surface-2', 'surface-hover'])('keeps --text-3 at 4.5:1 on --%s', (bg) => {
    expect(contrastRatio(lightToken('text-3'), lightToken(bg))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(['bg', 'surface-2'])('keeps --text-2 at 4.5:1 on --%s', (bg) => {
    expect(contrastRatio(lightToken('text-2'), lightToken(bg))).toBeGreaterThanOrEqual(4.5);
  });
});

describe('custom palette derivation', () => {
  const block = css.match(/:root\[data-palette='custom'\] \{([^}]+)\}/)![1];
  const rules = Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));

  const mix = (a: string, pct: number, b: string): string => {
    const [x, y] = [a, b].map((h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16)));
    return '#' + x.map((v, i) => Math.round((v * pct) / 100 + (y[i] * (100 - pct)) / 100).toString(16).padStart(2, '0')).join('');
  };

  function derive(base: Record<string, string>): Record<string, string> {
    const out: Record<string, string> = { ...base };
    const resolve = (name: string): string => {
      if (out[name]) return out[name];
      const rule = rules[name];
      if (!rule) throw new Error(`--${name} is neither a base color nor derived`);
      const ref = rule.match(/^var\(--([\w-]+)\)$/);
      if (ref) return (out[name] = resolve(ref[1]));
      const m = rule.match(/^color-mix\(in srgb, var\(--([\w-]+)\) (\d+)%, var\(--([\w-]+)\)\)$/);
      if (!m) throw new Error(`cannot evaluate --${name}: ${rule}`);
      return (out[name] = mix(resolve(m[1]), Number(m[2]), resolve(m[3])));
    };
    for (const name of Object.keys(rules)) resolve(name);
    return out;
  }

  describe.each([['light', LIGHT_COLORS], ['dark', DARK_COLORS]])('from the %s defaults', (_, base) => {
    const tokens = derive(base);

    it.each(['bg-subtle', 'surface-2', 'surface-hover', 'surface-active'])('keeps the derived --text-3 at 4.5:1 on --%s', (bg) => {
      expect(contrastRatio(tokens['text-3'], tokens[bg])).toBeGreaterThanOrEqual(4.5);
    });

    it('keeps the derived borders distinguishable from the surface they sit on', () => {
      expect(tokens['border-strong']).not.toBe(tokens.surface);
      expect(contrastRatio(tokens['border-strong'], tokens.surface)).toBeGreaterThan(contrastRatio(tokens.border, tokens.surface));
    });
  });
});

describe('phone tour wording', () => {
  it('does not tell phone users to click', () => {
    expect((en as Record<string, string>)['tour.clickCursor.phone']).not.toMatch(/On a computer/);
  });
});
