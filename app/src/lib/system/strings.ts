import { type Sub, t } from '../i18n.svelte';

type Key = Sub<'system'>;

export function st(key: Key, params?: Record<string, string | number>): string {
  return t(`system.${key}`, params);
}
