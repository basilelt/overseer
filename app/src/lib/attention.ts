import { newestFirst, type SidebarItem } from './grouping';
import { isAgentItem } from './agents';

export interface AttentionEntry {
  key: string;
  lineId?: string;
  termId?: string;
  title: string;
  /** What the session last did; the pending question itself is not carried by the line. */
  detail?: string;
  workspaceId: string | null;
  at: string;
}

// The banner list: sessions waiting for the user, most recent first. wsOf maps an item key to its workspace id.
export function attention(items: SidebarItem[], wsOf: ReadonlyMap<string, string>): AttentionEntry[] {
  return items
    .filter((i) => i.state === 'attention' && isAgentItem(i))
    .sort(newestFirst)
    .map((i) => ({ key: i.key, lineId: i.lineId, termId: i.termId, title: i.title, detail: i.status, workspaceId: wsOf.get(i.key) ?? null, at: i.at }));
}
