import { t } from './i18n.svelte';

export function age(iso: string, now = Date.now()): string {
  const ms = now - Date.parse(iso);
  if (!Number.isFinite(ms)) return '';
  const m = Math.floor(ms / 60_000);
  if (m < 1) return t('time.now');
  if (m < 60) return t('time.m', { n: m });
  const h = Math.floor(m / 60);
  if (h < 24) return t('time.h', { n: h });
  return t('time.d', { n: Math.floor(h / 24) });
}
