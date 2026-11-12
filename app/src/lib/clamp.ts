export function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

export function clampFinite(v: number, lo: number, hi: number): number {
  return Number.isFinite(v) ? clamp(v, lo, hi) : lo;
}

export function commitNumber(input: { valueAsNumber: number; value: string }, normalize: (v: number) => number, save: (v: number) => void) {
  const v = normalize(input.valueAsNumber);
  input.value = String(v);
  save(v);
}
