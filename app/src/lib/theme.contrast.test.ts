import { describe, expect, it } from 'vitest';
import css from '../app.css?raw';
import { en } from './i18n/en';

function lightToken(name: string): string {
  const root = css.slice(css.indexOf(':root {'), css.indexOf(":root[data-theme='dark']"));
  const m = root.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`));
  if (!m) throw new Error(`missing --${name}`);
  return m[1];
}

function lum(hex: string): number {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((x) => (x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

function ratio(a: string, b: string): number {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

describe('light theme contrast', () => {
  it.each(['bg', 'bg-subtle', 'surface', 'surface-2', 'surface-hover'])('keeps --text-3 at 4.5:1 on --%s', (bg) => {
    expect(ratio(lightToken('text-3'), lightToken(bg))).toBeGreaterThanOrEqual(4.5);
  });

  it.each(['bg', 'surface-2'])('keeps --text-2 at 4.5:1 on --%s', (bg) => {
    expect(ratio(lightToken('text-2'), lightToken(bg))).toBeGreaterThanOrEqual(4.5);
  });
});

describe('phone tour wording', () => {
  it('does not tell phone users to click', () => {
    expect((en as Record<string, string>)['tour.clickCursor.phone']).not.toMatch(/On a computer/);
  });
});
