import { newestFirst, type SidebarItem } from '../session/grouping';
import { STATE_URGENCY } from '../session/session-state';
import { bucketByTime, type TimeBucket } from '../time-bucket';
import { isShellItem, isAgentItem } from '../agents';

export interface WorkspaceSessions {
  /** In tab order; sessions without a tab after, by attention, running, open, paused, most recent first inside a state. */
  live: SidebarItem[];
  closed: { count: number; buckets: { bucket: TimeBucket; items: SidebarItem[] }[] };
  /** Plain shells still running, in tab order then most recent first, listed after the sessions. */
  shells: SidebarItem[];
}

const RANK = new Map(STATE_URGENCY.map((s, i) => [s, i]));

// Mode "Espaces": the sessions of one expanded workspace, plus its closed ones for the "+ N closed" list.
export function workspaceSessions(items: SidebarItem[], now: number, tabs: readonly string[] = []): WorkspaceSessions {
  const sorted = items.slice().sort(newestFirst);
  const sessions = sorted.filter((i) => isAgentItem(i));
  const live = byTabs(
    sessions.filter((i) => i.state !== 'closed').sort((a, b) => RANK.get(a.state)! - RANK.get(b.state)!),
    tabs,
  );
  const closed = sessions.filter((i) => i.state === 'closed');
  const shells = byTabs(
    sorted.filter((i) => isShellItem(i) && i.state !== 'closed'),
    tabs,
  );
  return { live, closed: { count: closed.length, buckets: bucketByTime(closed, now) }, shells };
}

// Stable: items without a tab keep their order, after the tabbed ones. A saved tab may name the terminal of a session.
function byTabs(list: SidebarItem[], tabs: readonly string[]): SidebarItem[] {
  if (!tabs.length) return list;
  const at = new Map(tabs.map((k, i) => [k, i]));
  const pos = new Map(list.map((i) => [i, at.get(i.key) ?? (i.termId ? at.get(`term:${i.termId}`) : undefined) ?? Infinity]));
  return list.sort((a, b) => {
    const pa = pos.get(a)!;
    const pb = pos.get(b)!;
    return pa === pb ? 0 : pa - pb;
  });
}
