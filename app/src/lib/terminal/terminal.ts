import type { Ghostty, ITheme } from 'ghostty-web';
import wasmUrl from 'ghostty-web/ghostty-vt.wasm?url';
import { installBlankResize, installOnDemandRendering, installWholeLineViewport } from './termrender';
import { settings } from '../settings/settings.svelte';
import { GRAPHITE_COLORS, SOLARIZED_COLORS, type ColorPalette } from '../theme/theme';

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

export const GRAPHITE_TERM_THEME: ITheme = {
  ...TERM_THEMES.dark,
  background: GRAPHITE_COLORS.bg,
  foreground: GRAPHITE_COLORS.text,
  cursor: GRAPHITE_COLORS.text,
  cursorAccent: GRAPHITE_COLORS.bg,
  selectionBackground: '#454F63',
  brightBlack: '#A8ADB7',
};

export const SOLARIZED_TERM_THEME: ITheme = {
  background: SOLARIZED_COLORS.bg,
  foreground: SOLARIZED_COLORS.text,
  cursor: SOLARIZED_COLORS.text,
  cursorAccent: SOLARIZED_COLORS.bg,
  selectionBackground: '#24515E',
  black: '#073642',
  red: '#E78278',
  green: '#A4BC66',
  yellow: '#D8B267',
  blue: '#70B5DB',
  magenta: '#D991BD',
  cyan: '#65C3BA',
  white: '#BAC4BC',
  brightBlack: '#93A1A1',
  brightRed: '#F5A398',
  brightGreen: '#BDD78C',
  brightYellow: '#E7C783',
  brightBlue: '#9CC7E0',
  brightMagenta: '#E5B0D2',
  brightCyan: '#8BD6CB',
  brightWhite: '#EEE8D5',
};

export function termTheme(theme: 'light' | 'dark', colors: { bg?: string; text?: string }, palette?: ColorPalette): ITheme {
  const base = palette === 'solarized' ? SOLARIZED_TERM_THEME : palette === 'graphite' ? GRAPHITE_TERM_THEME : TERM_THEMES[theme];
  const bg = colors.bg ?? base.background;
  const fg = colors.text ?? base.foreground;
  return { ...base, background: bg, foreground: fg, cursor: fg, cursorAccent: bg };
}

export function themeReport(theme: 'light' | 'dark', colors: { bg?: string; text?: string } = {}, palette?: ColorPalette) {
  const p = termTheme(theme, colors, palette);
  return { dark: theme === 'dark', fg: p.foreground, bg: p.background };
}

type GhosttyModule = typeof import('ghostty-web');

export interface GhosttyRuntime {
  mod: GhosttyModule;
  ghostty: Ghostty;
}

let loading: Promise<GhosttyRuntime> | null = null;

export function loadGhostty(): Promise<GhosttyRuntime> {
  loading ??= Promise.all([import('ghostty-web'), import('./termgl')]).then(async ([mod, { attachOverlay, installWebglRenderer, webglMode }]) => {
    installOnDemandRendering(mod);
    installBlankResize(mod);
    installWholeLineViewport(mod);
    const gl = webglMode();
    installWebglRenderer(mod, (r) => attachOverlay(r, gl === 'force'), () => gl !== 'off' || settings.gpuTerminal);
    return { mod, ghostty: await mod.Ghostty.load(wasmUrl) };
  });
  return loading;
}
