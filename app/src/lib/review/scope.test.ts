import { afterEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../mock';
import { app } from '../state.svelte';
import type { DaemonEvent, LocalLine, Task } from '../types';
import { parseReviewKey, reviewKey, titleOf } from './keys';
import { MOCK_MISSING, MOCK_ROOT, MOCK_WORKTREE } from './mock';
import { testingReviews } from './open';
import { changed, changedWorktrees, counts, folder, includeCurrent, scopeOptions } from './scope';
import { ReviewStore } from './store.svelte';
import { setPresence } from '../presence';
import type { Related } from './types';

const wt = (path: string, extra: Partial<Related['worktrees'][number]> = {}) => ({ path, branch: path.split('/').pop(), files: 0, dirty: 0, ahead: 0, agents: 0, current: false, main: false, at: '2026-10-29T10:00:00Z', ...extra });

describe('review scope selector', () => {
  it('round-trips branch review keys, branch names cannot contain a colon', () => {
    const t = { worktree: '/w/repo:odd', branch: 'feat/x' };
    expect(reviewKey(t)).toBe('review:br:feat/x:/w/repo:odd');
    expect(parseReviewKey(reviewKey(t))).toEqual(t);
    expect(parseReviewKey('review:br:feat/x:')).toBeNull();
    expect(titleOf(t, { lines: [], tasks: [] })).toBe('feat/x');
  });

  it('lists the worktrees the daemon returns plus open review tabs, keeping the tab key for the current worktree', () => {
    const related: Related = { current: '/r', worktrees: [wt('/r/.claude/worktrees/a', { branch: 'feat/a', files: 2, agents: 1 }), wt('/r', { branch: 'main', current: true, main: true })] };
    const open = [
      { key: 'review:line:L1', name: 'self', path: '/r' },
      { key: 'review:task:T1', name: '#3 Fix', path: '/r/.claude/worktrees/a' },
      { key: 'review:wt:/old/closed', name: 'closed', path: '/old/closed' },
      { key: 'review:br:feat/gone:/r', name: 'feat/gone', path: '/r' },
    ];
    const opts = scopeOptions(related, 'review:line:L1', open);
    expect(opts.map((o) => o.key)).toEqual(['review:wt:/r/.claude/worktrees/a', 'review:line:L1', 'review:wt:/old/closed', 'review:br:feat/gone:/r']);
    expect(changedWorktrees(related).map((w) => w.path)).toEqual(['/r/.claude/worktrees/a']);
    const branchKey = 'review:br:feat/gone:/r';
    const fromBranch = scopeOptions(related, branchKey);
    expect(fromBranch.find((o) => o.worktree?.current)?.key).toBe('review:wt:/r');
    expect(includeCurrent(fromBranch, branchKey, 'feat/gone', '/r').filter((o) => o.key === branchKey)).toHaveLength(1);
    expect(includeCurrent([], 'review:wt:/x', 'x', '/x')[0].key).toBe('review:wt:/x');
  });

  it('keeps in-testing reviews only for open agents or an open review tab', () => {
    const task = (id: string, lineId: string) => ({ id, lineId, status: 'in_testing', number: 1, title: id }) as Task;
    const tasks = [task('T1', 'L1'), task('T2', 'L2'), task('T3', 'L3'), { ...task('T4', 'L1'), status: 'completed' } as Task];
    const lines = [{ line: { id: 'L1' }, local: { pid: 42 } }, { line: { id: 'L2' }, local: {} }] as LocalLine[];
    expect(testingReviews(tasks, lines, []).map((x) => x.id)).toEqual(['T1']);
    expect(testingReviews(tasks, lines, ['review:task:T3']).map((x) => x.id)).toEqual(['T1', 'T3']);
  });
});

describe('review scope labels', () => {
  it('names a worktree after its last folder, tolerating trailing and missing slashes', () => {
    expect(folder('/w/repo/')).toBe('repo');
    expect(folder('repo')).toBe('repo');
    expect(folder('/')).toBe('/');
    expect(folder('')).toBe('');
  });

  it('labels a worktree option by its branch and falls back to the folder when detached', () => {
    const opts = scopeOptions({ current: '/r', worktrees: [wt('/r/a', { branch: '' }), wt('/r/b', { branch: 'feat/b' })] });
    expect(opts.map((o) => o.name)).toEqual(['a', 'feat/b']);
    expect(scopeOptions(null)).toEqual([]);
    expect(changedWorktrees(null)).toEqual([]);
  });

  it('treats unpushed commits as changes even with a clean tree', () => {
    expect(changed(wt('/a', { ahead: 1 }))).toBe(true);
    expect(changed(wt('/a', { files: 1 }))).toBe(true);
    expect(changed(wt('/a', { dirty: 3 }))).toBe(false);
  });

  it('summarises files, commits and agents with singular and plural forms', () => {
    expect(counts({ files: 0, ahead: 0 })).toBe('no changes');
    expect(counts({ files: 1, ahead: 1 })).toBe('1 file · 1 commit');
    expect(counts({ files: 3, ahead: 0, agents: 1 })).toBe('3 files · 1 agent');
    expect(counts({ files: 0, ahead: 2, agents: 4 })).toBe('2 commits · 4 agents');
    expect(counts({ files: 0, ahead: 0, agents: 2 })).toBe('no changes · 2 agents');
  });

  it('puts the current review first only when it is not already listed', () => {
    const opts = [{ key: 'review:wt:/a', name: 'a', path: '/a' }];
    expect(includeCurrent(opts, 'review:wt:/a', 'a', '/a')).toBe(opts);
    expect(includeCurrent(opts, '', 'x', '/x')).toBe(opts);
    expect(includeCurrent(opts, 'review:line:L', 'L', '/l').map((o) => o.key)).toEqual(['review:line:L', 'review:wt:/a']);
  });

  it('does not duplicate an open tab for a listed worktree path, but keeps open branch tabs', () => {
    const related: Related = { current: '/r', worktrees: [wt('/r', { current: true })] };
    const open = [
      { key: 'review:task:T', name: 't', path: '/r' },
      { key: 'review:br:main:/r', name: 'main', path: '/r' },
      { key: 'review:line:X', name: 'x', path: '' },
    ];
    expect(scopeOptions(related, undefined, open).map((o) => o.key)).toEqual(['review:wt:/r', 'review:br:main:/r', 'review:line:X']);
  });
});

describe('review store states', () => {
  let d: MockDaemon;
  afterEach(() => {
    d?.stop();
    app.client = null;
    vi.useRealTimers();
  });

  function connect() {
    d = new MockDaemon({ live: false });
    app.client = d;
    d.subscribe((e: DaemonEvent) => app.onEvent(e));
  }

  it('loads the changes, the unpushed commits and the related worktrees', async () => {
    connect();
    const s = new ReviewStore({ lineId: 'ln_03' });
    await s.load();
    await s.loadScope();
    expect(s.diff?.worktree).toBe(MOCK_WORKTREE);
    expect(s.diff?.files.length).toBe(8);
    expect(s.diff?.commits?.length).toBe(3);
    const cur = s.related?.worktrees.find((w) => w.current);
    expect(cur?.files).toBe(s.diff?.files.length);
    expect(changedWorktrees(s.related).length).toBe(2);
    expect(s.related?.worktrees.some((w) => w.branch === 'fix/review-panel')).toBe(false);
    expect(s.related?.worktrees.every((w) => w.current || w.agents > 0)).toBe(true);
    expect(s.readOnly).toBe(false);
  });

  it('remembers a picked base per worktree but never a since-commit base', async () => {
    connect();
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) });
    const s = new ReviewStore({ worktree: MOCK_WORKTREE });
    await s.load();
    expect(s.diff?.baseSource).toBe('created');
    await s.setBase('release/next', true);
    expect(s.diff?.base).toBe('release/next');
    await s.setBase('abc1234~1');
    const again = new ReviewStore({ lineId: 'ln_03' });
    await again.load();
    expect(again.base).toBe('release/next');
    expect(again.diff?.base).toBe('release/next');
    await again.setBase(null, true);
    expect(new ReviewStore({ worktree: MOCK_WORKTREE }).base).toBeNull();
    vi.unstubAllGlobals();
  });

  it('shows recent commits on a clean default branch instead of a blank review', async () => {
    connect();
    const s = new ReviewStore({ worktree: MOCK_ROOT });
    await s.load();
    expect(s.diff?.files).toEqual([]);
    expect(s.diff?.base).toBe('origin/main');
    expect(s.diff?.recent?.length).toBeGreaterThan(3);
    expect(s.error).toBe('');
  });

  it('reports a vanished worktree folder as missing, not as a generic error', async () => {
    connect();
    const s = new ReviewStore({ worktree: MOCK_MISSING });
    await s.load();
    expect(s.missing).toBe(true);
    expect(s.diff).toBeNull();
    expect(s.live).toBe(false);
  });

  it('opens a branch without a worktree read-only and without comment threads', async () => {
    connect();
    const spy = vi.spyOn(d.review, 'route');
    const s = new ReviewStore({ worktree: MOCK_ROOT, branch: 'fix/ssh-reconnect' });
    await s.load();
    expect(s.readOnly).toBe(true);
    expect(s.diff?.branch).toBe('fix/ssh-reconnect');
    expect(spy.mock.calls.some((c) => String(c[1]).startsWith('/v1/review/comments'))).toBe(false);
    expect(spy.mock.calls.some((c) => String(c[1]).includes('branch=fix%2Fssh-reconnect'))).toBe(true);
  });

  it('refreshes the selector from watcher events, debounced, and ignores unrelated worktrees', async () => {
    connect();
    const s = new ReviewStore({ worktree: MOCK_ROOT });
    s.start();
    await vi.waitFor(() => expect(s.related).not.toBeNull());
    vi.useFakeTimers();
    const spy = vi.spyOn(s, 'loadScope');
    const diff = vi.spyOn(s, 'reloadDiff');
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['src/term.rs'] });
    d.emit('review.changed', { worktree: MOCK_WORKTREE, files: ['src/resize.rs'] });
    d.emit('review.changed', { worktree: '/somewhere/else', files: ['x'] });
    expect(spy).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2100);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(diff).not.toHaveBeenCalled();
    d.emit('review.changed', { worktree: MOCK_ROOT, files: ['a'] });
    vi.advanceTimersByTime(400);
    expect(diff).toHaveBeenCalledTimes(1);
    s.stop();
  });

  it('holds watcher refreshes while the window is hidden and catches up once when shown', async () => {
    connect();
    const s = new ReviewStore({ worktree: MOCK_ROOT });
    s.start();
    await vi.waitFor(() => expect(s.related).not.toBeNull());
    vi.useFakeTimers();
    const scope = vi.spyOn(s, 'loadScope');
    const diff = vi.spyOn(s, 'reloadDiff');
    setPresence({ visible: false });
    try {
      for (let i = 0; i < 20; i++) {
        d.emit('review.changed', { worktree: MOCK_ROOT, files: [`f${i}`] });
        d.emit('worktree.updated', { worktree: MOCK_WORKTREE });
      }
      expect(vi.getTimerCount()).toBe(0);
      vi.advanceTimersByTime(10_000);
      expect(scope).not.toHaveBeenCalled();
      expect(diff).not.toHaveBeenCalled();
      setPresence({ visible: true });
      expect(diff).toHaveBeenCalledTimes(1);
      expect(scope).toHaveBeenCalledTimes(1);
      setPresence({ visible: false });
      setPresence({ visible: true });
      expect(diff).toHaveBeenCalledTimes(1);
    } finally {
      setPresence({ visible: true });
      s.stop();
    }
  });
});
