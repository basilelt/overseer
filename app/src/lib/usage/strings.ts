import { type Sub, t } from '../i18n.svelte';
import type { UsageAgent } from './types';

type Key = Sub<'usage'>;

export function ust(key: Key, params?: Record<string, string | number>): string {
  return t(`usage.${key}`, params);
}

const WINDOW_KEYS: Record<string, Key> = { '5h': 'win5h', wk: 'winWk', mo: 'winMo' };

export function windowLabel(key: string): string {
  const k = WINDOW_KEYS[key];
  return k ? ust(k) : key;
}

export function creditsNote(a: UsageAgent): string {
  const b = a.limits.credits?.balance;
  if (a.limits.credits?.unlimited || b == null) return ust('onCredits');
  return ust('onCreditsLeft', { n: b.toLocaleString(undefined, { maximumFractionDigits: 2 }) });
}
