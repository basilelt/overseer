import { describe, expect, it } from 'vitest';
import { parseReviewKey, reviewKey } from './keys';
import { tabInfo, TAB_KINDS } from '../tabs/tabs';

describe('review tab keys', () => {
  it('round-trips every target and reads legacy review:<lineId> keys', () => {
    for (const t of [{ worktree: '/w/a:b' }, { lineId: 'vm-01J' }, { taskId: '01JB' }]) expect(parseReviewKey(reviewKey(t))).toEqual(t);
    expect(parseReviewKey('review:ln_03')).toEqual({ lineId: 'ln_03' });
    expect(parseReviewKey('review:')).toBeNull();
    expect(parseReviewKey('review:wt:')).toBeNull();
    expect(parseReviewKey('line:x')).toBeNull();
  });

  it('is a registered tab kind that loads the review tab', () => {
    expect(tabInfo(reviewKey({ taskId: 't1' }), [])).toMatchObject({ kind: 'review', arg: 'task:t1' });
    expect(TAB_KINDS.review?.load).toBeTypeOf('function');
  });
});
