import '../i18n/parts/tour.use';
import { tourEn } from '../i18n/parts/tour';
import { type Sub, t } from '../i18n.svelte';

type Key = Sub<'tour'>;

export type TourKey = Key;

export function tt(key: Key, params?: Record<string, string | number>): string {
  return t(`tour.${key}`, params);
}

export function hasKey(key: string): key is Key {
  return `tour.${key}` in tourEn;
}
