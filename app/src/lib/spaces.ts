import type { SidebarItem } from './grouping';
import type { Workspace } from './types';
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

// Mode "Espaces": one pass over the items, then the workspaces with sessions (pinned first) and the empty ones.
export function spaces(workspaces: Workspace[], items: SidebarItem[], wsOf: ReadonlyMap<string, string>, filter: string): Spaces {
  const q = filter.trim().toLowerCase();
  const by = new Map<string, SidebarItem[]>();
  for (const it of items) {
    if (it.kind === 'shell') continue;
    const id = wsOf.get(it.key) ?? '';
    const list = by.get(id);
    if (list) list.push(it);
    else by.set(id, [it]);
  }
  const matches = (name: string, mine: SidebarItem[]) => !q || name.toLowerCase().includes(q) || mine.some((i) => i.title.toLowerCase().includes(q));
  const live = workspaces
    .filter((w) => !w.archived)
    .sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.rank - b.rank || a.name.localeCompare(b.name));
  const rows: SpaceRow[] = [];
  const empty: Workspace[] = [];
  for (const w of live) {
    const mine = by.get(w.id) ?? [];
    if (!matches(w.name, mine)) continue;
    if (mine.length) rows.push({ id: w.id, name: w.name, workspace: w, items: mine, summary: workspaceSummary(mine) });
    else empty.push(w);
  }
  const others = by.get('');
  if (others && matches('', others)) rows.push({ id: '', name: null, workspace: null, items: others, summary: workspaceSummary(others) });
  return { rows, empty };
}
