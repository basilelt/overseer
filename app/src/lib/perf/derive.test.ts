import { describe, expect, it } from 'vitest';
import { groupItems, newestFirst, sidebarItems, stableItems } from '../session/grouping';
import { getLocale, loadLocale, setLocale, t } from '../i18n.svelte';
import { MockDaemon } from '../daemon/mock';
import { unplacedCount } from '../workspace/organize';
import { applyEvent, deepEqual, type Snapshot } from '../reduce';
import type { DaemonEvent, LocalLine, Task, Term, Workspace } from '../types';
import { aggregate, itemWorkspaces, liveWorkspaces, memberIndex, sameMap } from '../workspace/workspaces';

const at = '2026-10-29T10:00:00Z';
const line = (id: string, path: string, over: Partial<LocalLine['line']> = {}): LocalLine => ({
  line: { id, agent: 'claude', state: 'active', lastActivityAt: at, worktreePath: path, ...over },
  local: { title: id },
});
const ws = (id: string, path: string, over: Partial<Workspace> = {}): Workspace => ({
  id,
  name: id,
  path,
  folderId: null,
  pinned: false,
  archived: false,
  auto: true,
  rank: 1,
  tabs: [],
  notifyIdle: false,
  createdAt: at,
  updatedAt: at,
  ...over,
});
const event = (type: DaemonEvent['type'], data: unknown): DaemonEvent => ({ id: '1', type, at, data });

describe('stable sidebar derivations', () => {
  const lines = Array.from({ length: 50 }, (_, i) => line(`l${i}`, `/repo/a/wt-${i}`));
  const terms: Term[] = [{ id: 's1', exited: false, cmd: ['zsh'], title: 'zsh', cwd: '/repo/b', createdAt: at } as Term];
  const task = { id: 't1', lineId: 'l1', status: 'in_progress', title: 'T', description: 'D', updatedAt: at } as Task;

  it('returns the previous array when a task only changes its timestamp', () => {
    const first = stableItems([], sidebarItems(lines, terms, [task]));
    const again = stableItems(first, sidebarItems(lines, terms, [{ ...task, updatedAt: '2026-10-29T11:00:00Z' }]));
    expect(again).toBe(first);
  });

  it('reuses unchanged items and replaces only the changed one', () => {
    const first = stableItems([], sidebarItems(lines, terms, [task]));
    const moved = lines.map((l) => (l.line.id === 'l7' ? { ...l, line: { ...l.line, lastActivityAt: '2026-10-29T12:00:00Z' } } : l));
    const next = stableItems(first, sidebarItems(moved, terms, [task]));
    expect(next).not.toBe(first);
    const changed = next.filter((it, i) => it !== first[i]);
    expect(changed.map((i) => i.key)).toEqual(['line:l7']);
    expect(next.find((i) => i.key === 'line:l3')).toBe(first.find((i) => i.key === 'line:l3'));
  });

  it('notices removed items', () => {
    const first = stableItems([], sidebarItems(lines, terms, []));
    expect(stableItems(first, sidebarItems(lines.slice(1), terms, []))).toHaveLength(first.length - 1);
  });

  it('keeps workspace membership identical with memoised prefixes', () => {
    const workspaces = [ws('a', '/repo/a/'), ws('b', '/repo/b'), ws('root', '/')];
    const items = sidebarItems(lines, terms, [task]);
    const snap = { lines, terms, tasks: [task] };
    const first = itemWorkspaces(items, snap, workspaces, []);
    expect(first.get('line:l0')).toBe('a');
    expect(first.get('term:s1')).toBe('b');
    expect(sameMap(first, itemWorkspaces(items, snap, workspaces, []))).toBe(true);
    const moved = [ws('a', '/elsewhere'), workspaces[1], workspaces[2]];
    expect(itemWorkspaces(items, snap, moved, []).get('line:l0')).toBe('root');
  });

  it('memoises the member index per input arrays', () => {
    const workspaces = [ws('a', '/repo/a')];
    const tasks = [task];
    const projects: never[] = [];
    expect(memberIndex(workspaces, projects, tasks)).toBe(memberIndex(workspaces, projects, tasks));
    expect(memberIndex(workspaces, projects, [...tasks])).not.toBe(memberIndex(workspaces, projects, tasks));
    expect(liveWorkspaces(workspaces)).toBe(liveWorkspaces(workspaces));
  });

  it('counts unplaced lines against live workspaces only', () => {
    const now = Date.parse(at);
    const workspaces = [ws('a', '/repo/a', { archived: true })];
    expect(unplacedCount(lines.slice(0, 3), workspaces, [], [], now)).toBe(3);
    expect(unplacedCount(lines.slice(0, 3), [ws('a', '/repo/a')], [], [], now)).toBe(0);
  });

  it('sorts newest first and aggregates without locale collation', () => {
    const items = sidebarItems([line('x', '/x', { lastActivityAt: '2026-10-29T09:00:00Z' }), line('y', '/y', { lastActivityAt: '2026-10-29T11:00:00Z' })], [], []);
    expect(items.slice().sort(newestFirst).map((i) => i.key)).toEqual(['line:y', 'line:x']);
    expect(groupItems(items).find((g) => g.items.length)!.items.map((i) => i.key)).toEqual(['line:y', 'line:x']);
    expect(aggregate(items).at).toBe('2026-10-29T11:00:00Z');
  });
});

describe('reducer equality', () => {
  it('treats missing and undefined fields as equal and ignores key order', () => {
    expect(deepEqual({ a: 1, b: undefined }, { a: 1 })).toBe(true);
    expect(deepEqual({ a: 1, b: [1, { c: 2 }] }, { b: [1, { c: 2 }], a: 1 })).toBe(true);
    expect(deepEqual({ a: [1, 2] }, { a: [2, 1] })).toBe(false);
    expect(deepEqual({ a: null }, { a: undefined })).toBe(false);
  });

  it('keeps the snapshot when an event repeats the current term', () => {
    const term = { id: 'x', exited: false, cmd: ['zsh'], title: 'zsh', cwd: '/', createdAt: at } as Term;
    const s: Snapshot = { lines: [], terms: [term], tasks: [], activity: {} };
    expect(applyEvent(s, event('term.updated', { term: { ...term } }))).toBe(s);
    expect(applyEvent(s, event('term.updated', { term: { ...term, title: 'vim' } }))).not.toBe(s);
  });
});

describe('lazy French dictionary', () => {
  it('falls back to English until loaded, then translates', async () => {
    setLocale('fr');
    expect(getLocale()).toBe('fr');
    await loadLocale('fr');
    expect(t('common.cancel')).toBe('Annuler');
    setLocale('en');
    expect(t('common.cancel')).toBe('Cancel');
  });
});

describe('mock load flags', () => {
  it('scales lines, terms and tasks', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0, scale: 300 });
    const base = new MockDaemon({ live: false, delayMs: 0 });
    expect(m.lines.length - base.lines.length).toBe(300);
    expect(m.tasks.length - base.tasks.length).toBe(300);
    expect(m.terms.length).toBeGreaterThan(base.terms.length + 200);
  });

  it('emits a mixed event storm', () => {
    const m = new MockDaemon({ live: false, delayMs: 0, scale: 20 });
    const seen: string[] = [];
    m.subscribe((e) => seen.push(e.type));
    for (let i = 0; i < 8; i++) m.stormTick();
    expect(new Set(seen)).toEqual(new Set(['line.activity', 'term.updated', 'task.updated', 'line.updated']));
  });
});
