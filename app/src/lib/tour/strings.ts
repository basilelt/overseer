import { en } from '../i18n/en';
import { type Sub, t } from '../i18n.svelte';

type Key = Sub<'tour'>;

export type TourKey = Key;

export function tt(key: Key, params?: Record<string, string | number>): string {
  return t(`tour.${key}`, params);
}

export function hasKey(key: string): key is Key {
  return `tour.${key}` in en;
}
