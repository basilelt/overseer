import { describe, expect, it } from 'vitest';
import { fuzzy, highlight, rank } from './fuzzy';

describe('fuzzy', () => {
  it('matches subsequences and merges adjacent ranges', () => {
    expect(fuzzy('nwcl', 'New Claude session')!.ranges).toEqual([[0, 1], [2, 3], [4, 6]]);
    expect(fuzzy('xyz', 'New Claude session')).toBeNull();
    expect(fuzzy('', 'anything')).toEqual({ score: 0, ranges: [] });
  });

  it('ranks contiguous and boundary matches first', () => {
    const r = rank('sh', ['Push branch', 'New shell', 'Show settings'], (x) => x);
    expect(r.map((x) => x.item)).toEqual(['Show settings', 'New shell', 'Push branch']);
    expect(rank('', ['b', 'a'], (x) => x).map((x) => x.item)).toEqual(['b', 'a']);
  });

  it('splits text into highlighted parts', () => {
    expect(highlight('abcd', [[1, 3]])).toEqual([
      { text: 'a', hit: false },
      { text: 'bc', hit: true },
      { text: 'd', hit: false },
    ]);
  });
});
