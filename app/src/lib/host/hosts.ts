import { t } from '../i18n.svelte';
import type { HostTarget } from '../types';

// Lane 5 is reddish and would read as an error next to a status dot.
const LANES = 4;

/** Local keeps the "ready" green; every SSH host gets one of the lane colours, the same each launch. */
export function hostColor(key: string): string {
  if (key === 'local') return 'var(--st-input)';
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
  return `var(--lane-${(Math.abs(h) % LANES) + 1})`;
}

export function hostName(target: HostTarget): string {
  return target.kind === 'local' ? t('host.local') : target.label || target.host || '';
}
