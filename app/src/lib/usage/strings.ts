import { type Sub, t } from '../i18n.svelte';

type Key = Sub<'usage'>;

export function ust(key: Key, params?: Record<string, string | number>): string {
  return t(`usage.${key}`, params);
}
