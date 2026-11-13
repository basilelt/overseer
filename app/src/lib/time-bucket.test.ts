import { describe, expect, it } from 'vitest';
import { bucketByTime, TIME_BUCKETS, timeBucket } from './time-bucket';

// Local-time stamps: the buckets are calendar days of the viewer, so the tests build them the same way.
const at = (y: number, m: number, d: number, h = 0, min = 0, s = 0) => new Date(y, m - 1, d, h, min, s).toISOString();
const now = new Date(2026, 10, 6, 14, 30).getTime();

describe('timeBucket', () => {
  it('lists the four buckets, newest first', () => {
    expect(TIME_BUCKETS).toEqual(['today', 'yesterday', 'thisWeek', 'older']);
  });

  it('is today for the same calendar day, from midnight to now', () => {
    expect(timeBucket(at(2026, 11, 6, 0, 0, 0), now)).toBe('today');
    expect(timeBucket(at(2026, 11, 6, 14, 30), now)).toBe('today');
  });

  it('is yesterday one second before midnight, and still yesterday at its first second', () => {
    expect(timeBucket(at(2026, 11, 5, 23, 59, 59), now)).toBe('yesterday');
    expect(timeBucket(at(2026, 11, 5, 0, 0, 0), now)).toBe('yesterday');
  });

  it('is thisWeek from Monday up to the day before yesterday', () => {
    expect(timeBucket(at(2026, 11, 4, 23, 59), now)).toBe('thisWeek');
    expect(timeBucket(at(2026, 11, 2, 0, 0, 0), now)).toBe('thisWeek');
  });

  it('starts the week on Monday: the Sunday before is older', () => {
    expect(timeBucket(at(2026, 11, 1, 23, 59, 59), now)).toBe('older');
  });

  it('counts a Sunday as the last day of the week, not the first', () => {
    const sunday = new Date(2026, 10, 8, 10).getTime();
    expect(timeBucket(at(2026, 11, 2), sunday)).toBe('thisWeek');
    expect(timeBucket(at(2026, 11, 1, 23, 59), sunday)).toBe('older');
  });

  it('keeps yesterday on a Monday even though it belongs to the previous week', () => {
    const monday = new Date(2026, 10, 2, 9).getTime();
    expect(timeBucket(at(2026, 11, 1, 20), monday)).toBe('yesterday');
    expect(timeBucket(at(2026, 10, 31, 20), monday)).toBe('older');
  });

  it('is older beyond the week, across months and years', () => {
    expect(timeBucket(at(2026, 10, 1), now)).toBe('older');
    expect(timeBucket(at(2025, 11, 6), now)).toBe('older');
  });

  it('crosses a year boundary: New Year morning sees 31 December as yesterday', () => {
    expect(timeBucket(at(2025, 12, 31, 18), new Date(2026, 0, 1, 8).getTime())).toBe('yesterday');
  });

  it('treats a timestamp slightly in the future (clock skew) as today', () => {
    expect(timeBucket(at(2026, 11, 6, 23, 59), now)).toBe('today');
    expect(timeBucket(at(2026, 11, 9), now)).toBe('today');
  });

  it('keeps calendar days across a daylight-saving change, where a day is 23 or 25 hours', () => {
    // Europe/Paris springs forward on 2026-03-29; the calendar logic must hold in any zone.
    const sunday = new Date(2026, 2, 29, 10).getTime();
    expect(timeBucket(at(2026, 3, 28, 0, 30), sunday)).toBe('yesterday');
    expect(timeBucket(at(2026, 3, 27, 23, 30), sunday)).toBe('thisWeek');
    expect(timeBucket(at(2026, 3, 23, 0, 30), sunday)).toBe('thisWeek');
    expect(timeBucket(at(2026, 3, 22, 23, 30), sunday)).toBe('older');
  });

  it('is older for a missing, empty or unreadable date', () => {
    expect(timeBucket(undefined, now)).toBe('older');
    expect(timeBucket(null, now)).toBe('older');
    expect(timeBucket('', now)).toBe('older');
    expect(timeBucket('not a date', now)).toBe('older');
  });
});

describe('bucketByTime', () => {
  const row = (id: string, when: string) => ({ id, at: when });

  it('splits rows into the buckets they fall in, in bucket order, keeping row order inside each', () => {
    const rows = [row('old', at(2026, 9, 1)), row('t1', at(2026, 11, 6, 9)), row('y', at(2026, 11, 5, 9)), row('t2', at(2026, 11, 6, 8)), row('w', at(2026, 11, 3))];
    expect(bucketByTime(rows, now).map((b) => [b.bucket, b.items.map((r) => r.id)])).toEqual([
      ['today', ['t1', 't2']],
      ['yesterday', ['y']],
      ['thisWeek', ['w']],
      ['older', ['old']],
    ]);
  });

  it('leaves out the empty buckets', () => {
    expect(bucketByTime([row('y', at(2026, 11, 5, 9))], now).map((b) => b.bucket)).toEqual(['yesterday']);
    expect(bucketByTime([], now)).toEqual([]);
  });
});
