import { describe, expect, it } from 'vitest';
import { costRows, type CostBy } from '../cost';
import type { Task } from '../types';

// Work is counted, never timed: every read of a task field.
function fixture(n: number) {
  let reads = 0;
  const tasks = Array.from({ length: n }, (_, i) => {
    const target = {
      id: `t${i}`, number: i, title: `Task ${i}`, description: '', project: '/p', status: i % 3 ? 'completed' : 'in_progress', agent: i % 2 ? 'claude' : 'codex',
      model: '', effort: 'high', rank: i, station: 's', createdAt: '', updatedAt: '', profile: `p${i % 5}`, ticketKey: i % 4 ? `AB-${i % 40}` : null,
      cost: i % 6 ? { input: 1, cacheWrite: 2, cacheRead: 3, output: 4, cost: i / 100, partial: false, sessions: 1 } : null,
    } as Task;
    return new Proxy(target, {
      get(o, p) {
        reads++;
        return o[p as keyof Task];
      },
    });
  });
  return { tasks, reads: () => reads };
}

function scale(by: CostBy): [number, number] {
  const out = [200, 2000].map((n) => {
    const f = fixture(n);
    costRows(f.tasks, by, (id) => id, '-');
    return f.reads();
  });
  return [out[0]!, out[1]!];
}

describe('cost rows scale', () => {
  for (const by of ['profile', 'ticket', 'task'] as const) {
    it(`reads each task a bounded number of times when grouping by ${by}`, () => {
      const [small, big] = scale(by);
      expect(big).toBeLessThan(small * 11);
      expect(big / 2000).toBeLessThanOrEqual(12);
    });
  }
});
