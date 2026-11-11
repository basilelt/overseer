import { newestFirst, type SidebarItem } from './grouping';
import { STATE_URGENCY } from './session-state';
import { bucketByTime, type TimeBucket } from './time-bucket';

export interface WorkspaceSessions {
  /** Attention, running, open, then paused; most recent first inside a state. */
  live: SidebarItem[];
  closed: { count: number; buckets: { bucket: TimeBucket; items: SidebarItem[] }[] };
  /** Plain shells still running, most recent first, listed after the sessions. */
  shells: SidebarItem[];
}

const RANK = new Map(STATE_URGENCY.map((s, i) => [s, i]));

// Mode "Espaces": the sessions of one expanded workspace, plus its closed ones for the "+ N closed" list.
export function workspaceSessions(items: SidebarItem[], now: number): WorkspaceSessions {
  const sorted = items.slice().sort(newestFirst);
  const sessions = sorted.filter((i) => i.kind !== 'shell');
  const live = sessions.filter((i) => i.state !== 'closed').sort((a, b) => RANK.get(a.state)! - RANK.get(b.state)!);
  const closed = sessions.filter((i) => i.state === 'closed');
  const shells = sorted.filter((i) => i.kind === 'shell' && i.state !== 'closed');
  return { live, closed: { count: closed.length, buckets: bucketByTime(closed, now) }, shells };
}
