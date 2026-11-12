import { describe, expect, it } from 'vitest';
import { DONE_STATUSES, FINISHED_STATUSES, SESSION_STATUSES, TASK_STATUSES, isDone, isFinished, isRunning, isTesting } from './task-status';

describe('task status vocabulary', () => {
  it('lists every column in board order', () => {
    expect(TASK_STATUSES).toEqual(['pending', 'auto', 'in_progress', 'in_testing', 'merged', 'completed']);
  });

  it('derives every named subset from the full list', () => {
    expect(SESSION_STATUSES).toEqual(['in_progress', 'in_testing', 'merged', 'completed']);
    expect(FINISHED_STATUSES).toEqual(['in_testing', 'merged', 'completed']);
    expect(DONE_STATUSES).toEqual(['merged', 'completed']);
    for (const s of [...SESSION_STATUSES, ...FINISHED_STATUSES, ...DONE_STATUSES]) expect(TASK_STATUSES).toContain(s);
  });

  it('answers the status predicates for each status', () => {
    expect(TASK_STATUSES.filter(isDone)).toEqual(['merged', 'completed']);
    expect(TASK_STATUSES.filter(isFinished)).toEqual(['in_testing', 'merged', 'completed']);
    expect(TASK_STATUSES.filter(isRunning)).toEqual(['in_progress']);
    expect(TASK_STATUSES.filter(isTesting)).toEqual(['in_testing']);
  });
});
