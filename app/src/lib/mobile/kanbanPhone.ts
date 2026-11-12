import { rankAt } from '../kanban/kanban';
import { SESSION_STATUSES, TASK_STATUSES } from '../kanban/task-status';
import type { Task, TaskStatus } from '../types';

export type Edge = 'top' | 'bottom';

export function moveChoices(current: TaskStatus, session = false): TaskStatus[] {
  return (session ? SESSION_STATUSES : TASK_STATUSES).filter((s) => s !== current);
}

export function edgeRank(column: Task[], moving: Task, where: Edge): number {
  return rankAt(column, where === 'top' ? 0 : column.length, moving);
}

export function edgeMoves(column: Task[], moving: Task): Edge[] {
  const others = column.filter((t) => t.id !== moving.id);
  if (!others.length) return [];
  return (['top', 'bottom'] as Edge[]).filter((w) => edgeRank(column, moving, w) !== moving.rank);
}

export function initialColumn(statuses: TaskStatus[], counts: number[]): number {
  const i = statuses.indexOf('in_progress');
  if (i >= 0 && counts[i] > 0) return i;
  const first = counts.findIndex((n) => n > 0);
  return first >= 0 ? first : 0;
}
