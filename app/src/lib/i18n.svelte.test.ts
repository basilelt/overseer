import { afterEach, describe, expect, it, vi } from 'vitest';
import { getLocale, isLocale, loadLocale, localeName, locales, setLocale, systemLocale, t } from './i18n.svelte';

vi.mock('./i18n/fr.ts', () => ({ dict: { 'common.cancel': 'Annuler' } }));

afterEach(() => {
  setLocale('en');
});

describe('locales', () => {
  it('lists English first and every locale file next to it', () => {
    expect(locales[0]).toBe('en');
    expect(locales).toContain('fr');
    expect(new Set(locales).size).toBe(locales.length);
  });

  it('accepts only known locales', () => {
    expect(isLocale('fr')).toBe(true);
    expect(isLocale('xx')).toBe(false);
    expect(isLocale(undefined)).toBe(false);
  });

  it('names each language in itself', () => {
    expect(localeName('en')).toBe('English');
    expect(localeName('fr')).toBe('Français');
  });

  it('falls back to the code when the runtime cannot name it', () => {
    expect(localeName('not a tag')).toBe('not a tag');
  });
});

describe('systemLocale', () => {
  it('picks the system language when it has a locale file', () => {
    vi.stubGlobal('navigator', { language: 'fr-CA' });
    expect(systemLocale()).toBe('fr');
  });

  it('falls back to English otherwise', () => {
    vi.stubGlobal('navigator', { language: 'xx-YY' });
    expect(systemLocale()).toBe('en');
    vi.stubGlobal('navigator', undefined);
    expect(systemLocale()).toBe('en');
  });
});

describe('t', () => {
  it('uses English for keys the locale has not translated yet', async () => {
    setLocale('fr');
    await loadLocale('fr');
    expect(t('common.cancel')).toBe('Annuler');
    expect(t('common.close')).toBe('Close');
  });

  it('drops the plural suffix when the count is 1', () => {
    expect(t('review.filesChanged', { n: 1 })).toBe('1 file');
    expect(t('review.filesChanged', { n: 2 })).toBe('2 files');
    expect(t('review.filesChanged', { n: 0 })).toBe('0 files');
    expect(t('review.fold.lines', { n: 1 })).toBe('1 line');
    expect(t('git.commits', { n: 1 })).toBe('1 commit');
  });

  it('spells the plural with es where the word needs it', () => {
    expect(t('system.procs', { n: 1 })).toBe('1 process');
    expect(t('system.procs', { n: 3 })).toBe('3 processes');
  });

  it('leaves strings without a suffix placeholder alone', () => {
    expect(t('review.pending', { n: 1 })).toBe('1 pending');
  });

  it('only uses suffix placeholders in strings that count with n', async () => {
    const { en } = await import('./i18n/en');
    const bad = Object.entries(en).filter(([, v]) => /\{e?s\}/.test(v) && !v.includes('{n}'));
    expect(bad).toEqual([]);
  });

  it('keeps a locale own string when the count is 1', async () => {
    setLocale('fr');
    await loadLocale('fr');
    expect(t('common.cancel', { n: 1 })).toBe('Annuler');
  });

  it('switches to English for an unknown locale', () => {
    setLocale('xx');
    expect(getLocale()).toBe('en');
    expect(t('common.cancel')).toBe('Cancel');
  });

  it('resolves at once for English and unknown locales', async () => {
    await expect(loadLocale('en')).resolves.toBeUndefined();
    await expect(loadLocale('xx')).resolves.toBeUndefined();
  });

  it('shares one load between concurrent callers', async () => {
    vi.resetModules();
    const fresh = await import('./i18n.svelte');
    expect(fresh.loadLocale('fr')).toBe(fresh.loadLocale('fr'));
  });
});
