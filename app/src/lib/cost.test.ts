import { describe, expect, it } from 'vitest';
import { costRows, isFinished, tokensOf } from './cost';
import type { Task, TaskCost } from './types';

const c = (cost: number | null, partial = false): TaskCost => ({ input: 1, cacheWrite: 2, cacheRead: 3, output: 4, cost, partial, sessions: 1 });

function task(id: string, over: Partial<Task>): Task {
  return {
    id,
    number: Number(id.slice(1)),
    title: `Task ${id}`,
    description: '',
    project: '/p',
    status: 'completed',
    agent: 'claude',
    model: '',
    effort: 'high',
    rank: 0,
    station: 's',
    createdAt: '2026-11-11T00:00:00Z',
    updatedAt: '2026-11-11T00:00:00Z',
    ...over,
  };
}

const name = (id: string) => `Profile ${id}`;

describe('cost rows', () => {
  it('sums tokens of a cost split', () => {
    expect(tokensOf(c(1))).toBe(10);
    expect(tokensOf(null)).toBe(0);
  });

  it('counts in_testing, merged and completed as finished', () => {
    expect(isFinished(task('t1', { status: 'merged' }))).toBe(true);
    expect(isFinished(task('t1', { status: 'in_progress' }))).toBe(false);
  });

  it('groups by profile and agent with the cost per finished task', () => {
    const rows = costRows(
      [
        task('t1', { profile: 'eco', cost: c(1) }),
        task('t2', { profile: 'eco', cost: c(3) }),
        task('t3', { profile: 'eco', cost: c(10), status: 'in_progress' }),
        task('t4', { profile: 'eco', agent: 'codex', cost: c(0.5) }),
        task('t5', { cost: c(2) }),
        task('t6', { profile: 'eco' }),
      ],
      'profile',
      name,
      'No profile',
    );
    expect(rows.map((r) => [r.label, r.agent, r.tasks, r.finished, r.cost, r.perTask])).toEqual([
      ['Profile eco', 'claude', 3, 2, 14, 2],
      ['No profile', 'claude', 1, 1, 2, 2],
      ['Profile eco', 'codex', 1, 1, 0.5, 0.5],
    ]);
  });

  it('groups by ticket and skips tasks without one', () => {
    const rows = costRows(
      [task('t1', { ticketKey: 'AB-1', cost: c(1) }), task('t2', { ticketKey: 'AB-1', cost: c(2, true) }), task('t3', { cost: c(5) })],
      'ticket',
      name,
      '-',
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({ label: 'AB-1', tasks: 2, cost: 3, partial: true, tokens: 20 });
  });

  it('lists each task with its id and keeps unpriced ones last', () => {
    const rows = costRows([task('t1', { cost: c(null, true) }), task('t2', { cost: c(1) })], 'task', name, '-');
    expect(rows.map((r) => [r.label, r.taskId, r.cost, r.perTask])).toEqual([
      ['#2 Task t2', 't2', 1, 1],
      ['#1 Task t1', 't1', null, null],
    ]);
  });

  it('caps the list at 50 rows', () => {
    const many = Array.from({ length: 60 }, (_, i) => task(`t${i}`, { cost: c(i) }));
    const rows = costRows(many, 'task', name, '-');
    expect(rows).toHaveLength(50);
    expect(rows[0].cost).toBe(59);
  });
});
