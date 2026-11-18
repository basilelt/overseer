import { afterEach, describe, expect, it } from 'vitest';
import { stableTabs, tabOrder } from '../tabs/tab-order';
import { bucketByTime } from '../time-bucket';
import { layoutFromTabs, type Layout } from '../workspace/workspace.svelte';

const RealDate = Date;
afterEach(() => {
  globalThis.Date = RealDate;
});

function datesBuilt(run: () => void): number {
  let built = 0;
  globalThis.Date = class extends RealDate {
    constructor(...a: unknown[]) {
      super(...(a as []));
      built++;
    }
  } as DateConstructor;
  try {
    run();
  } finally {
    globalThis.Date = RealDate;
  }
  return built;
}

const rows = (n: number) => Array.from({ length: n }, (_, i) => ({ at: new RealDate(2026, 10, 6 - (i % 12), 9).toISOString() }));

// Every open workspace of the sidebar lists its sessions; a switch must only recompute the lists
// whose tab order changed, and bucketing a list must not build dates per session.
describe('sidebar work on a workspace switch', () => {
  it('builds the same number of dates for 100 and 1,000 closed sessions', () => {
    const now = new RealDate(2026, 10, 6, 14).getTime();
    const small = rows(100);
    const big = rows(1_000);
    const a = datesBuilt(() => bucketByTime(small, now));
    const b = datesBuilt(() => bucketByTime(big, now));
    expect(a).toBeGreaterThan(0);
    expect(b).toBe(a);
    expect(bucketByTime(big, now).map((g) => g.bucket)).toEqual(['today', 'yesterday', 'thisWeek', 'older']);
  });

  function changedOnSwitch(workspaces: number): number {
    const ids = Array.from({ length: workspaces }, (_, i) => `w${i}`);
    const tabs = (id: string) => [`line:${id}-a`, `term:${id}-b`, `line:${id}-c`];
    const cached = new Map<string, Layout>(ids.map((id) => [id, layoutFromTabs(tabs(id))]));
    const memo = new Map<string, string[]>();
    const all = (active: string) => ids.map((id) => stableTabs(memo, id, tabOrder(id, active, tabs(active), cached, tabs(id))));
    const before = all('w0');
    const after = all('w1');
    return after.filter((t, i) => t !== before[i]).length;
  }

  it('hands every workspace the same tab list after a switch, for 84 and 840 workspaces', () => {
    expect(changedOnSwitch(84)).toBe(0);
    expect(changedOnSwitch(840)).toBe(0);
  });
});
