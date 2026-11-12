import { describe, expect, it, vi } from 'vitest';

async function load(stored: object | null, language = 'en-US') {
  vi.resetModules();
  vi.stubGlobal('navigator', { language });
  vi.stubGlobal('localStorage', { getItem: () => (stored ? JSON.stringify(stored) : null), setItem: () => {} });
  return import('./settings.svelte');
}

describe('locale setting', () => {
  it('defaults to the system language when it is available', async () => {
    expect((await load(null, 'fr-FR')).settings.locale).toBe('fr');
  });

  it('defaults to English for a system language without translations', async () => {
    expect((await load(null, 'xx-XX')).settings.locale).toBe('en');
  });

  it('keeps a saved locale', async () => {
    expect((await load({ locale: 'fr' }, 'en-US')).settings.locale).toBe('fr');
  });

  it('replaces a saved locale that no longer exists', async () => {
    expect((await load({ locale: 'xx' }, 'en-US')).settings.locale).toBe('en');
    expect((await load({ locale: 42 }, 'fr-FR')).settings.locale).toBe('fr');
  });
});
