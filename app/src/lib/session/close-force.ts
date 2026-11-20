import type { HoldPlan } from '../daemon/api';
import { t } from '../i18n.svelte';

const TURN = ['atPrompt', 'noToolRunning', 'notNeedsInput'];

const unknownInput = ([k, c]: [string, { ok: boolean; unknown?: boolean }]) => k === 'inputEmpty' && c.unknown === true;

export function stalled(p: HoldPlan | 'loading' | undefined): 'waiting' | 'turn' | 'unknown' | null {
  if (!p || p === 'loading' || p.error) return null;
  const failed = Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok);
  if (!failed.length) return null;
  const known = failed.filter((e) => !unknownInput(e));
  if (!known.every(([k]) => TURN.includes(k))) return null;
  if (known.some(([k]) => k === 'notNeedsInput')) return 'waiting';
  if (failed.some(unknownInput)) return 'unknown';
  return 'turn';
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
