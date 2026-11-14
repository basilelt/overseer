import { describe, expect, it, vi } from 'vitest';
import { DaemonError } from '../daemon/client';
import { MockDaemon } from '../daemon/mock';
import type { DaemonEvent } from '../types';
import { gitApi, scmBody, scmQuery, type GitApi } from './api';
import { Scm, fileCount, targetKey, touches } from './scm.svelte';
import type { ScmStatus } from './types';

const WT = '/home/alice/worktrees/overseer/feat-term-resize';

function setup() {
  const m = new MockDaemon({ live: false, delayMs: 0 });
  const notify = vi.fn();
  const scm = new Scm(() => gitApi(m), notify);
  return { m, scm, notify };
}

async function loaded(scm: Scm) {
  await vi.waitFor(() => expect(scm.status).not.toBeNull());
  return scm.status!;
}

const ev = (type: string, data: unknown) => ({ id: '1', type, at: '', data }) as DaemonEvent;

describe('scm request shape', () => {
  it('sends the worktree or line in the query and the body like the review routes', () => {
    expect(scmQuery({ worktree: '/a b/c' })).toBe('worktree=%2Fa%20b%2Fc');
    expect(scmQuery({ lineId: 'st-1' })).toBe('line=st-1');
    expect(scmQuery({})).toBe('');
    expect(scmBody({ worktree: '/x', lineId: 'y' })).toEqual({ worktree: '/x' });
    expect(scmBody({ lineId: 'y' })).toEqual({ lineId: 'y' });
    expect(scmBody({})).toEqual({});
  });

  it('posts each action to its own route with the agreed body', async () => {
    const calls: [string, string, unknown][] = [];
    const c = { request: (m: string, p: string, b?: unknown) => (calls.push([m, p, b]), Promise.resolve({})) } as never;
    const g = gitApi(c);
    const t = { worktree: '/w' };
    await g.status(t);
    await g.stage(t, ['a']);
    await g.unstage(t, 'all');
    await g.discard(t, ['b']);
    await g.commitChanges(t, { message: 'm', push: true });
    await g.fetch(t);
    await g.pull(t);
    await g.push(t);
    await g.switchBranch(t, 'x');
    await g.merge(t, 'main');
    await g.stash(t);
    await g.revert(t, 'abcd');
    expect(calls).toEqual([
      ['GET', '/v1/git/status?worktree=%2Fw', undefined],
      ['POST', '/v1/git/stage', { worktree: '/w', files: ['a'] }],
      ['POST', '/v1/git/unstage', { worktree: '/w', files: 'all' }],
      ['POST', '/v1/git/discard', { worktree: '/w', files: ['b'] }],
      ['POST', '/v1/git/commit', { worktree: '/w', message: 'm', push: true }],
      ['POST', '/v1/git/fetch', { worktree: '/w' }],
      ['POST', '/v1/git/pull', { worktree: '/w' }],
      ['POST', '/v1/git/push', { worktree: '/w' }],
      ['POST', '/v1/git/switch', { worktree: '/w', branch: 'x', create: false }],
      ['POST', '/v1/git/merge', { worktree: '/w', rev: 'main' }],
      ['POST', '/v1/git/stash', { worktree: '/w', pop: false }],
      ['POST', '/v1/git/revert', { worktree: '/w', sha: 'abcd' }],
    ]);
  });
});

describe('Scm store', () => {
  it('loads the status of the target it is given', async () => {
    const { scm } = setup();
    scm.setTarget({ worktree: WT });
    const s = await loaded(scm);
    expect(s.worktree).toBe(WT);
    expect(fileCount(s)).toBe(s.staged.length + s.changes.length);
    expect(scm.loading).toBe(false);
  });

  it('does not reload when the same target is set again', async () => {
    const { m, scm } = setup();
    const spy = vi.spyOn(m, 'request');
    scm.setTarget({ worktree: WT });
    await loaded(scm);
    scm.setTarget({ worktree: WT });
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('drops a status that arrives after the target changed', async () => {
    let release!: (s: ScmStatus) => void;
    const slow = new Promise<ScmStatus>((r) => (release = r));
    const api = { status: vi.fn((t) => (t.worktree === '/old' ? slow : Promise.resolve({ worktree: '/new' } as ScmStatus))) } as unknown as GitApi;
    const scm = new Scm(() => api);
    scm.setTarget({ worktree: '/old' });
    scm.setTarget({ worktree: '/new' });
    await vi.waitFor(() => expect(scm.status?.worktree).toBe('/new'));
    release({ worktree: '/old' } as ScmStatus);
    await Promise.resolve();
    expect(scm.status?.worktree).toBe('/new');
  });

  it('shows the load error and keeps no status', async () => {
    const api = { status: vi.fn(() => Promise.reject(new DaemonError(410, 'the worktree folder /x no longer exists'))) } as unknown as GitApi;
    const scm = new Scm(() => api);
    scm.setTarget({ worktree: '/x' });
    await vi.waitFor(() => expect(scm.error).toBe('the worktree folder /x no longer exists'));
    expect(scm.status).toBeNull();
  });

  it('does nothing without a client or a target', async () => {
    const scm = new Scm(() => null);
    scm.setTarget({ worktree: WT });
    await scm.load();
    expect(scm.status).toBeNull();
    expect(await scm.push()).toBeNull();
    const s2 = setup().scm;
    s2.setTarget(null);
    await s2.load();
    expect(s2.status).toBeNull();
  });

  it('stages, commits and pushes, taking the status from each reply', async () => {
    const { scm } = setup();
    scm.setTarget({ worktree: WT });
    const s = await loaded(scm);
    const file = s.changes[0].path;
    await scm.stage([file]);
    expect(scm.status!.staged.map((f) => f.path)).toContain(file);
    await scm.unstage([file]);
    expect(scm.status!.changes.map((f) => f.path)).toContain(file);
    const r = await scm.commit({ message: 'feat: x', all: true });
    expect(r?.commit).toMatch(/^[0-9a-f]{40}$/);
    expect(fileCount(scm.status)).toBe(0);
    expect(scm.status!.outgoing[0].subject).toBe('feat: x');
    await scm.push();
    expect(scm.status!.outgoing).toEqual([]);
    expect(scm.busy).toBeNull();
  });

  it('reports a rejected action and keeps the status', async () => {
    const { scm, notify } = setup();
    scm.setTarget({ worktree: WT });
    const before = await loaded(scm);
    expect(await scm.commit({ message: '  ' })).toBeNull();
    expect(notify).toHaveBeenCalledWith('commit message required');
    expect(scm.status).toEqual(before);
  });

  it('reloads once the daemon is back, so its worktree watch is registered again', async () => {
    const api = { status: vi.fn(() => Promise.resolve({ worktree: '/w' } as ScmStatus)) } as unknown as GitApi;
    const scm = new Scm(() => api);
    scm.setTarget({ worktree: '/w' });
    scm.connection('connected');
    await vi.waitFor(() => expect(scm.status).not.toBeNull());
    expect(api.status).toHaveBeenCalledTimes(1);
    scm.connection('reconnecting');
    expect(api.status).toHaveBeenCalledTimes(1);
    scm.connection('connected');
    expect(api.status).toHaveBeenCalledTimes(2);
    scm.connection('connected');
    expect(api.status).toHaveBeenCalledTimes(2);
  });

  it('runs one action at a time', async () => {
    let release!: () => void;
    const api = {
      status: () => Promise.resolve({ worktree: '/w' } as ScmStatus),
      fetch: vi.fn(() => new Promise((r) => (release = () => r({ status: { worktree: '/w' } })))),
      pull: vi.fn(),
    } as unknown as GitApi;
    const scm = new Scm(() => api);
    scm.setTarget({ worktree: '/w' });
    await vi.waitFor(() => expect(scm.status).not.toBeNull());
    const first = scm.fetch();
    expect(scm.busy).toBe('fetch');
    expect(await scm.pull()).toBeNull();
    expect(api.pull).not.toHaveBeenCalled();
    release();
    await first;
    expect(scm.busy).toBeNull();
  });

  it('runs actions on the resolved worktree, not the line it started from', async () => {
    const calls: unknown[] = [];
    const api = {
      status: () => Promise.resolve({ worktree: '/resolved' } as ScmStatus),
      stash: (t: unknown) => (calls.push(t), Promise.resolve({ status: { worktree: '/resolved' } })),
    } as unknown as GitApi;
    const scm = new Scm(() => api);
    scm.setTarget({ lineId: 'st-1' });
    await vi.waitFor(() => expect(scm.status).not.toBeNull());
    await scm.stash();
    expect(calls).toEqual([{ worktree: '/resolved' }]);
  });

  it('switches branch, merges, stashes, pops and reverts through the mock', async () => {
    const { scm } = setup();
    scm.setTarget({ worktree: WT });
    await loaded(scm);
    await scm.stash();
    expect(scm.status!.stashes).toBe(1);
    await scm.merge('main');
    await scm.revert(scm.status!.head!);
    await scm.pop();
    expect(scm.status!.stashes).toBe(0);
    await scm.stash();
    await scm.switchTo('feat/new', true);
    expect(scm.status!.branch).toBe('feat/new');
    expect(scm.status!.upstream).toBeNull();
    await scm.discard('all');
    await scm.fetch();
    await scm.switchTo('main');
    await scm.pull();
    expect(scm.status!.behind).toBe(0);
  });
});

describe('scm helpers', () => {
  const s = { worktree: '/w', project: '/p', staged: [{ path: 'a', status: 'M' }], changes: [], conflicts: [{ path: 'c', status: 'U' }] } as unknown as ScmStatus;

  it('keys targets by worktree first, then line', () => {
    expect(targetKey({ worktree: '/w', lineId: 'l' })).toBe('w:/w');
    expect(targetKey({ lineId: 'l' })).toBe('l:l');
    expect(targetKey(null)).toBe('');
  });

  it('counts staged, changed and conflicted files', () => {
    expect(fileCount(s)).toBe(2);
    expect(fileCount(null)).toBe(0);
  });

  it('reloads only for events about its worktree or project', () => {
    expect(touches(s, ev('review.changed', { worktree: '/w' }))).toBe(true);
    expect(touches(s, ev('review.changed', { worktree: '/other' }))).toBe(false);
    expect(touches(s, ev('git.changed', { project: '/p' }))).toBe(true);
    expect(touches(s, ev('worktree.updated', { project: '/q' }))).toBe(false);
    expect(touches(s, ev('worktree.updated', {}))).toBe(true);
    expect(touches(s, ev('line.activity', { lineId: 'x' }))).toBe(false);
    expect(touches(null, ev('git.changed', {}))).toBe(false);
  });
});
