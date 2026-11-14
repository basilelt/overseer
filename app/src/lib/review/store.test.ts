import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError } from '../daemon/client';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import type { DaemonEvent, LocalLine } from '../types';
import { reviewCounts } from './counts.svelte';
import type { Draft } from './draft';
import { MOCK_ROOT, MOCK_WORKTREE } from './mock';
import { ReviewStore } from './store.svelte';
import type { Delivery, ReviewTarget, Thread } from './types';

let d: MockDaemon;
let notify: ReturnType<typeof vi.spyOn>;

function connect() {
  d = new MockDaemon({ live: false });
  app.client = d;
  d.subscribe((e: DaemonEvent) => app.onEvent(e));
}

async function loaded(target: ReviewTarget = { worktree: MOCK_WORKTREE }) {
  const s = new ReviewStore(target);
  await s.load();
  return s;
}

function failOn(match: (method: string, path: string) => boolean, error: Error = new Error('boom')) {
  const route = d.review.route.bind(d.review);
  return vi.spyOn(d.review, 'route').mockImplementation((method, path, body) => {
    if (match(method, path)) throw error;
    return route(method, path, body);
  });
}

const draft = (body: string): Draft => ({ path: 'src/term.rs', side: 'new', anchor: 30, start: 30, end: 30, body, selecting: true });

beforeEach(() => {
  connect();
  notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
});

afterEach(() => {
  new ReviewStore({ worktree: MOCK_WORKTREE }).draft = null;
  d.stop();
  app.client = null;
  reviewCounts.threads = new Map();
  reviewCounts.resolved = {};
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe('ReviewStore without a daemon', () => {
  it('refuses API calls and skips loading while disconnected', async () => {
    app.client = null;
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    expect(() => s.api).toThrow('not connected');
    await s.load();
    await s.reloadDiff();
    await s.loadScope();
    expect(s.diff).toBeNull();
    expect(s.loading).toBe(false);
    expect(s.error).toBe('');
  });
});

describe('ReviewStore for a session outside a repository', () => {
  afterEach(() => {
    app.data = { lines: [], terms: [], tasks: [], activity: {} };
  });

  function plain(repo: string) {
    const line = { id: 'ln_plain', agent: 'claude', repo, state: 'idle', lastActivityAt: '' } as LocalLine['line'];
    app.data = { ...app.data, lines: [{ line }] as LocalLine[] };
  }

  it('makes no request and keeps the empty state for a line without a repository', async () => {
    plain('unknown');
    const spy = vi.spyOn(d.review, 'route');
    const reqs = vi.spyOn(d, 'request');
    const s = new ReviewStore({ lineId: 'ln_plain' });
    expect(s.notRepo).toBe(true);
    await s.load();
    await s.reloadDiff();
    await s.loadScope();
    expect(spy).not.toHaveBeenCalled();
    expect(reqs).not.toHaveBeenCalled();
    expect(s.error).toBe('');
    expect(s.diff).toBeNull();
  });

  it('loads normally for a line in a repository', async () => {
    plain('github.com/x/y');
    const s = new ReviewStore({ lineId: 'ln_plain' });
    expect(s.notRepo).toBe(false);
  });

  it('is never a non-repository state for worktree and task targets', () => {
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).notRepo).toBe(false);
  });
});

describe('ReviewStore loading', () => {
  it('reports a generic load failure without flagging the worktree as missing', async () => {
    failOn((m, p) => p.startsWith('/v1/review?'));
    const s = await loaded();
    expect(s.error).toBe('boom');
    expect(s.missing).toBe(false);
    expect(s.diff).toBeNull();
    expect(s.loading).toBe(false);
  });

  it('clears a previous error and bumps the revision on each successful load', async () => {
    const spy = failOn((m, p) => p.startsWith('/v1/review?'), new DaemonError(410, 'gone'));
    const s = await loaded();
    expect(s.missing).toBe(true);
    spy.mockRestore();
    await s.load();
    expect(s.error).toBe('');
    expect(s.missing).toBe(false);
    expect(s.live).toBe(true);
    expect(s.revision).toBe(1);
    await s.reloadDiff();
    expect(s.revision).toBe(2);
  });

  it('keeps the last diff on a failed reload but marks the review offline', async () => {
    const s = await loaded();
    const diff = s.diff;
    failOn((m, p) => p.startsWith('/v1/review?'));
    await s.reloadDiff();
    expect(s.diff).toBe(diff);
    expect(s.error).toBe('boom');
    expect(s.live).toBe(false);
  });

  it('aliases the review key to the resolved worktree for the sidebar counts', async () => {
    await loaded({ lineId: 'ln_03' });
    expect(reviewCounts.resolved['review:line:ln_03']).toBe(MOCK_WORKTREE);
  });

  it('keeps the previous worktree list when refreshing it fails', async () => {
    const s = await loaded();
    await s.loadScope();
    const related = s.related;
    failOn((m, p) => p.startsWith('/v1/review/worktrees'));
    await s.loadScope();
    expect(s.related).toBe(related);
  });

  it('stop() cancels pending debounced refreshes and goes offline', async () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    const off = s.start();
    await vi.waitFor(() => expect(s.live).toBe(true), { interval: 1 });
    vi.useFakeTimers();
    const reload = vi.spyOn(s, 'reloadDiff');
    const scope = vi.spyOn(s, 'loadScope');
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['src/term.rs'] });
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    off();
    vi.advanceTimersByTime(5000);
    expect(reload).not.toHaveBeenCalled();
    expect(scope).not.toHaveBeenCalled();
    expect(s.live).toBe(false);
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['src/term.rs'] });
    vi.advanceTimersByTime(5000);
    expect(reload).not.toHaveBeenCalled();
  });

  async function watched() {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    s.start();
    await vi.waitFor(() => expect(s.live).toBe(true), { interval: 1 });
    vi.useFakeTimers();
    const reload = vi.spyOn(s, 'reloadDiff').mockResolvedValue();
    return { s, reload };
  }

  function gateDiffRequests() {
    const real = d.request.bind(d);
    const gates: (() => void)[] = [];
    vi.spyOn(d, 'request').mockImplementation(async <T,>(method: string, path: string, body?: unknown) => {
      if (method === 'GET' && path.startsWith('/v1/review?')) await new Promise<void>((r) => gates.push(r));
      return real<T>(method, path, body);
    });
    return gates;
  }

  it('reloads every file the watcher events named during the debounce, once', async () => {
    const { s, reload } = await watched();
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['a'] });
    vi.advanceTimersByTime(200);
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['b'] });
    vi.advanceTimersByTime(299);
    expect(reload).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(reload).toHaveBeenCalledExactlyOnceWith(['a', 'b']);
    s.stop();
  });

  it('forgets the pending files once they were reloaded', async () => {
    const { s, reload } = await watched();
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['a'] });
    vi.advanceTimersByTime(300);
    expect(reload).toHaveBeenLastCalledWith(['a']);
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['b'] });
    vi.advanceTimersByTime(300);
    expect(reload).toHaveBeenCalledTimes(2);
    expect(reload).toHaveBeenLastCalledWith(['b']);
    s.stop();
  });

  it('reloads everything when one of the debounced events names no files', async () => {
    const { s, reload } = await watched();
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['a'] });
    d.emit('review.changed', { worktree: MOCK_WORKTREE });
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['b'] });
    vi.advanceTimersByTime(300);
    expect(reload).toHaveBeenCalledExactlyOnceWith(undefined);
    s.stop();
  });

  it('keeps the newest diff when an older reload answers last', async () => {
    const s = await loaded();
    const gates = gateDiffRequests();
    const first = s.reloadDiff();
    const second = s.setBase('origin/main');
    await vi.waitFor(() => expect(gates.length).toBe(2));
    gates[1]();
    await second;
    const rev = s.revision;
    const diff = s.diff;
    gates[0]();
    await first;
    expect(s.revision).toBe(rev);
    expect(s.diff).toBe(diff);
  });

  it('finishes the first load when a reload overtakes it', async () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    const gates = gateDiffRequests();
    const first = s.load();
    await vi.waitFor(() => expect(gates.length).toBe(1));
    const second = s.reloadDiff();
    await vi.waitFor(() => expect(gates.length).toBe(2));
    gates[1]();
    await second;
    gates[0]();
    await first;
    expect(s.live).toBe(true);
    expect(s.diff).not.toBeNull();
    expect(s.loading).toBe(false);
  });

  it('marks the review stale instead of reloading while live refresh is paused', async () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    s.start();
    await vi.waitFor(() => expect(s.live).toBe(true), { interval: 1 });
    expect(s.autoRefresh).toBe(true);
    s.setAutoRefresh(false);
    vi.useFakeTimers();
    const reload = vi.spyOn(s, 'reloadDiff').mockResolvedValue();
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['a'] });
    vi.advanceTimersByTime(5000);
    expect(reload).not.toHaveBeenCalled();
    expect(s.stale).toBe(true);
    s.setAutoRefresh(true);
    expect(reload).toHaveBeenCalledOnce();
    s.stop();
  });

  it('resumes live refresh without reloading when nothing changed while paused', async () => {
    const s = await loaded();
    const reload = vi.spyOn(s, 'reloadDiff').mockResolvedValue();
    s.setAutoRefresh(false);
    s.setAutoRefresh(true);
    expect(reload).not.toHaveBeenCalled();
  });

  it('remembers the live refresh choice across stores and clears staleness on reload', async () => {
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) });
    const s = await loaded();
    s.setAutoRefresh(false);
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).autoRefresh).toBe(false);
    s.stale = true;
    await s.reloadDiff();
    expect(s.stale).toBe(false);
    s.stale = true;
    await s.load();
    expect(s.stale).toBe(false);
  });

  it('defaults live refresh on when storage is unavailable', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('denied');
      },
    });
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    expect(s.autoRefresh).toBe(true);
    s.setAutoRefresh(false);
    expect(s.autoRefresh).toBe(false);
  });

  it('refreshes the worktree list on worktree lifecycle events', async () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    s.start();
    await vi.waitFor(() => expect(s.live).toBe(true), { interval: 1 });
    vi.useFakeTimers();
    const scope = vi.spyOn(s, 'loadScope').mockResolvedValue();
    d.emit('worktree.created', { worktree: '/w/new' });
    d.emit('worktree.removed', { worktree: '/w/old' });
    vi.advanceTimersByTime(2000);
    expect(scope).toHaveBeenCalledTimes(1);
    s.stop();
  });

  it('refreshes a branch review when the worktree holding that branch changes', async () => {
    const s = new ReviewStore({ worktree: MOCK_ROOT, branch: 'feat/term-resize' });
    await s.load();
    await s.loadScope();
    const holder = s.related?.worktrees.find((w) => w.branch === 'feat/term-resize');
    expect(holder).toBeDefined();
    vi.useFakeTimers();
    const reload = vi.spyOn(s, 'reloadDiff').mockResolvedValue();
    s.onEvent({ id: '1', type: 'review.changed', at: '', data: { worktree: holder!.path, files: ['x'] } });
    vi.advanceTimersByTime(300);
    expect(reload).toHaveBeenCalledExactlyOnceWith(undefined);
  });
});

describe('ReviewStore live thread events', () => {
  const thread = (over: Partial<Thread>) => ({ id: 'x1', worktree: MOCK_WORKTREE, path: 'a', line: 1, endLine: 1, status: 'pending', comments: [], ...over }) as Thread;

  it('upserts threads of this worktree only and removes deleted ones', async () => {
    const s = await loaded();
    const n = s.threads.length;
    s.onEvent({ id: '1', type: 'review.thread', at: '', data: { thread: thread({}) } });
    s.onEvent({ id: '2', type: 'review.thread', at: '', data: { thread: thread({ id: 'x2', worktree: '/elsewhere' }) } });
    expect(s.threads.length).toBe(n + 1);
    s.onEvent({ id: '3', type: 'review.thread', at: '', data: { thread: thread({ status: 'resolved' }) } });
    expect(s.threads.filter((t) => t.id === 'x1').map((t) => t.status)).toEqual(['resolved']);
    s.onEvent({ id: '4', type: 'review.thread_deleted', at: '', data: { id: 'x1' } });
    s.onEvent({ id: '5', type: 'review.thread_deleted', at: '', data: {} });
    expect(s.threads.length).toBe(n);
  });

  it('applies delivery updates to the current delivery and the threads it carried', async () => {
    const s = await loaded();
    const queued = { id: 'dl', status: 'queued' } as Delivery;
    s.delivery = queued;
    s.threads = [thread({ id: 'a', delivery: queued }), thread({ id: 'b' })];
    const delivered = { ...queued, status: 'delivered' } as Delivery;
    s.onEvent({ id: '1', type: 'review.delivery', at: '', data: { delivery: delivered, worktree: '/elsewhere' } });
    expect(s.delivery).toBe(queued);
    s.onEvent({ id: '2', type: 'review.delivery', at: '', data: { delivery: delivered, worktree: MOCK_WORKTREE } });
    expect(s.delivery).toBe(delivered);
    expect(s.threads.map((t) => t.delivery?.status)).toEqual(['delivered', undefined]);
    s.onEvent({ id: '3', type: 'review.delivery', at: '', data: { delivery: { id: 'other', status: 'queued' }, worktree: MOCK_WORKTREE } });
    expect(s.delivery).toBe(delivered);
    s.onEvent({ id: '4', type: 'review.delivery', at: '', data: { worktree: MOCK_WORKTREE } });
    expect(s.delivery).toBe(delivered);
  });
});

describe('ReviewStore view', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('remembers the chosen view and opens a saved graph view as the map', () => {
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) });
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    expect(s.view).toBe('list');
    s.setView('map');
    expect(localStorage.getItem('overseer.review.view')).toBe('map');
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).view).toBe('map');
    localStorage.setItem('overseer.review.view', 'graph');
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).view).toBe('map');
    s.setView('list');
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).view).toBe('list');
  });
});

describe('ReviewStore display mode', () => {
  it('persists the chosen mode and restores it in the next store', () => {
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) });
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    expect(s.mode).toBe('unified');
    s.setMode('split');
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).mode).toBe('split');
  });

  it('falls back to unified when storage is unavailable', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('denied');
      },
    });
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    expect(s.mode).toBe('unified');
    expect(() => s.setMode('split')).not.toThrow();
    expect(s.mode).toBe('split');
  });
});

describe('ReviewStore file context', () => {
  it('fetches full file lines once and expands each gap once', async () => {
    const spy = vi.spyOn(d.review, 'route');
    const fileCalls = () => spy.mock.calls.filter((c) => String(c[1]).startsWith('/v1/review/file')).length;
    const s = await loaded();
    await s.expand('src/term.rs', 'g1');
    await s.expand('src/term.rs', 'g1');
    await s.expand('src/term.rs', 'g2');
    expect(s.expanded['src/term.rs']).toEqual(['g1', 'g2']);
    expect(s.fileLines['src/term.rs'].length).toBeGreaterThan(30);
    expect(fileCalls()).toBe(1);
    await s.reloadDiff(['other.rs']);
    expect(fileCalls()).toBe(1);
    await s.reloadDiff(['src/term.rs']);
    expect(fileCalls()).toBe(2);
  });

  it('refetches expanded files after a full reload', async () => {
    const s = await loaded();
    await s.expand('src/term.rs', 'g1');
    const fetch = vi.spyOn(s, 'fetchLines');
    await s.load();
    expect(fetch).toHaveBeenCalledWith('src/term.rs', true);
  });

  it('notifies when a file cannot be read and does not cache anything', async () => {
    const s = await loaded();
    await s.fetchLines('nope.rs');
    expect(notify).toHaveBeenCalledWith('no file nope.rs', 'error');
    expect(s.fileLines['nope.rs']).toBeUndefined();
  });

  it('does nothing before the worktree is known', async () => {
    const s = new ReviewStore({ lineId: 'ln_03' });
    const spy = vi.spyOn(d, 'request');
    await s.fetchLines('src/term.rs');
    expect(spy).not.toHaveBeenCalled();
  });

  it('reads branch files at the branch head', async () => {
    const s = new ReviewStore({ worktree: MOCK_ROOT, branch: 'fix/ssh-reconnect' });
    await s.load();
    const spy = vi.spyOn(d, 'request');
    await s.fetchLines('src/term.rs');
    expect(spy.mock.calls[0][1]).toContain(`rev=${encodeURIComponent(s.diff!.head!)}`);
  });

  it('records unfolded hunks without duplicates', () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    s.unfold('a', 1);
    s.unfold('a', 1);
    s.unfold('a', 2);
    expect(s.unfolded.a).toEqual([1, 2]);
  });
});

describe('ReviewStore viewed files', () => {
  it('marks a file viewed, collapses it and tells the daemon', async () => {
    const s = await loaded();
    const f = s.diff!.files.find((x) => !x.viewed)!;
    await s.toggleViewed(f.path);
    expect(s.diff!.files.find((x) => x.path === f.path)!.viewed).toBe(true);
    expect(s.isCollapsed(f.path)).toBe(true);
    expect(d.review.viewed[f.path]).toBe(f.hash);
    await s.toggleViewed(f.path);
    expect(s.isCollapsed(f.path)).toBe(false);
    expect(d.review.viewed[f.path]).toBeUndefined();
  });

  it('rolls the viewed flag back and notifies when the daemon refuses', async () => {
    const s = await loaded();
    const f = s.diff!.files.find((x) => !x.viewed)!;
    failOn((m, p) => p === '/v1/review/viewed');
    await s.toggleViewed(f.path);
    expect(s.diff!.files.find((x) => x.path === f.path)!.viewed).toBe(false);
    expect(notify).toHaveBeenCalledWith('boom', 'error');
  });

  it('drops the rollback without throwing when the diff vanished during the request', async () => {
    const s = await loaded();
    const f = s.diff!.files.find((x) => !x.viewed)!;
    const route = d.review.route.bind(d.review);
    vi.spyOn(d.review, 'route').mockImplementation((method, path, body) => {
      if (path === '/v1/review/viewed') {
        s.diff = null;
        throw new Error('boom');
      }
      return route(method, path, body);
    });
    await expect(s.toggleViewed(f.path)).resolves.toBeUndefined();
    expect(s.diff).toBeNull();
    expect(notify).toHaveBeenCalledWith('boom', 'error');
  });

  it('only collapses files of a read-only branch review', async () => {
    const s = new ReviewStore({ worktree: MOCK_ROOT, branch: 'fix/ssh-reconnect' });
    await s.load();
    const spy = vi.spyOn(d, 'request');
    const f = s.diff!.files[0];
    const before = s.isCollapsed(f.path);
    await s.toggleViewed(f.path);
    expect(s.isCollapsed(f.path)).toBe(!before);
    expect(spy).not.toHaveBeenCalled();
  });

  it('ignores unknown paths and a missing diff', async () => {
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    const spy = vi.spyOn(d, 'request');
    await s.toggleViewed('src/term.rs');
    await s.load();
    spy.mockClear();
    await s.toggleViewed('ghost.rs');
    expect(spy).not.toHaveBeenCalled();
  });
});

describe('ReviewStore drafts and comments', () => {
  it('persists a non-empty draft per target and restores it in a new store', () => {
    const target = { worktree: `${MOCK_WORKTREE}-drafts` };
    const s = new ReviewStore(target);
    s.draft = draft('keep me');
    const again = new ReviewStore(target);
    expect(again.draft).toEqual({ ...draft('keep me'), selecting: false });
    s.draft = draft('   ');
    expect(new ReviewStore(target).draft).toBeNull();
  });

  it('turns a ready draft into a thread, counts it and clears the draft', async () => {
    const s = await loaded();
    const n = s.threads.length;
    s.draft = draft('  why?  ');
    await s.addDraft();
    expect(s.draft).toBeNull();
    expect(s.threads.length).toBe(n + 1);
    const t = s.threads.find((x) => x.comments.some((c) => c.body === 'why?'))!;
    expect(t.status).toBe('pending');
    expect(reviewCounts.threads.get(t.id)?.status).toBe('pending');
  });

  it('ignores an empty draft and a second submit while the first is in flight', async () => {
    const s = await loaded();
    const spy = vi.spyOn(d, 'request');
    s.draft = draft('  ');
    await s.addDraft();
    expect(spy).not.toHaveBeenCalled();
    s.draft = draft('once');
    await Promise.all([s.addDraft(), s.addDraft()]);
    expect(spy.mock.calls.filter((c) => c[0] === 'POST')).toHaveLength(1);
  });

  it('keeps the draft and notifies when creating the thread fails', async () => {
    const s = await loaded();
    s.draft = { ...draft('out of range'), start: 9999, end: 9999 };
    await s.addDraft();
    expect(s.draft?.body).toBe('out of range');
    expect(notify).toHaveBeenCalledWith('line outside the file', 'error');
  });

  it('sends the selected base with a new comment', async () => {
    const s = await loaded();
    await s.setBase('origin/main');
    const spy = vi.spyOn(d, 'request');
    s.draft = draft('based');
    await s.addDraft();
    expect(spy.mock.calls.find((c) => c[0] === 'POST')![2]).toMatchObject({ worktree: MOCK_WORKTREE, base: 'origin/main' });
  });

  it('replies with a trimmed body and rejects blank replies', async () => {
    const s = await loaded();
    const t = s.threads[0];
    const spy = vi.spyOn(d, 'request');
    expect(await s.reply(t.id, '   ')).toBe(false);
    expect(spy).not.toHaveBeenCalled();
    expect(await s.reply(t.id, '  ok  ')).toBe(true);
    expect(s.threads.find((x) => x.id === t.id)!.comments.at(-1)!.body).toBe('ok');
    expect(await s.reply('missing-thread', 'hi')).toBe(false);
    expect(notify).toHaveBeenCalled();
  });

  it('edits a pending comment and reports a refused edit', async () => {
    const s = await loaded();
    s.draft = draft('first');
    await s.addDraft();
    const t = s.threads.find((x) => x.comments.some((c) => c.body === 'first'))!;
    await s.edit(t.comments[0].id, 'second');
    expect(s.threads.find((x) => x.id === t.id)!.comments[0].body).toBe('second');
    const sent = s.threads.find((x) => x.comments.some((c) => !c.pending))!;
    await s.edit(sent.comments.find((c) => !c.pending)!.id, 'nope');
    expect(notify).toHaveBeenCalledWith('only pending comments can be edited', 'error');
  });

  it('resolves optimistically and restores the thread when the daemon fails', async () => {
    const s = await loaded();
    const t = s.threads.find((x) => x.status === 'sent')!;
    let seen = '';
    const route = d.review.route.bind(d.review);
    vi.spyOn(d.review, 'route').mockImplementation((m, p, b) => {
      if (m === 'PATCH') {
        seen = s.threads.find((x) => x.id === t.id)!.status;
        throw new Error('offline');
      }
      return route(m, p, b);
    });
    await s.setStatus(t, 'resolve');
    expect(seen).toBe('resolved');
    expect(s.threads.find((x) => x.id === t.id)!.status).toBe('sent');
    expect(notify).toHaveBeenCalledWith('offline', 'error');
  });

  it('reopens a resolved thread with the status the daemon decides', async () => {
    const s = await loaded();
    const t = s.threads.find((x) => x.status === 'resolved')!;
    await s.setStatus(t, 'reopen');
    expect(s.threads.find((x) => x.id === t.id)!.status).toBe('sent');
    expect(reviewCounts.threads.get(t.id)?.status).toBe('sent');
  });

  it('removes a whole pending thread and drops it from the counts', async () => {
    const s = await loaded();
    s.draft = draft('to delete');
    await s.addDraft();
    const t = s.threads.find((x) => x.comments.some((c) => c.body === 'to delete'))!;
    await s.remove(t.id, t.id);
    expect(s.threads.some((x) => x.id === t.id)).toBe(false);
    expect(reviewCounts.threads.has(t.id)).toBe(false);
  });

  it('refetches the threads after deleting a single reply', async () => {
    const s = await loaded();
    s.draft = draft('parent');
    await s.addDraft();
    const t = s.threads.find((x) => x.comments.some((c) => c.body === 'parent'))!;
    await s.reply(t.id, 'child');
    const child = s.threads.find((x) => x.id === t.id)!.comments.find((c) => c.body === 'child')!;
    const spy = vi.spyOn(d, 'request');
    await s.remove(child.id, t.id);
    expect(spy.mock.calls.map((c) => c[0])).toEqual(['DELETE', 'GET']);
    expect(spy.mock.calls[1][1]).toContain(`worktree=${encodeURIComponent(MOCK_WORKTREE)}`);
    expect(s.threads.find((x) => x.id === t.id)!.comments.map((c) => c.body)).toEqual(['parent']);
  });

  it('leaves threads alone when deleting fails', async () => {
    const s = await loaded();
    const before = s.threads;
    const sent = s.threads.find((x) => x.status === 'sent')!;
    await s.remove(sent.id, sent.id);
    expect(s.threads).toBe(before);
    expect(notify).toHaveBeenCalledWith('not pending', 'error');
  });
});

describe('ReviewStore applying suggestions', () => {
  it('applies a suggestion once even when clicked twice, then reloads that file', async () => {
    const s = await loaded();
    const t = s.threads.find((x) => x.path === 'app/src/lib/terminal.ts' && !x.outdated)!;
    const spy = vi.spyOn(d, 'request');
    const reload = vi.spyOn(s, 'reloadDiff');
    await Promise.all([s.apply(t), s.apply(t)]);
    expect(spy.mock.calls.filter((c) => String(c[1]).endsWith('/apply'))).toHaveLength(1);
    expect(s.threads.find((x) => x.id === t.id)!.status).toBe('resolved');
    expect(reload).toHaveBeenCalledWith([t.path]);
  });

  it('reports a suggestion that can no longer be applied without reloading', async () => {
    const s = await loaded();
    const t = s.threads.find((x) => x.outdated)!;
    const reload = vi.spyOn(s, 'reloadDiff');
    await s.apply(t);
    expect(notify).toHaveBeenCalledWith('no suggestion to apply', 'error');
    expect(reload).not.toHaveBeenCalled();
  });
});

describe('ReviewStore submit', () => {
  it('sends pending comments once, stores the delivery and marks the threads sent', async () => {
    const s = await loaded();
    const pending = s.threads.filter((t) => t.comments.some((c) => c.pending)).map((t) => t.id);
    expect(pending.length).toBeGreaterThan(0);
    const spy = vi.spyOn(d, 'request');
    const [r, again] = await Promise.all([s.submit('request_changes', ' please '), s.submit('approve', '')]);
    expect(again).toBeNull();
    expect(spy.mock.calls.filter((c) => c[1] === '/v1/review/submit')).toHaveLength(1);
    expect(r!.delivery.status).toBe('queued');
    expect(s.delivery).toEqual(r!.delivery);
    expect(s.submitting).toBeNull();
    for (const id of pending) expect(s.threads.find((t) => t.id === id)!.status).toBe('sent');
  });

  it('returns null and unlocks submitting when the daemon fails', async () => {
    const s = await loaded();
    failOn((m, p) => p === '/v1/review/submit');
    expect(await s.submit('comment', 'x')).toBeNull();
    expect(s.submitting).toBeNull();
    expect(s.delivery).toBeNull();
    expect(notify).toHaveBeenCalledWith('boom', 'error');
  });
});
