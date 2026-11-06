import { afterEach, describe, expect, it, vi } from 'vitest';
import { attention } from '../attention';
import { byState } from '../by-state';
import type { SidebarItem } from '../grouping';
import type { SessionState } from '../session-state';
import type { Workspace } from '../types';
import { spaces } from '../spaces';
import { bucketByTime, timeBucket } from '../time-bucket';
import { workspaceSessions } from '../workspace-sessions';
import { workspaceSummary } from '../workspace-summary';

// Work is counted, never timed: every read of an item field, and every date parse.
const STATES: SessionState[] = ['attention', 'running', 'open', 'paused', 'closed'];
const now = new Date(2026, 10, 6, 14, 30).getTime();

function fixture(n: number) {
  let reads = 0;
  const wsOf = new Map<string, string>();
  const items = Array.from({ length: n }, (_, i) => {
    const at = new Date(now - ((i * 7919) % 4_000_000) * 1000).toISOString();
    const target = { key: `k${i}`, kind: 'claude', group: 'ready', state: STATES[i % 5]!, title: 't', at } as SidebarItem;
    wsOf.set(target.key, `w${i % 7}`);
    return new Proxy(target, {
      get(o, p) {
        reads++;
        return o[p as keyof SidebarItem];
      },
    });
  });
  return { items, wsOf, reads: () => reads };
}

// Reads for 100 sessions, and for 1000.
function scale(run: (f: ReturnType<typeof fixture>) => unknown): [number, number] {
  const out = [100, 1000].map((n) => {
    const f = fixture(n);
    run(f);
    return f.reads();
  });
  return [out[0]!, out[1]!];
}

afterEach(() => vi.restoreAllMocks());

describe('session lists scale with the session count', () => {
  it('workspaceSummary reads each session a constant number of times', () => {
    // Given 100 and 1000 sessions in mixed states
    // When the workspace summary is computed
    // Then the reads per session are identical and the work grows exactly 10x
    const [small, big] = scale((f) => workspaceSummary(f.items));
    expect(big / small).toBe(10);
    expect(small / 100).toBeLessThanOrEqual(3);
  });

  it('attention, byState and workspaceSessions stay linearithmic, never quadratic', () => {
    // Given 100 and 1000 sessions in mixed states, spread over 7 workspaces
    // When each list is built
    // Then 10x the sessions costs well under the 100x of a quadratic pass
    for (const run of [(f: ReturnType<typeof fixture>) => attention(f.items, f.wsOf), (f: ReturnType<typeof fixture>) => byState(f.items, f.wsOf, now, new Set(['w1', 'w2'])), (f: ReturnType<typeof fixture>) => workspaceSessions(f.items, now), (f: ReturnType<typeof fixture>) => spaces(Array.from({ length: 7 }, (_, i) => ({ id: `w${i}`, name: `ws${i}`, rank: i, pinned: false, archived: false }) as Workspace), f.items, f.wsOf, 't')]) {
      const [small, big] = scale(run);
      expect(big / small).toBeGreaterThanOrEqual(10);
      expect(big / small).toBeLessThan(20);
    }
  });

  it('parses each date once: timeBucket, bucketByTime, byState and workspaceSessions', () => {
    // Given 1000 sessions with dates
    // When the closed ones are bucketed by interval
    // Then every row costs a single date parse
    const parse = vi.spyOn(Date, 'parse');
    const f = fixture(1000);
    const closed = f.items.filter((i) => i.state === 'closed');
    for (const run of [() => bucketByTime(closed, now), () => byState(f.items, f.wsOf, now), () => workspaceSessions(f.items, now)]) {
      parse.mockClear();
      run();
      expect(parse.mock.calls.length).toBe(closed.length);
    }
    parse.mockClear();
    for (let i = 0; i < 1000; i++) timeBucket('2026-11-05T10:00:00Z', now);
    expect(parse.mock.calls.length).toBe(1000);
  });
});
