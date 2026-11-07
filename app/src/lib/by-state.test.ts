import { describe, expect, it } from 'vitest';
import { byState } from './by-state';
import type { SidebarItem } from './grouping';
import { stateOfGroup } from './test-fixtures';

// Friday 2026-11-06, 14:30 local.
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

const ids = (list: SidebarItem[]) => list.map((i) => i.key);

describe('byState', () => {
  it('orders the sections running, open, paused, closed', () => {
    expect(byState([], new Map(), now).map((s) => s.state)).toEqual(['running', 'open', 'paused', 'closed']);
  });

  it('keeps sessions that require attention out of every section: they go to the banner', () => {
    const sections = byState([item('a', 'needs_input', day(6)), item('r', 'running', day(6))], new Map(), now);
    expect(sections.map((s) => s.count)).toEqual([1, 0, 0, 0]);
  });

  it('puts one group per workspace in a live section, most recently active workspace first', () => {
    const items = [item('a1', 'running', day(6, 9)), item('b1', 'running', day(6, 13)), item('a2', 'running', day(6, 10))];
    const wsOf = new Map([['a1', 'A'], ['a2', 'A'], ['b1', 'B']]);
    const running = byState(items, wsOf, now)[0]!;
    if (running.state === 'closed') throw new Error('unreachable');
    expect(running.groups.map((g) => [g.workspaceId, ids(g.items)])).toEqual([
      ['B', ['b1']],
      ['A', ['a2', 'a1']],
    ]);
  });

  it('puts sessions outside any workspace in a last group, whatever their recency', () => {
    const items = [item('loose', 'ready', day(6, 14)), item('w', 'ready', day(1))];
    const open = byState(items, new Map([['w', 'W']]), now)[1]!;
    if (open.state === 'closed') throw new Error('unreachable');
    expect(open.groups.map((g) => g.workspaceId)).toEqual(['W', null]);
  });

  it('gives each section its session count and each group its latest activity', () => {
    const items = [item('p1', 'paused', day(2)), item('p2', 'paused', day(4))];
    const paused = byState(items, new Map([['p1', 'W'], ['p2', 'W']]), now)[2]!;
    expect(paused.count).toBe(2);
    if (paused.state === 'closed') throw new Error('unreachable');
    expect(paused.groups[0]!.at).toBe(day(4));
  });

  it('splits the closed section by interval, each bucket grouped by workspace', () => {
    const items = [
      item('c1', 'stopped', day(6, 9)),
      item('c2', 'stopped', day(5)),
      item('c3', 'stopped', day(3)),
      item('c4', 'stopped', new Date(2026, 8, 1).toISOString()),
      item('c5', 'stopped', day(6, 11)),
    ];
    const wsOf = new Map([['c1', 'A'], ['c5', 'B'], ['c2', 'A']]);
    const closed = byState(items, wsOf, now)[3]!;
    if (closed.state !== 'closed') throw new Error('unreachable');
    expect(closed.count).toBe(5);
    expect(closed.buckets.map((b) => [b.bucket, b.count, b.groups.map((g) => [g.workspaceId, ids(g.items)])])).toEqual([
      ['today', 2, [['B', ['c5']], ['A', ['c1']]]],
      ['yesterday', 1, [['A', ['c2']]]],
      ['thisWeek', 1, [[null, ['c3']]]],
      ['older', 1, [[null, ['c4']]]],
    ]);
  });

  it('leaves out empty intervals and ignores plain shells', () => {
    const items = [item('c', 'stopped', day(5)), item('s', 'running', day(6), { kind: 'shell' })];
    const sections = byState(items, new Map(), now);
    expect(sections[0]!.count).toBe(0);
    const closed = sections[3]!;
    if (closed.state !== 'closed') throw new Error('unreachable');
    expect(closed.buckets.map((b) => b.bucket)).toEqual(['yesterday']);
  });

  it('sorts a session with an unreadable date last in its group', () => {
    const items = [item('na', 'running', 'n/a'), item('bad', 'running', ''), item('ok', 'running', day(6))];
    const running = byState(items, new Map(), now)[0]!;
    if (running.state === 'closed') throw new Error('unreachable');
    expect(ids(running.groups[0]!.items)).toEqual(['ok', 'na', 'bad']);
  });

  it('groups sessions of a workspace outside the visible set under no workspace', () => {
    // Given a session whose workspace is archived (not in the visible set)
    // When the sections are built with the visible workspaces
    // Then it lands in the last "no workspace" group, while visible ones keep theirs
    const wsOf = new Map([['x', 'gone'], ['y', 'w1']]);
    const open = byState([item('x', 'ready', day(6)), item('y', 'ready', day(5))], wsOf, now, new Set(['w1']))[1]!;
    expect(open.state === 'open' && open.groups.map((g) => g.workspaceId)).toEqual(['w1', null]);
  });
});
