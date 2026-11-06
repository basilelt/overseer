import { describe, expect, it } from 'vitest';
import type { SidebarItem } from './grouping';
import { stateOfGroup } from './test-fixtures';
import { workspaceSessions } from './workspace-sessions';

const now = new Date(2026, 10, 6, 14, 30).getTime();
const day = (d: number, h = 12) => new Date(2026, 10, d, h).toISOString();

const item = (key: string, group: SidebarItem['group'], at: string, over: Partial<SidebarItem> = {}): SidebarItem => ({
  key,
  kind: 'claude',
  group,
  state: stateOfGroup(group),
  title: key,
  at,
  lineId: key,
  ...over,
});

describe('workspaceSessions', () => {
  it('orders live sessions attention, running, open, paused, most recent first inside a state', () => {
    const items = [
      item('p', 'paused', day(6, 14)),
      item('o', 'ready', day(6, 13)),
      item('r1', 'running', day(6, 9)),
      item('a', 'needs_input', day(1)),
      item('r2', 'running', day(6, 10)),
    ];
    expect(workspaceSessions(items, now).live.map((i) => i.key)).toEqual(['a', 'r2', 'r1', 'o', 'p']);
  });

  it('counts the closed sessions and splits them by interval, most recent first', () => {
    const items = [item('c1', 'stopped', day(3)), item('c2', 'stopped', day(6, 9)), item('c3', 'stopped', day(6, 11)), item('x', 'running', day(6))];
    const { live, closed } = workspaceSessions(items, now);
    expect(live.map((i) => i.key)).toEqual(['x']);
    expect(closed.count).toBe(3);
    expect(closed.buckets.map((b) => [b.bucket, b.items.map((i) => i.key)])).toEqual([
      ['today', ['c3', 'c2']],
      ['thisWeek', ['c1']],
    ]);
  });

  it('is empty for a workspace without sessions, and ignores plain shells', () => {
    expect(workspaceSessions([], now)).toEqual({ live: [], closed: { count: 0, buckets: [] } });
    expect(workspaceSessions([item('s', 'running', day(6), { kind: 'shell' })], now).live).toEqual([]);
  });

  it('treats a session with an unreadable date as the least recent', () => {
    const items = [item('bad', 'ready', ''), item('na', 'ready', 'n/a'), item('ok', 'ready', day(6))];
    expect(workspaceSessions(items, now).live.map((i) => i.key)).toEqual(['ok', 'bad', 'na']);
  });
});
