import { t } from './i18n.svelte';
import type { ConnPhase, ConnState } from './types';

export function connLabel(c: Pick<ConnState, 'phase' | 'attempt' | 'version'>): string {
  if (c.phase === 'installing') return t('conn.installing', { version: c.version ?? '' });
  if (c.phase === 'reconnecting') return t('conn.reconnecting', { n: c.attempt ?? 1 });
  return t(`conn.${c.phase}`);
}

export const HOST_CHIP_DOT: Record<ConnPhase, string> = {
  idle: 'var(--st-held)',
  connecting: 'var(--st-working)',
  checking: 'var(--st-working)',
  downloading: 'var(--st-working)',
  installing: 'var(--st-working)',
  starting: 'var(--st-working)',
  reconnecting: 'var(--st-working)',
  connected: 'var(--st-input)',
  error: 'var(--st-failed)',
};

export function phoneHostDot(phase: ConnPhase): string {
  return phase === 'connected' ? 'var(--st-input)' : phase === 'error' ? 'var(--st-failed)' : 'var(--st-held)';
}
