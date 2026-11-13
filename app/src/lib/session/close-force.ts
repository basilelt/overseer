import type { HoldPlan } from '../daemon/api';

export function waitingOnly(p: HoldPlan | 'loading' | undefined): boolean {
  if (!p || p === 'loading' || p.error) return false;
  const failed = Object.entries(p.predicates ?? {}).filter(([, c]) => !c.ok);
  return failed.length === 1 && failed[0][0] === 'notNeedsInput';
}
