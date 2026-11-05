import type { ReviewFile, ReviewHunk, Tag, TagKind, Triage } from './types';

export const KINDS: TagKind[] = ['logic', 'test', 'move', 'format', 'imports', 'ident', 'rename', 'generated', 'comment', 'unknown'];
export const LOW = new Set<TagKind>(['move', 'format', 'imports', 'ident', 'rename', 'generated', 'comment']);

export const TAG_COLOR: Record<TagKind, string> = {
  logic: 'var(--st-failed)',
  test: 'var(--lane-2)',
  move: 'var(--lane-1)',
  format: 'var(--lane-0)',
  imports: 'var(--lane-4)',
  ident: 'var(--lane-3)',
  rename: 'var(--diff-hunk)',
  generated: 'var(--text-3)',
  comment: 'var(--lane-5)',
  unknown: 'var(--text-3)',
};

export interface Fold {
  kind: TagKind | 'mixed';
  ref?: { path: string; hunk: number };
  count: number;
}

export function foldOf(h: ReviewHunk): Fold | null {
  const tags = h.tags ?? [];
  if (!tags.length || !tags.every((t) => LOW.has(t.kind))) return null;
  const kinds = new Set(tags.map((t) => t.kind));
  const kind = kinds.size === 1 ? tags[0].kind : 'mixed';
  const count = tags.reduce((n, t) => n + t.to - t.from, 0);
  return { kind, ref: kind === 'move' ? tags.find((t) => t.ref)?.ref : undefined, count };
}

export function lineTags(h: ReviewHunk): (Tag | undefined)[] {
  const out: (Tag | undefined)[] = new Array(h.lines.length);
  for (const t of h.tags ?? []) for (let i = t.from; i < t.to && i < out.length; i++) out[i] = t;
  return out;
}

export function lowRiskKinds(t: Triage | undefined): TagKind[] {
  if (!t) return [];
  return KINDS.filter((k) => LOW.has(k) && (t.byKind[k] ?? 0) > 0).sort((a, b) => (t.byKind[b] ?? 0) - (t.byKind[a] ?? 0));
}

export function byRisk(files: ReviewFile[]): ReviewFile[] {
  return [...files].sort((a, b) => (b.risk ?? 0) - (a.risk ?? 0) || (b.residual ?? 0) - (a.residual ?? 0) || a.path.localeCompare(b.path));
}
