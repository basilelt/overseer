import type { HoldPlan } from '../daemon/api';

const TURN = ['atPrompt', 'noToolRunning', 'notNeedsInput'];

export function stalled(p: HoldPlan | 'loading' | undefined): 'waiting' | 'turn' | null {
  if (!p || p === 'loading' || p.error) return null;
  const failed = Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok).map(([k]) => k);
  if (!failed.length || !failed.every((k) => TURN.includes(k))) return null;
  return failed.includes('notNeedsInput') ? 'waiting' : 'turn';
}
