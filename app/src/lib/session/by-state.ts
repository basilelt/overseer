import { newestFirst, type SidebarItem } from './grouping';
import { bucketByTime, type TimeBucket } from '../time-bucket';
import { isAgentItem } from '../agents';

export interface WorkspaceGroup {
  workspaceId: string | null;
  items: SidebarItem[];
  at: string;
}

export type LiveSectionState = 'running' | 'open' | 'paused';

export type StateSection =
  | { state: LiveSectionState; count: number; groups: WorkspaceGroup[] }
  | { state: 'closed'; count: number; buckets: { bucket: TimeBucket; count: number; groups: WorkspaceGroup[] }[] };

const LIVE: LiveSectionState[] = ['running', 'open', 'paused'];

// items must already be newest first, so every group keeps that order.
function groupByWorkspace(items: SidebarItem[], wsOf: ReadonlyMap<string, string>, visible?: ReadonlySet<string>): WorkspaceGroup[] {
  const by = new Map<string | null, WorkspaceGroup>();
  for (const it of items) {
    const w = wsOf.get(it.key);
    const id = w !== undefined && (!visible || visible.has(w)) ? w : null;
    const g = by.get(id);
    if (g) g.items.push(it);
    else by.set(id, { workspaceId: id, items: [it], at: it.at });
  }
  return [...by.values()].sort((a, b) => (a.workspaceId === null ? 1 : 0) - (b.workspaceId === null ? 1 : 0) || newestFirst(a, b));
}

export function byState(items: SidebarItem[], wsOf: ReadonlyMap<string, string>, now: number, visible?: ReadonlySet<string>): StateSection[] {
  const sorted = items.filter((i) => isAgentItem(i)).sort(newestFirst);
  const live = LIVE.map((state) => {
    const mine = sorted.filter((i) => i.state === state);
    return { state, count: mine.length, groups: groupByWorkspace(mine, wsOf, visible) };
  });
  const closed = sorted.filter((i) => i.state === 'closed');
  const buckets = bucketByTime(closed, now).map((b) => ({ bucket: b.bucket, count: b.items.length, groups: groupByWorkspace(b.items, wsOf, visible) }));
  return [...live, { state: 'closed', count: closed.length, buckets }];
}
