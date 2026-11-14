import { describe, expect, it } from 'vitest';
import {
  migrateTheme,
  modeOf,
  resolveTheme,
  sanitizePresets,
  type CustomPreset,
  type ThemePref,
  type ThemeState,
} from './theme';
import { BUILTIN_PRESETS, DARK_COLORS, LIGHT_COLORS, contrastRatio, fullColors, readableOn, MIN_CONTRAST, addPreset, baseColors, contrastWarnings, nextName, presetColors, presetsFor, removePreset, updatePreset } from './theme-edit';

const custom = (id: string, mode: 'light' | 'dark', bg: string): CustomPreset => ({ id: `custom:${id}`, name: id, mode, colors: { ...(mode === 'dark' ? DARK_COLORS : LIGHT_COLORS), bg } });
const state = (over: Partial<ThemeState> = {}): ThemeState => ({ theme: 'auto', lightPreset: 'light', darkPreset: 'dark', customPresets: [], ...over });

describe('resolveTheme', () => {
  const themes: ThemePref[] = ['auto', 'light', 'dark'];
  it.each(themes.flatMap((t) => [true, false].map((os) => [t, os] as const)))('mode %s with OS dark=%s', (theme, os) => {
    const expected = theme === 'auto' ? (os ? 'dark' : 'light') : theme;
    expect(resolveTheme(state({ theme }), os).mode).toBe(expected);
    expect(modeOf(theme, os)).toBe(expected);
  });

  it('applies the preset of the resolved mode', () => {
    const s = state({ theme: 'auto', lightPreset: 'light', darkPreset: 'graphite' });
    expect(resolveTheme(s, true)).toMatchObject({ mode: 'dark', palette: 'graphite', custom: false });
    expect(resolveTheme(s, true).colors).toEqual({});
    expect(resolveTheme(s, false)).toMatchObject({ mode: 'light', palette: 'light', colors: {} });
  });

  it('gives default palettes no inline tokens', () => {
    expect(resolveTheme(state({ theme: 'dark' }), false)).toMatchObject({ mode: 'dark', palette: 'dark', custom: false, colors: {} });
  });

  it('applies a custom preset of the matching mode and its tokens', () => {
    const c = custom('a', 'dark', '#101010');
    const r = resolveTheme(state({ theme: 'dark', darkPreset: c.id, customPresets: [c] }), false);
    expect(r).toMatchObject({ mode: 'dark', custom: true, palette: 'dark' });
    expect(r.colors.bg).toBe('#101010');
  });

  it('falls back to the default palette for an unknown preset or one of the wrong mode', () => {
    expect(resolveTheme(state({ theme: 'dark', darkPreset: 'custom:gone' }), false).palette).toBe('dark');
    expect(resolveTheme(state({ theme: 'light', lightPreset: 'graphite' }), false).palette).toBe('light');
    const c = custom('l', 'light', '#fafafa');
    expect(resolveTheme(state({ theme: 'dark', darkPreset: c.id, customPresets: [c] }), false).palette).toBe('dark');
  });

  it('treats an unknown theme value as system', () => {
    expect(resolveTheme(state({ theme: 'neon' as ThemePref }), true).mode).toBe('dark');
  });

  it('lists Graphite and Solarized Soft as dark presets and only the default as light', () => {
    expect(presetsFor('dark').map((p) => p.id)).toEqual(['dark', 'graphite', 'solarized']);
    expect(presetsFor('light').map((p) => p.id)).toEqual(['light']);
    expect(BUILTIN_PRESETS.every((p) => Object.keys(presetColors(p.id)).length === 6)).toBe(true);
  });
});

describe('custom preset CRUD', () => {
  it('adds, updates and removes without mutating the input', () => {
    const base: CustomPreset[] = [];
    const a = custom('a', 'light', '#ffffff');
    const added = addPreset(base, a);
    expect(base).toEqual([]);
    expect(added).toEqual([a]);
    const edited = updatePreset(added, a.id, { name: 'Paper', colors: { bg: '#f5f0e6' } });
    expect(edited[0]).toMatchObject({ name: 'Paper' });
    expect(edited[0].colors.bg).toBe('#f5f0e6');
    expect(added[0].name).toBe('a');
    expect(removePreset(edited, a.id)).toEqual([]);
    expect(removePreset(edited, 'custom:nope')).toEqual(edited);
  });

  it('rejects invalid colors on update and caps the name length', () => {
    const a = custom('a', 'light', '#ffffff');
    const [e] = updatePreset([a], a.id, { name: 'x'.repeat(100), colors: { bg: 'nope', text: '#010203' } });
    expect(e.colors.bg).toBe('#ffffff');
    expect(e.colors.text).toBe('#010203');
    expect(e.name).toHaveLength(40);
  });

  it('names new presets without clashing', () => {
    expect(nextName([], 'dark', 'Custom')).toBe('Custom 1');
    const list = [{ ...custom('x', 'dark', '#000000'), name: 'Custom 1' }];
    expect(nextName(list, 'dark', 'Custom')).toBe('Custom 2');
    expect(nextName(list, 'light', 'Custom')).toBe('Custom 1');
  });

  it('starts from the selected preset colors', () => {
    const c = custom('a', 'dark', '#123456');
    const s = state({ darkPreset: c.id, customPresets: [c] });
    expect(baseColors(s, 'dark', c.id).bg).toBe('#123456');
    expect(baseColors(s, 'dark', 'solarized').bg).toBe('#002b36');
    expect(baseColors(s, 'dark', 'unknown')).toEqual(DARK_COLORS);
    expect(baseColors(s, 'light', 'unknown')).toEqual(LIGHT_COLORS);
  });

  it('drops malformed stored presets', () => {
    expect(sanitizePresets('nope')).toEqual([]);
    const ok = custom('ok', 'dark', '#111111');
    const out = sanitizePresets([ok, ok, null, { id: 'dark', mode: 'dark' }, { id: 'custom:m', mode: 'blue' }, { id: 'custom:n', mode: 'light', name: '  ', colors: { accent: 'bad' } }]);
    expect(out.map((p) => p.id)).toEqual(['custom:ok', 'custom:n']);
    expect(out[1]).toMatchObject({ name: 'Custom', colors: {} });
  });
});

describe('contrast', () => {
  it('computes the WCAG ratio', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5);
    expect(contrastRatio('#777777', '#777777')).toBeCloseTo(1, 5);
    expect(contrastRatio('#ffffff', '#000000')).toBe(contrastRatio('#000000', '#ffffff'));
  });

  it('accepts every built-in palette', () => {
    for (const p of BUILTIN_PRESETS) expect(contrastWarnings(presetColors(p.id)), p.id).toEqual([]);
  });

  it('warns for each text color below 4.5:1 on a background', () => {
    const w = contrastWarnings({ ...LIGHT_COLORS, text: '#cccccc', 'text-2': '#18181b' });
    expect(w.map((x) => `${x.fg}/${x.bg}`)).toEqual(['text/bg', 'text/surface']);
    expect(w.every((x) => x.ratio < MIN_CONTRAST)).toBe(true);
  });

  it('picks white or near-black text for an accent', () => {
    expect(readableOn('#2563eb')).toBe('#ffffff');
    expect(readableOn('#ffff00')).toBe('#0b0b0c');
  });
});

describe('migrateTheme', () => {
  it('maps whole themes to dark mode plus a dark preset', () => {
    expect(migrateTheme({ theme: 'graphite' })).toMatchObject({ theme: 'dark', darkPreset: 'graphite', lightPreset: 'light' });
    expect(migrateTheme({ theme: 'solarized' })).toMatchObject({ theme: 'dark', darkPreset: 'solarized' });
  });

  it('keeps system, light and dark and defaults to system', () => {
    expect(migrateTheme({ theme: 'auto' }).theme).toBe('auto');
    expect(migrateTheme({ theme: 'light' }).theme).toBe('light');
    expect(migrateTheme({ theme: 'dark' }).theme).toBe('dark');
    expect(migrateTheme({}).theme).toBe('auto');
    expect(migrateTheme({ theme: 'pink' }).theme).toBe('auto');
  });

  it('is idempotent', () => {
    const once = migrateTheme({ theme: 'graphite', colors: { dark: { bg: '#000000' }, graphite: { accent: '#abcdef' } } });
    expect(migrateTheme({ ...once })).toEqual(once);
  });

  it('keeps a saved preset choice and drops one that no longer exists', () => {
    expect(migrateTheme({ theme: 'dark', darkPreset: 'solarized' }).darkPreset).toBe('solarized');
    expect(migrateTheme({ theme: 'dark', darkPreset: 'custom:gone', lightPreset: 'graphite' })).toMatchObject({ darkPreset: 'dark', lightPreset: 'light' });
  });

  it('turns old overrides into custom presets and selects the ones in use', () => {
    const m = migrateTheme({ theme: 'light', colors: { light: { accent: '#ff0000', bg: 'bad' }, dark: { bg: '#050505' }, graphite: {}, solarized: { text: '#ffffff' } } });
    expect(m.lightPreset).toBe('custom:legacy-light');
    expect(m.darkPreset).toBe('custom:legacy-dark');
    const light = m.customPresets.find((p) => p.id === 'custom:legacy-light')!;
    expect(light).toMatchObject({ colors: { accent: '#ff0000' }, base: 'light', mode: 'light' });
    expect(fullColors(light)).toEqual({ ...LIGHT_COLORS, accent: '#ff0000' });
    expect(m.customPresets.find((p) => p.id === 'custom:legacy-solarized')?.colors.text).toBe('#ffffff');
    expect(m.customPresets.find((p) => p.id === 'custom:legacy-graphite')).toBeUndefined();
  });

  it('resolves a migrated preset to its original palette with only the old overrides inline', () => {
    const m = migrateTheme({ theme: 'graphite', colors: { graphite: { accent: '#abcdef' } } });
    expect(resolveTheme(m, false)).toMatchObject({ mode: 'dark', palette: 'graphite', custom: false, colors: { accent: '#abcdef' } });
    const l = migrateTheme({ theme: 'light', colors: { light: { text: '#112233' } } });
    expect(resolveTheme(l, true)).toMatchObject({ mode: 'light', palette: 'light', custom: false, colors: { text: '#112233' } });
  });

  it('keeps graphite overrides with graphite and does not select dark overrides over it', () => {
    const m = migrateTheme({ theme: 'graphite', colors: { dark: { bg: '#050505' }, graphite: { accent: '#abcdef' } } });
    expect(m.darkPreset).toBe('custom:legacy-graphite');
    expect(m.theme).toBe('dark');
    const plain = migrateTheme({ theme: 'solarized', colors: { dark: { bg: '#050505' } } });
    expect(plain.darkPreset).toBe('solarized');
  });
});
