import { describe, expect, it, vi } from 'vitest';
import css from '../../app.css?raw';
vi.mock('ghostty-web/ghostty-vt.wasm?url', () => ({ default: '' }));
import { GRAPHITE_COLORS, SOLARIZED_COLORS } from './theme';
import { termTheme } from '../terminal/terminal';

const rgb = (hex: string) => hex.slice(1).match(/../g)!.map((c) => parseInt(c, 16));
const luminance = (color: number[]) => color.map((v) => v / 255).map((v) => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4).reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
const contrast = (a: string, b: number[]) => { const values = [luminance(rgb(a)), luminance(b)].sort((x, y) => y - x); return (values[0] + 0.05) / (values[1] + 0.05); };

describe.each([['graphite', GRAPHITE_COLORS], ['solarized', SOLARIZED_COLORS]] as const)('%s palette', (preset, defaults) => {
  const block = css.match(new RegExp(":root\\[data-theme='dark'\\]\\[data-palette='" + preset + "'\\] \\{([^}]+)\\}"))![1];
  const tokens = Object.fromEntries([...block.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2]]));
  it('matches the preset defaults used by color pickers and terminals', () => {
    for (const [k, v] of Object.entries(defaults)) expect(tokens[k]).toBe(v);
  });

  it('keeps all text levels readable on panels and hover surfaces', () => {
    for (const fg of ['text', 'text-2', 'text-3']) for (const bg of ['bg', 'surface', 'surface-2', 'surface-hover']) expect(contrast(tokens[fg], rgb(tokens[bg])), `${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(tokens['accent-fg'], rgb(tokens.accent))).toBeGreaterThanOrEqual(4.5);
  });

  it('draws the sidebar frames and session rows from its own surfaces', () => {
    expect([tokens['ws-frame-border'], tokens['ws-frame-bg'], tokens['session-row-bg'], tokens['session-row-selected']]).toEqual(['var(--border)', 'var(--surface)', 'var(--bg)', 'var(--surface-active)']);
  });

  it('keeps diff text readable on its composited highlight backgrounds', () => {
    for (const kind of ['add', 'del']) {
      const [r, g, b, alpha] = tokens[`diff-${kind}-bg`].match(/[\d.]+/g)!.map(Number);
      for (const bg of ['bg', 'surface', 'surface-hover']) {
        const composite = rgb(tokens[bg]).map((v, i) => [r, g, b][i] * alpha + v * (1 - alpha));
        expect(contrast(tokens[`diff-${kind}`], composite)).toBeGreaterThanOrEqual(4.5);
      }
    }
    expect(contrast(tokens['diff-hunk'], rgb(tokens['surface-hover']))).toBeGreaterThanOrEqual(4.5);
  });

  it('keeps ANSI foreground colors readable on the terminal background', () => {
    const palette = termTheme('dark', {}, preset);
    for (const key of ['red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white', 'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan', 'brightWhite'] as const) expect(contrast(palette[key]!, rgb(palette.background!)), key).toBeGreaterThanOrEqual(4.5);
  });
});
