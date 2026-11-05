import type { Group, GroupId, SidebarItem } from '../../grouping';

export type Row = { type: 'header'; id: GroupId; count: number; collapsed: boolean } | { type: 'card'; item: SidebarItem };

export const HEADER_H = 30;
export const CARD_H = 76;

export function flatten(groups: Group[], collapsed: Set<GroupId>, filter = ''): Row[] {
  const q = filter.trim().toLowerCase();
  const rows: Row[] = [];
  for (const g of groups) {
    const items = q ? g.items.filter((i) => `${i.title} ${i.goal ?? ''} ${i.status ?? ''}`.toLowerCase().includes(q)) : g.items;
    if (!items.length) continue;
    const c = collapsed.has(g.id);
    rows.push({ type: 'header', id: g.id, count: items.length, collapsed: c });
    if (!c) for (const item of items) rows.push({ type: 'card', item });
  }
  return rows;
}

export function rowHeight(r: Row): number {
  return r.type === 'header' ? HEADER_H : CARD_H;
}

export interface Window {
  start: number;
  end: number;
  offsets: number[];
  total: number;
}

export function windowRows(rows: Row[], scrollTop: number, viewport: number, overscan = 4): Window {
  const offsets: number[] = [];
  let y = 0;
  for (const r of rows) {
    offsets.push(y);
    y += rowHeight(r);
  }
  let start = 0;
  while (start < rows.length && offsets[start] + rowHeight(rows[start]) <= scrollTop) start++;
  let end = start;
  while (end < rows.length && offsets[end] < scrollTop + viewport) end++;
  return { start: Math.max(0, start - overscan), end: Math.min(rows.length, end + overscan), offsets, total: y };
}

export function moveSelection(rows: Row[], current: string | null, delta: 1 | -1): string | null {
  const keys = rows.flatMap((r) => (r.type === 'card' ? [r.item.key] : []));
  if (!keys.length) return null;
  const i = current ? keys.indexOf(current) : -1;
  if (i < 0) return delta > 0 ? keys[0] : keys[keys.length - 1];
  return keys[Math.min(keys.length - 1, Math.max(0, i + delta))];
}
