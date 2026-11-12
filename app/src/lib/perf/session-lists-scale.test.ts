import { describe, expect, it, vi } from 'vitest';
import { attention } from '../attention';
import { byState } from '../session/by-state';
import type { SidebarItem } from '../session/grouping';
import type { SessionState } from '../session/session-state';
import type { Folder, Workspace } from '../types';
import { folderSpaces, spaceTrail, spaces } from '../workspace/spaces';
import { bucketByTime, timeBucket } from '../time-bucket';
import { workspaceSessions } from '../workspace/workspace-sessions';
import { workspaceSummary } from '../workspace/workspace-summary';

// Work is counted, never timed: every read of an item field, and every date parse.
const STATES: SessionState[] = ['attention', 'running', 'open', 'paused', 'closed'];
const now = new Date(2026, 10, 6, 14, 30).getTime();

function fixture(n: number) {
  let reads = 0;
  const wsOf = new Map<string, string>();
  const items = Array.from({ length: n }, (_, i) => {
    const at = new Date(now - ((i * 7919) % 4_000_000) * 1000).toISOString();
    const target = { key: `k${i}`, kind: i % 10 === 6 ? 'shell' : 'claude', group: 'ready', state: STATES[i % 5]!, title: 't', at } as SidebarItem;
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

describe('session lists scale with the session count', () => {
  it('workspaceSummary reads each session a constant number of times', () => {
    const [small, big] = scale((f) => workspaceSummary(f.items));
    expect(big / small).toBe(10);
    expect(small / 100).toBeLessThanOrEqual(3);
  });

  it('attention, byState and workspaceSessions stay linearithmic, never quadratic', () => {
    for (const run of [(f: ReturnType<typeof fixture>) => attention(f.items, f.wsOf), (f: ReturnType<typeof fixture>) => byState(f.items, f.wsOf, now, new Set(['w1', 'w2'])), (f: ReturnType<typeof fixture>) => workspaceSessions(f.items, now), (f: ReturnType<typeof fixture>) => workspaceSessions(f.items, now, Array.from({ length: f.items.length }, (_, i) => `k${f.items.length - 1 - i}`)), (f: ReturnType<typeof fixture>) => spaces(Array.from({ length: 7 }, (_, i) => ({ id: `w${i}`, name: `ws${i}`, rank: i, pinned: false, archived: false }) as Workspace), f.items, f.wsOf, 't')]) {
      const [small, big] = scale(run);
      expect(big / small).toBeGreaterThanOrEqual(10);
      expect(big / small).toBeLessThan(20);
    }
  });

  it('folderSpaces and spaceTrail scale with workspaces and folders, never quadratic', () => {
    const reads = [100, 1000].map((n) => {
      const f = fixture(n);
      let count = 0;
      const spy = <T extends object>(o: T): T =>
        new Proxy(o, {
          get(x, p) {
            count++;
            return x[p as keyof T];
          },
        });
      const folders = Array.from({ length: n / 5 }, (_, i) => spy({ id: `f${i}`, name: `f${i}`, parentId: i % 3 ? `f${i - 1}` : null, rank: i, collapsed: i % 7 === 0 } as Folder));
      const workspaces = Array.from({ length: n }, (_, i) => spy({ id: `w${i}`, name: `ws${i}`, rank: i, pinned: i % 50 === 0, archived: i % 9 === 0, folderId: `f${i % (n / 5)}`, parentId: i % 4 === 1 ? `w${i - 1}` : null } as Workspace));
      const wsOf = new Map(f.items.map((it, i) => [`k${i}`, `w${(i * 31) % n}`]));
      folderSpaces(spaces(workspaces, f.items, wsOf, '', true), folders, '');
      const trail = spaceTrail({ workspaces, folders, groups: [] });
      for (let i = 0; i < n; i++) trail(`w${i}`);
      return count + f.reads();
    });
    expect(reads[1]! / reads[0]!).toBeGreaterThanOrEqual(10);
    expect(reads[1]! / reads[0]!).toBeLessThan(20);
  });

  it('parses each date once: timeBucket, bucketByTime, byState and workspaceSessions', () => {
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
