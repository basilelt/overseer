import { describe, expect, it } from 'vitest';
import { DaemonError } from '../daemon/client';
import { tabInfo } from '../tabs/tabs';
import { EMPTY_LAYOUT, openInFocused } from '../workspace/workspace.svelte';
import { ghError, groupByRepo, reviewState, sizeLabel, sizeTone } from './format';
import { beside, newTerm, PR_REVIEWS_KEY } from './open.svelte';
import type { ReviewRequest } from './types';

const pr = (repo: string, number: number, updatedAt: string, more: Partial<ReviewRequest> = {}): ReviewRequest => ({
  repo,
  number,
  title: `PR ${number}`,
  url: `https://github.com/${repo}/pull/${number}`,
  author: 'bob',
  headRefName: 'feat',
  baseRefName: 'main',
  updatedAt,
  isDraft: false,
  additions: 1,
  deletions: 1,
  changedFiles: 1,
  reviewDecision: 'REVIEW_REQUIRED',
  reviewed: false,
  myState: null,
  localPath: null,
  ...more,
});

describe('github review requests', () => {
  it('groups by repo, newest repo and newest PR first', () => {
    const g = groupByRepo([pr('a/x', 1, '2026-10-01T00:00:00Z'), pr('b/y', 2, '2026-10-05T00:00:00Z'), pr('a/x', 3, '2026-10-03T00:00:00Z')]);
    expect(g.map((x) => x.repo)).toEqual(['b/y', 'a/x']);
    expect(g[1].prs.map((p) => p.number)).toEqual([3, 1]);
    expect(groupByRepo([])).toEqual([]);
  });

  it('formats size and review state', () => {
    expect(sizeLabel({ additions: 12, deletions: 3 })).toBe('+12 −3');
    expect(sizeTone({ additions: 4, deletions: 2 })).toBe('xs');
    expect(sizeTone({ additions: 400, deletions: 200 })).toBe('l');
    expect(sizeTone({ additions: 4000, deletions: 0 })).toBe('xl');
    expect(reviewState({ isDraft: true, reviewDecision: 'APPROVED', reviewed: true })).toBe('draft');
    expect(reviewState({ isDraft: false, reviewDecision: 'APPROVED', reviewed: false })).toBe('approved');
    expect(reviewState({ isDraft: false, reviewDecision: 'CHANGES_REQUESTED', reviewed: false })).toBe('changes');
    expect(reviewState({ isDraft: false, reviewDecision: 'REVIEW_REQUIRED', reviewed: true })).toBe('reviewedByYou');
    expect(reviewState({ isDraft: false, reviewDecision: null, reviewed: false })).toBe('required');
  });

  it('maps daemon errors to the command to run', () => {
    expect(ghError(new DaemonError(424, 'x', { error: 'gh is not logged in', code: 'auth' }))).toEqual({ code: 'auth', message: 'gh is not logged in', command: 'gh auth login' });
    expect(ghError(new DaemonError(424, 'x', { error: 'gh is not installed', code: 'missing' })).command).toBe('https://cli.github.com');
    expect(ghError(new Error('boom'))).toEqual({ code: 'failed', message: 'boom', command: null });
  });

  it('finds the term of a review that was still being prepared', () => {
    const term = (id: string, title: string) => ({ id, pid: 1, cwd: `/wt/${id}`, cmd: [], title, cols: 80, rows: 24, createdAt: '', exited: false });
    const known = new Set(['old']);
    const terms = [term('old', 'PR #7 a/x'), term('other', 'zsh')];
    expect(newTerm(terms, 'PR #7 a/x', known)).toBeUndefined();
    expect(newTerm([...terms, term('new', 'PR #7 a/x')], 'PR #7 a/x', known)?.id).toBe('new');
  });

  it('opens as its own tab kind and puts the agent beside the review', () => {
    expect(tabInfo(PR_REVIEWS_KEY, [])).toMatchObject({ kind: 'prs' });
    const base = openInFocused(EMPTY_LAYOUT, PR_REVIEWS_KEY);
    const l = beside(base, 'review:wt', 'term:t1', false);
    expect(l.panes.map((p) => p.tabs)).toEqual([[PR_REVIEWS_KEY, 'review:wt'], ['term:t1']]);
    expect(l.focused).toBe(1);
    const again = beside(l, 'review:wt2', 'term:t2', false);
    const pane = (k: string) => again.panes.findIndex((p) => p.tabs.includes(k));
    expect(again.panes).toHaveLength(2);
    expect(pane('review:wt2')).not.toBe(pane('term:t2'));
    expect(again.panes[again.focused].active).toBe('term:t2');
    expect(beside(EMPTY_LAYOUT, 'review:wt', 'term:t1', true).panes).toEqual([{ tabs: ['review:wt', 'term:t1'], active: 'term:t1' }]);
  });
});
