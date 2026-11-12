import { describe, expect, it, vi } from 'vitest';

function realStorage(initial: Record<string, string>) {
  const data = new Map(Object.entries(initial));
  return {
    get length() {
      return data.size;
    },
    key: (i: number) => [...data.keys()][i] ?? null,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    clear: () => data.clear(),
    data,
  };
}

async function load(initial: Record<string, string>) {
  vi.resetModules();
  const store = realStorage(initial);
  vi.stubGlobal('localStorage', store);
  const mod = await import('./settings.svelte');
  return { ...mod, store };
}

describe('settings inside the demo', () => {
  it('reads the demo-scoped settings, not the real ones, when loaded before the demo boot', async () => {
    const s = await load({
      'overseer.demo': '1',
      'overseer.settings': JSON.stringify({ theme: 'dark' }),
      'overseer.demo:overseer.settings': JSON.stringify({ theme: 'light' }),
    });
    expect(s.settings.theme).toBe('light');
  });

  it('reads the real settings outside the demo', async () => {
    const s = await load({ 'overseer.settings': JSON.stringify({ theme: 'dark' }) });
    expect(s.settings.theme).toBe('dark');
  });
});
