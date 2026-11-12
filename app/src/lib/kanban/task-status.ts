import type { TaskStatus } from '../types';

interface StatusKind {
  session: boolean;
  finished: boolean;
  done: boolean;
}

const KINDS: Record<TaskStatus, StatusKind> = {
  pending: { session: false, finished: false, done: false },
  auto: { session: false, finished: false, done: false },
  in_progress: { session: true, finished: false, done: false },
  in_testing: { session: true, finished: true, done: false },
  merged: { session: true, finished: true, done: true },
  completed: { session: true, finished: true, done: true },
};

const only = (key: keyof StatusKind): readonly TaskStatus[] => TASK_STATUSES.filter((s) => KINDS[s][key]);

export const TASK_STATUSES = Object.keys(KINDS) as readonly TaskStatus[];
export const SESSION_STATUSES = only('session');
export const FINISHED_STATUSES = only('finished');
export const DONE_STATUSES = only('done');

export const isDone = (status: TaskStatus): boolean => KINDS[status].done;
export const isFinished = (status: TaskStatus): boolean => KINDS[status].finished;
export const isPending = (status: TaskStatus): boolean => status === 'pending';
export const isQueued = (status: TaskStatus): boolean => status === 'auto';
export const isUnstarted = (status: TaskStatus): boolean => isPending(status) || isQueued(status);
export const isRunning = (status: TaskStatus): boolean => status === 'in_progress';
export const isTesting = (status: TaskStatus): boolean => status === 'in_testing';
