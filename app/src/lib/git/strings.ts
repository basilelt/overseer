import { t, type Key } from '../i18n.svelte';

type Strip<T> = T extends `git.${infer K}` ? K : never;
export type GKey = Strip<Key>;

export function gt(key: GKey, params?: Record<string, string | number>): string {
  return t(`git.${key}` as Key, params);
}
