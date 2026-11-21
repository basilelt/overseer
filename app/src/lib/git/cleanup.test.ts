import { describe, expect, it } from 'vitest';
import { DaemonError } from '../daemon/client';
import { PLAN_CONCURRENCY, cleanupTargets, planAll, planLines } from './cleanup';
import type { CleanupPlan } from './types';

const ok = (id: string): CleanupPlan => ({ dryRun: true, id, refused: null, remove: [{ kind: 'worktree', path: id }], keep: [] });
const targets = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `/w/${i}`, project: '/repo' }));

describe('planAll', () => {
  it('asks for one dry run per worktree with its project and keeps their order', async () => {
    const calls: unknown[][] = [];
    const api = {
      cleanup: async (id: string, dry: boolean, project?: string) => {
        calls.push([id, dry, project]);
        if (id.endsWith('0')) for (let i = 0; i < 10; i++) await Promise.resolve();
        return ok(id);
      },
    };
    const plans = await planAll(api, targets(5));
    expect(plans.map((p) => p.id)).toEqual(targets(5).map((t) => t.id));
    expect(calls).toHaveLength(5);
    expect(calls.every(([, dry, project]) => dry === true && project === '/repo')).toBe(true);
  });

  it('never has more dry runs in flight than the limit', async () => {
    let inFlight = 0;
    let peak = 0;
    const api = {
      cleanup: async (id: string) => {
        inFlight++;
        peak = Math.max(peak, inFlight);
        await Promise.resolve();
        await Promise.resolve();
        inFlight--;
        return ok(id);
      },
    };
    await planAll(api, targets(58));
    expect(peak).toBe(PLAN_CONCURRENCY);
  });

  it('keeps the other worktrees cleanable when one dry run fails', async () => {
    const api = {
      cleanup: async (id: string) => {
        if (id === '/w/1') throw new DaemonError(500, 'database is locked');
        return ok(id);
      },
    };
    const plans = await planAll(api, targets(3));
    expect(plans[1]).toMatchObject({ id: '/w/1', refused: expect.stringContaining('database is locked'), remove: [] });
    expect(cleanupTargets(plans)).toEqual(['/w/0', '/w/2']);
    expect(planLines(plans[1])).toEqual([{ kind: 'refused', label: 'Could not check: database is locked' }]);
  });

  it('returns nothing for no worktree', async () => {
    expect(await planAll({ cleanup: async () => ok('x') }, [])).toEqual([]);
  });
});
