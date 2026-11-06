import { t } from './i18n.svelte';
import type { ConnState } from './types';

// The connection phase in words, with its attempt or version filled in.
export function connLabel(c: Pick<ConnState, 'phase' | 'attempt' | 'version'>): string {
  if (c.phase === 'installing') return t('conn.installing', { version: c.version ?? '' });
  if (c.phase === 'reconnecting') return t('conn.reconnecting', { n: c.attempt ?? 1 });
  return t(`conn.${c.phase}`);
}
