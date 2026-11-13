import { describe, expect, it } from 'vitest';
import { DONE_STATUSES, FINISHED_STATUSES, SESSION_STATUSES, TASK_STATUSES, isDone, isFinished, isRunning, isTesting, isUnstarted } from './task-status';

describe('task status vocabulary', () => {
  it('lists every column in board order', () => {
    expect(TASK_STATUSES).toEqual(['pending', 'auto', 'in_progress', 'in_testing', 'merged', 'completed']);
  });

  it('keeps every named subset inside the full list, in board order, and in step with its predicate', () => {
    const subsets = [
      [SESSION_STATUSES, undefined],
      [FINISHED_STATUSES, isFinished],
      [DONE_STATUSES, isDone],
    ] as const;
    for (const [subset, predicate] of subsets) {
      expect(TASK_STATUSES.filter((s) => (subset as readonly string[]).includes(s))).toEqual(subset);
      if (predicate) expect(TASK_STATUSES.filter(predicate)).toEqual(subset);
    }
  });

  it('answers the status predicates for each status', () => {
    expect(TASK_STATUSES.filter(isDone)).toEqual(['merged', 'completed']);
    expect(TASK_STATUSES.filter(isFinished)).toEqual(['in_testing', 'merged', 'completed']);
    expect(TASK_STATUSES.filter(isRunning)).toEqual(['in_progress']);
    expect(TASK_STATUSES.filter(isUnstarted)).toEqual(['pending', 'auto']);
    expect(TASK_STATUSES.filter(isTesting)).toEqual(['in_testing']);
  });
});
