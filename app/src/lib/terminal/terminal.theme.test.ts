import { describe, expect, it, vi } from 'vitest';

vi.mock('ghostty-web/ghostty-vt.wasm?url', () => ({ default: '' }));

describe('termTheme', () => {
  it('uses custom background and text, keeping the ANSI palette', async () => {
    const { termTheme, TERM_THEMES } = await import('./terminal');
    const th = termTheme('dark', { bg: '#101010', text: '#eeeeee' });
    expect(th).toMatchObject({ background: '#101010', foreground: '#eeeeee', cursor: '#eeeeee', cursorAccent: '#101010', red: TERM_THEMES.dark.red });
    expect(termTheme('light', {})).toEqual(TERM_THEMES.light);
  });
});

describe('Graphite terminal palette', () => {
  it('keeps default ANSI colors while using a readable muted tone', async () => {
    const { termTheme, TERM_THEMES } = await import('./terminal');
    expect(termTheme('dark', {}, 'graphite')).toMatchObject({ background: '#202124', foreground: '#DADCE0', cursor: '#DADCE0', cursorAccent: '#202124', brightBlack: '#A8ADB7' });
    expect(termTheme('dark', {})).toEqual(TERM_THEMES.dark);
    expect(termTheme('dark', { bg: '#252525', text: '#eeeeee' }, 'graphite')).toMatchObject({ background: '#252525', foreground: '#eeeeee', cursor: '#eeeeee', cursorAccent: '#252525' });
  });
});

describe('Solarized Soft terminal palette', () => {
  it('uses teal and warm ANSI colors with independent custom background and text', async () => {
    const { termTheme, TERM_THEMES, GRAPHITE_TERM_THEME } = await import('./terminal');
    expect(termTheme('dark', {}, 'solarized')).toMatchObject({ background: '#002b36', foreground: '#A7B6B6', cursor: '#A7B6B6', cursorAccent: '#002b36', cyan: '#65C3BA', yellow: '#D8B267' });
    expect(termTheme('dark', {})).toEqual(TERM_THEMES.dark);
    expect(termTheme('dark', {}, 'graphite')).toEqual(GRAPHITE_TERM_THEME);
    expect(termTheme('dark', { bg: '#252525', text: '#eeeeee' }, 'solarized')).toMatchObject({ background: '#252525', foreground: '#eeeeee', cursor: '#eeeeee', cursorAccent: '#252525' });
  });
});
