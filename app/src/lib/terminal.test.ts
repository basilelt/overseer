import { describe, expect, it, vi } from 'vitest';

vi.mock('ghostty-web/ghostty-vt.wasm?url', () => ({ default: '' }));

const { TERM_THEMES, themeReport } = await import('./terminal');

function luminance(hex: string) {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const TEXT = ['foreground', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'brightBlack', 'brightRed', 'brightGreen', 'brightYellow', 'brightBlue', 'brightMagenta', 'brightCyan', 'white', 'brightWhite'] as const;

describe('terminal palettes', () => {
  for (const theme of ['light', 'dark'] as const) {
    it(`keeps ${theme} text colors readable on the background`, () => {
      const p = TERM_THEMES[theme];
      for (const k of TEXT) expect(contrast(p[k]!, p.background!), `${theme}.${k}`).toBeGreaterThanOrEqual(4.5);
    });
  }

  it('reports the palette colors and darkness to the daemon', () => {
    expect(themeReport('light')).toEqual({ dark: false, fg: '#18181b', bg: '#ffffff' });
    expect(themeReport('dark')).toEqual({ dark: true, fg: '#ededef', bg: '#0b0b0c' });
    expect(themeReport('light', { bg: '#fdf6e3', text: '#073642' })).toEqual({ dark: false, fg: '#073642', bg: '#fdf6e3' });
  });
});
