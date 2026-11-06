import { afterEach, describe, expect, it, vi } from 'vitest';

async function load(systemDark: boolean) {
  vi.resetModules();
  const listeners = new Map<string, (e: Event) => void>();
  vi.stubGlobal('matchMedia', () => ({ matches: systemDark, addEventListener: () => {} }));
  vi.stubGlobal('addEventListener', (type: string, fn: (e: Event) => void) => listeners.set(type, fn));
  const mod = await import('./settings.svelte');
  const systemTheme = (dark: boolean) => listeners.get('overseer:system-theme')?.({ detail: { dark } } as unknown as Event);
  return { ...mod, systemTheme };
}

afterEach(() => vi.unstubAllGlobals());

describe('auto theme', () => {
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

describe('custom colors', () => {
  it('applies only the overrides of the theme in use', async () => {
    const style = stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'light', colors: { light: { accent: '#ff0000' }, dark: { accent: '#00ff00', bg: '#000000' } } });
    expect(style.getPropertyValue('--accent')).toBe('#ff0000');
    expect(style.getPropertyValue('--bg')).toBe('');
    s.saveSettings({ theme: 'dark' });
    expect(style.getPropertyValue('--accent')).toBe('#00ff00');
    expect(style.getPropertyValue('--bg')).toBe('#000000');
    s.saveSettings({ colors: { light: {}, dark: {} } });
    expect(style.getPropertyValue('--accent')).toBe('');
  });

  it('ignores values that are not hex colors', async () => {
    const style = stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'light', colors: { light: { accent: 'red; background: url(x)', text: '#123456' }, dark: {} } });
    expect(s.customColors()).toEqual({ text: '#123456' });
    expect(style.getPropertyValue('--accent')).toBe('');
  });
});

describe('Graphite preset', () => {
  it('persists isolated overrides across reloads', async () => {
    const storage = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => storage.set(k, v) });
    const s = await load(false);
    s.saveSettings({ theme: 'graphite', colors: { light: { text: '#112233' }, dark: { bg: '#000000' }, graphite: { accent: '#abcdef' } } });
    const reloaded = await load(false);
    expect(reloaded.settings.theme).toBe('graphite');
    expect(reloaded.resolvedTheme()).toBe('dark');
    expect(reloaded.customColors()).toEqual({ ...reloaded.GRAPHITE_COLORS, accent: '#abcdef' });
    expect(reloaded.customColors('dark')).toEqual({ bg: '#000000' });
    reloaded.saveSettings({ theme: 'dark' });
    expect(reloaded.customColors()).toEqual({ bg: '#000000' });
    reloaded.saveSettings({ theme: 'light' });
    expect(reloaded.customColors()).toEqual({ text: '#112233' });
  });

  it('stays dark through native changes and clears its selector on system mode', async () => {
    stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'graphite' });
    s.systemTheme(false);
    expect(s.resolvedTheme()).toBe('dark');
    expect(document.documentElement.dataset).toEqual({ theme: 'dark', palette: 'graphite' });
    s.saveSettings({ theme: 'auto' });
    expect(document.documentElement.dataset).toEqual({ theme: 'light' });
    s.systemTheme(true);
    expect(document.documentElement.dataset).toEqual({ theme: 'dark' });
  });

  it('resets only its overrides and remains selected', async () => {
    const style = stubDocument();
    const s = await load(true);
    s.saveSettings({ theme: 'graphite', colors: { light: {}, dark: { accent: '#123456' }, graphite: { accent: '#abcdef', bg: 'invalid' } } });
    expect(style.getPropertyValue('--accent')).toBe('#abcdef');
    expect(style.getPropertyValue('--bg')).toBe(s.GRAPHITE_COLORS.bg);
    s.saveSettings({ colors: { ...s.settings.colors, graphite: {} } });
    expect(s.colorOverrides()).toEqual({});
    expect(s.customColors()).toEqual(s.GRAPHITE_COLORS);
    expect(s.settings.theme).toBe('graphite');
    expect(s.settings.colors.dark).toEqual({ accent: '#123456' });
  });

  it('accepts its URL and synchronizes from other windows with old color storage', async () => {
    vi.stubGlobal('location', { search: '?theme=graphite' });
    const style = stubDocument();
    const s = await load(false);
    expect(s.settings.theme).toBe('graphite');
    s.applyTheme();
    expect(style.getPropertyValue('--surface')).toBe(s.GRAPHITE_COLORS.surface);
    s.syncSettings(JSON.stringify({ theme: 'light', colors: { light: {}, dark: {} } }));
    expect(s.colorPalette()).toBe('light');
    s.syncSettings(JSON.stringify({ theme: 'graphite' }));
    expect(s.colorPalette()).toBe('graphite');
    expect(s.customColors()).toEqual(s.GRAPHITE_COLORS);
  });
});

describe('Solarized Soft preset', () => {
  it('persists and reloads without overwriting any other palette', async () => {
    const storage = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => storage.set(k, v) });
    const s = await load(false);
    s.saveSettings({ theme: 'solarized', colors: { light: { text: '#112233' }, dark: { bg: '#000000' }, graphite: { accent: '#abcdef' }, solarized: { accent: '#fedcba' } } });
    const reloaded = await load(false);
    expect(reloaded.settings.theme).toBe('solarized');
    expect(reloaded.resolvedTheme()).toBe('dark');
    expect(reloaded.customColors()).toEqual({ ...reloaded.SOLARIZED_COLORS, accent: '#fedcba' });
    expect(reloaded.customColors('dark')).toEqual({ bg: '#000000' });
    expect(reloaded.customColors('graphite')).toEqual({ ...reloaded.GRAPHITE_COLORS, accent: '#abcdef' });
    reloaded.saveSettings({ theme: 'graphite' });
    expect(reloaded.customColors().accent).toBe('#abcdef');
    reloaded.saveSettings({ theme: 'light' });
    expect(reloaded.customColors()).toEqual({ text: '#112233' });
    reloaded.saveSettings({ theme: 'solarized' });
    expect(reloaded.customColors().accent).toBe('#fedcba');
  });

  it('stays dark through native changes and removes its selector on system mode', async () => {
    stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'solarized' });
    s.systemTheme(false);
    expect(s.resolvedTheme()).toBe('dark');
    expect(document.documentElement.dataset).toEqual({ theme: 'dark', palette: 'solarized' });
    s.saveSettings({ theme: 'auto' });
    expect(document.documentElement.dataset).toEqual({ theme: 'light' });
    s.systemTheme(true);
    expect(document.documentElement.dataset).toEqual({ theme: 'dark' });
  });

  it('resets its overrides without resetting its preset or the other colors', async () => {
    const style = stubDocument();
    const s = await load(false);
    s.saveSettings({ theme: 'solarized', colors: { light: {}, dark: {}, graphite: { accent: '#abcdef' }, solarized: { accent: '#fedcba', bg: 'invalid' } } });
    expect(style.getPropertyValue('--accent')).toBe('#fedcba');
    expect(style.getPropertyValue('--bg')).toBe(s.SOLARIZED_COLORS.bg);
    s.saveSettings({ colors: { ...s.settings.colors, solarized: {} } });
    expect(s.colorOverrides()).toEqual({});
    expect(s.customColors()).toEqual(s.SOLARIZED_COLORS);
    expect(s.settings.theme).toBe('solarized');
    expect(s.settings.colors.graphite).toEqual({ accent: '#abcdef' });
  });

  it('accepts the URL and syncs storage with older color objects', async () => {
    vi.stubGlobal('location', { search: '?theme=solarized' });
    const style = stubDocument();
    const s = await load(false);
    expect(s.settings.theme).toBe('solarized');
    s.applyTheme();
    expect(style.getPropertyValue('--surface')).toBe(s.SOLARIZED_COLORS.surface);
    s.syncSettings(JSON.stringify({ theme: 'dark', colors: { light: {}, dark: {} } }));
    expect(s.colorPalette()).toBe('dark');
    s.syncSettings(JSON.stringify({ theme: 'solarized' }));
    expect(s.colorPalette()).toBe('solarized');
    expect(s.customColors()).toEqual(s.SOLARIZED_COLORS);
  });
});
