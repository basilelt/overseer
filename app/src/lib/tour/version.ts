export type Mode = 'full' | 'whatsNew';

const LEGACY: Record<number, string> = { 1: '0.15', 2: '0.17' };

export function compareVersions(a: string, b: string): number {
  const x = a.split('.').map(Number);
  const y = b.split('.').map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

export const TOUR_VERSION = '0.39';

export function seenVersion(seen: number | string | undefined): string | undefined {
  if (typeof seen === 'string') return seen;
  return seen ? (LEGACY[seen] ?? LEGACY[2]) : undefined;
}

export function modeFor(seen: number | string | undefined, version = TOUR_VERSION): Mode | null {
  const s = seenVersion(seen);
  if (!s) return 'full';
  return compareVersions(s, version) >= 0 ? null : 'whatsNew';
}
