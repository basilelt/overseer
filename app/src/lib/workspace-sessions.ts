import { newestFirst, type SidebarItem } from './grouping';
import { STATE_URGENCY } from './session-state';
import { bucketByTime, type TimeBucket } from './time-bucket';

export interface WorkspaceSessions {
  /** Attention, running, open, then paused; most recent first inside a state. */
  live: SidebarItem[];
  closed: { count: number; buckets: { bucket: TimeBucket; items: SidebarItem[] }[] };
}

const RANK = new Map(STATE_URGENCY.map((s, i) => [s, i]));

// Mode "Espaces": the sessions of one expanded workspace, plus its closed ones for the "+ N closed" list.
export function workspaceSessions(items: SidebarItem[], now: number): WorkspaceSessions {
  const sessions = items.filter((i) => i.kind !== 'shell').sort(newestFirst);
  const live = sessions.filter((i) => i.state !== 'closed').sort((a, b) => RANK.get(a.state)! - RANK.get(b.state)!);
  const closed = sessions.filter((i) => i.state === 'closed');
  return { live, closed: { count: closed.length, buckets: bucketByTime(closed, now) } };
}
