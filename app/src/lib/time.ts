import { coarseMinutes, wholeMinutes } from './duration';
import { t } from './i18n.svelte';

export function age(iso: string, now = Date.now()): string {
  const ms = now - Date.parse(iso);
  if (!Number.isFinite(ms)) return '';
  const c = coarseMinutes(wholeMinutes(ms));
  return c.unit === 'now' ? t('time.now') : t(`time.${c.unit}`, { n: c.n });
}
