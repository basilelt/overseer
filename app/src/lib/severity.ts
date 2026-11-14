export type Level = 'ok' | 'warn' | 'crit';

export const WARN_PCT = 80;
export const CRIT_PCT = 95;

export function level(pct: number | null | undefined): Level {
  if (pct == null || !Number.isFinite(pct)) return 'ok';
  if (pct >= CRIT_PCT) return 'crit';
  if (pct >= WARN_PCT) return 'warn';
  return 'ok';
}

export function worst(levels: Level[]): Level {
  return levels.includes('crit') ? 'crit' : levels.includes('warn') ? 'warn' : 'ok';
}
