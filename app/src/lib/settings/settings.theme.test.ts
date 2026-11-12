import { afterEach, describe, expect, it, vi } from 'vitest';
import { SOLARIZED_COLORS } from '../theme/theme';

async function load(systemDark: boolean) {
  vi.resetModules();
  const listeners = new Map<string, (e: Event) => void>();
  vi.stubGlobal('matchMedia', () => ({ matches: systemDark, addEventListener: () => {} }));
  vi.stubGlobal('addEventListener', (type: string, fn: (e: Event) => void) => listeners.set(type, fn));
  const mod = { ...(await import('./settings.svelte')), ...(await import('../theme/theme-edit.svelte')) };
  const systemTheme = (dark: boolean) => listeners.get('overseer:system-theme')?.({ detail: { dark } } as unknown as Event);
  return { ...mod, systemTheme };
}

afterEach(() => vi.unstubAllGlobals());

describe('system mode', () => {
  it('starts from the system scheme', async () => {
    const light = await load(false);
    light.settings.theme = 'auto';
    expect(light.resolvedTheme()).toBe('light');
    const dark = await load(true);
    dark.settings.theme = 'auto';
    expect(dark.resolvedTheme()).toBe('dark');
  });

  it('follows the native system theme event live', async () => {
    const s = await load(false);
    s.settings.theme = 'auto';
    s.systemTheme(true);
    expect(s.resolvedTheme()).toBe('dark');
    s.systemTheme(false);
    expect(s.resolvedTheme()).toBe('light');
  });

  it('keeps an explicit choice whatever the system says', async () => {
    const s = await load(false);
    s.settings.theme = 'dark';
    s.systemTheme(false);
    expect(s.resolvedTheme()).toBe('dark');
    s.settings.theme = 'light';
    s.systemTheme(true);
    expect(s.resolvedTheme()).toBe('light');
  });
});

describe('Claude plan limits', () => {
  it('does not call Anthropic with the Claude login unless turned on', async () => {
    const s = await load(false);
    expect(s.DEFAULT_SETTINGS.fetchClaudeLimits).toBe(false);
  });
});

function stubDocument() {
  const props = new Map<string, string>();
  const style = {
    setProperty: (k: string, v: string) => props.set(k, v),
    removeProperty: (k: string) => props.delete(k),
    getPropertyValue: (k: string) => props.get(k) ?? '',
  };
  vi.stubGlobal('document', { documentElement: { dataset: {}, style } });
  return style;
}

function stubStorage(initial: Record<string, unknown> = {}) {
  const storage = new Map<string, string>([['overseer.settings', JSON.stringify(initial)]]);
  vi.stubGlobal('localStorage', { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => storage.set(k, v) });
  return storage;
}

describe('applied tokens', () => {
  it('applies the dark preset when dark and the light preset when light, following the OS in system mode', async () => {
    const style = stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'auto', lightPreset: 'light', darkPreset: 'graphite' });
    expect(document.documentElement.dataset).toEqual({ theme: 'light' });
    expect(style.getPropertyValue('--bg')).toBe('');
    s.systemTheme(true);
    expect(document.documentElement.dataset).toEqual({ theme: 'dark', palette: 'graphite' });
    expect(style.getPropertyValue('--bg')).toBe('');
    s.systemTheme(false);
    expect(document.documentElement.dataset).toEqual({ theme: 'light' });
    expect(style.getPropertyValue('--bg')).toBe('');
  });

  it('keeps the dark preset in dark mode whatever the OS says', async () => {
    stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'dark', darkPreset: 'solarized' });
    s.systemTheme(false);
    expect(s.resolvedTheme()).toBe('dark');
    expect(s.colorPalette()).toBe('solarized');
    expect(document.documentElement.dataset).toEqual({ theme: 'dark', palette: 'solarized' });
  });

  it('applies a custom preset as inline tokens with a readable accent text', async () => {
    const style = stubDocument();
    const s = await load(false);
    const id = s.createPreset('light', 'Custom');
    s.editPreset(id, { colors: { bg: '#fdf6e3', accent: '#ffff00' } });
    s.saveSettings({ theme: 'light' });
    expect(document.documentElement.dataset).toEqual({ theme: 'light', palette: 'custom' });
    expect(style.getPropertyValue('--bg')).toBe('#fdf6e3');
    expect(style.getPropertyValue('--accent')).toBe('#ffff00');
    expect(style.getPropertyValue('--accent-fg')).toBe('#0b0b0c');
    s.saveSettings({ theme: 'dark' });
    expect(document.documentElement.dataset).toEqual({ theme: 'dark' });
    expect(style.getPropertyValue('--bg')).toBe('');
    expect(style.getPropertyValue('--accent-fg')).toBe('');
  });
});

describe('custom presets', () => {
  it('creates, edits, selects and deletes a preset, falling back to the default palette', async () => {
    stubDocument();
    const s = await load(false);
    const id = s.createPreset('dark', 'Custom');
    expect(s.settings.darkPreset).toBe(id);
    expect(s.settings.customPresets).toHaveLength(1);
    expect(s.settings.customPresets[0]).toMatchObject({ id, name: 'Custom 1', mode: 'dark' });
    s.editPreset(id, { name: 'Night', colors: { bg: '#101010' } });
    expect(s.settings.customPresets[0]).toMatchObject({ name: 'Night' });
    expect(s.settings.customPresets[0].colors.bg).toBe('#101010');
    expect(s.createPreset('dark', 'Custom')).not.toBe(id);
    expect(s.settings.customPresets.map((p) => p.name)).toEqual(['Night', 'Custom 1']);
    s.deletePreset(s.settings.darkPreset);
    expect(s.settings.darkPreset).toBe('dark');
    s.selectPreset('dark', id);
    s.deletePreset(id);
    expect(s.settings.darkPreset).toBe('dark');
    expect(s.settings.customPresets).toEqual([]);
  });

  it('starts a new preset from the colors of the selected one', async () => {
    stubDocument();
    const s = await load(false);
    s.selectPreset('dark', 'solarized');
    const id = s.createPreset('dark', 'Custom');
    expect(s.settings.customPresets.find((p) => p.id === id)?.colors).toEqual(SOLARIZED_COLORS);
  });

  it('persists presets and the selected palettes across reloads', async () => {
    stubStorage();
    const s = await load(false);
    const id = s.createPreset('light', 'Custom');
    s.editPreset(id, { name: 'Paper', colors: { bg: '#f5f0e6' } });
    s.selectPreset('dark', 'graphite');
    s.saveSettings({ theme: 'light' });
    const reloaded = await load(false);
    expect(reloaded.settings.theme).toBe('light');
    expect(reloaded.settings.lightPreset).toBe(id);
    expect(reloaded.settings.darkPreset).toBe('graphite');
    expect(reloaded.settings.customPresets[0]).toMatchObject({ name: 'Paper' });
    expect(reloaded.customColors().bg).toBe('#f5f0e6');
  });

  it('ignores invalid colors in stored presets', async () => {
    stubStorage({ theme: 'light', lightPreset: 'custom:x', customPresets: [{ id: 'custom:x', name: 'Bad', mode: 'light', colors: { bg: 'red; background: url(x)', text: '#123456' } }] });
    const s = await load(false);
    expect(s.customColors().bg).toBeUndefined();
    expect(s.customColors().text).toBe('#123456');
  });
});

describe('migration from whole themes', () => {
  it.each(['graphite', 'solarized'])('moves a saved %s theme to dark mode with that dark palette', async (name) => {
    const storage = stubStorage({ theme: name, fontSize: 15 });
    const s = await load(false);
    expect(s.settings.theme).toBe('dark');
    expect(s.settings.darkPreset).toBe(name);
    expect(s.settings.fontSize).toBe(15);
    s.saveSettings({ fontSize: 16 });
    const saved = JSON.parse(storage.get('overseer.settings')!);
    expect(saved).toMatchObject({ theme: 'dark', darkPreset: name, fontSize: 16 });
    expect(saved.colors).toBeUndefined();
  });

  it('keeps the old light, dark and system values working', async () => {
    for (const theme of ['light', 'dark', 'auto'] as const) {
      stubStorage({ theme });
      const s = await load(false);
      expect(s.settings.theme).toBe(theme);
      expect(s.settings.lightPreset).toBe('light');
      expect(s.settings.darkPreset).toBe('dark');
    }
  });

  it('turns old color overrides into custom presets that keep the look', async () => {
    stubStorage({ theme: 'graphite', colors: { light: { text: '#112233' }, dark: { bg: '#000000' }, graphite: { accent: '#abcdef' } } });
    const s = await load(false);
    expect(s.settings.theme).toBe('dark');
    expect(s.settings.lightPreset).toBe('custom:legacy-light');
    expect(s.settings.darkPreset).toBe('custom:legacy-graphite');
    expect(s.colorPalette()).toBe('graphite');
    expect(s.customColors()).toEqual({ accent: '#abcdef' });
    expect(s.settings.customPresets.map((p) => p.id).sort()).toEqual(['custom:legacy-dark', 'custom:legacy-graphite', 'custom:legacy-light']);
  });

  it('migrates a theme synced from an older window', async () => {
    stubDocument();
    const s = await load(false);
    s.syncSettings(JSON.stringify({ theme: 'solarized' }));
    expect(s.settings.theme).toBe('dark');
    expect(s.colorPalette()).toBe('solarized');
    s.syncSettings(JSON.stringify({ theme: 'light', colors: { light: { accent: '#ff0000' }, dark: {} } }));
    expect(s.settings.lightPreset).toBe('custom:legacy-light');
    expect(s.customColors().accent).toBe('#ff0000');
  });
});

describe('URL override', () => {
  it.each(['light', 'dark', 'auto'])('accepts ?theme=%s', async (theme) => {
    vi.stubGlobal('location', { search: `?theme=${theme}` });
    stubStorage({ theme: theme === 'light' ? 'dark' : 'light' });
    const s = await load(false);
    expect(s.settings.theme).toBe(theme);
  });

  it.each(['graphite', 'solarized'])('keeps ?theme=%s working as dark plus that palette', async (name) => {
    vi.stubGlobal('location', { search: `?theme=${name}` });
    stubDocument();
    const s = await load(false);
    expect(s.settings.theme).toBe('dark');
    s.applyTheme();
    expect(document.documentElement.dataset).toEqual({ theme: 'dark', palette: name });
  });

  it('ignores unknown values', async () => {
    vi.stubGlobal('location', { search: '?theme=neon' });
    const s = await load(false);
    expect(s.settings.theme).toBe('auto');
  });
});
