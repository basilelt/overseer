import type { BroadcastRequest, BroadcastResult } from '../types';

export type ComposeMode = 'queue' | 'now';

export interface ComposeTarget {
  lineId?: string;
  termId?: string;
}

export const COMPOSE_MAX = 64 * 1024;

export function composeTarget(selectedKey: string | null): ComposeTarget | null {
  if (!selectedKey) return null;
  if (selectedKey.startsWith('line:')) return { lineId: selectedKey.slice(5) };
  if (selectedKey.startsWith('term:')) return { termId: selectedKey.slice(5) };
  return null;
}

export function composeRequest(text: string, target: ComposeTarget | null, mode: ComposeMode): BroadcastRequest | null {
  const body = text.trim();
  if (!body || !target || body.length > COMPOSE_MAX) return null;
  const ids: Pick<BroadcastRequest, 'lineIds' | 'termIds'> = target.lineId ? { lineIds: [target.lineId] } : target.termId ? { termIds: [target.termId] } : {};
  if (!ids.lineIds && !ids.termIds) return null;
  return mode === 'now' ? { text: body, ...ids, onlyIdle: false } : { text: body, ...ids, onlyIdle: true, queue: true };
}

export type ComposeOutcome = { kind: 'sent' } | { kind: 'queued' } | { kind: 'failed'; reason: string };

export function composeOutcome(results: BroadcastResult[] | undefined): ComposeOutcome {
  const r = results?.[0];
  if (!r) return { kind: 'failed', reason: 'no target' };
  if (r.result === 'sent') return { kind: 'sent' };
  if (r.result === 'queued') return { kind: 'queued' };
  return { kind: 'failed', reason: r.reason ?? r.result };
}
