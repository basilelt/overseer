import type { ReviewHunk, ReviewLine } from './types';

export function lineDiff(a: string[], b: string[], context = 3): ReviewHunk[] {
  const n = a.length;
  const m = b.length;
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
  const ops: ReviewLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) ops.push({ kind: 'ctx', old: ++i, new: ++j, text: a[i - 1] });
    else if (i < n && (j >= m || lcs[i + 1][j] >= lcs[i][j + 1])) ops.push({ kind: 'del', old: ++i, text: a[i - 1] });
    else ops.push({ kind: 'add', new: ++j, text: b[j - 1] });
  }
  const changed = ops.map((o, k) => (o.kind !== 'ctx' ? k : -1)).filter((k) => k >= 0);
  if (!changed.length) return [];
  const groups: [number, number][] = [];
  for (const k of changed) {
    const lo = Math.max(0, k - context);
    const hi = Math.min(ops.length - 1, k + context);
    const last = groups.at(-1);
    if (last && lo <= last[1] + 1) last[1] = hi;
    else groups.push([lo, hi]);
  }
  return groups.map(([lo, hi]) => {
    const lines = ops.slice(lo, hi + 1);
    const olds = lines.filter((l) => l.kind !== 'add');
    const news = lines.filter((l) => l.kind !== 'del');
    const before = ops.slice(0, lo);
    const oldStart = olds.length ? olds[0].old! : before.filter((l) => l.kind !== 'add').length;
    const newStart = news.length ? news[0].new! : before.filter((l) => l.kind !== 'del').length;
    return {
      header: `@@ -${oldStart},${olds.length} +${newStart},${news.length} @@`,
      oldStart,
      oldLines: olds.length,
      newStart,
      newLines: news.length,
      lines,
    };
  });
}
