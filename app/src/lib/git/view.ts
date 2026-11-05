import { within } from '../workspaces';
import type { GraphCommit, GraphPage, GraphRef, Overlay, Worktree } from './types';

export const ROW_H = 28;
export const LANE_W = 14;
export const MAX_LANES = 14;
export const PALETTE_SIZE = 6;

export interface Range {
  start: number;
  end: number;
}

export function visibleRange(count: number, scrollTop: number, viewport: number, rowH = ROW_H, overscan = 12): Range {
  if (count <= 0) return { start: 0, end: 0 };
  const first = Math.max(0, Math.floor(scrollTop / rowH));
  const last = Math.min(count, Math.ceil((scrollTop + viewport) / rowH));
  return { start: Math.max(0, first - overscan), end: Math.min(count, last + overscan) };
}

export function needsMore(range: Range, count: number, hasNext: boolean, margin = 60): boolean {
  return hasNext && range.end >= count - margin;
}

export function laneX(lane: number): number {
  return 10 + lane * LANE_W;
}

export function graphWidth(lanes: number): number {
  return laneX(Math.min(Math.max(lanes, 3), MAX_LANES) - 1) + 14;
}

export interface Label {
  name: string;
  kind: 'head' | 'branch' | 'remote' | 'tag';
}

export function labelsBySha(page: Pick<GraphPage, 'refs' | 'head'>): Map<string, Label[]> {
  const out = new Map<string, Label[]>();
  const push = (sha: string, l: Label) => {
    const list = out.get(sha);
    if (list) list.push(l);
    else out.set(sha, [l]);
  };
  const order = { head: 0, branch: 1, remote: 2, tag: 3 };
  for (const r of page.refs) {
    const kind: Label['kind'] = r.kind === 'branch' && r.name === page.head.branch ? 'head' : r.kind;
    push(r.sha, { name: r.name, kind });
  }
  if (!page.head.branch && page.head.sha) push(page.head.sha, { name: 'HEAD', kind: 'head' });
  for (const list of out.values()) list.sort((a, b) => order[a.kind] - order[b.kind] || a.name.localeCompare(b.name));
  return collapseRemotes(out, page.refs);
}

function collapseRemotes(map: Map<string, Label[]>, refs: GraphRef[]): Map<string, Label[]> {
  const local = new Set(refs.filter((r) => r.kind === 'branch').map((r) => `${r.sha} ${r.name}`));
  for (const [sha, list] of map) {
    map.set(
      sha,
      list.filter((l) => l.kind !== 'remote' || !local.has(`${sha} ${l.name.replace(/^[^/]+\//, '')}`)),
    );
  }
  return map;
}

export function overlaysBySha(overlays: Overlay[]): Map<string, Overlay[]> {
  const out = new Map<string, Overlay[]>();
  for (const o of overlays) {
    const list = out.get(o.sha);
    if (list) list.push(o);
    else out.set(o.sha, [o]);
  }
  return out;
}

export function authors(commits: GraphCommit[]): string[] {
  const n = new Map<string, number>();
  for (const c of commits) n.set(c.author, (n.get(c.author) ?? 0) + 1);
  return [...n.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([a]) => a);
}

export function dimmed(c: GraphCommit, author: string): boolean {
  return !!author && c.author !== author;
}

export function searchMatches(commits: GraphCommit[], query: string): number[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const hex = /^[0-9a-f]{4,40}$/.test(q);
  const out: number[] = [];
  for (let i = 0; i < commits.length; i++) {
    const c = commits[i];
    if ((hex && c.sha.startsWith(q)) || c.subject.toLowerCase().includes(q)) out.push(i);
  }
  return out;
}

export function stepMatch(matches: number[], current: number, dir: 1 | -1): number {
  if (!matches.length) return -1;
  if (dir > 0) return matches.find((i) => i > current) ?? matches[0];
  for (let k = matches.length - 1; k >= 0; k--) if (matches[k] < current) return matches[k];
  return matches[matches.length - 1];
}

export function scrollToRow(index: number, scrollTop: number, viewport: number, rowH = ROW_H): number {
  const top = index * rowH;
  if (top < scrollTop) return top;
  if (top + rowH > scrollTop + viewport) return top + rowH - viewport;
  return scrollTop;
}

export function commitUrl(repo: string | undefined, sha: string): string | null {
  return repo?.startsWith('github.com/') ? `https://${repo}/commit/${sha}` : null;
}

export function formatSize(bytes: number | null | undefined): string {
  if (bytes == null) return '—';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  return `${v >= 10 || u === 0 ? Math.round(v) : v.toFixed(1)} ${units[u]}`;
}

export function sortWorktrees(list: Worktree[]): Worktree[] {
  const rank = { in_use: 0, idle: 1, stale: 2, missing: 3 };
  return list.slice().sort((a, b) => {
    if (a.kind === 'main' || b.kind === 'main') return a.kind === 'main' ? -1 : 1;
    return rank[a.state] - rank[b.state] || (b.lastActivityAt ?? '').localeCompare(a.lastActivityAt ?? '');
  });
}

export function cleanable(w: Worktree): boolean {
  return w.kind === 'managed' && (w.state === 'stale' || w.state === 'missing');
}

export function unused(w: Worktree): boolean {
  return w.kind === 'managed' && !w.lines.length && !w.termIds.length && !w.dirty && (w.state === 'missing' || (w.merged && w.state !== 'in_use'));
}

export const listFor = (listProject: string, panelProject: string): boolean => within(panelProject, listProject);
