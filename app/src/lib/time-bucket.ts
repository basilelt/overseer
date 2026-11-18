export type TimeBucket = 'today' | 'yesterday' | 'thisWeek' | 'older';

export const TIME_BUCKETS: readonly TimeBucket[] = ['today', 'yesterday', 'thisWeek', 'older'];

interface DayBounds {
  today: number;
  yesterday: number;
  week: number;
}

function dayBounds(now: number): DayBounds {
  const d = new Date(now);
  const [y, m, day] = [d.getFullYear(), d.getMonth(), d.getDate()];
  return { today: new Date(y, m, day).getTime(), yesterday: new Date(y, m, day - 1).getTime(), week: new Date(y, m, day - ((d.getDay() + 6) % 7)).getTime() };
}

function bucketOf(at: string | null | undefined, b: DayBounds): TimeBucket {
  const t = at ? Date.parse(at) : NaN;
  if (Number.isNaN(t)) return 'older';
  if (t >= b.today) return 'today';
  if (t >= b.yesterday) return 'yesterday';
  return t >= b.week ? 'thisWeek' : 'older';
}

// Calendar days of the viewer's timezone; the week starts on Monday, as in the EU locales Overseer ships.
// An unreadable or missing date is older, a timestamp ahead of `now` (clock skew) is today.
export function timeBucket(at: string | null | undefined, now: number): TimeBucket {
  return bucketOf(at, dayBounds(now));
}

export function bucketByTime<T extends { at: string }>(rows: T[], now: number): { bucket: TimeBucket; items: T[] }[] {
  const bounds = dayBounds(now);
  const by = new Map<TimeBucket, T[]>();
  for (const r of rows) {
    const b = bucketOf(r.at, bounds);
    const list = by.get(b);
    if (list) list.push(r);
    else by.set(b, [r]);
  }
  return TIME_BUCKETS.filter((b) => by.has(b)).map((bucket) => ({ bucket, items: by.get(bucket)! }));
}
