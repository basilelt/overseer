import { describe, expect, it } from 'vitest';
import type { GraphCommit, Worktree } from './types';
import { authors, cleanable, commitUrl, dimmed, formatSize, labelsBySha, needsMore, ROW_H, scrollToRow, searchMatches, sortWorktrees, stepMatch, visibleRange } from './view';

const commit = (sha: string, subject: string, author = 'Ada'): GraphCommit => ({ sha, parents: [], author, email: '', at: '2026-10-29T09:00:00Z', subject });

describe('virtual window', () => {
  it('renders only the visible rows plus overscan', () => {
    expect(visibleRange(10_000, 0, 600, ROW_H, 10)).toEqual({ start: 0, end: Math.ceil(600 / ROW_H) + 10 });
    const r = visibleRange(10_000, 5000 * ROW_H, 600, ROW_H, 10);
    expect(r.start).toBe(4990);
    expect(r.end - r.start).toBeLessThan(60);
    expect(visibleRange(30, 0, 10_000)).toEqual({ start: 0, end: 30 });
    expect(visibleRange(0, 0, 600)).toEqual({ start: 0, end: 0 });
  });

  it('asks for the next page near the end only when there is one', () => {
    expect(needsMore({ start: 300, end: 360 }, 400, true)).toBe(true);
    expect(needsMore({ start: 0, end: 60 }, 400, true)).toBe(false);
    expect(needsMore({ start: 300, end: 400 }, 400, false)).toBe(false);
  });

  it('scrolls a row into view', () => {
    expect(scrollToRow(0, 280, 560)).toBe(0);
    expect(scrollToRow(30, 0, 560)).toBe(31 * ROW_H - 560);
    expect(scrollToRow(12, 0, 560)).toBe(0);
  });
});

describe('filters', () => {
  const cs = [commit('aaaa111', 'Fix login', 'Ada'), commit('bbbb222', 'Add search', 'Hugo'), commit('cccc333', 'fix search paging', 'Ada'), commit('abcd444', 'Docs', 'Ines')];

  it('finds commits by sha prefix or subject and steps through them', () => {
    expect(searchMatches(cs, 'fix')).toEqual([0, 2]);
    expect(searchMatches(cs, 'SEARCH')).toEqual([1, 2]);
    expect(searchMatches(cs, 'abcd')).toEqual([3]);
    expect(searchMatches(cs, '  ')).toEqual([]);
    const m = searchMatches(cs, 'search');
    expect(stepMatch(m, -1, 1)).toBe(1);
    expect(stepMatch(m, 1, 1)).toBe(2);
    expect(stepMatch(m, 2, 1)).toBe(1);
    expect(stepMatch(m, 1, -1)).toBe(2);
    expect(stepMatch([], 0, 1)).toBe(-1);
  });

  it('dims other authors and lists authors by commit count', () => {
    expect(authors(cs)).toEqual(['Ada', 'Hugo', 'Ines']);
    expect(cs.filter((c) => dimmed(c, 'Ada')).map((c) => c.author)).toEqual(['Hugo', 'Ines']);
    expect(cs.some((c) => dimmed(c, ''))).toBe(false);
  });
});

describe('labels', () => {
  it('marks the checked-out branch, hides remotes that match their local branch and sorts', () => {
    const m = labelsBySha({
      head: { sha: 's1', branch: 'main' },
      refs: [
        { name: 'v1.0', kind: 'tag', sha: 's1' },
        { name: 'origin/main', kind: 'remote', sha: 's1' },
        { name: 'main', kind: 'branch', sha: 's1' },
        { name: 'origin/feat', kind: 'remote', sha: 's2' },
        { name: 'feat', kind: 'branch', sha: 's3' },
      ],
    });
    expect(m.get('s1')).toEqual([
      { name: 'main', kind: 'head' },
      { name: 'v1.0', kind: 'tag' },
    ]);
    expect(m.get('s2')).toEqual([{ name: 'origin/feat', kind: 'remote' }]);
  });

  it('labels a detached HEAD', () => {
    expect(labelsBySha({ head: { sha: 's9', branch: null }, refs: [] }).get('s9')).toEqual([{ name: 'HEAD', kind: 'head' }]);
  });

  it('builds GitHub commit links only for GitHub repos', () => {
    expect(commitUrl('github.com/Acme/x', 'abc')).toBe('https://github.com/Acme/x/commit/abc');
    expect(commitUrl('local:/home/u/x', 'abc')).toBeNull();
  });
});

describe('worktree helpers', () => {
  const w = (over: Partial<Worktree>): Worktree => ({
    id: over.path ?? '/x',
    path: '/x',
    project: '/p',
    branch: 'b',
    head: 'h',
    kind: 'managed',
    base: 'main',
    state: 'idle',
    dirty: false,
    changes: 0,
    merged: false,
    pushed: false,
    unpushed: 0,
    lines: [],
    termIds: [],
    locked: false,
    ...over,
  });

  it('formats sizes', () => {
    expect(formatSize(null)).toBe('—');
    expect(formatSize(512)).toBe('512 B');
    expect(formatSize(1536)).toBe('1.5 KB');
    expect(formatSize(48_123_904)).toBe('46 MB');
  });

  it('keeps the main checkout first, then in use, idle, stale, missing', () => {
    const list = sortWorktrees([
      w({ path: '/m', state: 'missing' }),
      w({ path: '/s', state: 'stale' }),
      w({ path: '/main', kind: 'main' }),
      w({ path: '/u', state: 'in_use' }),
      w({ path: '/i1', state: 'idle', lastActivityAt: '2026-10-01T00:00:00Z' }),
      w({ path: '/i2', state: 'idle', lastActivityAt: '2026-10-20T00:00:00Z' }),
    ]);
    expect(list.map((x) => x.path)).toEqual(['/main', '/u', '/i2', '/i1', '/s', '/m']);
  });

  it('offers bulk cleanup only for managed stale or missing worktrees', () => {
    expect(cleanable(w({ state: 'stale' }))).toBe(true);
    expect(cleanable(w({ state: 'missing' }))).toBe(true);
    expect(cleanable(w({ state: 'idle' }))).toBe(false);
    expect(cleanable(w({ state: 'stale', kind: 'orphan' }))).toBe(false);
  });
});
