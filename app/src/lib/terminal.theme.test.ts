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
