import { type Sub, t } from '../i18n.svelte';

type Key = Sub<'github'>;

export function gt(key: Key, params?: Record<string, string | number>): string {
  return t(`github.${key}`, params);
}
