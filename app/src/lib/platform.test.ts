import { describe, expect, it, vi } from 'vitest';

async function load(platform: string, ua: string) {
  vi.resetModules();
  vi.stubGlobal('navigator', { platform, userAgent: ua });
  return import('./platform');
}

describe('platform', () => {
  it('detects a mac from navigator.platform', async () => {
    const p = await load('MacIntel', 'Mozilla');
    expect(p.isMac).toBe(true);
    expect(p.modLabel()).toBe('⌘');
    expect(p.platformName()).toBe('MacIntel');
  });

  it('is not a mac on linux and labels the modifier Ctrl', async () => {
    const p = await load('Linux x86_64', 'X11');
    expect(p.isMac).toBe(false);
    expect(p.modLabel()).toBe('Ctrl');
  });

  it('detects windows from the user agent', async () => {
    expect((await load('Win32', 'Mozilla (Windows NT 10)')).isWindowsUA()).toBe(true);
    expect((await load('Linux', 'X11')).isWindowsUA()).toBe(false);
  });

  it('falls back to empty strings without navigator', async () => {
    vi.resetModules();
    vi.stubGlobal('navigator', undefined);
    const p = await import('./platform');
    expect(p.platformName()).toBe('');
    expect(p.userAgent()).toBe('');
    expect(p.isMac).toBe(false);
  });

  it('detects the Tauri runtime from the injected internals', async () => {
    const p = await load('Linux', 'X11');
    expect(p.isTauri()).toBe(false);
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    expect(p.isTauri()).toBe(true);
    vi.stubGlobal('window', undefined);
    expect(p.isTauri()).toBe(false);
  });
});
