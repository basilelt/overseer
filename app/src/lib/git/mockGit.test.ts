import { describe, expect, it } from 'vitest';
import { api } from '../api';
import { MockDaemon } from '../mock';
import type { DaemonEvent } from '../types';
import { gitApi } from './api';
import { chosen, cleanupTargets, planLines, runCleanup } from './cleanup';
import { layout } from './layout';
import { GraphLoader } from './loader';

const PROJECT = '/home/alice/Documents/Github/work/overseer';

function setup() {
  const m = new MockDaemon({ live: false, delayMs: 0 });
  const events: DaemonEvent[] = [];
  m.subscribe((e) => events.push(e));
  return { m, g: gitApi(m), a: api(m), events };
}

describe('mock git graph', () => {
  it('serves a few thousand commits in date order with refs and agent overlays', async () => {
    const { g } = setup();
    const p = await g.graph(PROJECT, { limit: 2000 });
    expect(p.commits).toHaveLength(2000);
    expect(p.next).toBe('2000');
    const index = new Map(p.commits.map((c, i) => [c.sha, i]));
    for (const [i, c] of p.commits.entries()) for (const par of c.parents) expect(index.get(par) ?? Infinity).toBeGreaterThan(i);
    const all = [...p.commits, ...(await g.graph(PROJECT, { limit: 2000, before: p.next })).commits];
    expect(all.length).toBeGreaterThan(2900);
    expect(all.some((c) => c.parents.length >= 3)).toBe(true);
    expect(p.head.branch).toBe('main');
    expect(p.refs.some((r) => r.kind === 'tag')).toBe(true);
    expect(p.refs.some((r) => r.kind === 'remote')).toBe(true);
    const o = p.overlays.find((x) => x.branch === 'feat/workspaces');
    expect(o).toMatchObject({ agent: 'claude', lineId: 'ln_04' });
    expect(p.refs.find((r) => r.name === 'feat/workspaces')?.sha).toBe(o?.sha);
  });

  it('narrows the history to the current branch or agents', async () => {
    const { g } = setup();
    const all = await g.graph(PROJECT, { limit: 2000 });
    const current = await g.graph(PROJECT, { limit: 2000, refs: 'current' });
    const agents = await g.graph(PROJECT, { limit: 2000, refs: 'agents' });
    const tip = (await g.graph(PROJECT, { limit: 1 })).overlays[0].sha;
    expect(current.commits.some((c) => c.sha === tip)).toBe(false);
    expect(agents.commits.some((c) => c.sha === tip)).toBe(true);
    expect(current.commits[0].sha).toBe(all.head.sha);
  });

  it('pages through the loader with the same layout as one pass', async () => {
    const { g } = setup();
    const l = new GraphLoader(g, PROJECT, 'all', 250);
    await l.load();
    await l.more();
    const s = await l.more();
    expect(s.commits).toHaveLength(750);
    const whole = await g.graph(PROJECT, { limit: 750 });
    expect(s.commits.map((c) => c.sha)).toEqual(whole.commits.map((c) => c.sha));
    expect(s.rows).toEqual(layout(whole.commits).rows);
    const r = await l.reload();
    expect(r.commits).toHaveLength(750);
  });

  it('serves commit details and refuses bad shas', async () => {
    const { g } = setup();
    const p = await g.graph(PROJECT, { limit: 50 });
    const c = p.commits.find((x) => x.parents.length === 1)!;
    const d = await g.commit(PROJECT, c.sha.slice(0, 10));
    expect(d.sha).toBe(c.sha);
    expect(d.files.length).toBeGreaterThan(0);
    expect(d.diff).toContain('diff --git');
    await expect(g.commit(PROJECT, 'zz')).rejects.toMatchObject({ status: 400 });
    await expect(g.commit(PROJECT, 'ffffffffff')).rejects.toMatchObject({ status: 404 });
  });

  it('creates a branch once', async () => {
    const { g, events } = setup();
    const p = await g.graph(PROJECT, { limit: 5 });
    await g.createBranch(PROJECT, 'spike/x', p.commits[3].sha);
    expect((await g.graph(PROJECT, { limit: 5 })).refs.some((r) => r.name === 'spike/x')).toBe(true);
    expect(events.some((e) => e.type === 'git.changed')).toBe(true);
    await expect(g.createBranch(PROJECT, 'spike/x', p.commits[3].sha)).rejects.toMatchObject({ status: 409 });
    await expect(g.createBranch(PROJECT, 'bad name', p.commits[3].sha)).rejects.toMatchObject({ status: 400 });
  });
});

describe('mock worktrees', () => {
  it('lists the main checkout, managed worktrees and orphans separately', async () => {
    const { g } = setup();
    const l = await g.worktrees(PROJECT);
    expect(l.worktrees[0].kind).toBe('main');
    expect(l.store).toMatch(/\.local\/state\/overseer\/worktrees\/overseer-[0-9a-f]{8}$/);
    expect(new Set(l.worktrees.map((w) => w.state))).toEqual(new Set(['idle', 'in_use', 'stale', 'missing']));
    expect(l.orphans.every((w) => w.kind === 'orphan')).toBe(true);
    expect(l.worktrees.filter((w) => w.kind === 'managed').every((w) => w.path.startsWith(`${l.store}/`))).toBe(true);
  });

  it('previews cleanup, refuses unsafe ones and removes only what the plan says', async () => {
    const { g, events } = setup();
    const l = await g.worktrees(PROJECT);
    const byBranch = (b: string) => l.worktrees.find((w) => w.branch === b)!;
    const merged = byBranch('sb/claude/1021-1412');
    const unmerged = byBranch('task/96-csv-export');
    const busy = l.worktrees.find((w) => w.state === 'in_use')!;
    const plans = await Promise.all([merged, unmerged, busy, l.worktrees[0], l.orphans[0]].map((w) => g.cleanup(w.id, true)));
    expect(plans[0]).toMatchObject({ refused: null, remove: [{ kind: 'worktree' }, { kind: 'branch', name: 'sb/claude/1021-1412' }], keep: [] });
    expect(plans[1].keep).toEqual([{ kind: 'branch', name: 'task/96-csv-export', reason: 'not merged into main' }]);
    expect(plans[2].refused).toMatch(/in use/);
    expect(plans[3].refused).toMatch(/main checkout/);
    expect(plans[4].refused).toMatch(/outside Overseer/);
    expect(planLines(plans[2])).toEqual([{ kind: 'refused', label: expect.stringContaining('in use') }]);
    expect(planLines(plans[1]).map((x) => x.kind)).toEqual(['remove', 'keep']);
    expect((await g.worktrees(PROJECT)).worktrees).toHaveLength(l.worktrees.length);

    const ids = cleanupTargets(plans);
    expect(ids).toEqual([merged.id, unmerged.id]);
    expect(chosen(ids, new Set([unmerged.id]))).toEqual([merged.id]);
    expect(chosen(ids, new Set())).toEqual(ids);
    const r = await runCleanup(g, [...ids, busy.id]);
    expect(r.done).toHaveLength(2);
    expect(r.failed).toEqual([{ id: busy.id, error: expect.stringContaining('in use') }]);
    const after = await g.worktrees(PROJECT);
    expect(after.worktrees.map((w) => w.id)).not.toContain(merged.id);
    expect(after.orphans).toHaveLength(l.orphans.length);
    expect(events.filter((e) => e.type === 'worktree.removed').map((e) => e.data.id)).toEqual(ids);
    const refs = (await g.graph(PROJECT, { limit: 1 })).refs.map((x) => x.name);
    expect(refs).not.toContain('sb/claude/1021-1412');
  });

  it('creates worktrees with sb/ and task/ names and suffixes taken names', async () => {
    const { g, events } = setup();
    const p = await g.graph(PROJECT, { limit: 10 });
    const a = await g.createWorktree({ project: PROJECT, base: p.commits[4].sha });
    expect(a.branch).toBe(`sb/${p.commits[4].sha.slice(0, 7)}`);
    const b = await g.createWorktree({ project: PROJECT, agent: 'codex', slug: 'fix-login' });
    const c = await g.createWorktree({ project: PROJECT, agent: 'codex', slug: 'fix-login' });
    expect([b.branch, c.branch]).toEqual(['sb/codex/fix-login', 'sb/codex/fix-login-2']);
    expect(c.path.endsWith('/sb-codex-fix-login-2')).toBe(true);
    expect(events.filter((e) => e.type === 'worktree.created')).toHaveLength(3);
  });

  it('merges the base and cherry-picks, refusing dirty worktrees', async () => {
    const { g } = setup();
    const l = await g.worktrees(PROJECT);
    const dirty = l.worktrees.find((w) => w.dirty)!;
    const clean = l.worktrees.find((w) => w.kind === 'managed' && !w.dirty && w.state !== 'missing' && (w.behind ?? 0) > 0)!;
    await expect(g.mergeBase(dirty.id)).rejects.toMatchObject({ status: 409 });
    expect((await g.mergeBase(clean.id)).behind).toBe(0);
    const sha = (await g.graph(PROJECT, { limit: 3 })).commits[2].sha;
    const r = await g.cherryPick(clean.id, sha);
    expect(r.commit).toMatch(/^[0-9a-f]{40}$/);
    await expect(g.cherryPick(dirty.id, sha)).rejects.toMatchObject({ status: 409 });
  });

  it('starts sessions in a new worktree and tasks in place on request', async () => {
    const { m, a } = setup();
    const term = await a.createTerm({ agent: 'claude', cwd: PROJECT, worktree: true });
    expect(term.cwd).toMatch(/\.local\/state\/overseer\/worktrees\/overseer-[0-9a-f]{8}\/sb-claude-\d{4}-\d{4}$/);
    const t = await a.createTask({ title: 'Fix login page', description: '', project: PROJECT, agent: 'claude', model: '' });
    const started = await a.startTask(t.id);
    expect(started.branch).toBe(`task/${t.number}-fix-login-page`);
    expect(m.terms.find((x) => x.id === started.termId)?.cwd).toBe(started.worktreePath);
    const t2 = await a.createTask({ title: 'In place', description: '', project: PROJECT, agent: 'claude', model: '' });
    const inPlace = await a.startTask(t2.id, { inPlace: true });
    expect(inPlace.worktreePath ?? null).toBeNull();
    expect(m.terms.find((x) => x.id === inPlace.termId)?.cwd).toBe(PROJECT);
  });
});
