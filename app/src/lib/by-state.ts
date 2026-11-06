import { newestFirst, type SidebarItem } from './grouping';
import { bucketByTime, type TimeBucket } from './time-bucket';

export interface WorkspaceGroup {
  /** null for the sessions outside any workspace ("Autres sessions"), always listed last. */
  workspaceId: string | null;
  items: SidebarItem[];
  /** The most recent activity in the group, which orders the groups. */
  at: string;
}

export type LiveSectionState = 'running' | 'open' | 'paused';

export type StateSection =
  | { state: LiveSectionState; count: number; groups: WorkspaceGroup[] }
  | { state: 'closed'; count: number; buckets: { bucket: TimeBucket; count: number; groups: WorkspaceGroup[] }[] };

const LIVE: LiveSectionState[] = ['running', 'open', 'paused'];

// items must already be newest first, so every group keeps that order.
function groupByWorkspace(items: SidebarItem[], wsOf: ReadonlyMap<string, string>): WorkspaceGroup[] {
  const by = new Map<string | null, WorkspaceGroup>();
  for (const it of items) {
    const id = wsOf.get(it.key) ?? null;
    const g = by.get(id);
    if (g) g.items.push(it);
    else by.set(id, { workspaceId: id, items: [it], at: it.at });
  }
  return [...by.values()].sort((a, b) => (a.workspaceId === null ? 1 : 0) - (b.workspaceId === null ? 1 : 0) || newestFirst(a, b));
}

// Mode "Par état": running, open, paused sections of workspace groups, then closed sessions by interval.
// Sessions requiring attention are left out (the banner lists them) and plain shells are not sessions.
export function byState(items: SidebarItem[], wsOf: ReadonlyMap<string, string>, now: number): StateSection[] {
  const sorted = items.filter((i) => i.kind !== 'shell').sort(newestFirst);
  const live = LIVE.map((state) => {
    const mine = sorted.filter((i) => i.state === state);
    return { state, count: mine.length, groups: groupByWorkspace(mine, wsOf) };
  });
  const closed = sorted.filter((i) => i.state === 'closed');
  const buckets = bucketByTime(closed, now).map((b) => ({ bucket: b.bucket, count: b.items.length, groups: groupByWorkspace(b.items, wsOf) }));
  return [...live, { state: 'closed', count: closed.length, buckets }];
}
