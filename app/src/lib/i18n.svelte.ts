import { en } from './i18n/en';

export type Key = keyof typeof en;
export type Dict = Record<Key, string>;
export type Sub<N extends string, K = Key> = K extends `${N}.${infer S}` ? S : never;

const files = import.meta.glob<{ dict: Partial<Dict> }>(['./i18n/*.ts', '!./i18n/en.ts', '!./i18n/*.test.ts']);
const fileOf = (l: string) => `./i18n/${l}.ts`;

export type Locale = string;
export const locales: Locale[] = ['en', ...Object.keys(files).map((f) => f.slice(7, -3))];

export function isLocale(l: unknown): l is Locale {
  return typeof l === 'string' && locales.includes(l);
}

export function localeName(l: Locale): string {
  try {
    const n = new Intl.DisplayNames([l], { type: 'language' }).of(l) ?? l;
    return n.charAt(0).toLocaleUpperCase(l) + n.slice(1);
  } catch {
    return l;
  }
}

export function systemLocale(): Locale {
  const l = globalThis.navigator?.language?.split('-')[0];
  return isLocale(l) ? l : 'en';
}

let locale = $state<Locale>('en');
let loaded = $state.raw<Record<Locale, Partial<Dict>>>({ en });
const pending = new Map<Locale, Promise<void>>();

export function loadLocale(l: Locale): Promise<void> {
  const load = files[fileOf(l)];
  if (loaded[l] || !load) return Promise.resolve();
  let p = pending.get(l);
  if (!p) {
    p = load()
      .then((m) => {
        loaded = { ...loaded, [l]: m.dict };
      })
      .catch(() => {
        pending.delete(l);
      });
    pending.set(l, p);
  }
  return p;
}

export function setLocale(l: Locale) {
  locale = isLocale(l) ? l : 'en';
  void loadLocale(locale);
}

export function getLocale(): Locale {
  return locale;
}

export function t(key: Key, params?: Record<string, string | number>): string {
  const s = loaded[locale]?.[key] ?? en[key] ?? key;
  return params ? s.replace(/\{(\w+)\}/g, (_, k) => String(params[k] ?? `{${k}}`)) : s;
}
