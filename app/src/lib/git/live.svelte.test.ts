import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { MockDaemon } from '../mock';
import { ui } from '../mobile/ui.svelte';
import { beside } from '../review/beside.svelte';
import { app } from '../state.svelte';
import { EMPTY_LAYOUT, allTabs, workspace } from '../workspace.svelte';
import { live, liveOn, liveTarget, sessionTarget, toggleLiveReview } from './live.svelte';
import { primaryAction } from './scm.svelte';
import type { ScmStatus } from './types';

const snapshot = app.data;
let d: MockDaemon;

beforeEach(() => {
  d = new MockDaemon({ live: false });
  app.data = { lines: d.lines, terms: d.terms, tasks: d.tasks, activity: {} } as typeof app.data;
  workspace.layout = { ...EMPTY_LAYOUT, panes: [{ tabs: ['line:ln_03'], active: 'line:ln_03' }] };
  beside.key = null;
  beside.pinned = false;
  ui.layout = 'desktop';
  app.view = 'agents';
  app.selectedKey = 'line:ln_03';
  app.liveReview = false;
  live.worktree = null;
});

afterEach(() => {
  d.stop();
  app.data = snapshot;
  app.liveReview = false;
  app.selectedKey = null;
  workspace.layout = EMPTY_LAYOUT;
  beside.key = null;
  live.worktree = null;
});

describe('live review', () => {
  it('opens the session review beside the agent and closes it again', () => {
    expect(toggleLiveReview()).toBe(true);
    expect(liveOn()).toBe(true);
    expect(beside.key).toBe('review:line:ln_03');
    expect(allTabs(workspace.layout)).toEqual(['line:ln_03', 'review:line:ln_03']);
    expect(toggleLiveReview()).toBe(false);
    expect(liveOn()).toBe(false);
    expect(beside.key).toBeNull();
    expect(allTabs(workspace.layout)).toEqual(['line:ln_03']);
  });

  it('moves an already open review tab beside the agent instead of duplicating it, and leaves no tab behind', () => {
    workspace.layout = { ...EMPTY_LAYOUT, panes: [{ tabs: ['line:ln_03', 'review:line:ln_03'], active: 'line:ln_03' }] };
    toggleLiveReview();
    expect(allTabs(workspace.layout).filter((k) => k === 'review:line:ln_03')).toHaveLength(1);
    toggleLiveReview();
    expect(allTabs(workspace.layout)).toEqual(['line:ln_03']);
  });

  it('does nothing for a tab that is not a session', () => {
    expect(toggleLiveReview('graph:/p')).toBe(false);
    expect(app.liveReview).toBe(false);
    expect(beside.key).toBeNull();
  });

  it('turns off by itself when the side review is closed another way', () => {
    toggleLiveReview();
    beside.key = null;
    expect(liveOn()).toBe(false);
    expect(toggleLiveReview()).toBe(true);
    expect(liveOn()).toBe(true);
  });

  it('is only on in the agents view', () => {
    toggleLiveReview();
    app.view = 'kanban';
    expect(liveOn()).toBe(false);
  });

  it('targets the picked worktree for git and the session for the review', () => {
    toggleLiveReview();
    expect(liveTarget()).toEqual({ lineId: 'ln_03' });
    expect(sessionTarget()).toEqual({ lineId: 'ln_03' });
    live.worktree = '/w/api';
    expect(liveTarget()).toEqual({ worktree: '/w/api' });
    expect(sessionTarget()).toEqual({ lineId: 'ln_03' });
  });

  it('forgets the picked worktree when it is turned on again', () => {
    toggleLiveReview();
    live.worktree = '/w/api';
    toggleLiveReview();
    toggleLiveReview();
    expect(live.worktree).toBeNull();
  });

  it('targets a shell worktree by path', () => {
    beside.key = 'review:wt:/home/x';
    app.liveReview = true;
    expect(liveTarget()).toEqual({ worktree: '/home/x' });
    expect(sessionTarget()).toEqual({ worktree: '/home/x' });
    beside.key = 'review:task:t1';
    expect(liveTarget()).toBeNull();
  });
});

describe('primary action of the bar', () => {
  const base = { staged: [], changes: [], conflicts: [], operation: null, branch: 'feat/x', upstream: 'origin/feat/x', ahead: 0, behind: 0 } as unknown as ScmStatus;
  const f = { path: 'a', status: 'M' } as const;

  it('commits staged files, or every change when nothing is staged, once there is a message', () => {
    expect(primaryAction({ ...base, staged: [f] }, '')).toEqual({ kind: 'commit', n: 1, enabled: false });
    expect(primaryAction({ ...base, staged: [f] }, 'x')).toEqual({ kind: 'commit', n: 1, enabled: true });
    expect(primaryAction({ ...base, changes: [f, f] }, 'x')).toEqual({ kind: 'commit', n: 2, enabled: true });
  });

  it('asks to finish a merge and blocks it while conflicts remain', () => {
    expect(primaryAction({ ...base, operation: 'merge', conflicts: [f] }, 'x')).toEqual({ kind: 'merge', n: 1, enabled: false });
    expect(primaryAction({ ...base, operation: 'merge', staged: [f] }, 'x')).toEqual({ kind: 'merge', n: 0, enabled: true });
  });

  it('brings publish, pull then push forward when the tree is clean', () => {
    expect(primaryAction({ ...base, upstream: null }, '')).toMatchObject({ kind: 'publish', enabled: true });
    expect(primaryAction({ ...base, behind: 2, ahead: 1 }, '')).toMatchObject({ kind: 'pull', n: 2 });
    expect(primaryAction({ ...base, ahead: 3 }, '')).toMatchObject({ kind: 'push', n: 3 });
    expect(primaryAction(base, '')).toEqual({ kind: 'none', n: 0, enabled: false });
    expect(primaryAction({ ...base, branch: null, upstream: null }, '')).toEqual({ kind: 'none', n: 0, enabled: false });
    expect(primaryAction(null, 'x')).toEqual({ kind: 'none', n: 0, enabled: false });
  });
});
