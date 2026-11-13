import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import type { DaemonEvent } from '../types';
import { reviewCounts } from './counts.svelte';
import { MOCK_WORKTREE } from './mock';

const snapshot = app.data;
let d: MockDaemon | undefined;
let unsub: (() => void) | undefined;

function connect() {
  d = new MockDaemon({ live: false });
  unsub = d.subscribe((e: DaemonEvent) => app.onEvent(e));
  app.client = d;
  app.connected = true;
  return d;
}

afterEach(() => {
  unsub?.();
  d?.stop();
  d = undefined;
  app.client = null;
  app.connected = false;
  app.data = snapshot;
  reviewCounts.threads = new Map();
  reviewCounts.resolved = {};
});

describe('reviewCounts.ensure', () => {
  it('does nothing while disconnected', () => {
    const m = new MockDaemon({ live: false });
    const spy = vi.spyOn(m, 'request');
    app.client = m;
    app.connected = false;
    reviewCounts.ensure();
    expect(spy).not.toHaveBeenCalled();
    m.stop();
  });

  it('loads every thread once per client and counts the unresolved ones by worktree', async () => {
    const m = connect();
    const spy = vi.spyOn(m, 'request');
    reviewCounts.ensure();
    reviewCounts.ensure();
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith('GET', '/v1/review/comments');
    await vi.waitFor(() => expect(reviewCounts.threads.size).toBeGreaterThan(0), { interval: 1 });
    const all = await m.review.route('GET', '/v1/review/comments', undefined);
    const open = (all as { status: string; worktree: string }[]).filter((t) => t.status !== 'resolved' && t.worktree === MOCK_WORKTREE).length;
    expect(reviewCounts.byWorktree[MOCK_WORKTREE]).toBe(open);
    expect(reviewCounts.any).toBe(true);
  });

  it('follows thread created, updated and deleted events from the daemon', async () => {
    const first = connect();
    reviewCounts.ensure();
    await vi.waitFor(() => expect(reviewCounts.threads.size).toBeGreaterThan(0), { interval: 1 });
    reviewCounts.threads = new Map();
    first.emit('review.thread', { thread: { id: 'n1', worktree: '/w/x', status: 'pending' } });
    expect(reviewCounts.byWorktree['/w/x']).toBe(1);
    first.emit('review.thread', { thread: { id: 'n1', worktree: '/w/x', status: 'resolved' } });
    expect(reviewCounts.byWorktree['/w/x']).toBeUndefined();
    expect(reviewCounts.threads.has('n1')).toBe(true);
    first.emit('review.thread_deleted', { id: 'n1' });
    expect(reviewCounts.threads.has('n1')).toBe(false);
    first.emit('review.thread', { thread: { worktree: '/w/x', status: 'pending' } });
    expect(reviewCounts.threads.size).toBe(0);

  });

  it('swaps its change listener when the client changes instead of stacking them', async () => {
    const offs: ReturnType<typeof vi.fn>[] = [];
    const original = app.onChange.bind(app);
    const spy = vi.spyOn(app, 'onChange').mockImplementation((fn) => {
      const off = vi.fn(original(fn));
      offs.push(off);
      return off;
    });
    connect();
    reviewCounts.ensure();
    unsub?.();
    d?.stop();
    const second = connect();
    reviewCounts.ensure();
    expect(spy).toHaveBeenCalledTimes(2);
    expect(offs[0]).toHaveBeenCalledTimes(1);
    expect(offs[1]).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(reviewCounts.threads.size).toBeGreaterThan(0), { interval: 1 });
    reviewCounts.threads = new Map();
    second.emit('review.thread', { thread: { id: 'new', worktree: '/w/new', status: 'sent' } });
    expect(reviewCounts.byWorktree).toEqual({ '/w/new': 1 });
    spy.mockRestore();
  });

  it('keeps the previous counts when the initial load fails', async () => {
    const m = connect();
    vi.spyOn(m, 'request').mockRejectedValueOnce(new Error('down'));
    reviewCounts.put({ id: 'a', worktree: '/w', status: 'pending' });
    reviewCounts.ensure();
    await new Promise((r) => setTimeout(r, 0));
    expect(reviewCounts.byWorktree).toEqual({ '/w': 1 });
  });

  it('retries the initial load on the next ensure after a failure', async () => {
    const m = connect();
    const spy = vi.spyOn(m, 'request').mockRejectedValueOnce(new Error('down'));
    reviewCounts.ensure();
    await new Promise((r) => setTimeout(r, 0));
    reviewCounts.ensure();
    await vi.waitFor(() => expect(reviewCounts.threads.size).toBeGreaterThan(0), { interval: 1 });
    expect(spy).toHaveBeenCalledTimes(2);
  });
});

describe('reviewCounts put and drop', () => {
  it('replaces the map on change so derived counts update, and ignores unknown ids', () => {
    reviewCounts.put({ id: 'a', worktree: '/w', status: 'pending' });
    reviewCounts.put({ id: 'b', worktree: '/w', status: 'addressed' });
    const before = reviewCounts.threads;
    expect(reviewCounts.byWorktree).toEqual({ '/w': 2 });
    reviewCounts.drop('zzz');
    expect(reviewCounts.threads).toBe(before);
    reviewCounts.drop('a');
    expect(reviewCounts.threads).not.toBe(before);
    expect(reviewCounts.byWorktree).toEqual({ '/w': 1 });
  });
});

describe('reviewCounts.forKey', () => {
  it('returns zero when nothing is unresolved, for branch reviews and for unknown keys', () => {
    expect(reviewCounts.forKey('review:wt:/w')).toBe(0);
    reviewCounts.put({ id: 'a', worktree: '/w', status: 'sent' });
    expect(reviewCounts.forKey('review:br:main:/w')).toBe(0);
    expect(reviewCounts.forKey('line:nope')).toBe(0);
    expect(reviewCounts.forKey('review:wt:/w')).toBe(1);
  });

  it('resolves line, task and terminal keys through the snapshot, and prefers an alias', () => {
    app.data = {
      ...snapshot,
      lines: [{ line: { id: 'L', worktreePath: '/w/a' } }],
      tasks: [{ id: 'T', number: 1, title: 't', lineId: 'L' }],
      terms: [{ id: 'M', cwd: '/w/b' }],
    } as unknown as typeof app.data;
    reviewCounts.put({ id: '1', worktree: '/w/a', status: 'pending' });
    reviewCounts.put({ id: '2', worktree: '/w/b', status: 'pending' });
    reviewCounts.put({ id: '3', worktree: '/w/b', status: 'pending' });
    expect(reviewCounts.forKey('line:L')).toBe(1);
    expect(reviewCounts.forKey('review:task:T')).toBe(1);
    expect(reviewCounts.forKey('term:M')).toBe(2);
    reviewCounts.alias({ lineId: 'L' }, '/w/b');
    expect(reviewCounts.forKey('review:line:L')).toBe(2);
    const resolved = reviewCounts.resolved;
    reviewCounts.alias({ lineId: 'L' }, '/w/b');
    expect(reviewCounts.resolved).toBe(resolved);
  });
});
