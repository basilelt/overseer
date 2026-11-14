import { afterEach, describe, expect, it } from 'vitest';
import { MEMORY_MAX, forgetAll, recall, remember, type ReviewMemory } from './memory';

const memo = (over: Partial<ReviewMemory> = {}): ReviewMemory => ({
  view: 'map',
  selected: 'src/a.ts',
  hot: '0\nsrc/a.ts',
  ind: 'reach',
  all: true,
  sortRisk: true,
  scrollTop: 420,
  folded: { '/w/api': true },
  collapsed: { 'src/b.ts': true },
  ...over,
});

afterEach(() => forgetAll());

describe('review memory', () => {
  it('gives back what a review showed when its tab comes back', () => {
    remember('wt:/w/web', memo());
    expect(recall('wt:/w/web')).toEqual(memo());
  });

  it('keeps each review apart and knows nothing of a review never left', () => {
    remember('wt:/w/web', memo());
    remember('wt:/w/api', memo({ view: 'list', hot: null }));
    expect(recall('wt:/w/api')?.view).toBe('list');
    expect(recall('wt:/w/web')?.view).toBe('map');
    expect(recall('wt:/w/new')).toBeUndefined();
  });

  it('replaces the older state of the same review', () => {
    remember('wt:/w/web', memo());
    remember('wt:/w/web', memo({ selected: null, scrollTop: 0 }));
    expect(recall('wt:/w/web')).toMatchObject({ selected: null, scrollTop: 0 });
  });

  it('forgets the least recently left reviews past its cap', () => {
    for (let i = 0; i <= MEMORY_MAX; i++) remember(`k${i}`, memo());
    expect(recall('k0')).toBeUndefined();
    expect(recall('k1')).toBeDefined();
    remember('k1', memo());
    remember('extra', memo());
    expect(recall('k1')).toBeDefined();
    expect(recall('k2')).toBeUndefined();
  });
});
