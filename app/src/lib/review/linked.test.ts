import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import type { DaemonEvent } from '../types';
import { reviewCounts } from './counts.svelte';
import { LinkedReviews, batchAll, parsePick, pickerGroups, submitAll } from './linked.svelte';
import { MOCK_LINKED, MOCK_ROOT, MOCK_WORKTREE } from './mock';
import { ReviewStore } from './store.svelte';
import type { SubmitResult, Thread } from './types';

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
