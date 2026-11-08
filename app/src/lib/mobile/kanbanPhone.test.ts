import { describe, expect, it } from 'vitest';
import type { Task, TaskStatus } from '../types';
import { edgeMoves, edgeRank, initialColumn, moveChoices } from './kanbanPhone';

const task = (id: string, rank: number, status: TaskStatus = 'pending'): Task => ({ id, number: rank, rank, status }) as unknown as Task;

describe('kanban on a phone', () => {
  it('offers every other column as a move target', () => {
    const c = moveChoices('auto');
    expect(c).not.toContain('auto');
    expect(c).toContain('pending');
    expect(c).toContain('completed');
    expect(c).toContain('merged');
    expect(c.length).toBe(5);
  });

  it('offers only the session columns for a session task', () => {
    expect(moveChoices('in_progress', true)).toEqual(['in_testing', 'merged', 'completed']);
    expect(moveChoices('completed', true)).toEqual(['in_progress', 'in_testing', 'merged']);
  });

  it('ranks a card at the top or bottom of its column', () => {
    const col = [task('a', 1), task('b', 2), task('c', 3)];
    expect(edgeRank(col, col[1], 'top')).toBe(0);
    expect(edgeRank(col, col[1], 'bottom')).toBe(4);
    expect(edgeMoves(col, col[0])).toEqual(['bottom']);
    expect(edgeMoves(col, col[2])).toEqual(['top']);
    expect(edgeMoves(col, col[1])).toEqual(['top', 'bottom']);
    expect(edgeMoves([col[0]], col[0])).toEqual([]);
  });

  it('ranks a card moved from another column at the end', () => {
    const col = [task('a', 1), task('b', 5)];
    expect(edgeRank(col, task('x', 2, 'auto'), 'bottom')).toBe(6);
    expect(edgeRank(col, task('x', 2, 'auto'), 'top')).toBe(0);
  });

  it('opens on the running column when it has work', () => {
    const s: TaskStatus[] = ['pending', 'auto', 'in_progress', 'in_testing', 'completed'];
    expect(initialColumn(s, [3, 0, 2, 0, 0])).toBe(2);
    expect(initialColumn(s, [0, 1, 0, 0, 4])).toBe(1);
    expect(initialColumn(s, [0, 0, 0, 0, 0])).toBe(0);
  });
});
