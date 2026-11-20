import type { HoldPlan } from '../daemon/api';
import { t } from '../i18n.svelte';

const TURN = ['atPrompt', 'noToolRunning', 'notNeedsInput'];

export function stalled(p: HoldPlan | 'loading' | undefined): 'waiting' | 'turn' | null {
  if (!p || p === 'loading' || p.error) return null;
  const failed = Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok).map(([k]) => k);
  if (!failed.length || !failed.every((k) => TURN.includes(k))) return null;
  return failed.includes('notNeedsInput') ? 'waiting' : 'turn';
}

export function blockers(p: HoldPlan | 'loading' | undefined): string[] {
  if (!p || p === 'loading') return [];
  const failed = Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok);
  const waiting = failed.some(([k]) => k === 'notNeedsInput');
  const out = p.error ? [p.error] : [];
  for (const [k, c] of failed) {
    if (k === 'atPrompt') {
      if (!waiting) out.push(t(c.detail?.includes('unknown') ? 'hold.why.unknown' : 'hold.why.busy'));
    } else if (k === 'notPinned') out.push(t('hold.why.pinned'));
    else if (k === 'notNeedsInput') out.push(t('hold.why.waiting'));
    else out.push(c.detail || t('hold.why.other'));
  }
  return out;
}
