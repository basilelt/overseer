import { describe, expect, it, vi } from 'vitest';

const KEY = 'overseer.settings';

function storage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k), data };
}

async function load(initial?: Record<string, string>) {
  vi.resetModules();
  const store = storage(initial);
  vi.stubGlobal('localStorage', store);
  const mod = await import('./settings.svelte');
  return { ...mod, store };
}

describe('gpuTerminal setting', () => {
  it('is off by default', async () => {
    const s = await load();
    expect(s.DEFAULT_SETTINGS.gpuTerminal).toBe(false);
    expect(s.settings.gpuTerminal).toBe(false);
  });

  it('stays off for settings saved before the option existed', async () => {
    const s = await load({ [KEY]: JSON.stringify({ fontSize: 14 }) });
    expect(s.settings.gpuTerminal).toBe(false);
    expect(s.settings.fontSize).toBe(14);
  });

  it('persists when turned on and reloads as on', async () => {
    const s = await load();
    s.saveSettings({ gpuTerminal: true });
    expect(s.settings.gpuTerminal).toBe(true);
    expect(JSON.parse(s.store.data.get(KEY)!).gpuTerminal).toBe(true);
    const again = await load({ [KEY]: s.store.data.get(KEY)! });
    expect(again.settings.gpuTerminal).toBe(true);
  });

  it('persists when turned back off', async () => {
    const s = await load({ [KEY]: JSON.stringify({ gpuTerminal: true }) });
    s.saveSettings({ gpuTerminal: false });
    const again = await load({ [KEY]: s.store.data.get(KEY)! });
    expect(again.settings.gpuTerminal).toBe(false);
  });

  it('treats a non-boolean stored value as off', async () => {
    const s = await load({ [KEY]: JSON.stringify({ gpuTerminal: 'yes' }) });
    expect(s.settings.gpuTerminal).toBe(false);
  });
});
