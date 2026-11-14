import { describe, expect, it } from 'vitest';
import { applyUpdate, type ChatState } from '../chat/store';
import { groupItems, sidebarItems, stableItems } from '../session/grouping';
import { applyEvent, type Snapshot } from '../reduce';
import type { Activity, ChatEntry, DaemonEvent, LocalLine, Task, Term, Workspace } from '../types';
import { aggregate, itemWorkspaces, sameMap } from '../workspace/workspaces';

const at = '2026-10-29T10:00:00Z';
const event = (type: DaemonEvent['type'], data: unknown): DaemonEvent => ({ id: '1', type, at, data });

function snapshot(n: number): Snapshot {
  const lines: LocalLine[] = Array.from({ length: n }, (_, i) => ({
    line: { id: `l${i}`, agent: 'claude', state: 'active', lastActivityAt: at, worktreePath: `/repo/p${i % 20}/wt-${i}`, branch: `b${i}` },
    local: { title: `line ${i}`, needsInput: false, busy: i % 3 === 0 },
  }));
  const terms = Array.from({ length: n / 4 }, (_, i) => ({ id: `s${i}`, exited: i % 5 === 0, cmd: ['zsh'], title: `zsh ${i}`, cwd: `/repo/p${i % 20}`, createdAt: at }) as Term);
  const tasks = Array.from({ length: n / 4 }, (_, i) => ({ id: `t${i}`, lineId: `l${i * 4}`, status: i % 7 === 0 ? 'in_testing' : 'in_progress', title: `T${i}`, description: 'D', project: `/repo/p${i % 20}`, updatedAt: at }) as Task);
  const activity: Record<string, Activity> = {};
  for (let i = 0; i < n; i += 10) activity[`l${i}`] = { workLog: [], lastAssistantText: `a${i}`, needsInput: false } as unknown as Activity;
  return { lines, terms, tasks, activity };
}

const workspaces: Workspace[] = Array.from({ length: 20 }, (_, i) => ({
  id: `w${i}`,
  name: `w${i}`,
  path: `/repo/p${i}`,
  folderId: null,
  pinned: false,
  archived: false,
  auto: true,
  rank: i,
  tabs: [],
  notifyIdle: false,
  createdAt: at,
  updatedAt: at,
}));

const LINEAR = 30;

function reps(fn: () => void): number {
  let n = 1;
  for (;;) {
    const t = performance.now();
    for (let i = 0; i < n; i++) fn();
    if (performance.now() - t >= 2 || n >= 4096) return n;
    n *= 2;
  }
}

function per(fn: () => void, n: number): number {
  const t = performance.now();
  for (let i = 0; i < n; i++) fn();
  return (performance.now() - t) / n;
}

function scaling(small: () => void, big: () => void, runs = 7): number {
  let best = Infinity;
  for (let attempt = 0; attempt < 3 && best >= LINEAR; attempt++) {
    const ns = reps(small);
    const nb = reps(big);
    let a = Infinity;
    let b = Infinity;
    for (let i = 0; i < runs; i++) {
      a = Math.min(a, per(small, ns));
      b = Math.min(b, per(big, nb));
    }
    best = Math.min(best, b / a);
  }
  return best;
}

describe('snapshot reducer at scale', () => {
  const s = snapshot(2000);

  it('a line update replaces one line and shares every other slice', () => {
    const next = applyEvent(s, event('line.updated', { line: { id: 'l777', branch: 'moved' } }));
    expect(next.terms).toBe(s.terms);
    expect(next.tasks).toBe(s.tasks);
    expect(next.activity).toBe(s.activity);
    const replaced = next.lines.filter((l, i) => l !== s.lines[i]);
    expect(replaced.map((l) => l.line.id)).toEqual(['l777']);
    expect(next.lines.length).toBe(s.lines.length);
  });

  it('a term update leaves lines, tasks and activity untouched', () => {
    const next = applyEvent(s, event('term.updated', { term: { id: 's42', title: 'vim' } }));
    expect(next.lines).toBe(s.lines);
    expect(next.tasks).toBe(s.tasks);
    expect(next.activity).toBe(s.activity);
    expect(next.terms.filter((t, i) => t !== s.terms[i]).map((t) => t.id)).toEqual(['s42']);
  });

  it('a task update leaves lines, terms and activity untouched', () => {
    const task: Task = { ...s.tasks[9], status: 'in_testing' };
    const next = applyEvent(s, event('task.updated', { task }));
    expect(next.lines).toBe(s.lines);
    expect(next.terms).toBe(s.terms);
    expect(next.activity).toBe(s.activity);
    expect(next.tasks.filter((t, i) => t !== s.tasks[i])).toEqual([task]);
  });

  it('activity that keeps needsInput only touches the activity map', () => {
    const activity = { workLog: [], lastAssistantText: 'new', needsInput: false } as unknown as Activity;
    const next = applyEvent(s, event('line.activity', { lineId: 'l1500', activity }));
    expect(next.lines).toBe(s.lines);
    expect(next.terms).toBe(s.terms);
    expect(next.tasks).toBe(s.tasks);
    expect(Object.keys(next.activity).filter((k) => next.activity[k] !== s.activity[k])).toEqual(['l1500']);
  });

  it('a storm of repeated events never allocates a new snapshot', () => {
    const storm = [
      event('line.updated', { line: { ...s.lines[5].line }, local: { ...s.lines[5].local } }),
      event('line.state_changed', { line: { id: 'l9', state: 'active' } }),
      event('term.updated', { term: { ...s.terms[3] } }),
      event('task.updated', { task: { ...s.tasks[2] } }),
      event('line.activity', { lineId: 'l10', activity: { ...s.activity.l10 } }),
      event('line.activity', { lineId: 'l11' }),
    ];
    let cur = s;
    for (let i = 0; i < 1000; i++) cur = applyEvent(cur, storm[i % storm.length]);
    expect(cur).toBe(s);
  });

  it('applying one event scales linearly with the snapshot', () => {
    const small = snapshot(2000);
    const big = snapshot(20000);
    const events = (n: number) => Array.from({ length: 50 }, (_, i) => event('line.updated', { line: { id: `l${(i * 37) % n}`, branch: `x${i}` } }));
    const es = events(2000);
    const eb = events(20000);
    const ratio = scaling(
      () => es.forEach((e) => applyEvent(small, e)),
      () => eb.forEach((e) => applyEvent(big, e)),
    );
    expect(ratio).toBeLessThan(LINEAR);
  });
});

describe('sidebar derivation chain at scale', () => {
  const s = snapshot(2000);
  const projects: never[] = [];

  it('one changed line yields one new item and the same workspace map', () => {
    const items = stableItems([], sidebarItems(s.lines, s.terms, s.tasks));
    const map = itemWorkspaces(items, s, workspaces, projects);
    const next = applyEvent(s, event('line.updated', { line: { id: 'l333', lastActivityAt: '2026-10-29T11:00:00Z' } }));
    const again = stableItems(items, sidebarItems(next.lines, next.terms, next.tasks));
    expect(again.filter((it, i) => it !== items[i]).map((i) => i.key)).toEqual(['line:l333']);
    expect(sameMap(map, itemWorkspaces(again, next, workspaces, projects))).toBe(true);
    expect(stableItems(again, sidebarItems(next.lines, next.terms, next.tasks))).toBe(again);
  });

  it('items, groups, workspace membership and aggregates scale linearly', () => {
    const small = snapshot(1000);
    const big = snapshot(10000);
    const run = (x: Snapshot) => () => {
      const items = stableItems([], sidebarItems(x.lines, x.terms, x.tasks));
      groupItems(items);
      itemWorkspaces(items, x, workspaces, projects);
      aggregate(items);
    };
    expect(scaling(run(small), run(big))).toBeLessThan(LINEAR);
  });
});

describe('chat transcript store at scale', () => {
  const entry = (seq: number): ChatEntry => ({ id: `e${seq}`, seq, rev: 1, at, kind: 'assistant', text: `entry ${seq}` });
  const state = (n: number, from = 10_000): ChatState => ({ entries: Array.from({ length: n }, (_, i) => entry(from + i)), cursor: 1, more: true, model: null, loaded: true });
  const page = (entries: ChatEntry[]) => ({ lineId: 'l', agent: 'claude', cursor: 2, reset: false, more: true, model: null, entries });

  it('appending a new entry keeps every existing entry object', () => {
    const s = state(600);
    const next = applyUpdate(s, page([entry(20_000)]), 1000);
    expect(next.entries.length).toBe(601);
    expect(next.entries.slice(0, 600).every((e, i) => e === s.entries[i])).toBe(true);
  });
});
