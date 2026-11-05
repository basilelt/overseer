import type { Ghostty, ITheme } from 'ghostty-web';
import wasmUrl from 'ghostty-web/ghostty-vt.wasm?url';
import { installOnDemandRendering } from './termrender';

export const TERM_THEMES: Record<'light' | 'dark', ITheme> = {
  dark: {
    background: '#0b0b0c',
    foreground: '#ededef',
    cursor: '#ededef',
    cursorAccent: '#0b0b0c',
    selectionBackground: '#2f3b52',
    black: '#1b1b1e',
    red: '#f87171',
    green: '#4ade80',
    yellow: '#facc15',
    blue: '#60a5fa',
    magenta: '#c084fc',
    cyan: '#22d3ee',
    white: '#d4d4d8',
    brightBlack: '#7c7c85',
    brightRed: '#fca5a5',
    brightGreen: '#86efac',
    brightYellow: '#fde047',
    brightBlue: '#93c5fd',
    brightMagenta: '#d8b4fe',
    brightCyan: '#67e8f9',
    brightWhite: '#fafafa',
  },
  light: {
    background: '#ffffff',
    foreground: '#18181b',
    cursor: '#18181b',
    cursorAccent: '#ffffff',
    selectionBackground: '#cfe0fc',
    black: '#18181b',
    red: '#b91c1c',
    green: '#166534',
    yellow: '#854d0e',
    blue: '#1d4ed8',
    magenta: '#7e22ce',
    cyan: '#155e75',
    white: '#71717a',
    brightBlack: '#52525b',
    brightRed: '#dc2626',
    brightGreen: '#15803d',
    brightYellow: '#a16207',
    brightBlue: '#2563eb',
    brightMagenta: '#9333ea',
    brightCyan: '#0e7490',
    brightWhite: '#52525b',
  },
};

export function termTheme(theme: 'light' | 'dark', colors: { bg?: string; text?: string }): ITheme {
  const base = TERM_THEMES[theme];
  const bg = colors.bg ?? base.background;
  const fg = colors.text ?? base.foreground;
  return { ...base, background: bg, foreground: fg, cursor: fg, cursorAccent: bg };
}

export function themeReport(theme: 'light' | 'dark', colors: { bg?: string; text?: string } = {}) {
  const p = termTheme(theme, colors);
  return { dark: theme === 'dark', fg: p.foreground, bg: p.background };
}

type GhosttyModule = typeof import('ghostty-web');

export interface GhosttyRuntime {
  mod: GhosttyModule;
  ghostty: Ghostty;
}

let loading: Promise<GhosttyRuntime> | null = null;

export function loadGhostty(): Promise<GhosttyRuntime> {
  loading ??= import('ghostty-web').then(async (mod) => {
    installOnDemandRendering(mod);
    return { mod, ghostty: await mod.Ghostty.load(wasmUrl) };
  });
  return loading;
}
