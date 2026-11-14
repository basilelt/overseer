export const COLOR_TOKENS = ['accent', 'bg', 'surface', 'border', 'text', 'text-2'] as const;
export type ColorToken = (typeof COLOR_TOKENS)[number];
export type ColorOverrides = Partial<Record<ColorToken, string>>;
export type Mode = 'light' | 'dark';
export type ThemePref = 'auto' | Mode;
export type ColorPalette = 'light' | 'dark' | 'graphite' | 'solarized';
export type PresetColors = Record<ColorToken, string>;

export interface CustomPreset {
  id: string;
  name: string;
  mode: Mode;
  colors: ColorOverrides;
  accentFg?: string;
  base?: ColorPalette;
}

export interface ThemeState {
  theme: ThemePref;
  lightPreset: string;
  darkPreset: string;
  customPresets: CustomPreset[];
}

export interface ResolvedTheme {
  mode: Mode;
  palette: ColorPalette;
  custom: boolean;
  colors: ColorOverrides;
  accentFg?: string;
}

export const GRAPHITE_COLORS = { accent: '#9AB8EF', bg: '#202124', surface: '#292B30', border: '#454850', text: '#DADCE0', 'text-2': '#A8ADB7' } as const;
export const SOLARIZED_COLORS = { accent: '#70B5DB', bg: '#002b36', surface: '#073642', border: '#38606B', text: '#A7B6B6', 'text-2': '#A7B6B6' } as const;

export const BUILTIN_MODE: Record<ColorPalette, Mode> = { light: 'light', dark: 'dark', graphite: 'dark', solarized: 'dark' };

const HEX = /^#[0-9a-f]{6}$/i;
export const isHex = (v: unknown): v is string => typeof v === 'string' && HEX.test(v);

export function hexColors(raw: unknown): ColorOverrides {
  const r = (raw ?? {}) as Record<string, unknown>;
  return Object.fromEntries(COLOR_TOKENS.filter((k) => isHex(r[k])).map((k) => [k, r[k]]));
}

export function sanitizePresets(raw: unknown): CustomPreset[] {
  const list = (Array.isArray(raw) ? raw : []) as Partial<CustomPreset>[];
  return list
    .filter((p, i) => typeof p?.id === 'string' && p.id.startsWith('custom:') && (p.mode === 'light' || p.mode === 'dark') && list.findIndex((x) => x?.id === p.id) === i)
    .map((p) => ({
      id: p.id!,
      name: (typeof p.name === 'string' && p.name.trim().slice(0, 40)) || 'Custom',
      mode: p.mode!,
      colors: hexColors(p.colors),
      accentFg: isHex(p.accentFg) ? p.accentFg : undefined,
      base: BUILTIN_MODE[p.base as ColorPalette] === p.mode ? p.base : undefined,
    }));
}

export function modeOf(theme: unknown, systemDark: boolean): Mode {
  return theme === 'light' || theme === 'dark' ? theme : systemDark ? 'dark' : 'light';
}

export function resolveTheme(state: ThemeState, systemDark: boolean): ResolvedTheme {
  const mode = modeOf(state.theme, systemDark);
  const id = mode === 'dark' ? state.darkPreset : state.lightPreset;
  const custom = state.customPresets.find((p) => p.id === id && p.mode === mode);
  if (custom) return { mode, palette: custom.base ?? mode, custom: !custom.base, colors: custom.colors, accentFg: custom.accentFg };
  const palette = BUILTIN_MODE[id as ColorPalette] === mode ? (id as ColorPalette) : mode;
  return { mode, palette, custom: false, colors: {} };
}

const LEGACY = [['light', 'My light'], ['dark', 'My dark'], ['graphite', 'My Graphite'], ['solarized', 'My Solarized']] as const;

export function migrateTheme(raw: Record<string, unknown>): ThemeState {
  const legacy = raw.theme === 'graphite' || raw.theme === 'solarized' ? raw.theme : undefined;
  const theme: ThemePref = raw.theme === 'light' || raw.theme === 'dark' ? raw.theme : legacy ? 'dark' : 'auto';
  let customPresets = sanitizePresets(raw.customPresets);
  let lightPreset = typeof raw.lightPreset === 'string' ? raw.lightPreset : 'light';
  let darkPreset = legacy ?? (typeof raw.darkPreset === 'string' ? raw.darkPreset : 'dark');
  const old = (raw.colors ?? {}) as Record<string, Record<string, unknown> | undefined>;
  for (const [key, name] of LEGACY) {
    const mode: Mode = key === 'light' ? 'light' : 'dark';
    const o = old[key] ?? {};
    if (!COLOR_TOKENS.some((k) => isHex(o[k]))) continue;
    const id = `custom:legacy-${key}`;
    if (!customPresets.some((p) => p.id === id)) customPresets = [...customPresets, { id, name, mode, colors: hexColors(o), base: key }];
    if (key === 'light' ? lightPreset === 'light' : key === 'dark' ? !legacy && darkPreset === 'dark' : legacy === key) {
      if (mode === 'light') lightPreset = id;
      else darkPreset = id;
    }
  }
  const known = (id: string, mode: Mode) => customPresets.some((p) => p.id === id && p.mode === mode) || BUILTIN_MODE[id as ColorPalette] === mode;
  return { theme, lightPreset: known(lightPreset, 'light') ? lightPreset : 'light', darkPreset: known(darkPreset, 'dark') ? darkPreset : 'dark', customPresets };
}
