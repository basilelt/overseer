import type { HoldPlan } from '../daemon/api';

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
