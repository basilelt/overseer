import { foldOf, lineTags, type Fold } from './triage';
import type { LineKind, ReviewFile, ReviewHunk, Side, TagKind, Thread } from './types';

export type Mode = 'unified' | 'split';

export interface Cell {
  kind: LineKind;
  no: number;
  text: string;
  tag?: TagKind;
  sig?: boolean;
}

export interface GapRow {
  type: 'gap';
  id: string;
  header: string;
  oldStart: number;
  newStart: number;
  count: number | null;
}

export interface LineRow {
  type: 'line';
  kind: LineKind;
  old?: number;
  new?: number;
  text: string;
  tag?: TagKind;
  sig?: boolean;
}

export interface FoldRow extends Fold {
  type: 'fold';
  id: string;
  hunk: number;
}

export interface PairRow {
  type: 'pair';
  left?: Cell;
  right?: Cell;
}

export type Row = GapRow | LineRow | PairRow | FoldRow;

export interface Gap {
  id: string;
  header: string;
  oldStart: number;
  newStart: number;
  count: number | null;
}

const hunkEnd = (h: ReviewHunk) => ({ old: h.oldStart + h.oldLines, new: h.newStart + h.newLines });

export function gaps(file: ReviewFile, totalNew?: number): Gap[] {
  const out: Gap[] = [];
  const growable = file.status !== 'added' && file.status !== 'untracked' && file.status !== 'deleted';
  let prev = { old: 1, new: 1 };
  file.hunks.forEach((h, i) => {
    const newStart = h.newLines === 0 ? h.newStart + 1 : h.newStart;
    const count = growable ? Math.max(0, newStart - prev.new) : 0;
    out.push({ id: `g${i}`, header: h.header, oldStart: prev.old, newStart: prev.new, count });
    const end = hunkEnd(h);
    prev = { old: h.oldLines === 0 ? h.oldStart + 1 : end.old, new: h.newLines === 0 ? h.newStart + 1 : end.new };
  });
  if (growable && file.hunks.length) {
    const count = totalNew === undefined ? null : Math.max(0, totalNew - prev.new + 1);
    if (count !== 0) out.push({ id: 'end', header: '', oldStart: prev.old, newStart: prev.new, count });
  }
  return out;
}

function expandedLines(g: Gap, fileLines: string[] | undefined): LineRow[] {
  if (!fileLines || g.count === 0) return [];
  const count = g.count ?? fileLines.length - g.newStart + 1;
  const out: LineRow[] = [];
  for (let i = 0; i < count; i++) {
    const n = g.newStart + i;
    if (n < 1 || n > fileLines.length) continue;
    out.push({ type: 'line', kind: 'ctx', old: g.oldStart + i, new: n, text: fileLines[n - 1] });
  }
  return out;
}

export function unifiedRows(file: ReviewFile, expanded: Set<string> = new Set(), fileLines?: string[], unfolded: Set<number> = new Set()): Row[] {
  const rows: Row[] = [];
  const gs = gaps(file, fileLines?.length);
  file.hunks.forEach((h, i) => {
    const g = gs[i];
    if (expanded.has(g.id) && fileLines) {
      rows.push(...expandedLines(g, fileLines));
      if (h.header) rows.push({ type: 'gap', id: `${g.id}h`, header: h.header, oldStart: h.oldStart, newStart: h.newStart, count: 0 });
    } else rows.push({ type: 'gap', ...g });
    const fold = unfolded.has(i) ? null : foldOf(h);
    if (fold) {
      rows.push({ type: 'fold', id: `f${i}`, hunk: i, ...fold });
      return;
    }
    const tags = lineTags(h);
    h.lines.forEach((l, j) => {
      const r: LineRow = { type: 'line', kind: l.kind, old: l.old, new: l.new, text: l.text };
      if (tags[j]) r.tag = tags[j]!.kind;
      if (tags[j]?.sig) r.sig = true;
      rows.push(r);
    });
  });
  const end = gs.find((g) => g.id === 'end');
  if (end) {
    if (expanded.has('end') && fileLines) rows.push(...expandedLines(end, fileLines));
    else rows.push({ type: 'gap', ...end });
  }
  return rows;
}

export function splitRows(file: ReviewFile, expanded: Set<string> = new Set(), fileLines?: string[], unfolded?: Set<number>): Row[] {
  const out: Row[] = [];
  let dels: Cell[] = [];
  let adds: Cell[] = [];
  const flush = () => {
    const n = Math.max(dels.length, adds.length);
    for (let i = 0; i < n; i++) out.push({ type: 'pair', left: dels[i], right: adds[i] });
    dels = [];
    adds = [];
  };
  const cell = (r: LineRow, kind: LineKind, no: number): Cell => {
    const c: Cell = { kind, no, text: r.text };
    if (r.tag) c.tag = r.tag;
    if (r.sig) c.sig = true;
    return c;
  };
  for (const r of unifiedRows(file, expanded, fileLines, unfolded)) {
    if (r.type === 'gap' || r.type === 'fold') {
      flush();
      out.push(r);
    } else if (r.type === 'line' && r.kind === 'del') dels.push(cell(r, 'del', r.old!));
    else if (r.type === 'line' && r.kind === 'add') adds.push(cell(r, 'add', r.new!));
    else if (r.type === 'line') {
      flush();
      out.push({ type: 'pair', left: { kind: 'ctx', no: r.old!, text: r.text }, right: { kind: 'ctx', no: r.new!, text: r.text } });
    }
  }
  flush();
  return out;
}

export function buildRows(file: ReviewFile, mode: Mode, expanded?: Set<string>, fileLines?: string[], unfolded?: Set<number>): Row[] {
  return mode === 'split' ? splitRows(file, expanded, fileLines, unfolded) : unifiedRows(file, expanded, fileLines, unfolded);
}

export function hunkAt(file: ReviewFile, side: Side, line: number): number {
  return file.hunks.findIndex((h) =>
    h.lines.some((l) => (side === 'new' ? l.kind !== 'del' && l.new === line : l.kind !== 'add' && l.old === line)),
  );
}

export function openHunks(file: ReviewFile, unfolded: number[] | undefined, anchors: { side: Side; line: number }[]): Set<number> {
  const out = new Set(unfolded ?? []);
  if (!file.hunks.some((h) => h.tags?.length)) return out;
  for (const a of anchors) {
    const i = hunkAt(file, a.side, a.line);
    if (i >= 0) out.add(i);
  }
  return out;
}

export const anchorKey = (side: Side, line: number) => `${side}:${line}`;

export function rowAnchors(r: Row): string[] {
  if (r.type === 'line') {
    const out: string[] = [];
    if (r.new !== undefined) out.push(anchorKey('new', r.new));
    if (r.kind === 'del' && r.old !== undefined) out.push(anchorKey('old', r.old));
    return out;
  }
  if (r.type === 'pair') {
    const out: string[] = [];
    if (r.left?.kind === 'del') out.push(anchorKey('old', r.left.no));
    if (r.right) out.push(anchorKey('new', r.right.no));
    return out;
  }
  return [];
}

export interface Placement {
  byRow: Map<number, Thread[]>;
  top: Thread[];
}

export function placeThreads(rows: Row[], threads: Thread[]): Placement {
  const index = new Map<string, number>();
  rows.forEach((r, i) => {
    for (const k of rowAnchors(r)) if (!index.has(k)) index.set(k, i);
  });
  const byRow = new Map<number, Thread[]>();
  const top: Thread[] = [];
  const sorted = [...threads].sort((a, b) => a.endLine - b.endLine || a.createdAt.localeCompare(b.createdAt));
  for (const t of sorted) {
    const i = t.outdated ? undefined : index.get(anchorKey(t.side, t.endLine));
    if (i === undefined) top.push(t);
    else byRow.set(i, [...(byRow.get(i) ?? []), t]);
  }
  return { byRow, top };
}

export function rowSelectable(r: Row): { side: Side; line: number } | null {
  if (r.type === 'line') {
    if (r.kind === 'del') return { side: 'old', line: r.old! };
    return r.new !== undefined ? { side: 'new', line: r.new } : null;
  }
  return null;
}

export function inRange(side: Side, line: number, sel: { side: Side; start: number; end: number } | null): boolean {
  return !!sel && sel.side === side && line >= sel.start && line <= sel.end;
}

export function linesFor(file: ReviewFile, side: Side, start: number, end: number, fileLines?: string[]): string[] {
  const out: string[] = [];
  for (let n = start; n <= end; n++) {
    let text: string | undefined;
    for (const h of file.hunks) {
      const l = h.lines.find((x) => (side === 'new' ? x.new === n && x.kind !== 'del' : x.old === n && x.kind !== 'add'));
      if (l) {
        text = l.text;
        break;
      }
    }
    if (text === undefined && side === 'new' && fileLines) text = fileLines[n - 1];
    out.push(text ?? '');
  }
  return out;
}

const sameTags = (a: ReviewFile, b: ReviewFile) => a.hunks.every((h, i) => JSON.stringify(h.tags ?? null) === JSON.stringify(b.hunks[i]?.tags ?? null));

export function keepUnchanged(prev: ReviewFile[] | undefined, next: ReviewFile[]): ReviewFile[] {
  if (!prev?.length) return next;
  const old = new Map(prev.map((f) => [f.path, f]));
  let same = prev.length === next.length;
  const out = next.map((f, i) => {
    const o = old.get(f.path);
    const keep = o && o.hash === f.hash && o.viewed === f.viewed && o.truncated === f.truncated && o.risk === f.risk && o.residual === f.residual && sameTags(o, f) ? o : f;
    if (keep !== prev[i]) same = false;
    return keep;
  });
  return same ? prev : out;
}

export function totals(files: ReviewFile[]) {
  return files.reduce((a, f) => ({ additions: a.additions + f.additions, deletions: a.deletions + f.deletions, viewed: a.viewed + (f.viewed ? 1 : 0) }), { additions: 0, deletions: 0, viewed: 0 });
}

export interface TreeNode {
  name: string;
  path: string;
  dir: boolean;
  file?: ReviewFile;
  children: TreeNode[];
}

export function fileTree(files: ReviewFile[]): TreeNode[] {
  const root: TreeNode = { name: '', path: '', dir: true, children: [] };
  const index = new Map<TreeNode, Map<string, TreeNode>>();
  for (const f of files) {
    const parts = f.path.split('/');
    let node = root;
    parts.forEach((p, i) => {
      const path = parts.slice(0, i + 1).join('/');
      const leaf = i === parts.length - 1;
      const key = `${leaf ? 'f' : 'd'}${p}`;
      let byKey = index.get(node);
      if (!byKey) index.set(node, (byKey = new Map()));
      let child = byKey.get(key);
      if (!child) {
        child = { name: p, path, dir: !leaf, file: leaf ? f : undefined, children: [] };
        node.children.push(child);
        byKey.set(key, child);
      }
      node = child;
    });
  }
  const compact = (n: TreeNode): TreeNode => {
    let c = n;
    while (c.dir && c.children.length === 1 && c.children[0].dir) {
      const only = c.children[0];
      c = { ...only, name: c.name ? `${c.name}/${only.name}` : only.name };
    }
    const children = c.children.map(compact).sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
    return { ...c, children };
  };
  return root.children.map(compact).sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name));
}
