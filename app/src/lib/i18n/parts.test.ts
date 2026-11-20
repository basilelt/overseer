import { afterEach, describe, expect, it, vi } from 'vitest';
import { core } from './parts/core';
import { en } from './en';
import { settingsEn } from './parts/settings';
import { tourEn } from './parts/tour';

const LAZY = { settings: { prefix: 'settings.', dict: settingsEn, use: 'settings.use' }, tour: { prefix: 'tour.', dict: tourEn, use: 'tour.use' } };

afterEach(() => {
  vi.resetModules();
});

describe('split English dictionary', () => {
  it('keeps every key once across the startup and lazy parts', () => {
    const parts = [core, settingsEn, tourEn];
    const total = parts.reduce((n, p) => n + Object.keys(p).length, 0);
    expect(Object.keys(en).length).toBe(total);
  });

  it('keeps each lazy namespace out of the startup dictionary', () => {
    for (const { prefix, dict } of Object.values(LAZY)) {
      expect(Object.keys(dict).every((k) => k.startsWith(prefix))).toBe(true);
      expect(Object.keys(core).filter((k) => k.startsWith(prefix))).toEqual([]);
    }
  });
});

describe('registering a lazy namespace', () => {
  it('answers with the raw key until the namespace is registered, then with English', async () => {
    const { t, setLocale } = await import('../i18n.svelte');
    setLocale('en');
    expect(t('tour.palette')).toBe('tour.palette');
    expect(t('common.cancel')).toBe('Cancel');
    await import('./parts/tour.use');
    expect(t('tour.palette')).toBe(tourEn['tour.palette']);
    expect(t('settings.theme')).toBe('settings.theme');
    await import('./parts/settings.use');
    expect(t('settings.theme')).toBe(settingsEn['settings.theme']);
    expect(t('tour.palette')).toBe(tourEn['tour.palette']);
  });

  it('is idempotent', async () => {
    const { t, registerEn } = await import('../i18n.svelte');
    registerEn(tourEn);
    registerEn(tourEn);
    expect(t('tour.palette')).toBe(tourEn['tour.palette']);
  });

  it('keeps a loaded locale ahead of the registered English', async () => {
    const { t, registerEn, setLocale, loadLocale } = await import('../i18n.svelte');
    await loadLocale('fr');
    setLocale('fr');
    registerEn(tourEn);
    expect(t('tour.palette')).not.toBe(tourEn['tour.palette']);
  });
});

describe('every reader of a lazy namespace registers it', () => {
  const files = Object.entries(
    import.meta.glob<string>(['../**/*.ts', '../**/*.svelte', '!../**/*.test.ts', '!../i18n/**'], { query: '?raw', import: 'default', eager: true }),
  );

  it.each(Object.entries(LAZY))('%s', (name, { prefix, use }) => {
    const reading = new RegExp(`['"\`]${prefix.replace('.', '\\.')}[A-Za-z$]`);
    const registering = new RegExp(`i18n/parts/${use.replace('.', '\\.')}'`);
    const readers = files.filter(([, src]) => reading.test(src) || (name === 'tour' && src.includes("tour/strings'")));
    expect(readers.length).toBeGreaterThan(0);
    const bare = readers.filter(([f, src]) => !registering.test(src) && !(name === 'tour' && src.includes("tour/strings'")) && !f.endsWith('/tour/strings.ts'));
    expect(bare.map(([f]) => f)).toEqual([]);
  });
});
