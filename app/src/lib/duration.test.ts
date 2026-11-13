import { describe, expect, it } from 'vitest';
import { coarseMinutes, splitMinutes, wholeMinutes } from './duration';

describe('duration thresholds', () => {
  it('coarsens minutes into now, minutes, hours and days', () => {
    expect(coarseMinutes(0)).toEqual({ unit: 'now', n: 0 });
    expect(coarseMinutes(1)).toEqual({ unit: 'm', n: 1 });
    expect(coarseMinutes(59)).toEqual({ unit: 'm', n: 59 });
    expect(coarseMinutes(60)).toEqual({ unit: 'h', n: 1 });
    expect(coarseMinutes(24 * 60 - 1)).toEqual({ unit: 'h', n: 23 });
    expect(coarseMinutes(24 * 60)).toEqual({ unit: 'd', n: 1 });
    expect(coarseMinutes(3 * 24 * 60 + 5)).toEqual({ unit: 'd', n: 3 });
  });

  it('treats a future instant as now and keeps NaN in the day bucket', () => {
    expect(coarseMinutes(-3).unit).toBe('now');
    expect(coarseMinutes(NaN).unit).toBe('d');
  });

  it('floors milliseconds to whole minutes', () => {
    expect(wholeMinutes(59_999)).toBe(0);
    expect(wholeMinutes(120_000)).toBe(2);
  });

  it('splits minutes into days, hours and minutes', () => {
    expect(splitMinutes(59)).toEqual({ d: 0, h: 0, m: 59 });
    expect(splitMinutes(61)).toEqual({ d: 0, h: 1, m: 1 });
    expect(splitMinutes(26 * 60 + 7)).toEqual({ d: 1, h: 2, m: 7 });
  });
});
