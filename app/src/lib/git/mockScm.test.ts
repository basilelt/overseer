import { describe, expect, it } from 'vitest';
import { MockDaemon } from '../mock';
import type { DaemonEvent } from '../types';
import { gitApi } from './api';
import { validPath } from './mockScm';

const WT = '/home/alice/worktrees/overseer/feat-term-resize';

function setup() {
  const m = new MockDaemon({ live: false, delayMs: 0 });
  const events: DaemonEvent[] = [];
  m.subscribe((e) => events.push(e));
  return { m, g: gitApi(m), events };
}

const fails = (p: Promise<unknown>, status: number, message: string) => expect(p).rejects.toMatchObject({ status, message });

describe('mock /v1/git routes', () => {
  it('resolves a line to its worktree and refuses unknown or missing targets like the daemon', async () => {
    const { m, g } = setup();
    const line = m.lines.find((l) => l.line.worktreePath && l.line.repo !== 'unknown')!;
    expect((await g.status({ lineId: line.line.id })).worktree).toBe(line.line.worktreePath);
    await fails(g.status({ lineId: 'nope' }), 404, "no line matches 'nope'");
    await fails(g.status({ worktree: 'rel/path' }), 400, 'rel/path is not an absolute path');
    await fails(g.status({}), 400, 'worktree or lineId required');
    await fails(g.push({}), 400, 'worktree or lineId required');
  });

  it('serves branch, upstream, files and commits for a worktree', async () => {
    const { g } = setup();
    const s = await g.status({ worktree: WT });
    expect(s.branch).toBeTruthy();
    expect(s.base).toBe('main');
    expect(s.branches).toContain('main');
    expect(s.operation).toBeNull();
    expect(s.submodules).toEqual([]);
    expect(s.staged.length + s.changes.length).toBeGreaterThan(0);
    expect(s.ahead).toBe(s.upstream ? s.outgoing.length : 0);
  });

  it('emits git.changed and worktree.updated after a change', async () => {
    const { g, events } = setup();
    await g.stash({ worktree: WT });
    expect(events.map((e) => e.type)).toEqual(expect.arrayContaining(['git.changed', 'worktree.updated']));
  });

  it('validates the files of stage, unstage and discard', async () => {
    const { g } = setup();
    await fails(g.stage({ worktree: WT }, []), 400, 'files required');
    await fails(g.unstage({ worktree: WT }, ['../x']), 400, 'invalid path ../x');
    await fails(g.discard({ worktree: WT }, ['/etc/passwd']), 400, 'invalid path /etc/passwd');
  });

  it('moves files between staged and changes, untracked ones as added', async () => {
    const { g } = setup();
    const s = await g.status({ worktree: WT });
    const untracked = s.changes.find((f) => f.status === '?')!;
    const staged = (await g.stage({ worktree: WT }, [untracked.path])).status;
    expect(staged.staged.find((f) => f.path === untracked.path)?.status).toBe('A');
    const back = (await g.unstage({ worktree: WT }, 'all')).status;
    expect(back.staged).toEqual([]);
    expect(back.changes.find((f) => f.path === untracked.path)?.status).toBe('?');
    const gone = (await g.discard({ worktree: WT }, [untracked.path])).status;
    expect(gone.changes.some((f) => f.path === untracked.path)).toBe(false);
  });

  it('commits staged files, refuses an empty message or nothing to commit, and amends', async () => {
    const { g } = setup();
    await fails(g.commitChanges({ worktree: WT }, { message: ' ' }), 400, 'commit message required');
    await g.unstage({ worktree: WT }, 'all');
    await fails(g.commitChanges({ worktree: WT }, { message: 'x' }), 409, 'nothing to commit');
    const r = await g.commitChanges({ worktree: WT }, { message: 'feat: all', all: true });
    expect(r.status.outgoing[0]).toEqual({ sha: r.commit, subject: 'feat: all' });
    const a = await g.commitChanges({ worktree: WT }, { message: '', amend: true });
    expect(a.status.outgoing[0].subject).toBe('feat: all');
    const p = await g.commitChanges({ worktree: WT }, { message: 'fix: y', amend: true, push: true });
    expect(p.status.outgoing).toEqual([]);
  });

  it('pulls, pushes and publishes a branch without upstream', async () => {
    const { g } = setup();
    const c = await g.switchBranch({ worktree: WT }, 'feat/fresh', true);
    expect(c.status.upstream).toBeNull();
    await fails(g.pull({ worktree: WT }), 409, 'feat/fresh has no upstream branch');
    const p = await g.push({ worktree: WT });
    expect(p.status.upstream).toBe('origin/feat/fresh');
    expect((await g.pull({ worktree: WT })).status.behind).toBe(0);
    expect((await g.fetch({ worktree: WT })).status.worktree).toBe(WT);
  });

  it('refuses bad or unknown branches with the git message', async () => {
    const { g } = setup();
    await fails(g.switchBranch({ worktree: WT }, 'bad name'), 400, 'invalid branch name bad name');
    await fails(g.switchBranch({ worktree: WT }, '-f'), 400, 'invalid branch name -f');
    await fails(g.switchBranch({ worktree: WT }, 'nope'), 409, 'git switch: fatal: invalid reference: nope');
    await fails(g.switchBranch({ worktree: WT }, 'main', true), 409, "git switch: fatal: a branch named 'main' already exists");
  });

  it('merges and reverts only on a clean tree, with checked revisions', async () => {
    const { g } = setup();
    const s = await g.status({ worktree: WT });
    const n = s.staged.length + s.changes.length;
    await fails(g.merge({ worktree: WT }, 'main'), 409, `${n} uncommitted change(s): commit or stash them first`);
    await fails(g.merge({ worktree: WT }, '-x'), 400, 'invalid revision -x');
    await fails(g.merge({ worktree: WT }, 'ghost'), 404, 'unknown revision ghost');
    await fails(g.revert({ worktree: WT }, 'zz'), 400, 'sha must be 4-64 hex digits');
    await fails(g.revert({ worktree: WT }, 'deadbeef'), 404, 'unknown commit deadbeef');
    await fails(g.merge({ worktree: WT }, 'deadbeef'), 404, 'unknown revision deadbeef');
    await g.stash({ worktree: WT });
    const m = await g.merge({ worktree: WT }, 'main');
    expect(m.status.outgoing[0].subject).toMatch(/^Merge main into /);
    const r = await g.revert({ worktree: WT }, m.status.head!);
    expect(r.commit).toBe(r.status.head);
  });

  it('stashes and pops, and refuses an empty stash or pop', async () => {
    const { g } = setup();
    await fails(g.stash({ worktree: WT }, true), 409, 'no stash to pop');
    const s = await g.stash({ worktree: WT });
    expect(s.status.stashes).toBe(1);
    await fails(g.stash({ worktree: WT }), 409, 'nothing to stash');
    const p = await g.stash({ worktree: WT }, true);
    expect(p.status.stashes).toBe(0);
    expect(p.status.changes.length).toBeGreaterThan(0);
  });

  it('merges a commit of the project graph by sha', async () => {
    const { g } = setup();
    const P = '/home/alice/Documents/Github/work/overseer';
    const page = await g.graph(P, { limit: 5 });
    const m = await g.merge({ worktree: P }, page.commits[3].sha.slice(0, 7));
    expect(m.status.outgoing[0].subject).toMatch(/^Merge /);
  });

  it('starts a main checkout clean on main', async () => {
    const { g } = setup();
    const s = await g.status({ worktree: '/home/alice/Documents/Github/work/overseer' });
    expect(s.branch).toBe('main');
    expect(s.staged.length + s.changes.length).toBe(0);
    expect(s.project).toBe('/home/alice/Documents/Github/work/overseer');
  });

  it('leaves other paths to the other mock routes', async () => {
    const { m } = setup();
    await expect(m.request('GET', '/v1/git/nope?worktree=%2Fx')).rejects.toMatchObject({ status: 404 });
    await expect(m.request('POST', '/v1/git/status', { worktree: '/x' })).rejects.toMatchObject({ status: 404 });
  });
});

describe('validPath', () => {
  it('accepts relative paths and refuses empty, absolute, parent or NUL ones', () => {
    expect(validPath('src/a b.rs')).toBe(true);
    for (const p of ['', '/a', 'a/../b', '..', 'a\0b', 3]) expect(validPath(p)).toBe(false);
  });
});
