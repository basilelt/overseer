import { STATE_URGENCY, type SessionState } from '../session/session-state';
import type { SidebarItem } from '../session/grouping';
import { isShellItem } from '../agents';

export interface WorkspaceSummary {
  total: number;
  counts: Record<SessionState, number>;
  urgent: SessionState | null;
}

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
