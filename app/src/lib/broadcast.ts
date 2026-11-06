import type { SidebarItem } from './grouping';
import type { BroadcastRequest, BroadcastResult } from './types';

export type TargetState = 'idle' | 'busy' | 'needs_input' | 'paused' | 'stopped';

export interface TargetRow {
  key: string;
  title: string;
  kind: string;
  state: TargetState;
  selectable: boolean;
  checked: boolean;
}

export function targetState(i: SidebarItem): TargetState {
  if (i.group === 'paused' || i.group === 'stopped') return i.group;
  if (i.kind === 'shell') return 'idle';
  if (i.group === 'needs_input') return 'needs_input';
  if (i.group === 'running') return 'busy';
  return 'idle';
}

export function targetRows(items: SidebarItem[], queue: boolean): TargetRow[] {
  const order: TargetState[] = ['idle', 'needs_input', 'busy', 'paused', 'stopped'];
  return items
    .filter((i) => i.lineId || i.termId)
    .map((i) => {
      const state = targetState(i);
      const selectable = state === 'idle' || (queue && state !== 'paused' && state !== 'stopped');
      return { key: i.key, title: i.title, kind: i.kind, state, selectable, checked: selectable && (state === 'idle' ? i.kind !== 'shell' : true) };
    })
    .sort((a, b) => order.indexOf(a.state) - order.indexOf(b.state) || a.title.localeCompare(b.title));
}

export function broadcastBody(keys: string[], items: SidebarItem[], text: string, queue: boolean): BroadcastRequest {
  const lineIds: string[] = [];
  const termIds: string[] = [];
  for (const k of keys) {
    const it = items.find((i) => i.key === k);
    if (it?.lineId) lineIds.push(it.lineId);
    else if (it?.termId) termIds.push(it.termId);
  }
  return { text: text.trim(), lineIds, termIds, onlyIdle: true, queue };
}

export function summarize(results: BroadcastResult[]): Record<BroadcastResult['result'], number> {
  const out = { sent: 0, skipped: 0, queued: 0, refused: 0 };
  for (const r of results) out[r.result]++;
  return out;
}
