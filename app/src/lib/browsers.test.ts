import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

const { browsers, openOutside } = await import('./browsers.svelte');

describe('installed browsers', () => {
  beforeEach(() => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {}, open: vi.fn() });
    invoke.mockReset();
  });
  afterEach(() => vi.unstubAllGlobals());

  it('asks the app once on desktop and never on phones', async () => {
    invoke.mockResolvedValue([{ id: 'firefox', name: 'Firefox' }]);
    expect(await browsers.load(false)).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
    await browsers.load(true);
    expect(await browsers.load(true)).toEqual([{ id: 'firefox', name: 'Firefox' }]);
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(await browsers.load(false)).toEqual([]);
  });

  it('opens with the chosen browser and falls back to the default one', async () => {
    invoke.mockResolvedValue(undefined);
    await openOutside('https://a.dev', { kind: 'browser', id: 'firefox' });
    expect(invoke.mock.calls).toEqual([['browser_open_with', { id: 'firefox', url: 'https://a.dev' }]]);
    invoke.mockReset();
    invoke.mockRejectedValueOnce(new Error('gone')).mockResolvedValue(undefined);
    await openOutside('https://a.dev', { kind: 'browser', id: 'firefox' });
    expect(invoke.mock.calls.map((c) => c[0])).toEqual(['browser_open_with', 'open_external']);
  });

  it('uses window.open outside the app', async () => {
    const open = vi.fn();
    vi.stubGlobal('window', { open });
    await openOutside('https://a.dev', { kind: 'default' });
    expect(open).toHaveBeenCalledWith('https://a.dev', '_blank', 'noopener');
    expect(invoke).not.toHaveBeenCalled();
  });
});
