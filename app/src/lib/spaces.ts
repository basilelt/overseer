import type { SidebarItem } from './grouping';
import type { ConnState, Workspace } from './types';
import { isGhost } from './workspaces';
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

const isVisible = (w: Workspace) => !isGhost(w) && !w.archived;

// The workspaces a session can be listed under; any other falls to the unassigned.
export const visibleIds = (workspaces: Workspace[]): Set<string> => new Set(workspaces.filter(isVisible).map((w) => w.id));

// Keeps the sessions whose title or workspace name contains the filter.
export function filterItems(items: SidebarItem[], wsName: (key: string) => string, filter: string): SidebarItem[] {
  const q = filter.trim().toLowerCase();
  if (!q) return items;
  return items.filter((i) => i.title.toLowerCase().includes(q) || wsName(i.key).toLowerCase().includes(q));
}

// Mode "Espaces": one pass over the items, then the workspaces with sessions (pinned first) and the empty ones.
// Sessions whose workspace is hidden (archived, ghost, unknown) fall to the "other" row.
export function spaces(workspaces: Workspace[], items: SidebarItem[], wsOf: ReadonlyMap<string, string>, filter: string): Spaces {
  const q = filter.trim().toLowerCase();
  const live = workspaces
    .filter(isVisible)
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.rank - b.rank || a.name.localeCompare(b.name));
  const visible = new Set(live.map((w) => w.id));
  const by = new Map<string, SidebarItem[]>();
  for (const it of items) {
    if (it.kind === 'shell') continue;
    const w = wsOf.get(it.key);
    const id = w !== undefined && visible.has(w) ? w : '';
    const list = by.get(id);
    if (list) list.push(it);
    else by.set(id, [it]);
  }
  // A name match keeps every session; a title match keeps only the sessions that match.
  const narrow = (name: string | null, mine: SidebarItem[]) => (!q || (name !== null && name.toLowerCase().includes(q)) ? mine : mine.filter((i) => i.title.toLowerCase().includes(q)));
  const rows: SpaceRow[] = [];
  const empty: Workspace[] = [];
  for (const w of live) {
    const all = by.get(w.id) ?? [];
    const mine = narrow(w.name, all);
    if (mine.length) rows.push({ id: w.id, name: w.name, workspace: w, items: mine, summary: workspaceSummary(mine) });
    else if (!all.length && (!q || w.name.toLowerCase().includes(q))) empty.push(w);
  }
  const others = narrow(null, by.get('') ?? []);
  if (others.length) rows.push({ id: '', name: null, workspace: null, items: others, summary: workspaceSummary(others) });
  return { rows, empty };
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
