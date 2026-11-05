import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../mock';
import { ui } from '../mobile/ui.svelte';
import { app } from '../state.svelte';
import type { Task } from '../types';
import { EMPTY_LAYOUT, allTabs, workspace, type Layout } from '../workspace.svelte';
import { beside } from './beside.svelte';
import {
  closeSideReview,
  followReview,
  isSideReview,
  itemTarget,
  modeFor,
  openReview,
  openReviewBeside,
  openReviewFromHash,
  openReviewNear,
  openReviewTabs,
  reviewTargetFor,
  reviewTitle,
  setReviewMode,
} from './open';

const snapshot = app.data;
const workspaceId = app.workspaceId;
const SHELL = 'term:tm_shell2';
let d: MockDaemon;

const two = (a: string[], b: string[], focused = 0): Layout => ({ split: 'horizontal', panes: [{ tabs: a, active: a[0] ?? null }, { tabs: b, active: b[0] ?? null }], focused, ratio: 0.5 });
const one = (tabs: string[]): Layout => ({ ...EMPTY_LAYOUT, panes: [{ tabs, active: tabs[0] ?? null }] });

beforeEach(() => {
  d = new MockDaemon({ live: false });
  app.data = { lines: d.lines, terms: d.terms, tasks: d.tasks, activity: {} } as typeof app.data;
  workspace.layout = EMPTY_LAYOUT;
  beside.key = null;
  beside.pinned = false;
  ui.layout = 'desktop';
  app.view = 'agents';
  app.selectedKey = null;
});

afterEach(() => {
  d.stop();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  app.data = snapshot;
  app.client = null;
  app.panelOpen = true;
  app.overlay = null;
  app.selectedKey = null;
  app.workspaceId = workspaceId;
  workspace.layout = EMPTY_LAYOUT;
  beside.key = null;
  beside.pinned = false;
  ui.layout = 'desktop';
});

describe('review titles and open tabs', () => {
  it('titles a review key from the snapshot and rejects non-review keys', () => {
    expect(reviewTitle('review:line:ln_03')).toMatch(/^#\d+ /);
    expect(reviewTitle('review:wt:/w/repo/')).toBe('repo');
    expect(reviewTitle('line:ln_03')).toBeNull();
  });

  it('lists only the review tabs of the layout with their worktree path', () => {
    workspace.layout = two(['line:ln_03', 'review:line:ln_03'], ['browser:x', 'review:wt:/w/a']);
    expect(openReviewTabs().map((o) => [o.key, o.path])).toEqual([
      ['review:line:ln_03', d.lines.find((l) => l.line.id === 'ln_03')!.line.worktreePath],
      ['review:wt:/w/a', '/w/a'],
    ]);
  });

  it('gives an empty path when the worktree of a review tab is unknown', () => {
    workspace.layout = one(['review:line:ghost']);
    expect(openReviewTabs()).toEqual([{ key: 'review:line:ghost', name: 'ghost', path: '' }]);
  });
});

describe('itemTarget', () => {
  it('targets the line of an agent item and the folder of a plain shell', () => {
    expect(itemTarget('line:ln_03')).toEqual({ lineId: 'ln_03' });
    expect(itemTarget(SHELL)).toEqual({ worktree: '/home/alice' });
  });

  it('returns null for missing, non agent and unknown keys', () => {
    expect(itemTarget(null)).toBeNull();
    expect(itemTarget(undefined)).toBeNull();
    expect(itemTarget('')).toBeNull();
    expect(itemTarget('review:line:ln_03')).toBeNull();
    expect(itemTarget('browser:x')).toBeNull();
    expect(itemTarget('term:ghost')).toBeNull();
  });
});

describe('reviewTargetFor', () => {
  it('prefers the item line, then its task, then the first task in testing', () => {
    expect(reviewTargetFor({ lineId: 'L', taskId: 'T' })).toEqual({ lineId: 'L' });
    expect(reviewTargetFor({ taskId: 'T' })).toEqual({ taskId: 'T' });
    expect(reviewTargetFor(null)).toEqual({ taskId: 'tk_10' });
  });

  it('skips tasks in testing that have neither a line nor a worktree', () => {
    const tasks = [{ id: 'A', status: 'in_testing' }, { id: 'B', status: 'completed', lineId: 'L' }, { id: 'C', status: 'in_testing', worktreePath: '/w' }] as Task[];
    expect(reviewTargetFor(null, tasks)).toEqual({ taskId: 'C' });
    expect(reviewTargetFor({}, tasks.slice(0, 2))).toBeNull();
  });
});

describe('openReview', () => {
  it('opens the review as a tab in the focused pane, selects it and closes overlays', () => {
    app.view = 'browser';
    app.open('settings');
    const key = openReview({ lineId: 'ln_03' });
    expect(key).toBe('review:line:ln_03');
    expect(app.view).toBe('agents');
    expect(app.overlay).toBeNull();
    expect(app.selectedKey).toBe(key);
    expect(workspace.layout.panes[0].tabs).toContain(key);
    expect(workspace.layout.panes[0].active).toBe(key);
  });
});

describe('openReviewBeside', () => {
  it('splits next to the agent, keeps the agent focused and remembers the side review', () => {
    workspace.layout = one(['line:ln_03']);
    const key = openReviewBeside({ lineId: 'ln_03' });
    expect(workspace.layout.panes.map((p) => p.tabs)).toEqual([['line:ln_03'], [key]]);
    expect(workspace.layout.focused).toBe(0);
    expect(beside).toEqual({ key, pinned: false });
    expect(isSideReview(key)).toBe(true);
    expect(isSideReview('review:line:other')).toBe(false);
    expect(isSideReview(null)).toBe(false);
  });

  it('replaces the open side review in place, and appends when the remembered one was closed', () => {
    workspace.layout = two(['line:ln_03'], ['browser:x', 'review:line:a']);
    beside.key = 'review:line:a';
    openReviewBeside({ lineId: 'b' }, 0, true);
    expect(workspace.layout.panes[1].tabs).toEqual(['browser:x', 'review:line:b']);
    expect(beside).toEqual({ key: 'review:line:b', pinned: true });
    beside.key = 'review:line:closed';
    openReviewBeside({ lineId: 'c' });
    expect(workspace.layout.panes[1].tabs).toEqual(['browser:x', 'review:line:b', 'review:line:c']);
  });

  it('opens a plain tab on a phone, where there is no room for a side pane', () => {
    ui.layout = 'phone';
    workspace.layout = one(['line:ln_03']);
    const key = openReviewBeside({ lineId: 'ln_03' });
    expect(workspace.layout.panes).toHaveLength(1);
    expect(workspace.layout.panes[0].tabs).toEqual(['line:ln_03', key]);
    expect(beside.key).toBeNull();
    expect(app.selectedKey).toBe(key);
  });
});

describe('openReviewNear', () => {
  it('opens a pinned side review from a side review and a tab from anywhere else', () => {
    workspace.layout = two(['line:ln_03'], ['review:line:a']);
    beside.key = 'review:line:a';
    openReviewNear({ worktree: '/w/b' }, 'review:line:a');
    expect(workspace.layout.panes[1].tabs).toEqual(['review:wt:/w/b']);
    expect(beside).toEqual({ key: 'review:wt:/w/b', pinned: true });
    expect(workspace.layout.focused).toBe(0);

    openReviewNear({ worktree: '/w/c' }, 'line:ln_03');
    expect(app.selectedKey).toBe('review:wt:/w/c');
    expect(beside.key).toBe('review:wt:/w/b');
  });
});

describe('review modes', () => {
  it('reports off, tab or side for the selected agent', () => {
    expect(modeFor('line:ln_03')).toBe('off');
    expect(modeFor(null)).toBe('off');
    workspace.layout = one(['line:ln_03', 'review:line:ln_03']);
    expect(modeFor('line:ln_03')).toBe('tab');
    workspace.layout = two(['line:ln_03'], ['review:line:x']);
    beside.key = 'review:line:x';
    expect(modeFor('line:ln_03')).toBe('side');
  });

  it('moves a review between side, tab and off without leaving a duplicate behind', () => {
    const t = { lineId: 'ln_03' };
    const key = 'review:line:ln_03';
    workspace.layout = one(['line:ln_03']);
    setReviewMode(t, 'side');
    expect(beside.key).toBe(key);
    expect(workspace.layout.panes).toHaveLength(2);

    setReviewMode(t, 'tab');
    expect(beside.key).toBeNull();
    expect(workspace.layout.panes).toHaveLength(1);
    expect(allTabs(workspace.layout).filter((k) => k === key)).toHaveLength(1);

    setReviewMode(t, 'side');
    expect(allTabs(workspace.layout).filter((k) => k === key)).toHaveLength(1);
    expect(beside.key).toBe(key);

    setReviewMode(t, 'off');
    expect(allTabs(workspace.layout)).not.toContain(key);
    expect(beside).toEqual({ key: null, pinned: false });
  });

  it('clamps the source pane when closing the tab collapsed the split', () => {
    workspace.layout = two(['line:ln_03'], ['review:line:ln_03'], 1);
    setReviewMode({ lineId: 'ln_03' }, 'side', 1);
    expect(workspace.layout.panes.map((p) => p.tabs)).toEqual([['line:ln_03'], ['review:line:ln_03']]);
    expect(workspace.layout.focused).toBe(0);
  });

  it('closeSideReview drops only the side review and forgets the pin', () => {
    workspace.layout = two(['line:ln_03', 'review:line:tab'], ['review:line:side']);
    beside.key = 'review:line:side';
    beside.pinned = true;
    closeSideReview();
    expect(allTabs(workspace.layout)).toEqual(['line:ln_03', 'review:line:tab']);
    expect(beside).toEqual({ key: null, pinned: false });
  });
});

describe('followReview', () => {
  beforeEach(() => {
    workspace.layout = two(['line:ln_03', SHELL], ['review:line:other']);
    beside.key = 'review:line:other';
  });

  it('swaps the side review to the newly selected agent', () => {
    followReview(SHELL);
    expect(workspace.layout.panes[1].tabs).toEqual(['review:wt:/home/alice']);
    expect(beside.key).toBe('review:wt:/home/alice');
  });

  it('leaves the layout alone when pinned, without a side review, or for a non agent selection', () => {
    const before = workspace.layout;
    beside.pinned = true;
    followReview(SHELL);
    beside.pinned = false;
    followReview('browser:x');
    followReview(null);
    beside.key = null;
    followReview(SHELL);
    expect(workspace.layout).toBe(before);
  });

  it('keeps the side review key when the wanted review is already open elsewhere', () => {
    workspace.layout = two(['line:ln_03', 'review:line:ln_03'], ['review:line:other']);
    const before = workspace.layout;
    followReview('line:ln_03');
    expect(workspace.layout).toBe(before);
    expect(beside.key).toBe('review:line:other');
  });
});

describe('openReviewFromHash', () => {
  function mockClient(hash: string) {
    app.client = d;
    vi.stubGlobal('location', { hash });
    vi.useFakeTimers();
  }

  it('ignores the hash unless running against the mock daemon', () => {
    vi.stubGlobal('location', { hash: '#review=review%3Aline%3Aln_03' });
    vi.useFakeTimers();
    openReviewFromHash();
    expect(vi.getTimerCount()).toBe(0);
    expect(app.panelOpen).toBe(true);
  });

  it('opens the review named in the hash after the first render', () => {
    mockClient('#review=review%3Awt%3A%2Fw%2Fa');
    openReviewFromHash();
    expect(app.panelOpen).toBe(false);
    expect(app.selectedKey).toBeNull();
    vi.advanceTimersByTime(50);
    expect(app.selectedKey).toBe('review:wt:/w/a');
  });

  it('opens the first task in testing for a bare review hash', () => {
    mockClient('#review');
    openReviewFromHash();
    vi.advanceTimersByTime(50);
    expect(app.selectedKey).toBe('review:task:tk_10');
  });

  it('does nothing for an unparsable review hash or an unrelated one', () => {
    mockClient('#review=garbage');
    openReviewFromHash();
    vi.stubGlobal('location', { hash: '#settings' });
    openReviewFromHash();
    vi.stubGlobal('location', { hash: '#reviewer' });
    openReviewFromHash();
    expect(vi.getTimerCount()).toBe(0);
    expect(app.panelOpen).toBe(true);
  });

  it('selects the demo agent then opens its review beside it', () => {
    mockClient('#review-beside');
    openReviewFromHash();
    expect(app.panelOpen).toBe(false);
    vi.advanceTimersByTime(50);
    expect(app.selectedKey).toBe('line:ln_03');
    expect(beside.key).toBeNull();
    vi.advanceTimersByTime(50);
    expect(beside.key).toBe('review:line:ln_03');
  });
});
