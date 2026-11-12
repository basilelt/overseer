import type { TaskStatus } from '../types';

export const TASK_STATUSES: readonly TaskStatus[] = ['pending', 'auto', 'in_progress', 'in_testing', 'merged', 'completed'];
export const SESSION_STATUSES: readonly TaskStatus[] = ['in_progress', 'in_testing', 'merged', 'completed'];
export const FINISHED_STATUSES: readonly TaskStatus[] = ['in_testing', 'merged', 'completed'];
export const DONE_STATUSES: readonly TaskStatus[] = ['merged', 'completed'];

export const isDone = (status: TaskStatus): boolean => DONE_STATUSES.includes(status);
export const isFinished = (status: TaskStatus): boolean => FINISHED_STATUSES.includes(status);
export const isRunning = (status: TaskStatus): boolean => status === 'in_progress';
export const isTesting = (status: TaskStatus): boolean => status === 'in_testing';
