export type TimeBucket = 'today' | 'yesterday' | 'thisWeek' | 'older';

export const TIME_BUCKETS: readonly TimeBucket[] = ['today', 'yesterday', 'thisWeek', 'older'];

// Calendar days of the viewer's timezone; the week starts on Monday, as in the EU locales Overseer ships.
// An unreadable or missing date is older, a timestamp ahead of `now` (clock skew) is today.
export function timeBucket(at: string | null | undefined, now: number): TimeBucket {
  const t = at ? Date.parse(at) : NaN;
  if (Number.isNaN(t)) return 'older';
  const d = new Date(now);
  const [y, m, day] = [d.getFullYear(), d.getMonth(), d.getDate()];
  if (t >= new Date(y, m, day).getTime()) return 'today';
  if (t >= new Date(y, m, day - 1).getTime()) return 'yesterday';
  return t >= new Date(y, m, day - ((d.getDay() + 6) % 7)).getTime() ? 'thisWeek' : 'older';
}

// Non-empty buckets in display order; rows keep their relative order inside a bucket.
export function bucketByTime<T extends { at: string }>(rows: T[], now: number): { bucket: TimeBucket; items: T[] }[] {
  const by = new Map<TimeBucket, T[]>();
  for (const r of rows) {
    const b = timeBucket(r.at, now);
    const list = by.get(b);
    if (list) list.push(r);
    else by.set(b, [r]);
  }
  return TIME_BUCKETS.filter((b) => by.has(b)).map((bucket) => ({ bucket, items: by.get(bucket)! }));
}
