export const KEEP_TERMS = 4;
export const KEEP_TERMS_PHONE = 2;

export function retain(kept: readonly string[], active: string | null, keep: (id: string) => boolean, max = KEEP_TERMS): string[] {
  const rest = kept.filter((id) => id !== active && keep(id));
  return (active ? [active, ...rest] : rest).slice(0, Math.max(1, max));
}

export interface Claim {
  active: string | null;
  index: number;
}

export function claim(owners: Map<string, number>, prev: Claim | null, next: Claim, live: ReadonlySet<string>): void {
  if (next.active && (next.active !== prev?.active || next.index !== prev?.index)) owners.set(next.active, next.index);
  for (const id of [...owners.keys()]) if (id !== next.active && !live.has(id)) owners.delete(id);
}

export function same(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}
