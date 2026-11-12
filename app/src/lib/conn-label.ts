import { t } from './i18n.svelte';
import type { ConnPhase, ConnState } from './types';

export function connLabel(c: Pick<ConnState, 'phase' | 'attempt' | 'version'>): string {
  if (c.phase === 'installing') return t('conn.installing', { version: c.version ?? '' });
  if (c.phase === 'reconnecting') return t('conn.reconnecting', { n: c.attempt ?? 1 });
  return t(`conn.${c.phase}`);
}

export const HOST_WORKING_DOT = 'var(--st-working)';

export function hostChipDot(phase: ConnPhase): string {
  return phase === 'idle' ? 'var(--st-held)' : phase === 'connected' ? 'var(--st-input)' : phase === 'error' ? 'var(--st-failed)' : HOST_WORKING_DOT;
}
