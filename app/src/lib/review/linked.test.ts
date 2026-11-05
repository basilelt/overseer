import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../mock';
import { app } from '../state.svelte';
import type { DaemonEvent } from '../types';
import { reviewCounts } from './counts.svelte';
import { LinkedReviews, batchAll, graphOrder, nextStep, parsePick, pickerGroups, submitAll } from './linked.svelte';
import { MOCK_LINKED, MOCK_ROOT, MOCK_WORKTREE } from './mock';
import { ReviewStore } from './store.svelte';
import type { ReviewGraph, SubmitResult, Thread } from './types';

let d: MockDaemon;
let linked: LinkedReviews;

beforeEach(() => {
  d = new MockDaemon({ live: false });
  app.client = d;
  d.subscribe((e: DaemonEvent) => app.onEvent(e));
  vi.spyOn(app, 'notify').mockImplementation(() => {});
  linked = new LinkedReviews();
});

afterEach(() => {
  linked.stop();
  d.stop();
  app.client = null;
  reviewCounts.threads = new Map();
  reviewCounts.resolved = {};
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function pending(wt: string, path: string, n = 1): Thread {
  return {
    id: `${wt}:${path}`,
    worktree: wt,
    path,
    comments: Array.from({ length: n }, (_, i) => ({ id: `c${i}`, pending: true })),
  } as unknown as Thread;
}

const result = { delivery: { id: 'd1' }, threads: [], text: '' } as unknown as SubmitResult;

async function settle() {
  await vi.waitFor(() => expect(linked.stores.every((s) => !s.loading && s.diff)).toBe(true));
}

describe('LinkedReviews', () => {
  it('opens a started store per linked worktree from the primary diff', async () => {
    const primary = new ReviewStore({ worktree: MOCK_WORKTREE });
    await primary.load();
    linked.sync(primary.diff?.linked);
    await settle();
    expect(linked.stores.map((s) => s.target.worktree)).toEqual([MOCK_LINKED]);
    expect(linked.visible[0].diff?.files.map((f) => f.path)).toEqual(['android/app/src/main/Terminal.kt', 'android/app/src/main/Rotation.kt']);
    expect(linked.visible[0].live).toBe(true);
  });

  it('keeps existing stores, stops dropped ones and stops all on teardown', async () => {
    linked.sync([{ worktree: MOCK_LINKED, branch: null }, { worktree: MOCK_ROOT, branch: 'main' }]);
    const [a, b] = linked.stores;
    const stopA = vi.spyOn(a, 'stop');
    const stopB = vi.spyOn(b, 'stop');
    linked.sync([{ worktree: MOCK_LINKED, branch: null }]);
    expect(linked.stores).toEqual([a]);
    expect(stopB).toHaveBeenCalledOnce();
    expect(stopA).not.toHaveBeenCalled();
    const before = linked.stores;
    linked.sync([{ worktree: MOCK_LINKED, branch: null }]);
    expect(linked.stores).toBe(before);
    linked.stop();
    expect(stopA).toHaveBeenCalledOnce();
    expect(linked.stores).toEqual([]);
  });

  it('hides linked worktrees with no changed files and no commits', async () => {
    linked.sync([{ worktree: MOCK_LINKED, branch: null }, { worktree: MOCK_ROOT, branch: 'main' }]);
    await settle();
    expect(linked.stores).toHaveLength(2);
    expect(linked.visible.map((s) => s.worktree)).toEqual([MOCK_LINKED]);
  });

  it('applies the live refresh choice to every linked store', () => {
    linked.sync([{ worktree: MOCK_LINKED, branch: null }, { worktree: MOCK_ROOT, branch: 'main' }]);
    linked.setAutoRefresh(false);
    expect(linked.stores.every((s) => !s.autoRefresh)).toBe(true);
    linked.setAutoRefresh(true);
    expect(linked.stores.every((s) => s.autoRefresh)).toBe(true);
  });
});

describe('submitting across worktrees', () => {
  it('sums pending comments of every store and names linked files by their worktree', () => {
    const primary = new ReviewStore({ worktree: MOCK_WORKTREE });
    primary.threads = [pending(MOCK_WORKTREE, 'src/term.rs', 2)];
    const other = new ReviewStore({ worktree: MOCK_LINKED });
    other.threads = [pending(MOCK_LINKED, 'a.kt')];
    const b = batchAll(primary, [other]);
    expect(b.comments).toBe(3);
    expect(b.threads).toBe(2);
    expect(b.files).toEqual([
      { path: 'src/term.rs', count: 2 },
      { path: 'overseer-mobile-wt/a.kt', count: 1 },
    ]);
  });

  it('submits the primary review, then each linked store that has pending comments', async () => {
    const primary = new ReviewStore({ worktree: MOCK_WORKTREE });
    const busy = new ReviewStore({ worktree: MOCK_LINKED });
    busy.threads = [pending(MOCK_LINKED, 'a.kt')];
    const idle = new ReviewStore({ worktree: MOCK_ROOT });
    const p = vi.spyOn(primary, 'submit').mockResolvedValue(result);
    const b = vi.spyOn(busy, 'submit').mockResolvedValue(result);
    const i = vi.spyOn(idle, 'submit').mockResolvedValue(result);
    expect(await submitAll(primary, [busy, idle], 'request_changes', 'fix it')).toBe(result);
    expect(p).toHaveBeenCalledWith('request_changes', 'fix it');
    expect(b).toHaveBeenCalledWith('request_changes', '');
    expect(i).not.toHaveBeenCalled();
  });

  it('leaves linked comments pending when the primary submit fails', async () => {
    const primary = new ReviewStore({ worktree: MOCK_WORKTREE });
    const busy = new ReviewStore({ worktree: MOCK_LINKED });
    busy.threads = [pending(MOCK_LINKED, 'a.kt')];
    vi.spyOn(primary, 'submit').mockResolvedValue(null);
    const b = vi.spyOn(busy, 'submit');
    expect(await submitAll(primary, [busy], 'comment', '')).toBeNull();
    expect(b).not.toHaveBeenCalled();
  });
});

describe('file picker', () => {
  it('groups linked files by worktree with keys that round-trip', async () => {
    linked.sync([{ worktree: MOCK_LINKED, branch: null }, { worktree: MOCK_ROOT, branch: 'main' }]);
    await settle();
    const groups = pickerGroups(linked.stores);
    expect(groups.map((g) => g.label)).toEqual(['overseer-mobile-wt']);
    expect(groups[0].options[0].label).toBe('android/app/src/main/Terminal.kt (+2 −1)');
    expect(parsePick(groups[0].options[0].value)).toEqual({ worktree: MOCK_LINKED, path: 'android/app/src/main/Terminal.kt' });
    expect(parsePick('src/term.rs')).toEqual({ worktree: null, path: 'src/term.rs' });
  });
});

describe('linked graphs', () => {
  function graphCalls(spy: { mock: { calls: unknown[][] } }): string[] {
    return spy.mock.calls.filter((c) => c[0] === 'GET' && String(c[1]).startsWith('/v1/review/graph')).map((c) => decodeURIComponent(String(c[1])));
  }

  it('loads no linked graph in list view and loads each one when the graph view opens', async () => {
    const spy = vi.spyOn(d.review, 'route');
    linked.sync([{ worktree: MOCK_LINKED, branch: null }]);
    await settle();
    expect(graphCalls(spy)).toEqual([]);
    linked.setView('graph');
    await vi.waitFor(() => expect(linked.stores[0].graph).not.toBeNull());
    expect(graphCalls(spy).filter((c) => c.includes(MOCK_LINKED))).toHaveLength(1);
    expect(graphOrder(linked.stores[0]).length).toBeGreaterThan(0);
  });

  it('opens new linked stores in the current view', async () => {
    linked.setView('graph');
    linked.sync([{ worktree: MOCK_LINKED, branch: null }]);
    await vi.waitFor(() => expect(linked.stores[0].graph).not.toBeNull());
    expect(linked.stores[0].view).toBe('graph');
  });

  function fake(ids: string[], at = -1): ReviewStore {
    const nodes = ids.map((id) => ({ id, name: id, path: `${id}.ts`, line: 1, hunks: [] }));
    return { graph: { revision: ids.join(), nodes, edges: [], order: ids } as unknown as ReviewGraph, orderAt: at } as unknown as ReviewStore;
  }

  it('continues the order walk in the next linked worktree that has nodes', () => {
    const a = fake(['x', 'y'], 0);
    const empty = fake([]);
    const b = fake(['x', 'z']);
    expect(nextStep([a, empty, b], a)).toMatchObject({ store: a, at: 1, node: { id: 'y' } });
    a.orderAt = 1;
    const st = nextStep([a, empty, b], a)!;
    expect(st.store).toBe(b);
    expect(st.at).toBe(0);
    expect(st.node).toBe(graphOrder(b)[0]);
    expect(st.node).not.toBe(graphOrder(a)[0]);
    b.orderAt = 1;
    expect(nextStep([a, empty, b], b)).toBeUndefined();
    expect(nextStep([a], b)).toBeUndefined();
  });

  it('memoizes the order per graph and recomputes for a new graph', () => {
    const s = fake(['a', 'b']);
    const first = graphOrder(s);
    expect(graphOrder(s)).toBe(first);
    s.graph = { ...s.graph!, order: ['b', 'a'] };
    expect(graphOrder(s).map((n) => n.id)).toEqual(['b', 'a']);
    expect(graphOrder({ graph: null } as unknown as ReviewStore)).toEqual([]);
  });
});
