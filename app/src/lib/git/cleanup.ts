import type { GitApi } from './api';
import { gt } from './strings';
import type { CleanupPlan } from './types';

export interface PlanLine {
  kind: 'remove' | 'keep' | 'refused';
  label: string;
  detail?: string;
}

export function planLines(p: CleanupPlan): PlanLine[] {
  if (p.refused) return [{ kind: 'refused', label: `${gt('refused')}: ${p.refused}` }];
  const out: PlanLine[] = [];
  for (const r of p.remove) {
    if (r.kind === 'worktree') out.push({ kind: 'remove', label: gt('removeDir') });
    else if (r.kind === 'branch') out.push({ kind: 'remove', label: gt('deleteBranch'), detail: r.name });
    else out.push({ kind: 'remove', label: gt('prune') });
  }
  for (const k of p.keep) out.push({ kind: 'keep', label: `${gt('keep')} ${k.name ?? k.path ?? ''}`.trim(), detail: k.reason });
  return out;
}

export function cleanupTargets(plans: CleanupPlan[]): string[] {
  return plans.filter((p) => !p.refused && p.remove.length).map((p) => p.id);
}

export async function runCleanup(api: GitApi, ids: string[], projectOf: (id: string) => string | undefined = () => undefined) {
  const done: CleanupPlan[] = [];
  const failed: { id: string; error: string }[] = [];
  for (const id of ids) {
    try {
      done.push(await api.cleanup(id, false, projectOf(id)));
    } catch (e) {
      failed.push({ id, error: String((e as Error)?.message ?? e) });
    }
  }
  return { done, failed };
}
