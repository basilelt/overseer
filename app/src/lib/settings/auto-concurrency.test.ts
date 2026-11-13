import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import { api } from '../daemon/api';

async function load(stored: Record<string, unknown> | null) {
  vi.resetModules();
  const saved: Record<string, string> = {};
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => (k === 'overseer.settings' && stored ? JSON.stringify(stored) : (saved[k] ?? null)),
    setItem: (k: string, v: string) => (saved[k] = v),
  });
  const s = await import('./settings.svelte');
  const c = await import('./auto-concurrency');
  return { ...c, settings: s.settings };
}

let m: MockDaemon;
let a: ReturnType<typeof api>;
beforeEach(() => {
  m = new MockDaemon();
  a = api(m);
});
afterEach(() => vi.unstubAllGlobals());

describe('auto concurrency sync', () => {
  it('adopts the daemon value when nothing was set locally', async () => {
    const { pullAutoConcurrency, settings } = await load(null);
    m.autoConcurrencyValue = 5;
    await pullAutoConcurrency(a);
    expect(settings.autoConcurrency).toBe(5);
    expect(settings.autoConcurrencySynced).toBe(true);
  });

  it('pushes a value set before the daemon knew it, once', async () => {
    const { pullAutoConcurrency, settings } = await load({ autoConcurrency: 6 });
    await pullAutoConcurrency(a);
    expect(m.autoConcurrencyValue).toBe(6);
    expect(settings.autoConcurrencySynced).toBe(true);
    m.autoConcurrencyValue = 3;
    await pullAutoConcurrency(a);
    expect(settings.autoConcurrency).toBe(3);
  });

  it('keeps the local value when the daemon is down', async () => {
    const { pullAutoConcurrency, settings } = await load({ autoConcurrency: 6 });
    vi.spyOn(m, 'request').mockRejectedValue(new Error('daemon down'));
    await expect(pullAutoConcurrency(a)).rejects.toThrow('daemon down');
    expect(settings.autoConcurrency).toBe(6);
    expect(settings.autoConcurrencySynced).toBeUndefined();
  });

  it('stores what the daemon accepted and leaves the value alone when it refuses', async () => {
    const { pushAutoConcurrency, settings } = await load(null);
    expect(await pushAutoConcurrency(a, 4)).toBe(4);
    expect(settings.autoConcurrency).toBe(4);
    await expect(pushAutoConcurrency(a, 17)).rejects.toMatchObject({ status: 400, message: expect.stringContaining('value must be an integer from 1 to 16') });
    expect(settings.autoConcurrency).toBe(4);
  });
});
