import { describe, expect, it } from 'vitest';
import type { Related } from '../review/types';
import { worktreeChoices } from './worktrees';

const rel = { current: '/w/web', worktrees: [{ path: '/w/web', branch: 'feat/x', files: 3 }, { path: '/w/api/', branch: null, files: 0 }] } as unknown as Related;

describe('worktreeChoices', () => {
  it('lists the session worktrees with their submodules right under their parent', () => {
    const c = worktreeChoices(rel, { '/w/web': [{ path: 'lib', worktree: '/w/web/lib' }] }, '/w/web/lib');
    expect(c.map((x) => [x.name, x.submodule, x.current])).toEqual([
      ['web', false, false],
      ['lib', true, true],
      ['api', false, false],
    ]);
    expect(c[0]).toMatchObject({ branch: 'feat/x', files: 3 });
  });

  it('falls back to the current worktree alone when the session list is unknown', () => {
    expect(worktreeChoices(null, {}, '/w/solo')).toEqual([{ path: '/w/solo', name: 'solo', branch: null, files: 0, submodule: false, current: true }]);
    expect(worktreeChoices(null, {}, null)).toEqual([]);
  });
});
