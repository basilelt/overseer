import { describe, expect, it } from 'vitest';
import { OPEN, unresolvedByWorktree } from './unresolved';

describe('unresolvedByWorktree', () => {
  it('counts pending, sent and addressed threads per worktree and skips resolved ones', () => {
    expect(OPEN).toEqual(['pending', 'sent', 'addressed']);
    const ts = [
      { worktree: '/a', status: 'pending' },
      { worktree: '/a', status: 'addressed' },
      { worktree: '/b', status: 'sent' },
      { worktree: '/b', status: 'resolved' },
    ] as const;
    expect(unresolvedByWorktree(ts)).toEqual({ '/a': 2, '/b': 1 });
  });
});
