import { STATE_URGENCY, type SessionState } from './session-state';
import type { SidebarItem } from './grouping';
import { isShellItem } from './agents';

export interface WorkspaceSummary {
  total: number;
  counts: Record<SessionState, number>;
  /** The most urgent state present, or null for a workspace without sessions. */
  urgent: SessionState | null;
}

// Counts agent sessions per state; plain shells are not sessions.
export function workspaceSummary(items: Pick<SidebarItem, 'state' | 'kind'>[]): WorkspaceSummary {
  const counts: Record<SessionState, number> = { attention: 0, running: 0, open: 0, paused: 0, closed: 0 };
  let total = 0;
  for (const it of items) {
    if (isShellItem(it)) continue;
    counts[it.state]++;
    total++;
  }
  return { total, counts, urgent: STATE_URGENCY.find((s) => counts[s] > 0) ?? null };
}
