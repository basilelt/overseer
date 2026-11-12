export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

export function clampFinite(v: number, lo: number, hi: number): number {
  return Number.isFinite(v) ? clamp(v, lo, hi) : lo;
}
