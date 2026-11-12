import { describe, expect, it } from 'vitest';
import css from '../../app.css?raw';
import { en } from '../i18n/en';
import { contrastRatio } from './theme-edit';

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

  it.each(['bg-subtle', 'surface-2', 'surface-hover', 'surface-active', 'border-strong', 'text-3', 'ws-frame-bg', 'session-row-selected'])('derives --%s from the six base colors', (name) => {
    expect(block).toMatch(new RegExp(`--${name}:\\s*(color-mix\\([^;]*var\\(--(bg|surface|border|text|text-2)\\)|var\\(--(bg|surface|border|text-2|surface-active)\\))`));
  });
});

describe('phone tour wording', () => {
  it('does not tell phone users to click', () => {
    expect((en as Record<string, string>)['tour.clickCursor.phone']).not.toMatch(/On a computer/);
  });
});
