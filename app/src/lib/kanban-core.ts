import type { Task, TaskStatus } from './types';

export const isSession = (t: Task): boolean => t.origin === 'session';

export const isDone = (status: TaskStatus): boolean => status === 'merged' || status === 'completed';

export function taskForItem(tasks: Task[], item: { lineId?: string | null; termId?: string | null } | undefined): Task | undefined {
  if (!item) return undefined;
  return tasks.find((t) => (item.lineId && t.lineId === item.lineId) || (item.termId && t.termId === item.termId));
}

export function movableStatuses(task: Task): TaskStatus[] {
  return isSession(task) ? ['in_progress', 'in_testing', 'merged', 'completed'] : ['in_testing', 'merged', 'completed'];
}

export const rankOf = (t: Task): number => (Number.isFinite(t.rank) ? t.rank : t.number);

export function sortColumn(tasks: Task[]): Task[] {
  return tasks.slice().sort((a, b) => rankOf(a) - rankOf(b) || a.number - b.number);
}

export const byId = (tasks: Task[]): Map<string, Task> => new Map(tasks.map((t) => [t.id, t]));

export function blockers(task: Task, tasks: Map<string, Task>): Task[] {
  if (!task.dependsOn?.length) return [];
  const out: Task[] = [];
  for (const id of task.dependsOn) {
    const d = tasks.get(id);
    if (d && !isDone(d.status)) out.push(d);
  }
  return out;
}

export function startable(tasks: Task[]): Task[] {
  const ids = byId(tasks);
  return sortColumn(tasks.filter((t) => t.status === 'auto' && !isSession(t) && !blockers(t, ids).length));
}
