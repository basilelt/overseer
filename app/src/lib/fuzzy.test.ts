import { describe, expect, it } from 'vitest';
import { bestSectionFirst, fuzzy, highlight, rank } from './fuzzy';

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

  it('puts the section of the best match first and keeps each section together', () => {
    const row = (section: string, score: number) => ({ item: { section }, match: { score, ranges: [] } });
    const rows = [row('views', 60), row('views', 20), row('actions', 1200), row('sessions', 40), row('actions', 30)];
    const out = bestSectionFirst(rows, 'tour').map((r) => `${r.item.section}:${r.match.score}`);
    expect(out).toEqual(['actions:1200', 'actions:30', 'views:60', 'views:20', 'sessions:40']);
  });

  it('keeps the section order when nothing is typed', () => {
    const row = (section: string) => ({ item: { section }, match: { score: 0, ranges: [] } });
    const rows = [row('views'), row('actions'), row('views')];
    expect(bestSectionFirst(rows, ' ')).toBe(rows);
  });

  it('splits text into highlighted parts', () => {
    expect(highlight('abcd', [[1, 3]])).toEqual([
      { text: 'a', hit: false },
      { text: 'bc', hit: true },
      { text: 'd', hit: false },
    ]);
  });
});
