import { BUILTIN_MODE, GRAPHITE_COLORS, SOLARIZED_COLORS, hexColors, type ColorPalette, type ColorToken, type CustomPreset, type Mode, type PresetColors, type ThemeState } from './theme';

export const MIN_CONTRAST = 4.5;
export const LIGHT_COLORS: PresetColors = { accent: '#2563eb', bg: '#ffffff', surface: '#ffffff', border: '#e4e4e7', text: '#18181b', 'text-2': '#52525b' };
export const DARK_COLORS: PresetColors = { accent: '#3b82f6', bg: '#0b0b0c', surface: '#141416', border: '#232327', text: '#ededef', 'text-2': '#a1a1aa' };
const TOKEN_PRESETS: Partial<Record<string, PresetColors>> = { graphite: GRAPHITE_COLORS, solarized: SOLARIZED_COLORS };

export function luminance(hex: string): number {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((x) => (x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

export const readableOn = (bg: string): string => (luminance(bg) > 0.179 ? '#0b0b0c' : '#ffffff');

export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

export interface ContrastWarning {
  fg: ColorToken;
  bg: ColorToken;
  ratio: number;
}

const PAIRS: [ColorToken, ColorToken][] = [['text', 'bg'], ['text', 'surface'], ['text-2', 'bg'], ['text-2', 'surface']];

export function contrastWarnings(colors: PresetColors): ContrastWarning[] {
  return PAIRS.map(([fg, bg]) => ({ fg, bg, ratio: contrastRatio(colors[fg], colors[bg]) })).filter((w) => w.ratio < MIN_CONTRAST);
}

export interface BuiltinPreset {
  id: ColorPalette;
  mode: Mode;
  label: 'settings.palette.default.light' | 'settings.palette.default.dark' | 'settings.theme.graphite' | 'settings.theme.solarized';
}

export const BUILTIN_PRESETS: BuiltinPreset[] = [
  { id: 'light', mode: 'light', label: 'settings.palette.default.light' },
  { id: 'dark', mode: 'dark', label: 'settings.palette.default.dark' },
  { id: 'graphite', mode: 'dark', label: 'settings.theme.graphite' },
  { id: 'solarized', mode: 'dark', label: 'settings.theme.solarized' },
];

export function presetsFor(mode: Mode): BuiltinPreset[] {
  return BUILTIN_PRESETS.filter((p) => p.mode === mode);
}

export function presetColors(id: ColorPalette): PresetColors {
  return TOKEN_PRESETS[id] ?? (BUILTIN_MODE[id] === 'dark' ? DARK_COLORS : LIGHT_COLORS);
}

export function customId(): string {
  return `custom:${Math.random().toString(36).slice(2, 8)}${Date.now().toString(36)}`;
}

export function fullColors(p: CustomPreset): PresetColors {
  return { ...presetColors(p.base ?? p.mode), ...p.colors };
}

export function baseColors(state: ThemeState, mode: Mode, id: string): PresetColors {
  const custom = state.customPresets.find((p) => p.id === id && p.mode === mode);
  if (custom) return fullColors(custom);
  return { ...(BUILTIN_MODE[id as ColorPalette] === mode ? presetColors(id as ColorPalette) : mode === 'dark' ? DARK_COLORS : LIGHT_COLORS) };
}

export function addPreset(list: CustomPreset[], preset: CustomPreset): CustomPreset[] {
  return [...list, preset];
}

export function updatePreset(list: CustomPreset[], id: string, patch: { name?: string; colors?: Partial<PresetColors> }): CustomPreset[] {
  return list.map((p) => {
    if (p.id !== id) return p;
    const name = patch.name === undefined ? p.name : patch.name.slice(0, 40);
    const colors = { ...fullColors(p), ...hexColors(patch.colors) };
    return { ...p, name, colors, accentFg: readableOn(colors.accent) };
  });
}

export function removePreset(list: CustomPreset[], id: string): CustomPreset[] {
  return list.filter((p) => p.id !== id);
}

export function nextName(list: CustomPreset[], mode: Mode, label: string): string {
  const used = new Set(list.filter((p) => p.mode === mode).map((p) => p.name));
  for (let n = 1; ; n++) {
    const name = `${label} ${n}`;
    if (!used.has(name)) return name;
  }
}
