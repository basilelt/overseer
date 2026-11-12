export const MIN_MS = 60_000;
export const HOUR_MIN = 60;
export const DAY_HOURS = 24;

export type Coarse = { unit: 'now' | 'm' | 'h' | 'd'; n: number };

export function coarseMinutes(m: number): Coarse {
  if (m < 1) return { unit: 'now', n: 0 };
  if (m < HOUR_MIN) return { unit: 'm', n: m };
  const h = Math.floor(m / HOUR_MIN);
  if (h < DAY_HOURS) return { unit: 'h', n: h };
  return { unit: 'd', n: Math.floor(h / DAY_HOURS) };
}

export function wholeMinutes(ms: number): number {
  return Math.floor(ms / MIN_MS);
}

export function splitMinutes(m: number): { d: number; h: number; m: number } {
  const h = Math.floor(m / HOUR_MIN);
  return { d: Math.floor(h / DAY_HOURS), h: h % DAY_HOURS, m: m % HOUR_MIN };
}
