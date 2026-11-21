import { describe, expect, it, vi } from 'vitest';

async function load(stored: object | null) {
  vi.resetModules();
  vi.stubGlobal('localStorage', { getItem: () => (stored ? JSON.stringify(stored) : null), setItem: () => {} });
  return import('./settings.svelte');
}

describe('usage agents hidden setting', () => {
  it('shows every agent by default', async () => {
    expect((await load(null)).settings.usageAgentsHidden).toEqual([]);
  });

  it('keeps a saved hidden list', async () => {
    expect((await load({ usageAgentsHidden: ['amp', 'vibe'] })).settings.usageAgentsHidden).toEqual(['amp', 'vibe']);
  });

  it('falls back to an empty list when the saved value is not a list', async () => {
    const { DEFAULT_SETTINGS, settings } = await load({ usageAgentsHidden: 'vibe' });
    expect(settings.usageAgentsHidden).toEqual(DEFAULT_SETTINGS.usageAgentsHidden);
    expect(settings.usageAgentsHidden).toEqual([]);
  });
});
