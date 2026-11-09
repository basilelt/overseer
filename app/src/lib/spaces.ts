import type { SidebarItem } from './grouping';
import { STATE_URGENCY } from './session-state';
import type { ConnState, Folder, Workspace } from './types';
import { byRank, isGhost, type WsData } from './workspaces';
import { workspaceSummary, type WorkspaceSummary } from './workspace-summary';

export interface SpaceRow {
  /** '' for the sessions outside any workspace. */
  id: string;
  name: string | null;
  workspace: Workspace | null;
  items: SidebarItem[];
  summary: WorkspaceSummary;
}

export interface Spaces {
  rows: SpaceRow[];
  /** Workspaces without a session: folded into one line of chips. */
  empty: Workspace[];
}

const isVisible = (w: Workspace, showArchived = false) => !isGhost(w) && (showArchived || !w.archived);

// The workspaces a session can be listed under; any other falls to the unassigned.
export const visibleIds = (workspaces: Workspace[]): Set<string> => new Set(workspaces.filter((w) => isVisible(w)).map((w) => w.id));

// Keeps the sessions whose title or workspace name contains the filter.
export function filterItems(items: SidebarItem[], wsName: (key: string) => string, filter: string): SidebarItem[] {
  const q = filter.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.title.toLowerCase().includes(q) || wsName(i.key).toLowerCase().includes(q));
}

// Mode "Espaces": one pass over the items, then the workspaces with sessions (pinned first) and the empty ones.
// Sessions whose workspace is hidden (archived unless showArchived, ghost, unknown) fall to the "other" row.
export function spaces(workspaces: Workspace[], items: SidebarItem[], wsOf: ReadonlyMap<string, string>, filter: string, showArchived = false): Spaces {
  const q = filter.trim().toLowerCase();
  const live = workspaces
    .filter((w) => isVisible(w, showArchived))
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.rank - b.rank || a.name.localeCompare(b.name));
  const visible = new Set(live.map((w) => w.id));
  const by = new Map<string, SidebarItem[]>();
  for (const it of items) {
    if (it.kind === 'shell' && it.state === 'closed') continue;
    const w = wsOf.get(it.key);
    const id = w !== undefined && visible.has(w) ? w : '';
    const list = by.get(id);
    if (list) list.push(it);
    else by.set(id, [it]);
  }
  // A name or path match keeps every session; a title match keeps only the sessions that match.
  const named = (w: Workspace | null) => !!w && [w.name, w.path, w.cwd].some((x) => x?.toLowerCase().includes(q));
  const narrow = (w: Workspace | null, mine: SidebarItem[]) => (!q || named(w) ? mine : mine.filter((i) => i.title.toLowerCase().includes(q)));
  const rows: SpaceRow[] = [];
  const empty: Workspace[] = [];
  for (const w of live) {
    const all = by.get(w.id) ?? [];
    const mine = narrow(w, all);
    if (mine.length) rows.push({ id: w.id, name: w.name, workspace: w, items: mine, summary: workspaceSummary(mine) });
    else if (!all.length && (!q || named(w))) empty.push(w);
  }
  const others = narrow(null, by.get('') ?? []);
  if (others.length) rows.push({ id: '', name: null, workspace: null, items: others, summary: workspaceSummary(others) });
  return { rows, empty };
}

export type SpaceNode =
  /** folder null is the pinned section; depth 0 is a section title. */
  | { type: 'folder'; key: string; folder: Folder | null; depth: number; open: boolean; folders: number; spaces: number; summary: WorkspaceSummary }
  | { type: 'space'; key: string; row: SpaceRow; depth: number }
  | { type: 'empty'; key: string; workspaces: Workspace[]; depth: number };

interface Agg {
  summary: WorkspaceSummary;
  /** Workspaces listed in the subtree, busy or empty. */
  spaces: number;
}

function push<K, V>(m: Map<K, V[]>, k: K, v: V) {
  const list = m.get(k);
  if (list) list.push(v);
  else m.set(k, [v]);
}

function sum(parts: WorkspaceSummary[]): WorkspaceSummary {
  const counts: WorkspaceSummary['counts'] = { attention: 0, running: 0, open: 0, paused: 0, closed: 0 };
  let total = 0;
  for (const p of parts) {
    total += p.total;
    for (const s of STATE_URGENCY) counts[s] += p.counts[s];
  }
  return { total, counts, urgent: STATE_URGENCY.find((s) => counts[s] > 0) ?? null };
}

// Mode "Espaces" with folders: files the rows of spaces() into the folder tree, as one flat list to render.
// Pinned workspaces leave their folder for a pinned section; a filter drops the folders without a match and opens the rest.
export function folderSpaces(view: Spaces, folders: Folder[], filter: string): SpaceNode[] {
  const q = filter.trim() !== '';
  const known = new Set(folders.map((f) => f.id));
  const kids = new Map<string | null, Folder[]>();
  for (const f of folders) push(kids, f.parentId && f.parentId !== f.id && known.has(f.parentId) ? f.parentId : null, f);
  for (const list of kids.values()) list.sort(byRank);
  // Folders caught in a parent cycle are never reached: their workspaces fall to the root.
  const reach = new Set<string>();
  const stack = [...(kids.get(null) ?? [])];
  for (let f = stack.pop(); f; f = stack.pop()) {
    reach.add(f.id);
    stack.push(...(kids.get(f.id) ?? []));
  }
  const at = (w: Workspace) => (w.folderId && reach.has(w.folderId) ? w.folderId : null);
  const rowsIn = new Map<string | null, SpaceRow[]>();
  const emptyIn = new Map<string | null, Workspace[]>();
  const pinnedRows: SpaceRow[] = [];
  const pinnedEmpty: Workspace[] = [];
  for (const r of view.rows) {
    if (r.workspace?.pinned) pinnedRows.push(r);
    else push(rowsIn, r.workspace ? at(r.workspace) : null, r);
  }
  for (const w of view.empty) {
    if (w.pinned) pinnedEmpty.push(w);
    else push(emptyIn, at(w), w);
  }
  const aggs = new Map<string, Agg>();
  const agg = (f: Folder): Agg => {
    const done = aggs.get(f.id);
    if (done) return done;
    const rows = rowsIn.get(f.id) ?? [];
    const subs = (kids.get(f.id) ?? []).map(agg);
    const a: Agg = {
      summary: sum([...rows.map((r) => r.summary), ...subs.map((x) => x.summary)]),
      spaces: rows.length + (emptyIn.get(f.id)?.length ?? 0) + subs.reduce((n, x) => n + x.spaces, 0),
    };
    aggs.set(f.id, a);
    return a;
  };
  const out: SpaceNode[] = [];
  const level = (rows: SpaceRow[] | undefined, empty: Workspace[] | undefined, depth: number, key: string, between?: () => void) => {
    for (const row of rows ?? []) out.push({ type: 'space', key: `w:${row.id}`, row, depth });
    between?.();
    if (empty?.length) out.push({ type: 'empty', key: `e:${key}`, workspaces: empty, depth });
  };
  const emit = (f: Folder, depth: number) => {
    const a = agg(f);
    if (q && !a.spaces) return;
    const subs = (kids.get(f.id) ?? []).filter((c) => !q || agg(c).spaces > 0);
    const open = q || !f.collapsed;
    out.push({ type: 'folder', key: `f:${f.id}`, folder: f, depth, open, folders: subs.length, spaces: a.spaces, summary: a.summary });
    if (open) level(rowsIn.get(f.id), emptyIn.get(f.id), depth + 1, f.id, () => subs.forEach((c) => emit(c, depth + 1)));
  };
  if (pinnedRows.length || pinnedEmpty.length) {
    out.push({ type: 'folder', key: 'f:*', folder: null, depth: 0, open: true, folders: 0, spaces: pinnedRows.length + pinnedEmpty.length, summary: sum(pinnedRows.map((r) => r.summary)) });
    level(pinnedRows, pinnedEmpty, 0, '*');
  }
  for (const f of kids.get(null) ?? []) emit(f, 0);
  level(rowsIn.get(null), emptyIn.get(null), 0, '');
  return out;
}

// The attention banner's path to a workspace: its folder chain, then its name.
export function spaceTrail(data: WsData): (workspaceId: string | null | undefined) => string | undefined {
  const ws = new Map(data.workspaces.map((w) => [w.id, w]));
  const fs = new Map(data.folders.map((f) => [f.id, f]));
  return (id) => {
    const w = id ? ws.get(id) : undefined;
    if (!w) return undefined;
    const names = [w.name];
    const seen = new Set<string>();
    for (let f = w.folderId ? fs.get(w.folderId) : undefined; f && !seen.has(f.id); f = f.parentId ? fs.get(f.parentId) : undefined) {
      seen.add(f.id);
      names.unshift(f.name);
    }
    return names.join(' › ');
  };
}

export interface SpaceHost {
  name: string;
  phase: ConnState['phase'];
  attempt?: number;
  version?: string;
}

// The host chip of a remote connection (ssh or direct URL); a local daemon shows none.
export function hostFor(conn: Pick<ConnState, 'target' | 'phase' | 'attempt' | 'version'>): SpaceHost | undefined {
  const { target } = conn;
  if (target.kind === 'local') return undefined;
  return { name: target.host ?? target.label, phase: conn.phase, attempt: conn.attempt, version: conn.version };
}

// The saved expanded list after toggling id: unset starts from the fallback; ids of gone workspaces are dropped,
// but never while the list is still empty (loading, or no persistence) since that would wipe the host's choices.
export function toggleOpen(saved: string[] | undefined, id: string, known: ReadonlySet<string>, fallback: string): string[] {
  const next = new Set((saved ?? [fallback]).filter((x) => known.size <= 1 || known.has(x)));
  if (!next.delete(id)) next.add(id);
  return [...next];
}

// The expanded list after picking id, or null when nothing changes. An unset list stands for "the active workspace only":
// picking another one pins that default first, otherwise the previous workspace would fold as the active one changes.
export function openOnPick(saved: string[] | undefined, active: string, id: string): string[] | null {
  const open = saved ?? [active];
  if (open.includes(id)) return null;
  return [...open, id];
}
