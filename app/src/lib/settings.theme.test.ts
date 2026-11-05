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
