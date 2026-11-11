import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Task } from '../../types';

function task(id: string, over: Partial<Task>): Task {
  return { id, number: 1, title: id, description: '', project: '/p', status: 'completed', agent: 'claude', model: '', effort: 'high', rank: 0, station: 's', createdAt: '', updatedAt: '', ...over };
}

const cost = (c: number) => ({ input: 1, cacheWrite: 0, cacheRead: 0, output: 1, cost: c, partial: false, sessions: 1 });

async function rendered(tasks: Task[]) {
  vi.resetModules();
  const { app } = await import('../../state.svelte');
  app.data = { ...app.data, tasks };
  app.profiles = { profiles: [{ id: 'eco', name: 'Economy', builtin: true, agents: {} }], scopes: {}, default: '' } as never;
  const { render } = await import('svelte/server');
  const { default: TaskCosts } = await import('./TaskCosts.svelte');
  return render(TaskCosts, { props: {} }).body.replace(/<!--[\s\S]*?-->/g, '');
}

afterEach(() => vi.restoreAllMocks());

describe('TaskCosts', () => {
  it('groups tasks by profile with the cost per finished task', async () => {
    const h = await rendered([task('a', { profile: 'eco', cost: cost(2) }), task('b', { profile: 'eco', cost: cost(4) }), task('c', { cost: cost(1), status: 'in_progress' })]);
    expect(h).toContain('Economy');
    expect(h).toContain('2 tasks · 2 finished');
    expect(h).toContain('avg ≈$3.00');
    expect(h).toContain('title="≈$3.00 per finished task"');
    expect(h).toContain('≈$6.00');
    expect(h).toContain('No profile');
  });

  it('says "1 task" for a single task', async () => {
    const h = await rendered([task('a', { profile: 'eco', cost: cost(2), status: 'in_progress' })]);
    expect(h).toContain('1 task · 0 finished');
    expect(h).not.toContain('1 tasks');
  });

  it('says when no task has a measured cost', async () => {
    expect(await rendered([task('a', {})])).toContain('No task has a measured cost yet.');
  });
});
